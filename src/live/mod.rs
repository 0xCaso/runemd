//! Live reading: the files open in tabs reload when an agent writes them.
//!
//! `Live` holds the watcher, "writing" and the app's own writes, shared by all
//! tabs; each tab has a [`Follow`] with its own reread. A file is reread at each `Changed`
//! of its path and, after a suspicious read (file half written), once more after
//! `SETTLE`. Reading and preparing the update (blocks, hash, diff with stable ids)
//! run on a separate thread (`reload::read`); on the UI thread stays `reload::commit`, which
//! replaces the blocks. The thread wakes the app with the `Waker` when it is done.
//!
//! The watcher watches the watched folders with their subfolders (`index::roots_from`). For
//! each open file its folder is added, without subfolders (a file in `~` must not make it
//! watch the whole home), and the `.git` of its repo, for the `.git/HEAD` changes that turn off
//! "writing" (`activity`); if they are already under a watched folder the watcher does not
//! add them (`Watcher::add_root`), and each event arrives only once.
//!
//! The roots added for open files are counted: when the last tab that needs them
//! closes they leave the watcher (`Live::unfollow`).
//!
//! `Live::drain` drains the watcher at each `poll`; the events of that wakeup stay in
//! `drained` for the tabs (`Live::poll`) and for the index.

pub mod activity;
pub mod arriving;
pub mod below;
pub mod caret;
pub mod reload;

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant, SystemTime};

use rune::Waker;

use crate::doc::document::{Change, Document};
use crate::watch::{FsEvent, Watcher};

use activity::Activity;
use reload::{Commit, OwnWrites, Read};

/// After a suspicious read the file is reread after this time; if the content is the same, the file
/// is no longer changing and it holds. An in-place rewrite of 22 KB takes a few ms
/// (`docs/agent-writes.md`): 200 ms cover it with margin (`ipotesi`).
const SETTLE: Duration = Duration::from_millis(200);

/// An applied reload: `at` for "writing", the ids in `change` for the mark on changed
/// blocks.
#[derive(Debug)]
pub struct Reloaded {
    pub at: Instant,
    /// The mtime of the file read: when the agent wrote it (latency probe).
    pub written: Option<SystemTime>,
    pub change: Change,
}

#[derive(Default)]
pub struct Live {
    watcher: Option<Watcher>,
    pub own: OwnWrites,
    /// "Writing": only applied external reloads light it.
    activity: Activity,
    /// The roots added for open files (folder of the file, folder of HEAD) and how many
    /// tabs use them: the last one that closes removes it from the watcher.
    roots: BTreeMap<PathBuf, usize>,
    waker: Option<Waker>,
    /// The events of the last `drain`, for the tabs and for the index.
    drained: Vec<FsEvent>,
    /// In tests the read is waited for at once, so the fake clock decides alone.
    #[cfg(test)]
    sync: bool,
}

/// The reread of an open file (a tab): each has its own, with its own read in progress.
pub struct Follow {
    /// The file as FSEvents reports it (canonical).
    watched: PathBuf,
    /// The roots this file asked the watcher for (`Live::roots`).
    roots: Vec<PathBuf>,
    /// The hash of a skipped suspicious read: if the reread gives the same, it is applied.
    held: Option<u64>,
    recheck_at: Option<Instant>,
    /// A reread to do: `Some(true)` if it is the timed one after a suspicious read.
    want: Option<bool>,
    /// The read in progress on the separate thread.
    job: Option<Job>,
}

impl Follow {
    /// The followed file, canonical (as FSEvents and `Activity` report it).
    pub fn path(&self) -> &Path {
        &self.watched
    }

    /// Rereads the file at the next `poll`: after editing (Cmd+E), which suspends reloading and
    /// loses its events.
    pub fn reread(&mut self) {
        self.want = Some(self.want.unwrap_or(false));
    }
}

struct Job {
    rx: Receiver<io::Result<Read>>,
    timed: bool,
}

/// The folders to watch for `path`, without subfolders: its folder and the folder of its
/// repo's HEAD file, `.git/` in a repo, `<main repo>/.git/worktrees/<name>/` in a
/// worktree (`repo::head_path`).
fn roots_for(path: &Path) -> Vec<PathBuf> {
    root_of(path).into_iter().chain(head_dir(path)).collect()
}

