//! The index of the Markdown files in the watched folders: Recent and Cmd+P read from here.
//!
//! At startup the index loads from the cache (`index.json` in Application Support), then a
//! scan on its own thread replaces it with what is on disk; from then on the
//! watcher's events update it. Events that arrive while the scan runs are applied again
//! on top of its result, so a write that happened after the scan read the file is not
//! lost.
//!
//! The rules on what is Markdown and which folders to skip are the watcher's
//! (`watch::filter`): `.git`, `node_modules`, `target` skipped; the other hidden folders,
//! `.claude/` included, are in (spec, "Watching files": no `.gitignore` filter, the agents'
//! files are exactly the ones to see).

pub mod cache;
pub mod scan;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use rune::Waker;

use crate::watch::FsEvent;

pub use scan::{Access, scan_in_background};

/// How often, at most, the cache is rewritten after an event. The startup scan
/// rebuilds everything anyway: losing the last seconds at close costs nothing.
const SAVE_EVERY: Duration = Duration::from_secs(30);

/// A Markdown file in the index and when it last changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub mtime: SystemTime,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Index {
    /// Sorted by path, without duplicates.
    entries: Vec<Entry>,
    /// Only the paths below these folders get in; `None` = no limit.
    roots: Option<Vec<PathBuf>>,
    /// The changes applied since the scan started (`None` if none is running).
    since_scan: Option<Vec<Logged>>,
}

/// A change applied during a scan, to apply again on top of its result.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Logged {
    /// Path and mtime read (`None` = removed).
    Set(PathBuf, Option<SystemTime>),
    /// Everything that was below this folder is gone.
    Under(PathBuf),
}

/// The mtime of `path` following links; `None` if it is not (or no longer) a file.
fn mtime_of(path: &Path) -> Option<SystemTime> {
    let meta = std::fs::metadata(path).ok()?;
    meta.is_file().then(|| meta.modified().ok()).flatten()
}

impl Index {
    /// An index with these entries (in any order; for a duplicate path the last one wins).
    pub fn from_entries(entries: Vec<Entry>) -> Index {
        let mut index = Index::default();
        index.replace(entries);
        index
    }

    /// The index saved in `path`; empty if missing or unreadable (the scan fills it).
    pub fn load_cache(path: &Path) -> Index {
        Index::from_entries(cache::load(path).unwrap_or_default())
    }

    /// Writes the index to `path` atomically (temporary file and rename). The app saves from
    /// `Library::save`, with a copy of the entries on another thread.
    #[cfg(test)]
    pub fn save_cache(&self, path: &Path) {
        if let Err(e) = cache::save(path, &self.entries) {
            eprintln!("runemd: {}: index not saved: {e}", path.display());
        }
    }

    /// Limits the index to the paths below `roots` and removes the entries left outside.
    pub fn set_roots(&mut self, roots: Vec<PathBuf>) {
        self.entries.retain(|e| roots.iter().any(|r| e.path.starts_with(r)));
        self.roots = Some(roots);
    }

