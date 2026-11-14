//! Un solo stream FSEvents per tutto: schede, barra laterale e ricarica leggono da qui.
//!
//! Il callback di `notify` mette in coda i percorsi grezzi; `drain` li filtra, li raggruppa (50 ms)
//! e decide per ogni percorso guardando il disco, non il tipo di evento: FSEvents consegna
//! rinomina come due eventi o flag fusi. Un file mancante diventa `Removed` solo se dopo 2 s non
//! è tornato.

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
/// Quanto un file mancante può restare tale prima di contare come rimosso.
const REMOVED_GRACE: Duration = Duration::from_secs(2);
/// Ogni quanto il ticker sveglia l'app finché ci sono eventi in attesa.
const TICK: Duration = Duration::from_millis(25);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsEvent {
    Changed(PathBuf),
    Removed(PathBuf),
    /// `.git/HEAD`, `ORIG_HEAD` o `FETCH_HEAD` è cambiato: la radice del repo.
    GitHead(PathBuf),
    /// Sotto questa cartella i file possono essere cambiati senza un evento per ognuno: una
    /// cartella spostata, rinominata, creata o tolta (FSEvents riporta solo la cartella), o
    /// eventi persi sotto una radice ricorsiva. Chi tiene un elenco dei file la riscandisce (fuori
    /// dal thread della UI); se la cartella non c'è più, ciò che stava sotto è sparito.
    Rescan(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum Key {
    File(PathBuf),
    Head(PathBuf),
    Dir(PathBuf),
}

/// La radice del repo se `path` è `<repo>/.git/HEAD`, `ORIG_HEAD` o `FETCH_HEAD`: checkout
/// scrive `HEAD`; pull e `reset --hard` spostano solo i ref, ma scrivono `ORIG_HEAD` e
/// `FETCH_HEAD`. Per l'app sono tutti "il repo ha cambiato HEAD" (`FsEvent::GitHead`).
///
/// In un worktree `.git` è un file (`gitdir: ...`) e HEAD sta in
/// `<repo principale>/.git/worktrees/<nome>/HEAD`: la radice è la cartella del worktree, letta
/// dal file `gitdir` accanto (che punta al `.git` del worktree). Limite noto: nei submodule HEAD
/// sta in `.git/modules/<m>/HEAD` e non esce `GitHead`.
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

/// La logica senza thread né notify, per provarla con un orologio finto.
struct Core {
    debounce: Debounce<Key>,
    /// File mancanti e da quando.
    missing: HashMap<PathBuf, Instant>,
    /// File visti esistere (uscito `Changed`): da ricontrollare dopo un rescan.
    known: HashSet<PathBuf>,
    /// Rescan chiesti e non ancora eseguiti: tanti si accorpano in uno.
    rescan_at: Option<Instant>,
}

impl Core {
    fn new() -> Self {
        Core { debounce: Debounce::new(DEBOUNCE_MS), missing: HashMap::new(), known: HashSet::new(), rescan_at: None }
    }

    fn observe(&mut self, path: PathBuf, at: Instant) {
        // `.git/HEAD` prima del filtro: `wanted` esclude tutto ciò che sta sotto `.git`.
        if let Some(root) = git_head_root(&path) {
            self.debounce.push(Key::Head(root), at);
        } else if filter::wanted(&path) {
            self.debounce.push(Key::File(path), at);
        }
    }

    /// Una cartella spostata, creata o tolta: esce come `Rescan` dopo il debounce.
    fn observe_dir(&mut self, path: PathBuf, at: Instant) {
        self.debounce.push(Key::Dir(path), at);
    }

    fn request_rescan(&mut self, at: Instant) {
        self.rescan_at = Some(self.rescan_at.map_or(at, |old| old.max(at)));
    }

    fn take_rescan(&mut self) -> Option<Instant> {
        self.rescan_at.take()
    }

    /// Notify ha perso eventi: ricontrolla ogni percorso noto e i figli diretti delle radici
    /// non ricorsive. Una radice ricorsiva non si percorre qui (sul thread della UI sarebbe una
    /// scansione intera, 0,6–2,8 s su `~/Desktop/GitHub`): esce un `Rescan` della radice e chi
    /// tiene l'elenco dei file la riscandisce su un thread a parte.
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

/// I file `wanted` in `dir`, e sotto se `recursive`; errori di lettura ignorati. Non entra nelle
/// cartelle saltate né nei link simbolici a cartelle (un ciclo non finirebbe mai).
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

/// Una cartella guardata: con le sottocartelle (`recursive`) o solo i suoi figli diretti.
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

/// Cosa mettere in coda per un percorso di un evento di notify; `None` se non interessa. Gira
/// sul thread di notify, così i percorsi di una build (`target/`, `node_modules/`, `.git/objects`)
/// non tengono sveglia l'app e lo stat di una rinomina non pesa sulla UI.
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
        // Una rinomina non dice se era una cartella: lo è se ora c'è una cartella o se non c'è
        // più niente (il lato vecchio). Per un file non Markdown sparito `Rescan` non costa:
        // sotto non c'è nulla.
        EventKind::Modify(ModifyKind::Name(_)) => std::fs::symlink_metadata(&path).map_or(true, |m| m.is_dir()),
        _ => false,
    };
    dir.then_some(Raw::Dir(path, at))
}

#[derive(Default)]
struct Shared {
    queue: Mutex<Vec<Raw>>,
    /// Vero se qualcosa attende il debounce o la tolleranza: il ticker sveglia l'app.
    busy: AtomicBool,
    stop: AtomicBool,
}

pub struct Watcher {
    notify: Option<RecommendedWatcher>,
    roots: Vec<Root>,
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
                let now = Instant::now();
                let event = match event {
                    Ok(event) => event,
                    Err(e) => {
                        eprintln!("runemd: errore del watcher: {e}");
                        // Un errore può voler dire eventi persi: ricontrolla dal disco.
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
                eprintln!("runemd: watcher non avviato: {e}");
                None
            }
        };
        let mut watcher = Watcher { notify, roots: Vec::new(), core: Core::new(), shared, ticker: Some(ticker) };
        for root in roots {
            watcher.add_root(root, true);
        }
        watcher
    }

    /// Guarda `root`, con le sottocartelle se `recursive`, altrimenti solo i suoi figli diretti
    /// (notify scarta gli eventi più in basso). Una radice già guardata senza sottocartelle
    /// diventa ricorsiva se lo si chiede. Una radice non ricorsiva sotto una ricorsiva già
    /// guardata non si aggiunge: i suoi eventi arrivano già, e un secondo percorso sullo stesso
    /// stream li farebbe arrivare due volte. Le radici sono canoniche: FSEvents riporta percorsi reali.
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
            Err(e) => eprintln!("runemd: {}: non si può guardare: {e}", root.display()),
        }
    }

    /// Smette di guardare `root` se è una radice senza sottocartelle (la cartella di un file
    /// aperto, task 19). Le cartelle osservate (ricorsive) restano: le usano l'indice e la sidebar.
    pub fn remove_root(&mut self, root: &Path) {
        let root = root.canonicalize().unwrap_or_else(|_| root.to_owned());
        let Some(i) = self.roots.iter().position(|r| r.path == root && !r.recursive) else { return };
        if let Some(notify) = self.notify.as_mut() {
            let _ = notify.unwatch(&root);
        }
        self.roots.remove(i);
    }

    fn drop_missing_roots(&mut self) {
        let Some(notify) = self.notify.as_mut() else { return };
        self.roots.retain(|root| {
            if root.path.is_dir() {
                return true;
            }
            eprintln!("runemd: {}: radice sparita, non più guardata", root.path.display());
            let _ = notify.unwatch(&root.path);
            false
        });
    }

    /// Gli eventi pronti a `now` (debounce passato, tolleranza della rimozione scaduta).
    ///
    /// Il chiamante deve chiamarla a ogni risveglio: il watcher sveglia l'app finché c'è
    /// qualcosa in attesa e si ferma quando `drain` ha svuotato tutto.
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
        // Una sola passata su disco per drain, quanti che siano i rescan in coda.
        if let Some(at) = self.core.take_rescan() {
            self.core.rescan(&self.roots, at);
        }
        self.drop_missing_roots();
        let out = self.core.drain(now);
        // Sotto lock: un evento arrivato ora non può perdere la sua sveglia.
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

    /// Fa girare `drain` finché passa `until` o scade la scadenza; raccoglie tutto.
    fn collect(w: &mut Watcher, window: Duration, until: impl Fn(&[FsEvent]) -> bool) -> Vec<FsEvent> {
        // La sentinella `ready.md` può avere eventi in ritardo: non fa parte delle asserzioni.
        let mut seen = collect_raw(w, window, |s| until(&without_sentinel(s)));
        seen = without_sentinel(&seen);
        seen
    }

    /// Toglie anche i `Rescan`: il lato vecchio di una rinomina atomica (`x.md.tmp`) ne dà uno,
    /// e queste prove guardano i file.
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
        // Sentinella: quando arriva il suo Changed, FSEvents è partito.
        let ready = root.join("ready.md");
        fs::write(&ready, "ready").unwrap();
        let seen = collect_raw(&mut w, Duration::from_secs(5), |s| s.contains(&FsEvent::Changed(ready.clone())));
        assert!(seen.contains(&FsEvent::Changed(ready)), "watcher non pronto");
        (dir, root, w)
    }

    /// Crea `path` e aspetta il suo `Changed`, così il test parte da un file già visto.
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
        // Per tutta la tolleranza non deve comparire un Removed.
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
        // Eventi persi: `gone` sparisce, `fresh` nasce, nessun evento arriva.
        fs::remove_file(&gone).unwrap();
        fs::write(&fresh, "x").unwrap();
        core.rescan(&[Root { path: root.clone(), recursive: false }], at(100));
        let mut first = core.drain(at(200));
        first.sort_by_key(|e| format!("{e:?}"));
        assert_eq!(first, vec![FsEvent::Changed(fresh), FsEvent::Changed(kept)]);
        assert_eq!(core.drain(at(2300)), vec![FsEvent::Removed(gone)]);
    }

    /// Una radice ricorsiva non si percorre sul thread di chi chiama `drain`: esce un `Rescan`
    /// della radice e basta. I percorsi noti si ricontrollano ancora.
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
        assert_eq!(out, vec![FsEvent::Changed(kept), FsEvent::Rescan(root)], "`deep.md` non letto qui");
    }

    /// Misura (`docs/perf.md`): il costo sul thread della UI di un rescan con le cartelle
    /// osservate vere (`RUNEMD_ROOTS` o `~/Desktop/GitHub`), radici ricorsive.
    /// `cargo test --profile perf rescan_ui_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn rescan_ui_cost() {
        let roots: Vec<Root> = crate::index::roots().into_iter().map(|path| Root { path, recursive: true }).collect();
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

    /// Il filtro sul thread di notify: i file di una build non entrano in coda; i Markdown,
    /// `.git/HEAD` e le cartelle spostate sì.
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
        // Rinomina: il lato che non c'è più conta come cartella; un file esistente no.
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
        assert_eq!(core.take_rescan(), None, "una sola passata");
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
        assert!(seen.contains(&FsEvent::Changed(ready)), "watcher non pronto");
        fs::write(root.join("sub/x.md"), "# sotto").unwrap();
        fs::write(root.join("top.md"), "# sopra").unwrap();
        let events = collect(&mut w, Duration::from_secs(5), |s| s.contains(&FsEvent::Changed(root.join("top.md"))));
        assert!(events.contains(&FsEvent::Changed(root.join("top.md"))), "{events:?}");
        let rest = collect(&mut w, Duration::from_millis(300), |_| false);
        assert!(!events.iter().chain(&rest).any(|e| matches!(e, FsEvent::Changed(p) if p.starts_with(root.join("sub")))), "{events:?} {rest:?}");
    }

    /// La cartella del file aperto dentro una radice ricorsiva non diventa una seconda radice.
    #[test]
    fn a_non_recursive_root_under_a_recursive_one_is_not_added() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::create_dir_all(root.join("repo/.git")).unwrap();
        let mut w = Watcher::start(vec![root.clone()], waker());
        w.add_root(root.join("repo"), false);
        w.add_root(root.join("repo/.git"), false);
        assert_eq!(w.roots, vec![Root { path: root.clone(), recursive: true }]);
        // Fuori dalla radice ricorsiva sì.
        let other = tempfile::tempdir().unwrap();
        w.add_root(other.path().to_path_buf(), false);
        assert_eq!(w.roots.len(), 2);
    }

    /// Una radice senza sottocartelle si toglie; una ricorsiva (cartella osservata) resta.
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

    /// Pull e `reset --hard` non toccano `.git/HEAD` ma `ORIG_HEAD` e `FETCH_HEAD`: anche
    /// quelli sono un cambio di HEAD del repo. Gli altri file di `.git` no.
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

    /// In un worktree HEAD sta nel repo principale: l'evento porta la radice del worktree,
    /// letta dal file `gitdir`. Una cartella `worktrees` fuori da `.git` non conta.
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
        assert_eq!(git_head_root(&admin.join("HEAD")), Some(wt), "gitdir relativo");
        assert_eq!(git_head_root(&base.join("x/worktrees/wt/HEAD")), None);
        assert_eq!(git_head_root(&base.join("main/.git/worktrees/missing/HEAD")), None, "senza gitdir");
    }
}
