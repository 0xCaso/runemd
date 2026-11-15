//! Reread of the open file: when an agent writes it, the document updates with
//! `Document::update` (stable ids, so the row being read stays still).

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::SystemTime;

use crate::doc::document::{Change, Document, Prepared, Snapshot};

/// The hashes of the contents the app itself wrote, per path: their reread is not
/// an agent write. Each entry holds only once: after the app writes X, an agent Y,
/// then X again, that X is the agent's.
#[derive(Debug, Default)]
pub struct OwnWrites {
    hashes: Mutex<HashMap<PathBuf, u64>>,
}

impl OwnWrites {
    /// To call after the app wrote `hash` to `path` (`edit::save`).
    pub fn remember(&mut self, path: &Path, hash: u64) {
        self.map().insert(path.to_owned(), hash);
    }

    /// Whether `hash` is the app's last write to `path` not yet reread. Does not consume the entry.
    #[cfg(test)]
    pub fn is_own(&self, path: &Path, hash: u64) -> bool {
        self.map().get(path) == Some(&hash)
    }

    /// Like `is_own`, and consumes the entry: the reread of the app's write.
    fn take(&self, path: &Path, hash: u64) -> bool {
        let mut map = self.map();
        let own = map.get(path) == Some(&hash);
        if own {
            map.remove(path);
        }
        own
    }

    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, u64>> {
        self.hashes.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A read shorter than this fraction of the previous one (1/10) is suspicious: whoever rewrites
/// on the same inode (Codex, `docs/agent-writes.md`) truncates the file and then writes it, and a
/// read in the middle sees the file empty or half written.
const SHORT_READ_DIVISOR: usize = 10;

/// The read is much shorter than the document being shown (`before` bytes of source): maybe
/// the file is half written.
pub fn suspect(before: usize, new_source: &str) -> bool {
    new_source.len() * SHORT_READ_DIVISOR < before
}

/// A read of the file prepared off the UI thread.
#[derive(Debug)]
pub struct Read {
    pub hash: u64,
    pub suspect: bool,
    /// The mtime of the file read: when it was written (latency probe).
    pub written: Option<SystemTime>,
    /// `None` if the content is the snapshot's.
    prepared: Option<Prepared>,
}

/// Reads `path` and prepares the update of `snap`: all the heavy work (read, UTF-8,
/// hash, blocks, diff), to do on another thread.
pub fn read(path: &Path, snap: &Snapshot) -> io::Result<Read> {
    let (source, hash, _) = crate::doc::disk::read_disk(path)?;
    let written = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let prepared = (hash != snap.file_hash).then(|| snap.prepare(&source));
    Ok(Read { hash, suspect: suspect(snap.source_len(), &source), written, prepared })
}

pub enum Commit {
    Applied(Change),
    /// Same content or app write: nothing to show.
    Skipped,
    /// The document changed after the snapshot: it must be reread.
    Stale,
}

/// The step of [`read`] on the UI thread: drops the app's own writes and applies the rest.
pub fn commit(doc: &mut Document, read: Read, own_writes: &OwnWrites) -> Commit {
    if own_writes.take(&doc.path, read.hash) || read.hash == doc.file_hash {
        return Commit::Skipped;
    }
    let Some(prepared) = read.prepared else { return Commit::Stale };
    doc.commit(prepared).map_or(Commit::Stale, Commit::Applied)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::doc::ids::hash_str;

    /// A ready read of `source`, as [`read`] would give it on the worker thread.
    fn read_of(doc: &Document, source: &str) -> Read {
        let snap = doc.snapshot();
        let hash = hash_str(source);
        Read {
            hash,
            suspect: suspect(snap.source_len(), source),
            written: None,
            prepared: (hash != snap.file_hash).then(|| snap.prepare(source)),
        }
    }

    /// Like the app: drops suspicious reads, then [`commit`].
    fn apply(doc: &mut Document, source: &str, own: &OwnWrites) -> Option<Change> {
        let read = read_of(doc, source);
        if read.suspect {
            return None;
        }
        apply_settled(doc, source, own)
    }

    /// The second identical read of a suspicious content: it is accepted even if short.
    fn apply_settled(doc: &mut Document, source: &str, own: &OwnWrites) -> Option<Change> {
        match commit(doc, read_of(doc, source), own) {
            Commit::Applied(change) => Some(change),
            Commit::Skipped | Commit::Stale => None,
        }
    }

    fn ten_kb() -> String {
        (0..100).map(|n| format!("Paragraph {n}: {}", "words ".repeat(16))).collect::<Vec<_>>().join("\n\n")
    }

    #[test]
    fn own_write_is_ignored() {
        let p = Path::new("/tmp/runemd-own.md");
        let mut doc = Document::from_source(p, "# a\n");
        let mut own = OwnWrites::default();
        let mine = "# a\n\nwritten by the app\n";
        own.remember(p, hash_str(mine));
        assert_eq!(apply(&mut doc, mine, &own), None);
        assert_eq!(doc.revision, 0, "the document is not touched");
    }

    #[test]
    fn empty_read_during_write_is_skipped() {
        let src = ten_kb();
        assert!(src.len() >= 10_000);
        let mut doc = Document::from_source(Path::new("a.md"), &src);
        assert_eq!(apply(&mut doc, "", &OwnWrites::default()), None);
        assert_eq!(doc.blocks.len(), 100);
    }

    #[test]
    fn much_shorter_read_is_skipped() {
        let src = ten_kb();
        let mut doc = Document::from_source(Path::new("a.md"), &src);
        let partial = &src[..src.len() / 20];
        assert_eq!(apply(&mut doc, partial, &OwnWrites::default()), None);
        assert_eq!(doc.revision, 0);
    }

    #[test]
    fn an_agent_write_updates_the_document() {
        let mut doc = Document::from_source(Path::new("a.md"), "# a\n\nb\n");
        let first = doc.blocks[0].id;
        let change = apply(&mut doc, "# a\n\nb\n\nc new\n", &OwnWrites::default()).expect("change");
        assert_eq!(change.added.len(), 1);
        assert_eq!(doc.blocks[0].id, first);
        assert_eq!(doc.revision, 1);
    }

    /// The app writes X, an agent Y, then X again: the second X is the agent's.
    #[test]
    fn an_own_write_is_ignored_only_once() {
        let p = Path::new("/tmp/runemd-own-once.md");
        let (x, y) = ("# a\n\nX by the app\n", "# a\n\nY by the agent\n");
        let mut doc = Document::from_source(p, "# a\n");
        let mut own = OwnWrites::default();
        own.remember(p, hash_str(x));
        assert_eq!(apply(&mut doc, x, &own), None, "the reread of the app's write");
        assert!(!own.is_own(p, hash_str(x)), "entry consumed");
        assert!(apply(&mut doc, y, &own).is_some());
        assert!(apply(&mut doc, x, &own).is_some(), "X rewritten by the agent");
    }

    /// Even when the document already shows the content the app wrote, the reread consumes
    /// the entry.
    #[test]
    fn an_own_write_already_shown_is_consumed() {
        let p = Path::new("/tmp/runemd-own-shown.md");
        let x = "# a\n\nX\n";
        let mut doc = Document::from_source(p, x);
        let mut own = OwnWrites::default();
        own.remember(p, hash_str(x));
        assert_eq!(apply(&mut doc, x, &own), None);
        assert!(!own.is_own(p, hash_str(x)));
    }

    #[test]
    fn same_content_is_no_change() {
        let mut doc = Document::from_source(Path::new("a.md"), "# a\n");
        assert_eq!(apply(&mut doc, "# a\n", &OwnWrites::default()), None);
        assert_eq!(doc.revision, 0);
    }

    #[test]
    fn a_short_read_that_holds_still_is_applied() {
        let mut doc = Document::from_source(Path::new("a.md"), &ten_kb());
        assert!(suspect(doc.snapshot().source_len(), "# short\n"));
        assert!(apply_settled(&mut doc, "# short\n", &OwnWrites::default()).is_some());
        assert_eq!(doc.blocks.len(), 1);
    }
}

#[cfg(test)]
mod cost {
    //! Cost of a reload of `fixtures/big.md`. `all`: read, UTF-8, hash and
    //! `Document::update` (split, block hashes, Myers diff) as if they were on the UI
    //! thread. `ui`: the part that stays there, `snapshot` + `commit`; `worker`: `read`.
    //! `cargo test --profile perf reload_cost -- --ignored --nocapture`

    use std::path::Path;
    use std::time::Instant;

    use super::*;

    fn stats(ms: &mut [f64]) -> String {
        ms.sort_by(f64::total_cmp);
        let pct = |p: f64| ms[((ms.len() - 1) as f64 * p).round() as usize];
        format!("n={} p50={:.2} ms p95={:.2} ms max={:.2} ms", ms.len(), pct(0.5), pct(0.95), pct(1.0))
    }

    fn measure(case: &str, before: &str, after: &str) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.md");
        std::fs::write(&path, after).unwrap();
        let (mut all, mut ui, mut worker) = (Vec::new(), Vec::new(), Vec::new());
        let ms = |t: Instant| t.elapsed().as_secs_f64() * 1000.0;
        for _ in 0..40 {
            let mut doc = Document::from_source(&path, before);
            let t = Instant::now();
            let r = read(&path, &doc.snapshot()).unwrap();
            assert!(matches!(
                commit(&mut doc, r, &OwnWrites::default()),
                Commit::Applied(_)
            ));
            all.push(ms(t));

            let mut doc = Document::from_source(&path, before);
            let t = Instant::now();
            let snap = doc.snapshot();
            let ui_snap = ms(t);
            let t = Instant::now();
            let r = read(Path::new(&path), &snap).unwrap();
            worker.push(ms(t));
            let t = Instant::now();
            assert!(matches!(commit(&mut doc, r, &OwnWrites::default()), Commit::Applied(_)));
            ui.push(ui_snap + ms(t));
        }
        eprintln!("reload_cost {case} ({} bytes): all {}", after.len(), stats(&mut all));
        eprintln!("reload_cost {case}: ui (snapshot+commit) {}", stats(&mut ui));
        eprintln!("reload_cost {case}: worker (read) {}", stats(&mut worker));
    }

    #[test]
    #[ignore]
    fn reload_cost() {
        let big = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/big.md")).unwrap();
        measure("append", &big, &format!("{big}\n\nA new paragraph at the end.\n"));
        // Every block changes: no hash in common, the worst case for Myers.
        let all: String = crate::doc::blocks::split(&big).iter().map(|b| format!("{b} x")).collect::<Vec<_>>().join("\n\n");
        measure("full rewrite", &big, &all);
    }
}
