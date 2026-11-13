//! Un solo stream FSEvents per tutto: schede, barra laterale e ricarica leggono da qui.
//!
//! Il callback di `notify` mette in coda i percorsi grezzi; `drain` li filtra, li raggruppa (50 ms)
//! e decide per ogni percorso guardando il disco, non il tipo di evento: FSEvents consegna
//! rinomina come due eventi o flag fusi. Un file mancante diventa `Removed` solo se dopo 2 s non
//! è tornato.

// Usato dal task 10 (schede e ricarica).
#![allow(dead_code)]

pub mod debounce;
pub mod filter;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};
use rune::Waker;

use debounce::Debounce;

const DEBOUNCE_MS: u64 = 50;
/// Quanto un file mancante può restare tale prima di contare come rimosso.
const REMOVED_GRACE: Duration = Duration::from_secs(2);
/// Ogni quanto il ticker sveglia l'app finché ci sono eventi in attesa.
const TICK: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsEvent {
    Changed(PathBuf),
    Removed(PathBuf),
    /// `.git/HEAD` è cambiato: la radice del repo.
    GitHead(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Key {
    File(PathBuf),
    Head(PathBuf),
}

/// La radice del repo se `path` è `<repo>/.git/HEAD`.
fn git_head_root(path: &Path) -> Option<PathBuf> {
    if path.file_name()? != "HEAD" {
        return None;
    }
    let git = path.parent()?;
    if git.file_name()? != ".git" {
        return None;
    }
    git.parent().map(Path::to_path_buf)
}

/// La logica senza thread né notify, per provarla con un orologio finto.
struct Core {
    debounce: Debounce<Key>,
    /// File mancanti e da quando.
    missing: HashMap<PathBuf, Instant>,
}

impl Core {
    fn new() -> Self {
        Core { debounce: Debounce::new(DEBOUNCE_MS), missing: HashMap::new() }
    }

    fn observe(&mut self, path: PathBuf, at: Instant) {
        // `.git/HEAD` prima del filtro: `wanted` esclude tutto ciò che sta sotto `.git`.
        if let Some(root) = git_head_root(&path) {
            self.debounce.push(Key::Head(root), at);
        } else if filter::wanted(&path) {
            self.debounce.push(Key::File(path), at);
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
                Key::File(path) if path.is_file() => {
                    self.missing.remove(&path);
                    out.push(FsEvent::Changed(path));
                }
                // Cartella con nome `x.md`: non è un documento.
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
            out.push(if back { FsEvent::Changed(path) } else { FsEvent::Removed(path) });
        }
        out
    }
}

#[derive(Default)]
struct Shared {
    queue: Mutex<Vec<(PathBuf, Instant)>>,
    /// Vero se qualcosa attende il debounce o la tolleranza: il ticker sveglia l'app.
    busy: AtomicBool,
    stop: AtomicBool,
}

pub struct Watcher {
    notify: Option<RecommendedWatcher>,
    roots: Vec<PathBuf>,
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
                let Ok(event) = event else { return };
                if event.kind.is_access() {
                    return;
                }
                let now = Instant::now();
                {
                    let mut queue = shared.queue.lock().unwrap();
                    queue.extend(event.paths.into_iter().map(|p| (p, now)));
                    shared.busy.store(true, Ordering::Release);
                }
                ticker.unpark();
            }
        };
        let notify = match notify::recommended_watcher(handler) {
            Ok(w) => Some(w),
            Err(e) => {
                eprintln!("runemd: watcher non avviato: {e}");
                None
            }
        };
        let mut watcher = Watcher { notify, roots: Vec::new(), core: Core::new(), shared, ticker: Some(ticker) };
        for root in roots {
            watcher.add_root(root);
        }
        watcher
    }

    /// Guarda `root` in modo ricorsivo. Le radici sono canoniche: FSEvents riporta percorsi reali.
    pub fn add_root(&mut self, root: PathBuf) {
        let root = root.canonicalize().unwrap_or(root);
        if self.roots.contains(&root) {
            return;
        }
        let Some(notify) = self.notify.as_mut() else { return };
        match notify.watch(&root, RecursiveMode::Recursive) {
            Ok(()) => self.roots.push(root),
            Err(e) => eprintln!("runemd: {}: non si può guardare: {e}", root.display()),
        }
    }

    fn drop_missing_roots(&mut self) {
        let Some(notify) = self.notify.as_mut() else { return };
        self.roots.retain(|root| {
            if root.is_dir() {
                return true;
            }
            eprintln!("runemd: {}: radice sparita, non più guardata", root.display());
            let _ = notify.unwatch(root);
            false
        });
    }

    /// Gli eventi pronti a `now` (debounce passato, tolleranza della rimozione scaduta).
    pub fn drain(&mut self, now: Instant) -> Vec<FsEvent> {
        let queued = {
            let mut queue = self.shared.queue.lock().unwrap();
            std::mem::take(&mut *queue)
        };
        for (path, at) in queued {
            self.core.observe(path, at);
        }
        self.drop_missing_roots();
        let out = self.core.drain(now);
        // Sotto lock: un evento arrivato ora non può perdere la sua sveglia.
        let queue = self.shared.queue.lock().unwrap();
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

    /// Fa girare `drain` finché passa `until` o scade la scadenza; raccoglie tutto.
    fn collect(w: &mut Watcher, window: Duration, until: impl Fn(&[FsEvent]) -> bool) -> Vec<FsEvent> {
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
        // Lascia che FSEvents parta prima di toccare i file.
        thread::sleep(Duration::from_millis(300));
        w.drain(Instant::now());
        (dir, root, w)
    }

    #[test]
    fn rename_atomic_keeps_file() {
        let (_dir, root, mut w) = setup();
        let target = root.join("x.md");
        fs::write(root.join("x.md.tmp"), "# a").unwrap();
        fs::rename(root.join("x.md.tmp"), &target).unwrap();
        let first = collect(&mut w, Duration::from_secs(5), |s| !s.is_empty());
        assert_eq!(first, vec![FsEvent::Changed(target.clone())]);
        // Per tutta la tolleranza non deve comparire un Removed.
        let rest = collect(&mut w, Duration::from_millis(2600), |_| false);
        assert!(rest.iter().all(|e| matches!(e, FsEvent::Changed(p) if *p == target)), "{rest:?}");
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

    // Logica con orologio finto.
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
        assert!(core.drain(at(60)).is_empty(), "mancante: in tolleranza");
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
    fn core_git_head_bypasses_filter_only_for_head() {
        let t0 = Instant::now();
        let mut core = Core::new();
        core.observe(PathBuf::from("/r/.git/HEAD"), t0);
        core.observe(PathBuf::from("/r/.git/config"), t0);
        assert_eq!(core.drain(t0 + Duration::from_millis(60)), vec![FsEvent::GitHead(PathBuf::from("/r"))]);
        assert!(!filter::wanted(Path::new("/r/.git/HEAD")));
    }
}