    fn covers(&self, path: &Path) -> bool {
        self.roots.as_ref().is_none_or(|roots| roots.iter().any(|r| path.starts_with(r)))
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// A watcher event. True if the index changed.
    pub fn apply(&mut self, ev: &FsEvent) -> bool {
        let (path, mtime) = match ev {
            FsEvent::Changed(p) => (p, mtime_of(p)),
            FsEvent::Removed(p) => (p, None),
            FsEvent::GitHead(_) => return false,
            // A scan of the folder is needed on another thread: `Library` does it.
            FsEvent::Rescan(_) => return false,
        };
        if !self.covers(path) {
            return false;
        }
        if let Some(log) = &mut self.since_scan {
            log.push(Logged::Set(path.clone(), mtime));
        }
        self.set(path, mtime)
    }

    /// Removes the entries below `dir` (a folder gone or moved elsewhere). True if it removed any.
    pub fn remove_under(&mut self, dir: &Path) -> bool {
        if let Some(log) = &mut self.since_scan {
            log.push(Logged::Under(dir.to_owned()));
        }
        self.drop_under(dir)
    }

    /// The entries below `dir` are contiguous in component order: a range, without
    /// walking the whole index.
    fn drop_under(&mut self, dir: &Path) -> bool {
        let range = self.range_under(dir);
        let any = !range.is_empty();
        self.entries.drain(range);
        any
    }

    fn range_under(&self, dir: &Path) -> std::ops::Range<usize> {
        let start = self.entries.partition_point(|e| e.path.as_path() < dir);
        let len = self.entries[start..].iter().take_while(|e| e.path.starts_with(dir)).count();
        start..start + len
    }

    /// Puts (`Some`) or removes (`None`) the entry of `path`. True if something changed.
    fn set(&mut self, path: &Path, mtime: Option<SystemTime>) -> bool {
        let at = self.entries.binary_search_by(|e| e.path.as_path().cmp(path));
        match (at, mtime) {
            (Ok(i), Some(m)) => std::mem::replace(&mut self.entries[i].mtime, m) != m,
            (Ok(i), None) => {
                self.entries.remove(i);
                true
            }
            (Err(i), Some(m)) => {
                self.entries.insert(i, Entry { path: path.to_owned(), mtime: m });
                true
            }
            (Err(_), None) => false,
        }
    }

    /// A scan starts: from here on, applied events are remembered to apply them again on top of
    /// its result.
    pub fn begin_scan(&mut self) {
        self.since_scan = Some(Vec::new());
    }

    /// The result of a scan of the folders `under` (sorted by path and without
    /// duplicates, as `scan::scan` gives it) replaces the entries below them; the events
    /// that arrived in the meantime win on top. On the UI thread: only linear passes, no
    /// sorting (cost in `docs/perf.md`).
    pub fn finish_scan(&mut self, under: &[PathBuf], mut scanned: Vec<Entry>) {
        debug_assert!(scanned.windows(2).all(|w| w[0].path < w[1].path), "scan not sorted");
        let log = self.since_scan.take().unwrap_or_default();
        scanned.retain(|e| self.covers(&e.path));
        let mut kept = std::mem::take(&mut self.entries);
        kept.retain(|e| !under.iter().any(|u| e.path.starts_with(u)));
        self.entries = merge(kept, scanned);
        for change in log {
            match change {
                Logged::Set(path, mtime) => {
                    self.set(&path, mtime);
                }
                Logged::Under(dir) => {
                    self.drop_under(&dir);
                }
            }
        }
    }

    /// A scan that went wrong: nothing to apply again.
    fn abort_scan(&mut self) {
        self.since_scan = None;
    }

    fn replace(&mut self, mut entries: Vec<Entry>) {
        entries.retain(|e| self.covers(&e.path));
        // Stable: between two entries of the same path the last one stays.
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        entries.reverse();
        entries.dedup_by(|a, b| a.path == b.path);
        entries.reverse();
        self.entries = entries;
    }

    /// The `n` most recently changed entries, newest first (equal mtimes by path).
    pub fn recent(&self, n: usize) -> Vec<&Entry> {
        let newest_first = |a: &&Entry, b: &&Entry| b.mtime.cmp(&a.mtime).then_with(|| a.path.cmp(&b.path));
        let mut out: Vec<&Entry> = self.entries.iter().collect();
        if n < out.len() {
            out.select_nth_unstable_by(n, newest_first);
            out.truncate(n);
        }
        out.sort_by(newest_first);
        out
    }

    /// The entries below `dir`, in path order: a range of the index, without walking
    /// all of it (the tree of the file's repo).
    pub fn under(&self, dir: &Path) -> &[Entry] {
        &self.entries[self.range_under(dir)]
    }

    /// The mtime of `path` if it is in the index (binary search: entries are sorted by path).
    pub fn mtime(&self, path: &Path) -> Option<SystemTime> {
        self.entries.binary_search_by(|e| e.path.as_path().cmp(path)).ok().map(|i| self.entries[i].mtime)
    }

    /// All the paths, in alphabetical order (Cmd+P, repo tree).
    pub fn names(&self) -> impl Iterator<Item = &Path> {
        self.entries.iter().map(|e| e.path.as_path())
    }
}

/// Merges two lists sorted by path; for the same path `b` wins.
fn merge(a: Vec<Entry>, b: Vec<Entry>) -> Vec<Entry> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    let mut a = a.into_iter().peekable();
    let mut b = b.into_iter().peekable();
    loop {
        let take_a = match (a.peek(), b.peek()) {
            (Some(x), Some(y)) => match x.path.cmp(&y.path) {
                std::cmp::Ordering::Less => true,
                std::cmp::Ordering::Equal => {
                    a.next();
                    false
                }
                std::cmp::Ordering::Greater => false,
            },
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => return out,
        };
        out.extend(if take_a { a.next() } else { b.next() });
    }
}

