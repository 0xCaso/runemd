//! Lettura live: il file aperto si ricarica quando un agente lo scrive.
//!
//! `Live` tiene il watcher (task 9) e decide quando rileggere il file aperto: a ogni `Changed`
//! del suo percorso e, dopo una lettura sospetta (file a metà scrittura), ancora una volta dopo
//! `SETTLE`. `reload::apply` aggiorna il `Document` con id stabili.

pub mod reload;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use rune::Waker;

use crate::doc::document::{Change, Document};
use crate::doc::ids;
use crate::watch::{FsEvent, Watcher};

use reload::OwnWrites;

/// Dopo una lettura sospetta si rilegge dopo questo tempo; se il contenuto è lo stesso, il file
/// non sta più cambiando e vale. Una riscrittura sul posto di 22 KB dura qualche ms
/// (`docs/agent-writes.md`): 200 ms la coprono con margine (`ipotesi`).
const SETTLE: Duration = Duration::from_millis(200);

/// Un ricaricamento applicato. Il task 11 ("sta scrivendo") usa `at`, il 13 (segno dei blocchi
/// cambiati) gli id in `change`.
#[derive(Debug)]
pub struct Reloaded {
    pub at: Instant,
    /// L'mtime del file letto: quando l'agente l'ha scritto (sonda di latenza).
    pub written: Option<SystemTime>,
    pub change: Change,
}

#[derive(Default)]
pub struct Live {
    watcher: Option<Watcher>,
    pub own: OwnWrites,
    /// Il file aperto come lo riporta FSEvents (canonico).
    watched: Option<PathBuf>,
    /// L'hash di una lettura sospetta saltata: se la rilettura dà lo stesso, si applica.
    held: Option<u64>,
    recheck_at: Option<Instant>,
}

/// La radice da guardare per `path`: la sua cartella.
fn root_of(path: &Path) -> Option<PathBuf> {
    path.parent().map(|p| if p.as_os_str().is_empty() { PathBuf::from(".") } else { p.to_owned() })
}

impl Live {
    /// Avvia il watcher (da `AppSpec::setup`, che ha il waker) e guarda il file già aperto.
    pub fn start(&mut self, waker: Waker) {
        let roots = self.watched.as_deref().and_then(root_of).into_iter().collect();
        self.watcher = Some(Watcher::start(roots, waker));
    }