/// The folder of the HEAD file of `path`'s repo; `None` outside a repo.
fn head_dir(path: &Path) -> Option<PathBuf> {
    crate::repo::head_path(&crate::repo::of(path).root)?.parent().map(Path::to_path_buf)
}

/// True if `path` changed less than the "writing" window ago: a `Changed` that rereads the
/// known files after lost events (`Core::rescan`) does not light the ring of files still for hours.
fn written_recently(path: &Path) -> bool {
    let modified = std::fs::metadata(path).and_then(|m| m.modified());
    modified.is_ok_and(|m| SystemTime::now().duration_since(m).unwrap_or_default() < activity::WRITING_WINDOW)
}

/// The folder to watch for `path` (not recursive).
fn root_of(path: &Path) -> Option<PathBuf> {
    path.parent().map(|p| if p.as_os_str().is_empty() { PathBuf::from(".") } else { p.to_owned() })
}

impl Live {
    /// Starts the watcher (from `AppSpec::setup`, which has the waker) on the watched folders `roots`,
    /// with their subfolders, and watches the files already open.
    pub fn start(&mut self, waker: Waker, roots: Vec<PathBuf>) {
        let mut watcher = Watcher::start(roots, waker.clone());
        for root in self.roots.keys() {
            watcher.add_root(root.clone(), false);
        }
        self.watcher = Some(watcher);
        self.waker = Some(waker);
    }