/// The default watched folders (`settings::default_roots`), or those of `RUNEMD_ROOTS`.
/// The watched folders: `RUNEMD_ROOTS` (separated by `:`) if set, otherwise `chosen` (from the
/// settings). Canonical, like the paths FSEvents reports.
pub fn roots_from(chosen: &[PathBuf]) -> Vec<PathBuf> {
    let raw: Vec<PathBuf> = env_roots().unwrap_or_else(|| chosen.to_vec());
    raw.into_iter().map(|p| canonical(&p)).collect()
}

/// The folders of `RUNEMD_ROOTS`, if set: tests and measurements do not touch the settings.
pub fn env_roots() -> Option<Vec<PathBuf>> {
    let v = std::env::var_os("RUNEMD_ROOTS")?;
    Some(std::env::split_paths(&v).filter(|p| !p.as_os_str().is_empty()).collect())
}

/// `p` canonical if it exists, otherwise as it is.
pub fn canonical(p: &Path) -> PathBuf {
    p.canonicalize().unwrap_or_else(|_| p.to_owned())
}

/// Where the index is saved: `~/Library/Application Support/RuneMD/index.json` (below
/// `RUNE_DIRS_ROOT` if set, for tests).
pub fn cache_path() -> Option<PathBuf> {
    rune::dirs::AppDirs::new("RuneMD").map(|d| d.data().join("index.json"))
}

/// The app's index: cache, background scan, watcher events, saves.
pub struct Library {
    pub index: Index,
    roots: Vec<PathBuf>,
    cache: Option<PathBuf>,
    /// The running scan and the folders it covers.
    scan: Option<(Vec<PathBuf>, Receiver<scan::Scanned>)>,
    /// Folders to scan as soon as the running scan ends (moved folders, lost events).
    queued: Vec<PathBuf>,
    waker: Option<Waker>,
    /// The first scan is done: the index is the disk's (not only the cache's).
    pub scanned: bool,
    /// The permissions outcome of the last finished scan.
    pub access: Access,
    /// The folders of the scan collected in the last `poll` (empty if none): changes
    /// to the index without an event per file (the repo tree watches them).
    pub landed: Vec<PathBuf>,
    /// Changed since the last save.
    dirty: bool,
    saved_at: Option<Instant>,
    saving: Option<JoinHandle<()>>,
}

impl Library {
    /// Loads the cache (if any) limited to `roots`. It does not scan: `start` does that.
    pub fn load(roots: Vec<PathBuf>, cache: Option<PathBuf>) -> Library {
        let mut index = cache.as_deref().map(Index::load_cache).unwrap_or_default();
        index.set_roots(roots.clone());
        Library { index, roots, cache, scan: None, queued: Vec::new(), waker: None, scanned: false, access: Access::Ok, landed: Vec::new(), dirty: false, saved_at: None, saving: None }
    }

    /// The watched folders.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// One more watched folder (from the settings): it enters the index and is scanned on
    /// another thread, if the app has started. True if it is new.
    pub fn add_root(&mut self, root: PathBuf) -> bool {
        if self.roots.iter().any(|r| r == &root) {
            return false;
        }
        self.roots.push(root.clone());
        self.index.set_roots(self.roots.clone());
        if self.waker.is_some() {
            self.queued.push(root);
            self.launch();
        }
        true
    }

