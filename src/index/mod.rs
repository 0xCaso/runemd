//! L'indice dei Markdown nelle cartelle osservate: da qui leggono Recenti (task 16) e Cmd+P (task 18).
//!
//! All'avvio l'indice si carica dalla cache (`index.json` in Application Support), poi una
//! scansione su un thread a parte lo sostituisce con quello che c'è su disco; da lì in avanti lo
//! aggiornano gli eventi del watcher. Gli eventi arrivati mentre la scansione gira si riapplicano
//! sopra il suo risultato, così una scrittura avvenuta dopo che la scansione ha letto il file non
//! si perde.
//!
//! Le regole su cosa è un Markdown e quali cartelle saltare sono quelle del watcher
//! (`watch::filter`): `.git`, `node_modules`, `target` saltate; le altre cartelle nascoste,
//! `.claude/` compresa, entrano (spec, "Osservare i file": niente filtro `.gitignore`, i file
//! degli agenti sono proprio quelli da vedere).

pub mod cache;
pub mod scan;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime};

use rune::Waker;

use crate::watch::FsEvent;

pub use scan::scan_in_background;

/// Le cartelle osservate finché non arrivano le impostazioni (task 27), relative alla home.
pub const DEFAULT_ROOTS: [&str; 1] = ["Desktop/GitHub"];

/// Ogni quanto, al massimo, si riscrive la cache dopo un evento. La scansione all'avvio
/// ricostruisce comunque tutto: perdere gli ultimi secondi alla chiusura non costa nulla.
const SAVE_EVERY: Duration = Duration::from_secs(30);

/// Un Markdown dell'indice e quando è cambiato l'ultima volta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub mtime: SystemTime,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Index {
    /// Ordinate per percorso, senza doppioni.
    entries: Vec<Entry>,
    /// Solo i percorsi sotto queste cartelle entrano; `None` = nessun limite.
    roots: Option<Vec<PathBuf>>,
    /// Gli eventi applicati da quando è partita la scansione (`None` se nessuna è in corso):
    /// percorso e mtime letto (`None` = tolto).
    since_scan: Option<Vec<(PathBuf, Option<SystemTime>)>>,
}

/// L'mtime di `path` seguendo i link; `None` se non è (più) un file.
fn mtime_of(path: &Path) -> Option<SystemTime> {
    let meta = std::fs::metadata(path).ok()?;
    meta.is_file().then(|| meta.modified().ok()).flatten()
}

impl Index {
    /// Un indice con queste voci (in qualsiasi ordine; per un percorso doppio vale l'ultima).
    pub fn from_entries(entries: Vec<Entry>) -> Index {
        let mut index = Index::default();
        index.replace(entries);
        index
    }

    /// L'indice salvato in `path`; vuoto se manca o non si legge (la scansione lo riempie).
    pub fn load_cache(path: &Path) -> Index {
        Index::from_entries(cache::load(path).unwrap_or_default())
    }

    /// Scrive l'indice in `path` in modo atomico (file temporaneo e rename).
    pub fn save_cache(&self, path: &Path) {
        if let Err(e) = cache::save(path, &self.entries) {
            eprintln!("runemd: {}: indice non salvato: {e}", path.display());
        }
    }

    /// Limita l'indice ai percorsi sotto `roots` e toglie le voci che ne restano fuori.
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

    #[cfg_attr(not(test), allow(dead_code))] // per Recenti vuoto (task 16); oggi solo dai test
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Un evento del watcher. Vero se l'indice è cambiato.
    pub fn apply(&mut self, ev: &FsEvent) -> bool {
        let (path, mtime) = match ev {
            FsEvent::Changed(p) => (p, mtime_of(p)),
            FsEvent::Removed(p) => (p, None),
            FsEvent::GitHead(_) => return false,
        };
        if !self.covers(path) {
            return false;
        }
        if let Some(log) = &mut self.since_scan {
            log.push((path.clone(), mtime));
        }
        self.set(path, mtime)
    }

    /// Mette (`Some`) o toglie (`None`) la voce di `path`. Vero se è cambiato qualcosa.
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

    /// Una scansione parte: da qui gli eventi applicati si ricordano per riapplicarli sopra il
    /// suo risultato.
    pub fn begin_scan(&mut self) {
        self.since_scan = Some(Vec::new());
    }

    /// Il risultato della scansione sostituisce le voci; gli eventi arrivati nel frattempo
    /// valgono sopra di esso.
    pub fn finish_scan(&mut self, scanned: Vec<Entry>) {
        let log = self.since_scan.take().unwrap_or_default();
        self.replace(scanned);
        for (path, mtime) in log {
            self.set(&path, mtime);
        }
    }

    fn replace(&mut self, mut entries: Vec<Entry>) {
        entries.retain(|e| self.covers(&e.path));
        // Stabile: fra due voci dello stesso percorso resta l'ultima.
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        entries.reverse();
        entries.dedup_by(|a, b| a.path == b.path);
        entries.reverse();
        self.entries = entries;
    }

    /// Le `n` voci cambiate più di recente, dalla più nuova (a parità di mtime, per percorso).
    #[cfg_attr(not(test), allow(dead_code))] // letto da Recenti (task 16); oggi solo dai test
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