    /// A file opens in a tab: its folder and the HEAD one are watched (unless another tab or a
    /// watched folder already watches them, `Watcher::add_root`).
    pub fn follow(&mut self, path: &Path) -> Follow {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_owned());
        let roots = roots_for(&canonical);
        for root in &roots {
            let n = self.roots.entry(root.clone()).or_insert(0);
            *n += 1;
            if *n == 1
                && let Some(w) = self.watcher.as_mut()
            {
                w.add_root(root.clone(), false);
            }
        }
        Follow { watched: canonical, roots, held: None, recheck_at: None, want: None, job: None }
    }

    /// The tab of `f` closes: the roots no other tab uses leave the watcher (the
    /// watched folders stay, `Watcher::remove_root`). The read in progress is lost with `f`.
    pub fn unfollow(&mut self, f: Follow) {
        for root in f.roots {
            let Some(n) = self.roots.get_mut(&root) else { continue };
            *n -= 1;
            if *n == 0 {
                self.roots.remove(&root);
                if let Some(w) = self.watcher.as_mut() {
                    w.remove_root(&root);
                }
            }
        }
    }

    /// A new watched folder (from the settings): the watcher watches it with its
    /// subfolders. The folders of open files that are now under it leave the watcher, so
    /// their events do not arrive twice (they stay counted for the tabs).
    pub fn add_tree(&mut self, root: &Path) {
        let Some(w) = self.watcher.as_mut() else { return };
        w.add_root(root.to_owned(), true);
        for open in self.roots.keys().filter(|r| r.starts_with(root)) {
            w.remove_root(open);
        }
    }

    /// A watched folder removed: the watcher stops watching it, and the folders of the open
    /// files that were under it are watched on their own again.
    pub fn remove_tree(&mut self, root: &Path) {
        let Some(w) = self.watcher.as_mut() else { return };
        w.remove_tree(root);
        for open in self.roots.keys().filter(|r| r.starts_with(root)) {
            w.add_root(open.clone(), false);
        }
    }

    /// The watcher roots and whether with subfolders (empty before `start`).
    #[cfg(test)]
    pub fn watched_roots(&self) -> Vec<(PathBuf, bool)> {
        self.watcher.as_ref().map(Watcher::watched).unwrap_or_default()
    }

    /// The roots added for open files (for the tests and the memory measure).
    #[cfg(test)]
    pub fn open_roots(&self) -> Vec<PathBuf> {
        self.roots.keys().cloned().collect()
    }

    /// To call at each `poll`, before [`Live::poll`] for the tabs: drains the watcher (it must
    /// be done at each wakeup) and notes the writes of files that are not open. `open`: the files followed
    /// by the tabs, which go only through the reload.
    pub fn drain(&mut self, open: &[&Path], now: Instant) {
        let events = self.watcher.as_mut().map(|w| w.drain(now)).unwrap_or_default();
        self.note(&events, open, now);
        self.drained = events;
    }

    /// For a tab, after [`Live::drain`]: collects the finished read and starts one if its
    /// file changed. Returns the applied change and, if needed, how soon to call it again.
    pub fn poll(&mut self, f: &mut Follow, doc: &mut Document, now: Instant) -> (Option<Reloaded>, Option<Duration>) {
        // A `Rescan` above the file (lost events, folder moved): it is reread; if the
        // content is the same the reload changes nothing.
        let changed = self.drained.iter().any(|e| match e {
            FsEvent::Changed(p) => *p == f.watched,
            FsEvent::Rescan(dir) => f.watched.starts_with(dir),
            _ => false,
        });
        self.step(f, doc, changed, now)
    }

    /// The last `drain` has a `Changed` of `path` (canonical).
    pub fn changed(&self, path: &Path) -> bool {
        self.drained.iter().any(|e| matches!(e, FsEvent::Changed(p) if p == path))
    }

    /// A file open in the source (Cmd+E), which does not go through the reload, was written from
    /// outside: "writing" lights as for an applied reload.
    pub fn touch_open(&mut self, path: &Path, now: Instant) {
        self.activity.touch(path, now);
    }

    /// The watcher events drained by the last `drain`.
    pub fn drained(&self) -> &[FsEvent] {
        &self.drained
    }

    /// How soon `path` (canonical) stops "writing"; `None` if it is not.
    pub fn writing_left(&self, path: &Path, now: Instant) -> Option<Duration> {
        self.activity.left(path, now)
    }

    /// The files "writing" at `now`, open ones included, and how soon they stop.
    pub fn writing(&self, now: Instant) -> Vec<(PathBuf, Duration)> {
        self.activity.writing(now).map(|(p, d)| (p.to_owned(), d)).collect()
    }

    /// The events that are not reloads of open files: a change of `.git/HEAD`, or another
    /// file written recently (an agent writes it: the app writes only open files).
    fn note(&mut self, events: &[FsEvent], open: &[&Path], now: Instant) {
        for e in events {
            match e {
                FsEvent::GitHead(repo) => self.activity.git_head(repo, now),
                FsEvent::Changed(p) if !open.contains(&p.as_path()) && written_recently(p) => self.activity.touch(p, now),
                _ => {}
            }
        }
    }

    /// `poll` without the watcher: `changed` says whether a `Changed` of `f`'s file arrived.
    fn step(&mut self, f: &mut Follow, doc: &mut Document, changed: bool, now: Instant) -> (Option<Reloaded>, Option<Duration>) {
        if f.recheck_at.is_some_and(|t| now >= t) {
            f.recheck_at = None;
            f.want = Some(true);
        } else if changed {
            f.want = Some(f.want.unwrap_or(false));
        }
        let out = self.collect(f, doc, now);
        if f.job.is_none()
            && let Some(timed) = f.want.take()
        {
            self.spawn(f, doc, timed);
        }
        #[cfg(test)]
        let out = if self.sync && out.is_none() { self.collect(f, doc, now) } else { out };
        (out, f.recheck_at.map(|t| t.saturating_duration_since(now)))
    }

    /// Reads the file on a separate thread, starting from the current blocks.
    fn spawn(&mut self, f: &mut Follow, doc: &Document, timed: bool) {
        let (tx, rx) = mpsc::channel();
        let (path, snap, waker) = (doc.path.clone(), doc.snapshot(), self.waker.clone());
        std::thread::spawn(move || {
            let _ = tx.send(reload::read(&path, &snap));
            if let Some(wake) = waker {
                wake();
            }
        });
        f.job = Some(Job { rx, timed });
    }

    /// The finished read, if any, decided and applied.
    fn collect(&mut self, f: &mut Follow, doc: &mut Document, now: Instant) -> Option<Reloaded> {
        let job = f.job.take()?;
        #[cfg(test)]
        let polled = if self.sync { job.rx.recv().map_err(|_| TryRecvError::Disconnected) } else { job.rx.try_recv() };
        #[cfg(not(test))]
        let polled = job.rx.try_recv();
        match polled {
            Ok(Ok(read)) => self.settle(f, doc, read, job.timed, now),
            // File missing or unreadable during a rename: the watcher reports it when it comes back.
            Ok(Err(_)) | Err(TryRecvError::Disconnected) => None,
            Err(TryRecvError::Empty) => {
                f.job = Some(job);
                None
            }
        }
    }

    fn settle(&mut self, f: &mut Follow, doc: &mut Document, read: Read, timed: bool, now: Instant) -> Option<Reloaded> {
        if read.suspect {
            // A suspicious read holds only at the timed reread, equal to the held one.
            // Before that, even another `Changed` with the same content waits: the write may
            // not be finished.
            if !(timed && f.held == Some(read.hash)) {
                if f.held != Some(read.hash) || f.recheck_at.is_none() {
                    f.held = Some(read.hash);
                    f.recheck_at = Some(now + SETTLE);
                }
                return None;
            }
        }
        f.held = None;
        f.recheck_at = None;
        let written = read.written;
        match reload::commit(doc, read, &self.own) {
            Commit::Applied(change) => {
                // The only place that lights "writing" for an open file: never for the
                // app's own writes, skipped reads or held suspicious ones.
                self.activity.touch(&f.watched, now);
                Some(Reloaded { at: now, written, change })
            }
            Commit::Skipped => None,
            Commit::Stale => {
                f.want = Some(f.want.unwrap_or(false));
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Arc;
    use std::thread;

    use super::*;

    /// `Live` with a single tab, like the app before tabs.
    struct One {
        live: Live,
        f: Follow,
    }

    impl One {
        fn new(live: Live, path: &Path) -> One {
            let mut live = live;
            let f = live.follow(path);
            One { live, f }
        }

        fn poll(&mut self, doc: &mut Document, now: Instant) -> (Option<Reloaded>, Option<Duration>) {
            let watched = self.f.watched.clone();
            self.live.drain(&[&watched], now);
            self.live.poll(&mut self.f, doc, now)
        }

        fn step(&mut self, doc: &mut Document, changed: bool, now: Instant) -> (Option<Reloaded>, Option<Duration>) {
            self.live.step(&mut self.f, doc, changed, now)
        }

        fn writing_left(&self, now: Instant) -> Option<Duration> {
            self.live.writing_left(&self.f.watched, now)
        }

        fn note(&mut self, events: &[FsEvent], now: Instant) {
            let watched = self.f.watched.clone();
            self.live.note(events, &[&watched], now);
        }
    }

    /// Runs `poll` like the app (every ~16 ms) until a reload arrives or `window` expires.
    fn poll_until(live: &mut One, doc: &mut Document, window: Duration) -> Option<Reloaded> {
        let deadline = Instant::now() + window;
        while Instant::now() < deadline {
            if let (Some(r), _) = live.poll(doc, Instant::now()) {
                return Some(r);
            }
            thread::sleep(Duration::from_millis(16));
        }
        None
    }

    fn started(path: &Path) -> (One, Document) {
        let mut live = One::new(Live::default(), path);
        live.live.start(Arc::new(|| {}), Vec::new());
        let doc = Document::load(path).unwrap();
        // FSEvents starts asynchronously: a first identical write acts as a sentinel.
        thread::sleep(Duration::from_millis(300));
        (live, doc)
    }

    #[test]
    fn an_external_write_reloads_the_open_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n\none\n").unwrap();
        let (mut live, mut doc) = started(&path);
        let first = doc.blocks[0].id;
        fs::write(&path, "# a\n\none\n\ntwo\n").unwrap();
        let r = poll_until(&mut live, &mut doc, Duration::from_secs(5)).expect("reload");
        assert_eq!(r.change.added.len(), 1);
        assert_eq!(doc.blocks.len(), 3);
        assert_eq!(doc.blocks[0].id, first, "stable ids");
        assert!(r.written.is_some());
    }

    /// The open file is in a subfolder of a watched folder (recursive): a
    /// write reloads it only once, and the event stays in `drained` for the index.
    #[test]
    fn a_file_under_a_watched_root_reloads_once() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::create_dir_all(root.join("repo/docs")).unwrap();
        let path = root.join("repo/docs/a.md");
        fs::write(&path, "# a\n").unwrap();
        let mut live = One::new(Live::default(), &path);
        live.live.start(Arc::new(|| {}), vec![root.clone()]);
        let mut doc = Document::load(&path).unwrap();
        thread::sleep(Duration::from_millis(300));
        fs::write(&path, "# a\n\nb\n").unwrap();
        let mut seen = Vec::new();
        let mut reloads = 0;
        let mut deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline {
            if live.poll(&mut doc, Instant::now()).0.is_some() {
                reloads += 1;
                // After the first one, 500 ms more to see a possible second reload.
                deadline = deadline.min(Instant::now() + Duration::from_millis(500));
            }
            seen.extend(live.live.drained().iter().cloned());
            thread::sleep(Duration::from_millis(16));
        }
        assert_eq!(reloads, 1, "{seen:?}");
        assert_eq!(doc.blocks.len(), 2);
        assert!(seen.contains(&FsEvent::Changed(path)), "{seen:?}");
    }

    #[test]
    fn a_truncation_is_applied_after_settle() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let long: String = (0..200).map(|n| format!("Test paragraph {n}.\n\n")).collect();
        fs::write(&path, &long).unwrap();
        let (mut live, mut doc) = started(&path);
        fs::write(&path, "# short\n").unwrap();
        let t0 = Instant::now();
        let r = poll_until(&mut live, &mut doc, Duration::from_secs(5)).expect("reload after SETTLE");
        assert!(t0.elapsed() >= SETTLE, "the short read waits for the second");
        assert_eq!(doc.blocks.len(), 1);
        assert!(!r.change.removed.is_empty());
    }

    #[test]
    fn a_half_written_read_is_replaced_by_the_full_one() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let long: String = (0..200).map(|n| format!("Test paragraph {n}.\n\n")).collect();
        fs::write(&path, &long).unwrap();
        let (mut live, mut doc) = started(&path);
        // Codex: truncates, then writes the whole file (longer) on the same inode.
        fs::write(&path, "").unwrap();
        let mut seen_empty = false;
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline && !seen_empty {
            let (r, wait) = live.poll(&mut doc, Instant::now());
            assert!(r.is_none(), "an empty read is not applied");
            seen_empty = wait.is_some();
            thread::sleep(Duration::from_millis(10));
        }
        assert!(seen_empty, "the empty read is suspicious");
        let full = format!("{long}New at the end.\n");
        fs::write(&path, &full).unwrap();
        let r = poll_until(&mut live, &mut doc, Duration::from_secs(5)).expect("reload");
        assert_eq!(r.change.added.len(), 1, "only the new block: {:?}", r.change);
        assert_eq!(doc.blocks.len(), 201);
    }

    /// Truncation, then a second `Changed` 60 ms later with the same empty file: nothing is
    /// applied before the reread at 200 ms; the full content that comes back earlier keeps the ids.
    #[test]
    fn a_second_changed_does_not_skip_the_settle_wait() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let long: String = (0..200).map(|n| format!("Test paragraph {n}.\n\n")).collect();
        fs::write(&path, &long).unwrap();
        let mut doc = Document::load(&path).unwrap();
        let ids: Vec<u64> = doc.blocks.iter().map(|b| b.id).collect();
        let mut live = One::new(Live { sync: true, ..Live::default() }, &path);
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        fs::write(&path, "").unwrap();
        let (r, wait) = live.step(&mut doc, true, at(0));
        assert!(r.is_none());
        assert_eq!(wait, Some(SETTLE));
        fs::write(&path, "").unwrap();
        let (r, wait) = live.step(&mut doc, true, at(60));
        assert!(r.is_none(), "second identical Changed before the reread");
        assert_eq!(wait, Some(Duration::from_millis(140)), "the reread stays at 200 ms");
        assert!(live.step(&mut doc, false, at(150)).0.is_none());
        assert_eq!(doc.blocks.len(), 200);
        let full = format!("{long}New at the end.\n");
        fs::write(&path, &full).unwrap();
        let (r, _) = live.step(&mut doc, true, at(180));
        assert_eq!(r.expect("full content").change.added.len(), 1);
        assert_eq!(doc.blocks[..200].iter().map(|b| b.id).collect::<Vec<_>>(), ids, "ids kept");
        assert!(live.step(&mut doc, false, at(400)).0.is_none(), "no reread pending");
    }

    /// A real truncation is applied at the timed reread, not before.
    #[test]
    fn a_held_truncation_applies_only_on_the_timed_recheck() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let long: String = (0..200).map(|n| format!("Test paragraph {n}.\n\n")).collect();
        fs::write(&path, &long).unwrap();
        let mut doc = Document::load(&path).unwrap();
        let mut live = One::new(Live { sync: true, ..Live::default() }, &path);
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        fs::write(&path, "").unwrap();
        assert!(live.step(&mut doc, true, at(0)).0.is_none());
        assert!(live.step(&mut doc, true, at(199)).0.is_none());
        assert!(live.step(&mut doc, false, at(200)).0.is_some(), "at the timed reread");
        assert!(doc.blocks.is_empty());
    }

    fn sync_live(path: &Path) -> (One, Document) {
        let doc = Document::load(path).unwrap();
        (One::new(Live { sync: true, ..Live::default() }, path), doc)
    }

    /// Only an applied external reload lights "writing": the reread of an
    /// app write does not.
    #[test]
    fn own_writes_do_not_touch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n").unwrap();
        let (mut live, mut doc) = sync_live(&path);
        let t0 = Instant::now();
        // Same content (a `touch` without changes): read skipped, no ring.
        fs::write(&path, "# a\n").unwrap();
        assert!(live.step(&mut doc, true, t0).0.is_none());
        assert_eq!(live.writing_left(t0), None, "read skipped");
        let mine = "# a\n\nwritten by the app\n";
        live.live.own.remember(&doc.path, crate::doc::ids::hash_str(mine));
        fs::write(&path, mine).unwrap();
        assert!(live.step(&mut doc, true, t0).0.is_none());
        assert_eq!(live.writing_left(t0), None, "app write");
        fs::write(&path, "# a\n\nby the agent\n").unwrap();
        assert!(live.step(&mut doc, true, t0).0.is_some());
        assert_eq!(live.writing_left(t0), Some(activity::WRITING_WINDOW), "agent write");
    }

    /// A held suspicious read does not light the ring; it lights it when it is applied.
    #[test]
    fn a_held_suspect_read_does_not_touch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let long: String = (0..200).map(|n| format!("Test paragraph {n}.\n\n")).collect();
        fs::write(&path, &long).unwrap();
        let (mut live, mut doc) = sync_live(&path);
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        fs::write(&path, "").unwrap();
        assert!(live.step(&mut doc, true, at(0)).0.is_none());
        assert_eq!(live.writing_left(at(0)), None, "held");
        assert!(live.step(&mut doc, false, at(200)).0.is_some());
        assert!(live.writing_left(at(200)).is_some(), "applied at the reread");
    }

    /// A change of `.git/HEAD` of the file's repo turns off the reloads of the 2 s around it.
    #[test]
    fn a_git_checkout_reload_does_not_touch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n").unwrap();
        let (mut live, mut doc) = sync_live(&path);
        let repo = dir.path().canonicalize().unwrap();
        let t0 = Instant::now();
        live.note(&[FsEvent::GitHead(repo)], t0);
        fs::write(&path, "# a\n\ndal checkout\n").unwrap();
        let at = t0 + Duration::from_millis(500);
        assert!(live.step(&mut doc, true, at).0.is_some(), "the document updates");
        assert_eq!(live.writing_left(at), None, "but the ring stays off");
    }

    #[test]
    fn head_dir_is_git_or_the_worktree_gitdir() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("main/.git/worktrees/wt")).unwrap();
        fs::create_dir_all(dir.path().join("main/docs/sub")).unwrap();
        assert_eq!(head_dir(&dir.path().join("main/docs/sub/a.md")), Some(dir.path().join("main/.git")));
        let wt = dir.path().join("wt");
        fs::create_dir_all(&wt).unwrap();
        fs::write(wt.join(".git"), "gitdir: ../main/.git/worktrees/wt\n").unwrap();
        assert_eq!(head_dir(&wt.join("a.md")), Some(wt.join("../main/.git/worktrees/wt")));
        let other = tempfile::tempdir().unwrap();
        assert_eq!(head_dir(&other.path().join("a.md")), None);
    }

    /// Another file written recently is "writing" (rows of Recents and tree); one still for
    /// hours reported by a rescan is not; the open file goes only through the reload.
    #[test]
    fn other_files_written_recently_are_writing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n").unwrap();
        let (mut live, _doc) = sync_live(&path);
        let fresh = dir.path().join("b.md");
        fs::write(&fresh, "# b\n").unwrap();
        let old = dir.path().join("c.md");
        fs::write(&old, "# c\n").unwrap();
        fs::File::options().write(true).open(&old).unwrap().set_modified(SystemTime::now() - Duration::from_secs(3600)).unwrap();
        let now = Instant::now();
        let watched = live.f.watched.clone();
        live.note(&[FsEvent::Changed(fresh.clone()), FsEvent::Changed(old), FsEvent::Changed(watched)], now);
        let writing: Vec<PathBuf> = live.live.writing(now).into_iter().map(|(p, _)| p).collect();
        assert_eq!(writing, vec![fresh]);
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

    /// Two tabs: the write of `b` reloads only `b`, and its ring lights only for it.
    #[test]
    fn each_tab_reloads_only_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.md"), dir.path().join("b.md"));
        fs::write(&a, "# a\n").unwrap();
        fs::write(&b, "# b\n").unwrap();
        let mut live = Live { sync: true, ..Live::default() };
        let (mut fa, mut fb) = (live.follow(&a), live.follow(&b));
        let (mut da, mut db) = (Document::load(&a).unwrap(), Document::load(&b).unwrap());
        let t0 = Instant::now();
        fs::write(&b, "# b\n\nnew\n").unwrap();
        live.drained = vec![FsEvent::Changed(fb.watched.clone())];
        assert!(live.poll(&mut fa, &mut da, t0).0.is_none());
        assert!(live.poll(&mut fb, &mut db, t0).0.is_some());
        assert_eq!((da.revision, db.revision), (0, 1));
        assert!(live.writing_left(fb.path(), t0).is_some());
        assert!(live.writing_left(fa.path(), t0).is_none());
    }

    /// It holds for every tab, not only the active one: a tab's file disappears (the read
    /// fails, the document stays as it was) and comes back within 2 s: the tab reloads.
    #[test]
    fn a_deleted_file_that_comes_back_reloads_its_tab() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.md"), dir.path().join("b.md"));
        fs::write(&a, "# a\n").unwrap();
        fs::write(&b, "# b\n\none\n").unwrap();
        let mut live = Live { sync: true, ..Live::default() };
        let (mut fa, mut fb) = (live.follow(&a), live.follow(&b));
        let (mut da, mut db) = (Document::load(&a).unwrap(), Document::load(&b).unwrap());
        let t0 = Instant::now();
        let watched = fb.watched.clone();
        fs::remove_file(&b).unwrap();
        live.drained = vec![FsEvent::Changed(watched.clone())];
        assert!(live.poll(&mut fb, &mut db, t0).0.is_none(), "file missing");
        assert_eq!(db.blocks.len(), 2, "the tab keeps the last content");
        fs::write(&b, "# b\n\none\n\ntwo\n").unwrap();
        let t1 = t0 + Duration::from_millis(1500);
        live.drained = vec![FsEvent::Changed(watched)];
        assert!(live.poll(&mut fa, &mut da, t1).0.is_none());
        assert_eq!(live.poll(&mut fb, &mut db, t1).0.expect("comes back").change.added.len(), 1);
    }

    /// The roots of open files are counted: they are removed when the last tab that uses them closes.
    #[test]
    fn closing_the_last_tab_of_a_folder_releases_its_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::create_dir_all(root.join("x")).unwrap();
        let (a, b, c) = (root.join("a.md"), root.join("b.md"), root.join("x/c.md"));
        for p in [&a, &b, &c] {
            fs::write(p, "# t\n").unwrap();
        }
        let mut live = Live::default();
        let (fa, fb, fc) = (live.follow(&a), live.follow(&b), live.follow(&c));
        assert_eq!(live.open_roots(), vec![root.clone(), root.join("x")]);
        live.unfollow(fa);
        assert_eq!(live.open_roots(), vec![root.clone(), root.join("x")], "b still uses the folder");
        live.unfollow(fc);
        assert_eq!(live.open_roots(), vec![root.clone()]);
        live.unfollow(fb);
        assert!(live.open_roots().is_empty());
    }
}
