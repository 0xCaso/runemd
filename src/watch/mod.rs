//! A single FSEvents stream for everything: tabs, sidebar and reload read from here.
//!
//! The `notify` callback queues the raw paths; `drain` filters them, groups them (50 ms)
//! and decides for each path by looking at the disk, not at the event type: FSEvents delivers
//! renames as two events or merged flags. A missing file becomes `Removed` only if it has not
//! come back after 2 s.
//!
//! A root that disappears (the folder deleted, a checkout without it) leaves the watcher and
//! waits: while it is missing the ticker wakes the app every second, and `drain` picks it up again when the
//! folder reappears. Disappearing and coming back gives a `Rescan`.

pub mod debounce;
pub mod filter;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};
use rune::Waker;

use debounce::Debounce;

const DEBOUNCE_MS: u64 = 50;
/// How long a missing file can stay missing before it counts as removed.
const REMOVED_GRACE: Duration = Duration::from_secs(2);
/// How often the ticker wakes the app while there are pending events.
const TICK: Duration = Duration::from_millis(25);
/// How often the ticker wakes the app while a root is gone: `drain` checks whether it came back.
const DORMANT_TICK: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsEvent {
    Changed(PathBuf),
    Removed(PathBuf),
    /// `.git/HEAD`, `ORIG_HEAD` or `FETCH_HEAD` changed: the repo root.
    GitHead(PathBuf),
    /// Below this folder the files may have changed without an event for each one: a
    /// folder moved, renamed, created or removed (FSEvents reports only the folder), or
    /// events lost below a recursive root. Whoever keeps a list of the files rescans it (off
    /// the UI thread); if the folder is gone, what was below it is gone.
    Rescan(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Key {
    File(PathBuf),
    Head(PathBuf),
    Dir(PathBuf),
}

/// The repo root if `path` is `<repo>/.git/HEAD`, `ORIG_HEAD` or `FETCH_HEAD`: checkout
/// writes `HEAD`; pull and `reset --hard` move only the refs, but write `ORIG_HEAD` and
/// `FETCH_HEAD`. For the app they all mean "the repo changed HEAD" (`FsEvent::GitHead`).
///
/// In a worktree `.git` is a file (`gitdir: ...`) and HEAD is in
/// `<main repo>/.git/worktrees/<name>/HEAD`: the root is the worktree folder, read
/// from the `gitdir` file next to it (which points to the worktree's `.git`). Known limit: in submodules HEAD
/// is in `.git/modules/<m>/HEAD` and no `GitHead` comes out.
fn git_head_root(path: &Path) -> Option<PathBuf> {
    if !["HEAD", "ORIG_HEAD", "FETCH_HEAD"].iter().any(|n| path.file_name() == Some(n.as_ref())) {
        return None;
    }
    let git = path.parent()?;
    if git.file_name()? == ".git" {
        return git.parent().map(Path::to_path_buf);
    }
    let worktrees = git.parent()?;
    if worktrees.file_name()? != "worktrees" || worktrees.parent()?.file_name()? != ".git" {
        return None;
    }
    let target = std::fs::read_to_string(git.join("gitdir")).ok()?;
    let dot_git = git.join(target.trim());
    let root = dot_git.parent()?;
    Some(root.canonicalize().unwrap_or_else(|_| root.to_owned()))
}

/// The logic without threads or notify, to test it with a fake clock.
struct Core {
    debounce: Debounce<Key>,
    /// Missing files and since when.
    missing: HashMap<PathBuf, Instant>,
    /// Files seen to exist (`Changed` emitted): to check again after a rescan.
    known: HashSet<PathBuf>,
    /// Rescans asked for and not yet run: many merge into one.
    rescan_at: Option<Instant>,
}

impl Core {
    fn new() -> Self {
        Core { debounce: Debounce::new(DEBOUNCE_MS), missing: HashMap::new(), known: HashSet::new(), rescan_at: None }
    }

    fn observe(&mut self, path: PathBuf, at: Instant) {
        // `.git/HEAD` before the filter: `wanted` excludes everything below `.git`.
        if let Some(root) = git_head_root(&path) {
            self.debounce.push(Key::Head(root), at);
        } else if filter::wanted(&path) {
            self.debounce.push(Key::File(path), at);
        }
    }

    /// A folder moved, created or removed: comes out as `Rescan` after the debounce.
    fn observe_dir(&mut self, path: PathBuf, at: Instant) {
        self.debounce.push(Key::Dir(path), at);
    }

    fn request_rescan(&mut self, at: Instant) {
        self.rescan_at = Some(self.rescan_at.map_or(at, |old| old.max(at)));
    }

    fn take_rescan(&mut self) -> Option<Instant> {
        self.rescan_at.take()
    }

    /// Notify lost events: check again every known path and the direct children of the
    /// non-recursive roots. A recursive root is not walked here (on the UI thread it would be a
    /// full scan, 0.6–2.8 s on `~/Desktop/GitHub`): a `Rescan` of the root comes out and whoever
    /// keeps the list of files rescans it on its own thread.
    fn rescan(&mut self, roots: &[Root], at: Instant) {
        let mut paths: Vec<PathBuf> = self.known.iter().cloned().collect();
        paths.extend(self.missing.keys().cloned());
        for root in roots {
            if root.recursive {
                self.observe_dir(root.path.clone(), at);
            } else {
                walk(&root.path, false, &mut paths);
            }
        }
        for path in paths {
            self.observe(path, at);
        }
    }

    fn pending(&self) -> bool {
        !self.debounce.is_empty() || !self.missing.is_empty()
    }

    fn drain(&mut self, now: Instant) -> Vec<FsEvent> {
        let mut out = Vec::new();
        for key in self.debounce.ready(now) {
            match key {
                Key::Head(root) => out.push(FsEvent::GitHead(root)),
                Key::Dir(dir) => out.push(FsEvent::Rescan(dir)),
                Key::File(path) if path.is_file() => {
                    self.missing.remove(&path);
                    self.known.insert(path.clone());
                    out.push(FsEvent::Changed(path));
                }
                // Folder named `x.md`: not a document.
                Key::File(path) if path.exists() => {
                    self.missing.remove(&path);
                }
                Key::File(path) => {
                    self.missing.entry(path).or_insert(now);
                }
            }
        }
        let mut settled = Vec::new();
        for (path, since) in &self.missing {
            if path.is_file() {
                settled.push((path.clone(), true));
            } else if now.saturating_duration_since(*since) >= REMOVED_GRACE {
                settled.push((path.clone(), false));
            }
        }
        for (path, back) in settled {
            self.missing.remove(&path);
            if back {
                self.known.insert(path.clone());
            } else {
                self.known.remove(&path);
            }
            out.push(if back { FsEvent::Changed(path) } else { FsEvent::Removed(path) });
        }
        out
    }
}

/// The `wanted` files in `dir`, and below if `recursive`; read errors ignored. It does not enter
/// skipped folders nor symbolic links to folders (a cycle would never end).
pub(crate) fn walk(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue };
        if kind.is_dir() {
            if recursive && !path.file_name().is_some_and(filter::skipped_name) {
                walk(&path, true, out);
            }
        } else if filter::wanted(&path) && (!kind.is_symlink() || path.is_file()) {
            out.push(path);
        }
    }
}