    /// One fewer watched folder: its entries leave the index (and Recent) at once. A
    /// running scan that covers it does not put them back: `finish_scan` keeps only what the
    /// folders cover. True if it was there.
    pub fn remove_root(&mut self, root: &Path) -> bool {
        let before = self.roots.len();
        self.roots.retain(|r| r != root);
        if self.roots.len() == before {
            return false;
        }
        self.queued.retain(|q| !q.starts_with(root));
        self.index.set_roots(self.roots.clone());
        self.dirty = true;
        true
    }

    /// Starts the scan on its own thread; the thread wakes the app when it is done.
    pub fn start(&mut self, waker: Waker) {
        self.waker = Some(waker);
        self.queued = self.roots.clone();
        self.launch();
    }

    /// Starts the scan of the queued folders, if none is running.
    fn launch(&mut self) {
        if self.scan.is_some() || self.queued.is_empty() {
            return;
        }
        let Some(waker) = self.waker.clone() else { return };
        let under = std::mem::take(&mut self.queued);
        self.index.begin_scan();
        let rx = scan_in_background(under.clone(), waker);
        self.scan = Some((under, rx));
    }

    /// A watcher `Rescan`: a folder that exists is rescanned on another thread; one that
    /// is gone is removed at once (its entries are a range of the index).
    fn rescan(&mut self, dir: &Path) -> bool {
        if !self.index.covers(dir) {
            return false;
        }
        if !dir.is_dir() {
            return self.index.remove_under(dir);
        }
        if self.queued.iter().any(|q| dir.starts_with(q)) {
            return false;
        }
        self.queued.retain(|q| !q.starts_with(dir));
        self.queued.push(dir.to_owned());
        false
    }

    /// Call on every `poll` with the watcher events of this wake-up. It collects the
    /// finished scan, applies the events and saves the cache when needed. True if the index
    /// changed.
    pub fn poll(&mut self, events: &[FsEvent], now: Instant) -> bool {
        let mut changed = false;
        self.landed.clear();
        for ev in events {
            changed |= match ev {
                FsEvent::Rescan(dir) => self.rescan(dir),
                ev => self.index.apply(ev),
            };
        }
        match self.scan.as_ref().map(|(_, rx)| rx.try_recv()) {
            Some(Ok((entries, access))) => {
                self.access = access;
                let (under, _) = self.scan.take().expect("scan running");
                self.index.finish_scan(&under, entries);
                self.landed = under;
                changed = true;
                if !self.scanned {
                    self.scanned = true;
                    // Right after the first scan, so the next launch starts from here.
                    self.saved_at = None;
                }
            }
            // The thread died without a result: the index stays as it is, updated by the events.
            Some(Err(TryRecvError::Disconnected)) => {
                self.scan = None;
                self.index.abort_scan();
            }
            Some(Err(TryRecvError::Empty)) | None => {}
        }
        self.launch();
        self.dirty |= changed;
        if self.dirty && self.saved_at.is_none_or(|t| now.saturating_duration_since(t) >= SAVE_EVERY) {
            self.save(now);
        }
        changed
    }