    /// Il file aperto ora è `path`: si guarda la sua cartella e si dimentica lo stato del
    /// file di prima.
    pub fn watch(&mut self, path: &Path) {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_owned());
        if let (Some(w), Some(root)) = (self.watcher.as_mut(), root_of(&canonical)) {
            w.add_root(root);
        }
        self.watched = Some(canonical);
        self.held = None;
        self.recheck_at = None;
    }

    /// Da chiamare a ogni `poll`: svuota il watcher (va fatto a ogni risveglio) e, se il file
    /// aperto è cambiato, lo rilegge e aggiorna `doc`. Ritorna il cambio applicato e, se serve,
    /// fra quanto rileggere.
    pub fn poll(&mut self, doc: Option<&mut Document>, now: Instant) -> (Option<Reloaded>, Option<Duration>) {
        let events = self.watcher.as_mut().map(|w| w.drain(now)).unwrap_or_default();
        let mut reread = self.recheck_at.is_some_and(|t| now >= t);
        for event in events {
            if let FsEvent::Changed(path) = event
                && self.watched.as_ref() == Some(&path)
            {
                reread = true;
            }
        }
        let Some(doc) = doc.filter(|_| reread) else {
            return (None, self.recheck_at.map(|t| t.saturating_duration_since(now)));
        };
        self.recheck_at = None;
        self.reread(doc, now)
    }

    fn reread(&mut self, doc: &mut Document, now: Instant) -> (Option<Reloaded>, Option<Duration>) {
        // File assente o illeggibile durante un rename: il watcher lo riporta quando torna.
        let Ok(bytes) = std::fs::read(&doc.path) else { return (None, None) };
        let written = std::fs::metadata(&doc.path).and_then(|m| m.modified()).ok();
        let source = String::from_utf8_lossy(&bytes);
        let hash = ids::hash_str(&source);
        let change = if self.held == Some(hash) {
            reload::apply_settled(doc, &source, &self.own)
        } else if reload::suspect(doc, &source) {
            self.held = Some(hash);
            self.recheck_at = Some(now + SETTLE);
            return (None, Some(SETTLE));
        } else {
            reload::apply(doc, &source, &self.own)
        };
        self.held = None;
        (change.map(|change| Reloaded { at: now, written, change }), None)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;
    use std::thread;

    use super::*;

    /// Fa girare `poll` come l'app (ogni ~16 ms) finché arriva un ricaricamento o scade `window`.
    fn poll_until(live: &mut Live, doc: &mut Document, window: Duration) -> Option<Reloaded> {
        let deadline = Instant::now() + window;
        while Instant::now() < deadline {
            if let (Some(r), _) = live.poll(Some(doc), Instant::now()) {
                return Some(r);
            }
            thread::sleep(Duration::from_millis(16));
        }
        None
    }

    fn started(path: &Path) -> (Live, Document) {
        let mut live = Live::default();
        live.watch(path);
        live.start(Arc::new(|| {}));
        let doc = Document::load(path).unwrap();
        // FSEvents parte in modo asincrono: una prima scrittura uguale fa da sentinella.
        thread::sleep(Duration::from_millis(300));
        (live, doc)
    }

    #[test]
    fn an_external_write_reloads_the_open_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n\nuno\n").unwrap();
        let (mut live, mut doc) = started(&path);
        let first = doc.blocks[0].id;
        fs::write(&path, "# a\n\nuno\n\ndue\n").unwrap();
        let r = poll_until(&mut live, &mut doc, Duration::from_secs(5)).expect("ricarica");
        assert_eq!(r.change.added.len(), 1);
        assert_eq!(doc.blocks.len(), 3);
        assert_eq!(doc.blocks[0].id, first, "id stabili");
        assert!(r.written.is_some());
    }

    #[test]
    fn a_truncation_is_applied_after_settle() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let long: String = (0..200).map(|n| format!("Paragrafo {n} di prova.\n\n")).collect();
        fs::write(&path, &long).unwrap();
        let (mut live, mut doc) = started(&path);
        fs::write(&path, "# corto\n").unwrap();
        let t0 = Instant::now();
        let r = poll_until(&mut live, &mut doc, Duration::from_secs(5)).expect("ricarica dopo SETTLE");
        assert!(t0.elapsed() >= SETTLE, "la lettura corta aspetta la seconda");
        assert_eq!(doc.blocks.len(), 1);
        assert!(!r.change.removed.is_empty());
    }

    #[test]
    fn a_half_written_read_is_replaced_by_the_full_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let long: String = (0..200).map(|n| format!("Paragrafo {n} di prova.\n\n")).collect();
        fs::write(&path, &long).unwrap();
        let (mut live, mut doc) = started(&path);
        // Codex: tronca, poi scrive tutto il file (più lungo) sullo stesso inode.
        fs::write(&path, "").unwrap();
        let mut seen_empty = false;
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline && !seen_empty {
            let (r, wait) = live.poll(Some(&mut doc), Instant::now());
            assert!(r.is_none(), "una lettura vuota non si applica");
            seen_empty = wait.is_some();
            thread::sleep(Duration::from_millis(10));
        }
        assert!(seen_empty, "la lettura vuota è sospetta");
        let full = format!("{long}Nuovo in fondo.\n");
        fs::write(&path, &full).unwrap();
        let r = poll_until(&mut live, &mut doc, Duration::from_secs(5)).expect("ricarica");
        assert_eq!(r.change.added.len(), 1, "solo il blocco nuovo: {:?}", r.change);
        assert_eq!(doc.blocks.len(), 201);
    }

    #[test]
    fn other_files_do_not_reload() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n").unwrap();
        let (mut live, mut doc) = started(&path);
        fs::write(dir.path().join("b.md"), "# b\n").unwrap();
        assert!(poll_until(&mut live, &mut doc, Duration::from_millis(500)).is_none());
        assert_eq!(doc.revision, 0);
    }
}