/// A watched folder: with its subfolders (`recursive`) or only its direct children.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Root {
    path: PathBuf,
    recursive: bool,
}

#[derive(Debug, PartialEq)]
enum Raw {
    Path(PathBuf, Instant),
    Dir(PathBuf, Instant),
    Rescan(Instant),
}

/// What to queue for a path of a notify event; `None` if it does not matter. It runs
/// on the notify thread, so the paths of a build (`target/`, `node_modules/`, `.git/objects`)
/// do not keep the app awake and the stat of a rename does not weigh on the UI.
fn classify(path: PathBuf, kind: &notify::EventKind, at: Instant) -> Option<Raw> {
    use notify::event::{CreateKind, ModifyKind, RemoveKind};
    use notify::EventKind;
    if git_head_root(&path).is_some() || filter::wanted(&path) {
        return Some(Raw::Path(path, at));
    }
    if filter::skipped(&path) {
        return None;
    }
    let dir = match kind {
        EventKind::Create(CreateKind::Folder) | EventKind::Remove(RemoveKind::Folder) => true,
        // A rename does not say whether it was a folder: it is one if there is now a folder or if there is
        // nothing left (the old side). For a vanished non-Markdown file `Rescan` costs nothing:
        // there is nothing below.
        EventKind::Modify(ModifyKind::Name(_)) => std::fs::symlink_metadata(&path).map_or(true, |m| m.is_dir()),
        _ => false,
    };
    dir.then_some(Raw::Dir(path, at))
}

#[derive(Default)]
struct Shared {
    queue: Mutex<Vec<Raw>>,
    /// True if something waits for the debounce or the grace period: the ticker wakes the app.
    busy: AtomicBool,
    /// True if a root is gone: the ticker wakes the app every `DORMANT_TICK`, so `drain`
    /// picks it up again when it reappears (no event brings it back: it is no longer watched).
    dormant: AtomicBool,
    stop: AtomicBool,
}

pub struct Watcher {
    notify: Option<RecommendedWatcher>,
    roots: Vec<Root>,
    /// The roots that are gone (folder deleted, a checkout without it): they are watched again when
    /// the folder reappears.
    dormant: Vec<Root>,
    core: Core,
    shared: Arc<Shared>,
    ticker: Option<JoinHandle<()>>,
}