    /// Saves a copy of the index on its own thread; one at a time.
    fn save(&mut self, now: Instant) {
        let Some(path) = self.cache.clone() else { return };
        if self.saving.as_ref().is_some_and(|h| !h.is_finished()) {
            return;
        }
        // Already sorted and without duplicates: a copy, no sorting on the UI thread.
        let entries = self.index.entries.clone();
        self.saving = Some(std::thread::spawn(move || {
            if let Err(e) = cache::save(&path, &entries) {
                eprintln!("runemd: {}: index not saved: {e}", path.display());
            }
        }));
        self.dirty = false;
        self.saved_at = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;

    use super::*;

    fn at(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
    }

    fn entry(path: &str, secs: u64) -> Entry {
        Entry { path: PathBuf::from(path), mtime: at(secs) }
    }

    fn paths(entries: &[&Entry]) -> Vec<String> {
        entries.iter().map(|e| e.path.display().to_string()).collect()
    }

    /// A folder added while the first scan runs is scanned as soon as that one ends
    /// (`poll` starts the queue again).
    #[test]
    fn a_root_added_during_a_scan_is_scanned_after_it() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        let (a, b) = (base.join("a"), base.join("b"));
        for d in [&a, &b] {
            fs::create_dir_all(d).unwrap();
        }
        for i in 0..200 {
            fs::write(a.join(format!("{i}.md")), "# a\n").unwrap();
        }
        fs::write(b.join("b.md"), "# b\n").unwrap();
        let mut lib = Library::load(vec![a.clone()], None);
        lib.start(Arc::new(|| {}));
        assert!(lib.scan.is_some(), "the scan of a runs");
        assert!(lib.add_root(b.clone()));
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline && !lib.index.names().any(|p| p == b.join("b.md")) {
            lib.poll(&[], Instant::now());
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(lib.index.names().any(|p| p == b.join("b.md")), "b scanned after a");
        assert_eq!(lib.index.len(), 201);
    }

    #[test]
    fn recent_is_newest_first_and_truncated() {
        let index = Index::from_entries(vec![entry("/r/a.md", 10), entry("/r/b.md", 30), entry("/r/c.md", 20), entry("/r/d.md", 5)]);
        assert_eq!(paths(&index.recent(2)), ["/r/b.md", "/r/c.md"]);
        assert_eq!(paths(&index.recent(10)), ["/r/b.md", "/r/c.md", "/r/a.md", "/r/d.md"]);
        assert!(index.recent(0).is_empty());
    }

    #[test]
    fn recent_breaks_ties_by_path() {
        let index = Index::from_entries(vec![entry("/r/b.md", 10), entry("/r/a.md", 10), entry("/r/c.md", 10)]);
        assert_eq!(paths(&index.recent(2)), ["/r/a.md", "/r/b.md"]);
    }

    #[test]
    fn names_are_sorted_and_unique() {
        let index = Index::from_entries(vec![entry("/r/b.md", 1), entry("/r/a.md", 1), entry("/r/b.md", 2)]);
        assert_eq!(index.names().collect::<Vec<_>>(), [Path::new("/r/a.md"), Path::new("/r/b.md")]);
        assert_eq!(index.recent(1)[0].mtime, at(2), "for a duplicate path the last entry wins");
    }

    #[test]
    fn apply_removed_drops_the_entry() {
        let mut index = Index::from_entries(vec![entry("/r/a.md", 1), entry("/r/b.md", 2)]);
        assert!(index.apply(&FsEvent::Removed(PathBuf::from("/r/a.md"))));
        assert_eq!(index.names().collect::<Vec<_>>(), [Path::new("/r/b.md")]);
        assert!(!index.apply(&FsEvent::Removed(PathBuf::from("/r/a.md"))), "already removed");
    }

    #[test]
    fn apply_changed_adds_or_updates_with_the_mtime_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.md");
        fs::write(&file, "# a").unwrap();
        let mtime = fs::metadata(&file).unwrap().modified().unwrap();
        let mut index = Index::from_entries(vec![Entry { path: file.clone(), mtime: at(1) }]);
        assert!(index.apply(&FsEvent::Changed(file.clone())));
        assert_eq!(index.recent(1)[0].mtime, mtime);
        assert!(!index.apply(&FsEvent::Changed(file.clone())), "same mtime: nothing to change");
        let new = dir.path().join("b.md");
        fs::write(&new, "# b").unwrap();
        assert!(index.apply(&FsEvent::Changed(new.clone())));
        assert_eq!(index.len(), 2);
        // A Changed for a file already gone removes it.
        fs::remove_file(&new).unwrap();
        assert!(index.apply(&FsEvent::Changed(new)));
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn apply_ignores_paths_outside_the_roots_and_git_head() {
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().join("outside.md");
        fs::write(&outside, "x").unwrap();
        let mut index = Index::default();
        index.set_roots(vec![PathBuf::from("/r")]);
        assert!(!index.apply(&FsEvent::Changed(outside)));
        assert!(!index.apply(&FsEvent::GitHead(PathBuf::from("/r"))));
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn set_roots_drops_entries_outside() {
        let mut index = Index::from_entries(vec![entry("/r/a.md", 1), entry("/rr/b.md", 1), entry("/s/c.md", 1)]);
        index.set_roots(vec![PathBuf::from("/r")]);
        assert_eq!(index.names().collect::<Vec<_>>(), [Path::new("/r/a.md")], "`/rr` is not below `/r`");
    }

    /// An event that arrives while the scan runs wins on top of its result: the scan
    /// may have read the file before the write, or before it disappeared.
    #[test]
    fn events_during_a_scan_win_over_its_result() {
        let dir = tempfile::tempdir().unwrap();
        let fresh = dir.path().join("fresh.md");
        fs::write(&fresh, "x").unwrap();
        let mtime = fs::metadata(&fresh).unwrap().modified().unwrap();
        let gone = dir.path().join("gone.md");
        let mut index = Index::default();
        index.begin_scan();
        index.apply(&FsEvent::Changed(fresh.clone()));
        index.apply(&FsEvent::Removed(gone.clone()));
        let mut scanned = vec![Entry { path: fresh.clone(), mtime: at(1) }, Entry { path: gone, mtime: at(1) }];
        scanned.sort_by(|a, b| a.path.cmp(&b.path));
        index.finish_scan(&[dir.path().to_path_buf()], scanned);
        assert_eq!(index.recent(10), [&Entry { path: fresh.clone(), mtime }]);
        // Once the scan is done, events are no longer remembered.
        index.apply(&FsEvent::Removed(fresh));
        index.finish_scan(&[PathBuf::from("/r")], vec![entry("/r/x.md", 1)]);
        assert_eq!(index.names().collect::<Vec<_>>(), [Path::new("/r/x.md")]);
    }

    #[test]
    fn scan_skips_build_dirs_and_includes_claude() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        for d in [".git", "node_modules/p", "target/doc", ".claude/plans", "docs/deep", ".github"] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        for f in [".git/x.md", "node_modules/p/README.md", "target/doc/x.md", ".claude/plans/p.md", "docs/deep/a.MD", ".github/b.markdown", "c.md", "c.txt"] {
            fs::write(root.join(f), "x").unwrap();
        }
        std::os::unix::fs::symlink("..", root.join("docs/loop")).unwrap();
        let mut found: Vec<PathBuf> = scan::scan(std::slice::from_ref(&root)).into_iter().map(|e| e.path).collect();
        found.sort();
        let want: Vec<PathBuf> = [".claude/plans/p.md", ".github/b.markdown", "c.md", "docs/deep/a.MD"].iter().map(|f| root.join(f)).collect();
        assert_eq!(found, want);
        let mtime = fs::metadata(root.join("c.md")).unwrap().modified().unwrap();
        assert!(scan::scan(std::slice::from_ref(&root)).iter().any(|e| e.path == root.join("c.md") && e.mtime == mtime));
    }

    #[test]
    fn scan_in_background_sends_the_result_and_wakes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::write(root.join("a.md"), "x").unwrap();
        let (tx, woke) = std::sync::mpsc::channel();
        let rx = scan_in_background(vec![root.clone()], Arc::new(move || { let _ = tx.send(()); }));
        let (entries, access) = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(access, Access::Ok);
        assert_eq!(entries.iter().map(|e| &e.path).collect::<Vec<_>>(), [&root.join("a.md")]);
        woke.recv_timeout(Duration::from_secs(5)).expect("wake-up");
    }