    /// Tutti i percorsi, in ordine alfabetico (Cmd+P, albero del repo).
    #[cfg_attr(not(test), allow(dead_code))] // letto da Cmd+P e dall'albero (task 16, 18); oggi solo dai test
    pub fn names(&self) -> impl Iterator<Item = &Path> {
        self.entries.iter().map(|e| e.path.as_path())
    }
}

/// Le cartelle osservate: `RUNEMD_ROOTS` (separate da `:`) se c'è, altrimenti `DEFAULT_ROOTS`
/// sotto la home. Canoniche, come i percorsi che riporta FSEvents.
pub fn roots() -> Vec<PathBuf> {
    let raw: Vec<PathBuf> = match std::env::var_os("RUNEMD_ROOTS") {
        Some(v) => std::env::split_paths(&v).filter(|p| !p.as_os_str().is_empty()).collect(),
        None => {
            let Some(home) = std::env::var_os("HOME") else { return Vec::new() };
            DEFAULT_ROOTS.iter().map(|r| PathBuf::from(&home).join(r)).collect()
        }
    };
    raw.into_iter().map(|p| p.canonicalize().unwrap_or(p)).collect()
}

/// Dove si salva l'indice: `~/Library/Application Support/RuneMD/index.json` (sotto
/// `RUNE_DIRS_ROOT` se impostata, per le prove).
pub fn cache_path() -> Option<PathBuf> {
    rune::dirs::AppDirs::new("RuneMD").map(|d| d.data().join("index.json"))
}

/// L'indice dell'app: cache, scansione in background, eventi del watcher, salvataggi.
pub struct Library {
    pub index: Index,
    roots: Vec<PathBuf>,
    cache: Option<PathBuf>,
    scan: Option<Receiver<Vec<Entry>>>,
    /// La scansione è finita: l'indice è quello del disco (non solo della cache).
    pub scanned: bool,
    /// Cambiato dall'ultimo salvataggio.
    dirty: bool,
    saved_at: Option<Instant>,
    saving: Option<JoinHandle<()>>,
}

impl Library {
    /// Carica la cache (se c'è) limitata a `roots`. Non scandisce: quello lo fa `start`.
    pub fn load(roots: Vec<PathBuf>, cache: Option<PathBuf>) -> Library {
        let mut index = cache.as_deref().map(Index::load_cache).unwrap_or_default();
        index.set_roots(roots.clone());
        Library { index, roots, cache, scan: None, scanned: false, dirty: false, saved_at: None, saving: None }
    }

    /// Le cartelle osservate.
    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    /// Fa partire la scansione su un thread a parte; il thread sveglia l'app quando ha finito.
    pub fn start(&mut self, waker: Waker) {
        self.index.begin_scan();
        self.scan = Some(scan_in_background(self.roots.clone(), waker));
    }

    /// Da chiamare a ogni `poll` con gli eventi del watcher di questo risveglio. Raccoglie la
    /// scansione finita, applica gli eventi e salva la cache quando serve. Vero se l'indice è
    /// cambiato.
    pub fn poll(&mut self, events: &[FsEvent], now: Instant) -> bool {
        let mut changed = false;
        for ev in events {
            changed |= self.index.apply(ev);
        }
        match self.scan.as_ref().map(Receiver::try_recv) {
            Some(Ok(entries)) => {
                self.scan = None;
                self.index.finish_scan(entries);
                self.scanned = true;
                changed = true;
                // Subito dopo la scansione, così il prossimo avvio parte da qui.
                self.saved_at = None;
            }
            // Il thread è morto senza risultato: resta la cache, aggiornata dagli eventi.
            Some(Err(TryRecvError::Disconnected)) => {
                self.scan = None;
                self.index.since_scan = None;
            }
            Some(Err(TryRecvError::Empty)) | None => {}
        }
        self.dirty |= changed;
        if self.dirty && self.saved_at.is_none_or(|t| now.saturating_duration_since(t) >= SAVE_EVERY) {
            self.save(now);
        }
        changed
    }

    /// Salva su un thread a parte una copia dell'indice; uno alla volta.
    fn save(&mut self, now: Instant) {
        let Some(path) = self.cache.clone() else { return };
        if self.saving.as_ref().is_some_and(|h| !h.is_finished()) {
            return;
        }
        let index = Index::from_entries(self.index.entries.clone());
        self.saving = Some(std::thread::spawn(move || index.save_cache(&path)));
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
        assert_eq!(index.recent(1)[0].mtime, at(2), "per un percorso doppio vale l'ultima voce");
    }