impl Watcher {
    pub fn start(roots: Vec<PathBuf>, waker: Waker) -> Watcher {
        let shared = Arc::new(Shared::default());
        let ticker = {
            let shared = shared.clone();
            let waker = waker.clone();
            thread::spawn(move || {
                while !shared.stop.load(Ordering::Acquire) {
                    if shared.busy.load(Ordering::Acquire) {
                        waker();
                        thread::park_timeout(TICK);
                    } else if shared.dormant.load(Ordering::Acquire) {
                        waker();
                        thread::park_timeout(DORMANT_TICK);
                    } else {
                        thread::park();
                    }
                }
            })
        };
        let handler = {
            let shared = shared.clone();
            let ticker = ticker.thread().clone();
            move |event: notify::Result<notify::Event>| {
                let now = Instant::now();
                let event = match event {
                    Ok(event) => event,
                    Err(e) => {
                        eprintln!("runemd: watcher error: {e}");
                        // An error can mean lost events: check again from the disk.
                        let mut queue = shared.queue.lock().unwrap_or_else(PoisonError::into_inner);
                        queue.push(Raw::Rescan(now));
                        shared.busy.store(true, Ordering::Release);
                        drop(queue);
                        ticker.unpark();
                        return;
                    }
                };
                if event.kind.is_access() {
                    return;
                }
                let rescan = event.need_rescan();
                let raws: Vec<Raw> = event.paths.into_iter().filter_map(|p| classify(p, &event.kind, now)).collect();
                if raws.is_empty() && !rescan {
                    return;
                }
                {
                    let mut queue = shared.queue.lock().unwrap_or_else(PoisonError::into_inner);
                    if rescan {
                        queue.push(Raw::Rescan(now));
                    }
                    queue.extend(raws);
                    shared.busy.store(true, Ordering::Release);
                }
                ticker.unpark();
            }
        };
        let notify = match notify::recommended_watcher(handler) {
            Ok(w) => Some(w),
            Err(e) => {
                eprintln!("runemd: watcher not started: {e}");
                None
            }
        };
        let mut watcher = Watcher { notify, roots: Vec::new(), dormant: Vec::new(), core: Core::new(), shared, ticker: Some(ticker) };
        for root in roots {
            watcher.add_root(root, true);
        }
        watcher
    }

    /// Watches `root`, with its subfolders if `recursive`, otherwise only its direct children
    /// (notify drops the events further down). A root already watched without subfolders
    /// becomes recursive if asked. A non-recursive root below a recursive one already
    /// watched is not added: its events already arrive, and a second path on the same
    /// stream would make them arrive twice. Roots are canonical: FSEvents reports real paths.
    pub fn add_root(&mut self, root: PathBuf, recursive: bool) {
        let root = root.canonicalize().unwrap_or(root);
        let old = self.roots.iter().position(|r| r.path == root);
        if old.is_some_and(|i| self.roots[i].recursive || !recursive) {
            return;
        }
        if !recursive && self.roots.iter().any(|r| r.recursive && root.starts_with(&r.path)) {
            return;
        }
        let Some(notify) = self.notify.as_mut() else { return };
        if let Some(i) = old {
            let _ = notify.unwatch(&root);
            self.roots.remove(i);
        }
        let mode = if recursive { RecursiveMode::Recursive } else { RecursiveMode::NonRecursive };
        match notify.watch(&root, mode) {
            Ok(()) => self.roots.push(Root { path: root, recursive }),
            // A folder that does not exist (yet): it is watched when it appears (`check_roots`).
            Err(_) if !root.exists() => {
                self.dormant.retain(|r| r.path != root);
                self.dormant.push(Root { path: root, recursive });
            }
            Err(e) => eprintln!("runemd: {}: cannot watch: {e}", root.display()),
        }
    }