    #[test]
    fn cache_saved_and_reloaded_is_equal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("RuneMD/index.json");
        // Nanoseconds included: the mtime must come back identical.
        let index = Index::from_entries(vec![
            Entry { path: PathBuf::from("/r/a.md"), mtime: SystemTime::UNIX_EPOCH + Duration::new(1_763_000_000, 123_456_789) },
            entry("/r/àccent with spaces.md", 2),
        ]);
        index.save_cache(&path);
        assert_eq!(Index::load_cache(&path), index);
        assert!(fs::read_dir(path.parent().unwrap()).unwrap().count() == 1, "no temporary file left");
    }

    #[test]
    fn a_missing_or_broken_cache_loads_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.json");
        assert_eq!(Index::load_cache(&path).len(), 0);
        fs::write(&path, "{ broken").unwrap();
        assert_eq!(Index::load_cache(&path).len(), 0);
    }

    #[test]
    fn remove_under_drops_only_that_folder() {
        let mut index = Index::from_entries(vec![entry("/r/docs/a.md", 1), entry("/r/docs/sub/b.md", 1), entry("/r/docs-old/c.md", 1), entry("/r/docs.md", 1), entry("/r/z.md", 1)]);
        assert!(index.remove_under(Path::new("/r/docs")));
        assert_eq!(index.names().collect::<Vec<_>>(), [Path::new("/r/docs-old/c.md"), Path::new("/r/docs.md"), Path::new("/r/z.md")]);
        assert!(!index.remove_under(Path::new("/r/nothing")));
    }

    /// A scan of a subfolder replaces only the entries below it.
    #[test]
    fn finish_scan_replaces_only_under_the_scanned_folders() {
        let mut index = Index::from_entries(vec![entry("/r/a/old.md", 1), entry("/r/b/keep.md", 1)]);
        index.begin_scan();
        index.finish_scan(&[PathBuf::from("/r/a")], vec![entry("/r/a/new.md", 2), entry("/r/a/x/y.md", 3)]);
        assert_eq!(index.names().collect::<Vec<_>>(), [Path::new("/r/a/new.md"), Path::new("/r/a/x/y.md"), Path::new("/r/b/keep.md")]);
    }

    /// Runs watcher and index like the app until `until` holds or time runs out.
    fn run_until(w: &mut crate::watch::Watcher, lib: &mut Library, until: impl Fn(&Library) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            let events = w.drain(Instant::now());
            lib.poll(&events, Instant::now());
            if until(lib) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(16));
        }
        false
    }

    /// A folder renamed below a root: FSEvents reports only the folder. The index
    /// removes the old entries and finds the new ones without a restart; inside and outside the root.
    #[test]
    fn a_renamed_folder_updates_the_index_without_restart() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap().join("root");
        fs::create_dir_all(root.join("repo/docs/sub")).unwrap();
        fs::write(root.join("repo/docs/a.md"), "x").unwrap();
        fs::write(root.join("repo/docs/sub/b.md"), "x").unwrap();
        let outside = dir.path().canonicalize().unwrap().join("outside");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("c.md"), "x").unwrap();
        let mut w = crate::watch::Watcher::start(vec![root.clone()], Arc::new(|| {}));
        let mut lib = Library::load(vec![root.clone()], None);
        lib.start(Arc::new(|| {}));
        assert!(run_until(&mut w, &mut lib, |l| l.scanned && l.index.len() == 2), "initial scan");
        std::thread::sleep(Duration::from_millis(300));
        let names = |l: &Library| l.index.names().map(Path::to_path_buf).collect::<Vec<_>>();
        fs::rename(root.join("repo/docs"), root.join("repo/notes")).unwrap();
        let want = vec![root.join("repo/notes/a.md"), root.join("repo/notes/sub/b.md")];
        assert!(run_until(&mut w, &mut lib, |l| names(l) == want), "rename: {:?}", names(&lib));
        // A folder moved in from outside, and one moved out.
        fs::rename(&outside, root.join("inside")).unwrap();
        fs::rename(root.join("repo/notes"), dir.path().join("via")).unwrap();
        let want = vec![root.join("inside/c.md")];
        assert!(run_until(&mut w, &mut lib, |l| names(l) == want), "moved: {:?}", names(&lib));
    }

    /// Measurement (`docs/perf.md`): the UI-thread cost of collecting a full scan
    /// and of saving its copy. `cargo test --profile perf scan_pickup_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn scan_pickup_cost() {
        let roots = roots_from(&crate::settings::default_roots());
        let scanned = scan::scan(&roots);
        let mut index = Index::from_entries(scanned.clone());
        index.set_roots(roots.clone());
        let (mut pickup, mut copy) = (Vec::new(), Vec::new());
        for _ in 0..40 {
            let fresh = scanned.clone();
            index.begin_scan();
            let t = Instant::now();
            index.finish_scan(&roots, fresh);
            pickup.push(t.elapsed().as_secs_f64() * 1000.0);
            let t = Instant::now();
            let entries = index.entries.clone();
            copy.push(t.elapsed().as_secs_f64() * 1000.0);
            drop(entries);
        }
        eprintln!("scan_pickup_cost {} voci: finish_scan {}", index.len(), stats(&mut pickup));
        eprintln!("scan_pickup_cost copy for the save {}", stats(&mut copy));
    }

    fn stats(ms: &mut [f64]) -> String {
        ms.sort_by(f64::total_cmp);
        let pct = |p: f64| ms[((ms.len() - 1) as f64 * p).round() as usize];
        format!("n={} p50={:.2} ms p95={:.2} ms max={:.2} ms", ms.len(), pct(0.5), pct(0.95), pct(1.0))
    }

    /// Measurement (`docs/perf.md`): scan and reload from cache on the real watched folders.
    /// `cargo test --profile perf scan_cost -- --ignored --nocapture` (cache in a temporary
    /// folder, never the app's).
    #[test]
    #[ignore]
    fn scan_cost() {
        let roots = roots_from(&crate::settings::default_roots());
        let dir = tempfile::tempdir().unwrap();
        let cache = dir.path().join("index.json");
        let ms = |t: Instant| t.elapsed().as_secs_f64() * 1000.0;
        for round in 1..=5 {
            let t = Instant::now();
            let entries = scan::scan(&roots);
            let scan_ms = ms(t);
            let index = Index::from_entries(entries);
            let t = Instant::now();
            index.save_cache(&cache);
            let save_ms = ms(t);
            let t = Instant::now();
            let loaded = Index::load_cache(&cache);
            let load_ms = ms(t);
            assert_eq!(loaded.len(), index.len());
            let t = Instant::now();
            let recent = index.recent(20).len();
            let recent_ms = ms(t);
            eprintln!(
                "scan_cost round {round}: {} entries, scan {scan_ms:.1} ms, save {save_ms:.1} ms, reload {load_ms:.1} ms, recent(20)={recent} {recent_ms:.2} ms, cache {} KB",
                index.len(),
                fs::metadata(&cache).unwrap().len() / 1024
            );
        }
        // How many entries are in hidden folders other than `.claude` (they get in).
        let index = Index::from_entries(scan::scan(&roots));
        let mut hidden: std::collections::BTreeMap<String, usize> = Default::default();
        for p in index.names() {
            let rel = roots.iter().find_map(|r| p.strip_prefix(r).ok()).unwrap();
            if let Some(c) = rel.components().find(|c| c.as_os_str().to_string_lossy().starts_with('.')) {
                *hidden.entry(c.as_os_str().to_string_lossy().into_owned()).or_default() += 1;
            }
        }
        eprintln!("scan_cost hidden folders: {hidden:?}");
    }

    /// The app: cache at startup, a scan that replaces it, events, cache rewritten.
    #[test]
    fn library_loads_cache_then_scan_then_saves() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let cache = dir.path().join("support/index.json");
        Index::from_entries(vec![Entry { path: root.join("old.md"), mtime: at(1) }, entry("/elsewhere/x.md", 1)]).save_cache(&cache);
        fs::write(root.join("new.md"), "x").unwrap();
        let mut lib = Library::load(vec![root.clone()], Some(cache.clone()));
        assert_eq!(lib.index.names().collect::<Vec<_>>(), [root.join("old.md").as_path()], "from the cache, only below the roots");
        lib.start(Arc::new(|| {}));
        let deadline = Instant::now() + Duration::from_secs(5);
        while !lib.scanned && Instant::now() < deadline {
            lib.poll(&[], Instant::now());
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(lib.scanned);
        assert_eq!(lib.index.names().collect::<Vec<_>>(), [root.join("new.md").as_path()]);
        lib.saving.take().unwrap().join().unwrap();
        assert_eq!(Index::load_cache(&cache).names().collect::<Vec<_>>(), [root.join("new.md").as_path()]);
    }
}