    #[test]
    fn apply_removed_drops_the_entry() {
        let mut index = Index::from_entries(vec![entry("/r/a.md", 1), entry("/r/b.md", 2)]);
        assert!(index.apply(&FsEvent::Removed(PathBuf::from("/r/a.md"))));
        assert_eq!(index.names().collect::<Vec<_>>(), [Path::new("/r/b.md")]);
        assert!(!index.apply(&FsEvent::Removed(PathBuf::from("/r/a.md"))), "già tolta");
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
        assert!(!index.apply(&FsEvent::Changed(file.clone())), "stesso mtime: niente da cambiare");
        let new = dir.path().join("b.md");
        fs::write(&new, "# b").unwrap();
        assert!(index.apply(&FsEvent::Changed(new.clone())));
        assert_eq!(index.len(), 2);
        // Un Changed per un file già sparito lo toglie.
        fs::remove_file(&new).unwrap();
        assert!(index.apply(&FsEvent::Changed(new)));
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn apply_ignores_paths_outside_the_roots_and_git_head() {
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().join("fuori.md");
        fs::write(&outside, "x").unwrap();
        let mut index = Index::default();
        index.set_roots(vec![PathBuf::from("/r")]);
        assert!(!index.apply(&FsEvent::Changed(outside)));
        assert!(!index.apply(&FsEvent::GitHead(PathBuf::from("/r"))));
        assert!(index.is_empty());
    }

    #[test]
    fn set_roots_drops_entries_outside() {
        let mut index = Index::from_entries(vec![entry("/r/a.md", 1), entry("/rr/b.md", 1), entry("/s/c.md", 1)]);
        index.set_roots(vec![PathBuf::from("/r")]);
        assert_eq!(index.names().collect::<Vec<_>>(), [Path::new("/r/a.md")], "`/rr` non sta sotto `/r`");
    }

    /// Un evento arrivato mentre la scansione gira vale sopra il suo risultato: la scansione
    /// può aver letto il file prima della scrittura, o prima che sparisse.
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
        index.finish_scan(vec![Entry { path: fresh.clone(), mtime: at(1) }, Entry { path: gone, mtime: at(1) }]);
        assert_eq!(index.recent(10), [&Entry { path: fresh.clone(), mtime }]);
        // Finita la scansione, gli eventi non si ricordano più.
        index.apply(&FsEvent::Removed(fresh));
        index.finish_scan(vec![entry("/r/x.md", 1)]);
        assert_eq!(index.len(), 1);
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
        let entries = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(entries.iter().map(|e| &e.path).collect::<Vec<_>>(), [&root.join("a.md")]);
        woke.recv_timeout(Duration::from_secs(5)).expect("sveglia");
    }

    #[test]
    fn cache_saved_and_reloaded_is_equal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("RuneMD/index.json");
        // Nanosecondi compresi: l'mtime deve tornare identico.
        let index = Index::from_entries(vec![
            Entry { path: PathBuf::from("/r/a.md"), mtime: SystemTime::UNIX_EPOCH + Duration::new(1_763_000_000, 123_456_789) },
            entry("/r/àccento con spazi.md", 2),
        ]);
        index.save_cache(&path);
        assert_eq!(Index::load_cache(&path), index);
        assert!(fs::read_dir(path.parent().unwrap()).unwrap().count() == 1, "nessun file temporaneo rimasto");
    }

    #[test]
    fn a_missing_or_broken_cache_loads_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("index.json");
        assert!(Index::load_cache(&path).is_empty());
        fs::write(&path, "{ rotto").unwrap();
        assert!(Index::load_cache(&path).is_empty());
    }

    /// Misura (`docs/perf.md`): scansione e ricarica da cache sulle cartelle osservate vere.
    /// `cargo test --profile perf scan_cost -- --ignored --nocapture` (cache in una cartella
    /// temporanea, mai quella dell'app).
    #[test]
    #[ignore]
    fn scan_cost() {
        let roots = roots();
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
                "scan_cost giro {round}: {} voci, scansione {scan_ms:.1} ms, salvataggio {save_ms:.1} ms, ricarica {load_ms:.1} ms, recent(20)={recent} {recent_ms:.2} ms, cache {} KB",
                index.len(),
                fs::metadata(&cache).unwrap().len() / 1024
            );
        }
        // Quante voci stanno in cartelle nascoste diverse da `.claude` (spec: entrano).
        let index = Index::from_entries(scan::scan(&roots));
        let mut hidden: std::collections::BTreeMap<String, usize> = Default::default();
        for p in index.names() {
            let rel = roots.iter().find_map(|r| p.strip_prefix(r).ok()).unwrap();
            if let Some(c) = rel.components().find(|c| c.as_os_str().to_string_lossy().starts_with('.')) {
                *hidden.entry(c.as_os_str().to_string_lossy().into_owned()).or_default() += 1;
            }
        }
        eprintln!("scan_cost cartelle nascoste: {hidden:?}");
    }

    /// L'app: cache all'avvio, scansione che la sostituisce, eventi, cache riscritta.
    #[test]
    fn library_loads_cache_then_scan_then_saves() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let cache = dir.path().join("support/index.json");
        Index::from_entries(vec![Entry { path: root.join("old.md"), mtime: at(1) }, entry("/altrove/x.md", 1)]).save_cache(&cache);
        fs::write(root.join("new.md"), "x").unwrap();
        let mut lib = Library::load(vec![root.clone()], Some(cache.clone()));
        assert_eq!(lib.index.names().collect::<Vec<_>>(), [root.join("old.md").as_path()], "dalla cache, solo sotto le radici");
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