    /// Stops watching `root` if it is a root without subfolders (the folder of an open
    /// file). The watched folders (recursive) stay: the index and the sidebar use them.
    pub fn remove_root(&mut self, root: &Path) {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_owned());
        self.dormant.retain(|r| !(r.path == root && !r.recursive));
        let Some(i) = self.roots.iter().position(|r| r.path == root && !r.recursive) else { return };
        if let Some(notify) = self.notify.as_mut() {
            let _ = notify.unwatch(&root);
        }
        self.roots.remove(i);
    }

    /// Stops watching `root`, even if it is a watched (recursive) folder: it is being removed
    /// from the settings. The non-recursive roots below, which `add_root` had not added,
    /// are put back by whoever uses them (`Live::remove_tree`).
    pub fn remove_tree(&mut self, root: &Path) {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_owned());
        self.dormant.retain(|r| r.path != root);
        let Some(i) = self.roots.iter().position(|r| r.path == root) else { return };
        if let Some(notify) = self.notify.as_mut() {
            let _ = notify.unwatch(&root);
        }
        self.roots.remove(i);
    }

    /// The watched roots and whether they include subfolders.
    #[cfg(test)]
    pub fn watched(&self) -> Vec<(PathBuf, bool)> {
        self.roots.iter().map(|r| (r.path.clone(), r.recursive)).collect()
    }

    /// The roots that are gone leave the watcher and wait in `dormant`; the ones that came back
    /// come in again. For each one a `Rescan` comes out: whoever keeps a list of files removes or takes back
    /// the ones below, and the tabs below reread their file.
    fn check_roots(&mut self, out: &mut Vec<FsEvent>) {
        let Some(notify) = self.notify.as_mut() else { return };
        let (gone, kept): (Vec<Root>, Vec<Root>) = std::mem::take(&mut self.roots).into_iter().partition(|r| !r.path.is_dir());
        self.roots = kept;
        for root in gone {
            eprintln!("runemd: {}: root gone, it will be picked up again if it reappears", root.path.display());
            let _ = notify.unwatch(&root.path);
            out.push(FsEvent::Rescan(root.path.clone()));
            self.dormant.push(root);
        }
        let (back, still): (Vec<Root>, Vec<Root>) = std::mem::take(&mut self.dormant).into_iter().partition(|r| r.path.is_dir());
        self.dormant = still;
        for root in back {
            out.push(FsEvent::Rescan(root.path.clone()));
            self.add_root(root.path, root.recursive);
        }
        let dormant = !self.dormant.is_empty();
        if self.shared.dormant.swap(dormant, Ordering::AcqRel) != dormant
            && let Some(ticker) = &self.ticker
        {
            ticker.thread().unpark();
        }
    }

    /// The events ready at `now` (debounce passed, removal grace period expired).
    ///
    /// The caller must call it on every wake-up: the watcher wakes the app while there is
    /// something pending and stops when `drain` has emptied everything.
    pub fn drain(&mut self, now: Instant) -> Vec<FsEvent> {
        let queued = {
            let mut queue = self.shared.queue.lock().unwrap_or_else(PoisonError::into_inner);
            std::mem::take(&mut *queue)
        };
        for raw in queued {
            match raw {
                Raw::Path(path, at) => self.core.observe(path, at),
                Raw::Dir(path, at) => self.core.observe_dir(path, at),
                Raw::Rescan(at) => self.core.request_rescan(at),
            }
        }
        // A single pass on disk per drain, however many rescans are queued.
        if let Some(at) = self.core.take_rescan() {
            self.core.rescan(&self.roots, at);
        }
        let mut out = Vec::new();
        self.check_roots(&mut out);
        out.extend(self.core.drain(now));
        // Under the lock: an event that arrives now cannot lose its wake-up.
        let queue = self.shared.queue.lock().unwrap_or_else(PoisonError::into_inner);
        self.shared.busy.store(self.core.pending() || !queue.is_empty(), Ordering::Release);
        out
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        self.notify = None;
        self.shared.stop.store(true, Ordering::Release);
        if let Some(ticker) = self.ticker.take() {
            ticker.thread().unpark();
            let _ = ticker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::AtomicUsize;

    use super::*;

    fn waker() -> Waker {
        Arc::new(|| {})
    }

    /// Runs `drain` until `until` holds or the deadline expires; collects everything.
    fn collect(w: &mut Watcher, window: Duration, until: impl Fn(&[FsEvent]) -> bool) -> Vec<FsEvent> {
        // The `ready.md` sentinel can have late events: it is not part of the assertions.
        let mut seen = collect_raw(w, window, |s| until(&without_sentinel(s)));
        seen = without_sentinel(&seen);
        seen
    }

    /// Also removes the `Rescan`s: the old side of an atomic rename (`x.md.tmp`) gives one,
    /// and these tests look at the files.
    fn without_sentinel(events: &[FsEvent]) -> Vec<FsEvent> {
        events
            .iter()
            .filter(|e| match e {
                FsEvent::Changed(p) | FsEvent::Removed(p) | FsEvent::GitHead(p) => p.file_name() != Some("ready.md".as_ref()),
                FsEvent::Rescan(_) => false,
            })
            .cloned()
            .collect()
    }

    fn collect_raw(w: &mut Watcher, window: Duration, until: impl Fn(&[FsEvent]) -> bool) -> Vec<FsEvent> {
        let deadline = Instant::now() + window;
        let mut seen = Vec::new();
        while Instant::now() < deadline && !until(&seen) {
            seen.extend(w.drain(Instant::now()));
            thread::sleep(Duration::from_millis(20));
        }
        seen
    }

    fn setup() -> (tempfile::TempDir, PathBuf, Watcher) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let mut w = Watcher::start(vec![root.clone()], waker());
        // Sentinel: when its Changed arrives, FSEvents has started.
        let ready = root.join("ready.md");
        fs::write(&ready, "ready").unwrap();
        let seen = collect_raw(&mut w, Duration::from_secs(5), |s| s.contains(&FsEvent::Changed(ready.clone())));
        assert!(seen.contains(&FsEvent::Changed(ready)), "watcher not ready");
        (dir, root, w)
    }

    /// Creates `path` and waits for its `Changed`, so the test starts from a file already seen.
    fn settle_file(w: &mut Watcher, path: &Path) {
        fs::write(path, "# a").unwrap();
        let seen = collect(w, Duration::from_secs(5), |s| s.contains(&FsEvent::Changed(path.to_path_buf())));
        assert!(seen.contains(&FsEvent::Changed(path.to_path_buf())), "{seen:?}");
    }

    #[test]
    fn rename_atomic_keeps_file() {
        let (_dir, root, mut w) = setup();
        let target = root.join("x.md");
        fs::write(root.join("x.md.tmp"), "# a").unwrap();
        fs::rename(root.join("x.md.tmp"), &target).unwrap();
        let first = collect(&mut w, Duration::from_secs(5), |s| !s.is_empty());
        assert_eq!(first, vec![FsEvent::Changed(target.clone())]);
        // For the whole grace period no Removed must appear.
        let rest = collect(&mut w, Duration::from_millis(2600), |_| false);
        assert!(rest.iter().all(|e| matches!(e, FsEvent::Changed(p) if *p == target)), "{rest:?}");
    }

    #[test]
    fn rename_atomic_over_existing_file() {
        let (_dir, root, mut w) = setup();
        let target = root.join("x.md");
        settle_file(&mut w, &target);
        collect(&mut w, Duration::from_millis(300), |_| false);
        fs::write(root.join("x.md.tmp"), "# b").unwrap();
        fs::rename(root.join("x.md.tmp"), &target).unwrap();
        let mut events = collect(&mut w, Duration::from_secs(5), |s| s.contains(&FsEvent::Changed(target.clone())));
        assert!(events.contains(&FsEvent::Changed(target.clone())), "{events:?}");
        events.extend(collect(&mut w, Duration::from_millis(2600), |_| false));
        assert!(!events.iter().any(|e| matches!(e, FsEvent::Removed(_))), "{events:?}");
    }

    #[test]
    fn rename_away_and_back_within_2s() {
        let (_dir, root, mut w) = setup();
        let x = root.join("x.md");
        settle_file(&mut w, &x);
        collect(&mut w, Duration::from_millis(300), |_| false);
        fs::rename(&x, root.join("y.md")).unwrap();
        fs::rename(root.join("y.md"), &x).unwrap();
        let mut events = collect(&mut w, Duration::from_secs(5), |s| s.contains(&FsEvent::Changed(x.clone())));
        assert!(events.contains(&FsEvent::Changed(x.clone())), "{events:?}");
        events.extend(collect(&mut w, Duration::from_millis(2600), |_| false));
        assert!(!events.contains(&FsEvent::Removed(x)), "{events:?}");
    }

    #[test]
    fn delete_then_recreate_within_2s() {
        let (_dir, root, mut w) = setup();
        let target = root.join("x.md");
        fs::write(&target, "# a").unwrap();
        collect(&mut w, Duration::from_secs(5), |s| !s.is_empty());
        fs::remove_file(&target).unwrap();
        thread::sleep(Duration::from_millis(300));
        w.drain(Instant::now());
        fs::write(&target, "# b").unwrap();
        let events = collect(&mut w, Duration::from_millis(3500), |_| false);
        assert!(events.contains(&FsEvent::Changed(target.clone())), "{events:?}");
        assert!(!events.iter().any(|e| matches!(e, FsEvent::Removed(_))), "{events:?}");
    }

    #[test]
    fn delete_for_good_becomes_removed_after_grace() {
        let (_dir, root, mut w) = setup();
        let target = root.join("x.md");
        fs::write(&target, "# a").unwrap();
        collect(&mut w, Duration::from_secs(5), |s| !s.is_empty());
        fs::remove_file(&target).unwrap();
        let events = collect(&mut w, Duration::from_secs(6), |s| s.contains(&FsEvent::Removed(target.clone())));
        assert!(events.contains(&FsEvent::Removed(target)), "{events:?}");
    }

    #[test]
    fn git_head_produces_event_even_though_git_is_filtered() {
        let (_dir, root, mut w) = setup();
        fs::create_dir(root.join(".git")).unwrap();
        fs::write(root.join(".git/HEAD"), "ref: refs/heads/main").unwrap();
        fs::write(root.join(".git/x.md"), "# no").unwrap();
        let events = collect(&mut w, Duration::from_secs(5), |s| s.contains(&FsEvent::GitHead(root.clone())));
        assert!(events.contains(&FsEvent::GitHead(root.clone())), "{events:?}");
        assert!(!events.iter().any(|e| matches!(e, FsEvent::Changed(_))), "{events:?}");
    }

    #[test]
    fn vanished_root_does_not_panic() {
        let (dir, _root, mut w) = setup();
        drop(dir);
        thread::sleep(Duration::from_millis(300));
        w.drain(Instant::now());
        assert!(w.roots.is_empty());
    }

    /// A root that disappears gives a `Rescan` (whoever keeps a list removes its files) and
    /// is watched again when the folder reappears: a write afterwards gives `Changed`. While it is
    /// gone the waker keeps waking the app, which notices it in `drain`.
    #[test]
    fn a_root_that_comes_back_is_watched_again() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap().join("docs");
        fs::create_dir(&root).unwrap();
        let wakes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let w2 = wakes.clone();
        let mut w = Watcher::start(vec![root.clone()], Arc::new(move || { w2.fetch_add(1, Ordering::SeqCst); }));
        thread::sleep(Duration::from_millis(300));
        fs::remove_dir_all(&root).unwrap();
        let seen = collect_raw(&mut w, Duration::from_secs(5), |s| s.contains(&FsEvent::Rescan(root.clone())));
        assert!(seen.contains(&FsEvent::Rescan(root.clone())), "{seen:?}");
        assert!(w.roots.is_empty(), "no longer watched");
        let before = wakes.load(Ordering::SeqCst);
        thread::sleep(DORMANT_TICK * 2 + Duration::from_millis(100));
        assert!(wakes.load(Ordering::SeqCst) > before, "the waker wakes the app while the root is missing");
        fs::create_dir(&root).unwrap();
        let back = collect_raw(&mut w, Duration::from_secs(5), |s| s.contains(&FsEvent::Rescan(root.clone())));
        assert!(back.contains(&FsEvent::Rescan(root.clone())), "{back:?}");
        assert_eq!(w.watched(), vec![(root.clone(), true)], "watched again, with subfolders");
        thread::sleep(Duration::from_millis(300));
        let file = root.join("a.md");
        fs::write(&file, "# a").unwrap();
        let events = collect(&mut w, Duration::from_secs(5), |s| s.contains(&FsEvent::Changed(file.clone())));
        assert!(events.contains(&FsEvent::Changed(file)), "{events:?}");
    }

    /// A root that is gone and then removed (settings, last tab closed) never comes back.
    #[test]
    fn a_removed_dormant_root_stays_removed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap().join("docs");
        fs::create_dir(&root).unwrap();
        let mut w = Watcher::start(Vec::new(), waker());
        w.add_root(root.clone(), false);
        fs::remove_dir_all(&root).unwrap();
        w.drain(Instant::now());
        assert!(w.roots.is_empty());
        w.remove_root(&root);
        fs::create_dir(&root).unwrap();
        w.drain(Instant::now());
        assert!(w.roots.is_empty(), "removed: it does not come back");
    }

    #[test]
    fn missing_root_at_start_does_not_panic() {
        let mut w = Watcher::start(vec![PathBuf::from("/nonexistent/runemd-root")], waker());
        assert!(w.roots.is_empty());
        assert!(w.drain(Instant::now()).is_empty());
    }

    #[test]
    fn waker_fires_while_events_are_pending() {
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let mut w = Watcher::start(vec![root.clone()], Arc::new(move || { c.fetch_add(1, Ordering::SeqCst); }));
        thread::sleep(Duration::from_millis(300));
        fs::write(root.join("a.md"), "# a").unwrap();
        collect(&mut w, Duration::from_secs(5), |s| !s.is_empty());
        assert!(count.load(Ordering::SeqCst) > 0);
    }

    // Logic with a fake clock.
    #[test]
    fn core_waits_for_debounce_then_reports_changed() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.md");
        fs::write(&file, "x").unwrap();
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        let mut core = Core::new();
        for ms in [0, 20, 40] {
            core.observe(file.clone(), at(ms));
        }
        assert!(core.drain(at(60)).is_empty());
        assert_eq!(core.drain(at(90)), vec![FsEvent::Changed(file)]);
    }

    #[test]
    fn core_removed_only_after_grace() {
        let gone = PathBuf::from("/nonexistent/runemd/gone.md");
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        let mut core = Core::new();
        core.observe(gone.clone(), at(0));
        assert!(core.drain(at(60)).is_empty(), "missing: within the grace period");
        assert!(core.drain(at(2000)).is_empty());
        assert_eq!(core.drain(at(2100)), vec![FsEvent::Removed(gone)]);
        assert!(!core.pending());
    }

    #[test]
    fn core_reappearing_file_is_changed_not_removed() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.md");
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        let mut core = Core::new();
        core.observe(file.clone(), at(0));
        assert!(core.drain(at(60)).is_empty());
        fs::write(&file, "x").unwrap();
        assert_eq!(core.drain(at(1000)), vec![FsEvent::Changed(file)]);
        assert!(core.drain(at(3000)).is_empty());
    }

    #[test]
    fn core_rescan_rechecks_known_paths_and_roots_from_disk() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let (kept, gone, fresh) = (root.join("kept.md"), root.join("gone.md"), root.join("fresh.md"));
        fs::write(&kept, "x").unwrap();
        fs::write(&gone, "x").unwrap();
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        let mut core = Core::new();
        core.observe(kept.clone(), at(0));
        core.observe(gone.clone(), at(0));
        assert_eq!(core.drain(at(60)).len(), 2);
        // Lost events: `gone` disappears, `fresh` is born, no event arrives.
        fs::remove_file(&gone).unwrap();
        fs::write(&fresh, "x").unwrap();
        core.rescan(&[Root { path: root.clone(), recursive: false }], at(100));
        let mut first = core.drain(at(200));
        first.sort_by_key(|e| format!("{e:?}"));
        assert_eq!(first, vec![FsEvent::Changed(fresh), FsEvent::Changed(kept)]);
        assert_eq!(core.drain(at(2300)), vec![FsEvent::Removed(gone)]);
    }

    /// A recursive root is not walked on the thread that calls `drain`: only a `Rescan`
    /// of the root comes out. Known paths are still checked again.
    #[test]
    fn core_rescan_does_not_walk_a_recursive_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("a/b")).unwrap();
        let (kept, deep) = (root.join("kept.md"), root.join("a/b/deep.md"));
        fs::write(&kept, "x").unwrap();
        fs::write(&deep, "x").unwrap();
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        let mut core = Core::new();
        core.observe(kept.clone(), at(0));
        assert_eq!(core.drain(at(60)), vec![FsEvent::Changed(kept.clone())]);
        core.rescan(&[Root { path: root.clone(), recursive: true }], at(100));
        let mut out = core.drain(at(200));
        out.sort_by_key(|e| format!("{e:?}"));
        assert_eq!(out, vec![FsEvent::Changed(kept), FsEvent::Rescan(root)], "`deep.md` not read here");
    }

    /// Measurement (`docs/perf.md`): the UI-thread cost of a rescan with the real watched
    /// folders (`RUNEMD_ROOTS` or `~/Desktop/GitHub`), recursive roots.
    /// `cargo test --profile perf rescan_ui_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn rescan_ui_cost() {
        let roots: Vec<Root> = crate::index::roots_from(&crate::settings::default_roots()).into_iter().map(|path| Root { path, recursive: true }).collect();
        let mut ms = Vec::new();
        for _ in 0..40 {
            let mut core = Core::new();
            let t0 = Instant::now();
            core.rescan(&roots, t0);
            let out = core.drain(t0 + Duration::from_millis(60));
            ms.push(t0.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(out.len(), roots.len());
        }
        ms.sort_by(f64::total_cmp);
        let pct = |p: f64| ms[((ms.len() - 1) as f64 * p).round() as usize];
        eprintln!("rescan_ui_cost n={} p50={:.3} ms p95={:.3} ms max={:.3} ms", ms.len(), pct(0.5), pct(0.95), pct(1.0));
    }

    /// The filter on the notify thread: the files of a build do not enter the queue; Markdown files,
    /// `.git/HEAD` and moved folders do.
    #[test]
    fn handler_prefilter_drops_build_files() {
        use notify::event::{CreateKind, ModifyKind, RemoveKind, RenameMode};
        use notify::EventKind;
        let t = Instant::now();
        let file = EventKind::Create(CreateKind::File);
        assert_eq!(classify(PathBuf::from("/r/target/x.o"), &file, t), None);
        assert_eq!(classify(PathBuf::from("/r/node_modules/p/x.js"), &file, t), None);
        assert_eq!(classify(PathBuf::from("/r/.git/objects/ab/cd"), &file, t), None);
        assert_eq!(classify(PathBuf::from("/r/target"), &EventKind::Create(CreateKind::Folder), t), None);
        assert_eq!(classify(PathBuf::from("/r/src/x.o"), &file, t), None);
        assert_eq!(classify(PathBuf::from("/r/a.md"), &file, t), Some(Raw::Path(PathBuf::from("/r/a.md"), t)));
        assert_eq!(classify(PathBuf::from("/r/.git/HEAD"), &file, t), Some(Raw::Path(PathBuf::from("/r/.git/HEAD"), t)));
        assert_eq!(classify(PathBuf::from("/r/docs"), &EventKind::Remove(RemoveKind::Folder), t), Some(Raw::Dir(PathBuf::from("/r/docs"), t)));
        // Rename: the side that is gone counts as a folder; an existing file does not.
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("x.rs"), "x").unwrap();
        let name = EventKind::Modify(ModifyKind::Name(RenameMode::Any));
        assert_eq!(classify(dir.path().join("x.rs"), &name, t), None);
        assert_eq!(classify(dir.path().to_path_buf(), &name, t), Some(Raw::Dir(dir.path().to_path_buf(), t)));
        assert_eq!(classify(dir.path().join("via"), &name, t), Some(Raw::Dir(dir.path().join("via"), t)));
    }

    #[test]
    fn core_coalesces_rescans_into_one() {
        let t0 = Instant::now();
        let mut core = Core::new();
        for ms in [0, 5, 10] {
            core.request_rescan(t0 + Duration::from_millis(ms));
        }
        assert_eq!(core.take_rescan(), Some(t0 + Duration::from_millis(10)));
        assert_eq!(core.take_rescan(), None, "a single pass");
    }

    #[test]
    fn rescan_survives_symlink_loop_and_skips_skipped_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("a")).unwrap();
        fs::create_dir_all(root.join("node_modules/p")).unwrap();
        fs::write(root.join("a/x.md"), "x").unwrap();
        fs::write(root.join("node_modules/p/y.md"), "x").unwrap();
        std::os::unix::fs::symlink("..", root.join("a/link")).unwrap();
        let mut found = Vec::new();
        walk(&root, true, &mut found);
        assert_eq!(found, vec![root.join("a/x.md")]);
        let mut core = Core::new();
        core.rescan(&[Root { path: root, recursive: true }], Instant::now());
        assert!(core.pending());
    }

    #[test]
    fn rescan_of_a_non_recursive_root_reads_only_its_own_entries() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        fs::create_dir_all(root.join("sub/deep")).unwrap();
        fs::write(root.join("top.md"), "x").unwrap();
        fs::write(root.join("sub/a.md"), "x").unwrap();
        fs::write(root.join("sub/deep/b.md"), "x").unwrap();
        let mut found = Vec::new();
        walk(&root, false, &mut found);
        assert_eq!(found, vec![root.join("top.md")]);
        let t0 = Instant::now();
        let mut core = Core::new();
        core.rescan(&[Root { path: root.clone(), recursive: false }], t0);
        assert_eq!(core.drain(t0 + Duration::from_millis(60)), vec![FsEvent::Changed(root.join("top.md"))]);
    }

    #[test]
    fn a_non_recursive_root_ignores_subfolders() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::create_dir(root.join("sub")).unwrap();
        let mut w = Watcher::start(Vec::new(), waker());
        w.add_root(root.clone(), false);
        let ready = root.join("ready.md");
        fs::write(&ready, "ready").unwrap();
        let seen = collect_raw(&mut w, Duration::from_secs(5), |s| s.contains(&FsEvent::Changed(ready.clone())));
        assert!(seen.contains(&FsEvent::Changed(ready)), "watcher not ready");
        fs::write(root.join("sub/x.md"), "# below").unwrap();
        fs::write(root.join("top.md"), "# above").unwrap();
        let events = collect(&mut w, Duration::from_secs(5), |s| s.contains(&FsEvent::Changed(root.join("top.md"))));
        assert!(events.contains(&FsEvent::Changed(root.join("top.md"))), "{events:?}");
        let rest = collect(&mut w, Duration::from_millis(300), |_| false);
        assert!(!events.iter().chain(&rest).any(|e| matches!(e, FsEvent::Changed(p) if p.starts_with(root.join("sub")))), "{events:?} {rest:?}");
    }

    /// The open file's folder inside a recursive root does not become a second root.
    #[test]
    fn a_non_recursive_root_under_a_recursive_one_is_not_added() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::create_dir_all(root.join("repo/.git")).unwrap();
        let mut w = Watcher::start(vec![root.clone()], waker());
        w.add_root(root.join("repo"), false);
        w.add_root(root.join("repo/.git"), false);
        assert_eq!(w.roots, vec![Root { path: root.clone(), recursive: true }]);
        // Outside the recursive root it does.
        let other = tempfile::tempdir().unwrap();
        w.add_root(other.path().to_path_buf(), false);
        assert_eq!(w.roots.len(), 2);
    }

    /// A root without subfolders is removed; a recursive one (watched folder) stays.
    #[test]
    fn remove_root_drops_only_non_recursive_roots() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let other = tempfile::tempdir().unwrap();
        let other = other.path().canonicalize().unwrap();
        let mut w = Watcher::start(vec![root.clone()], waker());
        w.add_root(other.clone(), false);
        assert_eq!(w.roots.len(), 2);
        w.remove_root(&root);
        w.remove_root(&other);
        assert_eq!(w.roots, vec![Root { path: root, recursive: true }]);
    }

    #[test]
    fn core_git_head_bypasses_filter_only_for_head() {
        let t0 = Instant::now();
        let mut core = Core::new();
        core.observe(PathBuf::from("/r/.git/HEAD"), t0);
        core.observe(PathBuf::from("/r/.git/config"), t0);
        assert_eq!(core.drain(t0 + Duration::from_millis(60)), vec![FsEvent::GitHead(PathBuf::from("/r"))]);
        assert!(!filter::wanted(Path::new("/r/.git/HEAD")));
    }

    /// Pull and `reset --hard` do not touch `.git/HEAD` but `ORIG_HEAD` and `FETCH_HEAD`: those
    /// too are a HEAD change of the repo. The other files in `.git` are not.
    #[test]
    fn orig_head_and_fetch_head_are_head_events() {
        for name in ["HEAD", "ORIG_HEAD", "FETCH_HEAD"] {
            assert_eq!(git_head_root(Path::new(&format!("/r/.git/{name}"))), Some(PathBuf::from("/r")), "{name}");
        }
        for path in ["/r/.git/refs/heads/HEAD", "/r/.git/index", "/r/ORIG_HEAD", "/r/.git/refs/heads/main"] {
            assert_eq!(git_head_root(Path::new(path)), None, "{path}");
        }
        let t0 = Instant::now();
        let mut core = Core::new();
        core.observe(PathBuf::from("/r/.git/ORIG_HEAD"), t0);
        assert_eq!(core.drain(t0 + Duration::from_millis(60)), vec![FsEvent::GitHead(PathBuf::from("/r"))]);
    }

    /// In a worktree HEAD is in the main repo: the event carries the worktree root,
    /// read from the `gitdir` file. A `worktrees` folder outside `.git` does not count.
    #[test]
    fn worktree_head_is_a_head_event_of_the_worktree() {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().canonicalize().unwrap();
        let admin = base.join("main/.git/worktrees/wt");
        fs::create_dir_all(&admin).unwrap();
        let wt = base.join("wt");
        fs::create_dir_all(&wt).unwrap();
        fs::write(admin.join("gitdir"), format!("{}\n", wt.join(".git").display())).unwrap();
        assert_eq!(git_head_root(&admin.join("HEAD")), Some(wt.clone()));
        fs::write(admin.join("gitdir"), "../../../../wt/.git\n").unwrap();
        assert_eq!(git_head_root(&admin.join("HEAD")), Some(wt), "relative gitdir");
        assert_eq!(git_head_root(&base.join("x/worktrees/wt/HEAD")), None);
        assert_eq!(git_head_root(&base.join("main/.git/worktrees/missing/HEAD")), None, "without gitdir");
    }
}
