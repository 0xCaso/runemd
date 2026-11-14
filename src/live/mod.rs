//! Lettura live: il file aperto si ricarica quando un agente lo scrive.
//!
//! `Live` tiene il watcher (task 9) e decide quando rileggere il file aperto: a ogni `Changed`
//! del suo percorso e, dopo una lettura sospetta (file a metà scrittura), ancora una volta dopo
//! `SETTLE`. La lettura e la preparazione dell'aggiornamento (blocchi, hash, diff con id stabili)
//! girano su un thread a parte (`reload::read`); sul thread della UI resta `reload::commit`, che
//! sostituisce i blocchi. Il thread sveglia l'app con il `Waker` quando ha finito.
//!
//! Il watcher guarda con le sottocartelle le cartelle osservate (`index::roots`, task 14). Per
//! il file aperto si aggiungono la sua cartella, senza sottocartelle (un file in `~` non deve far
//! guardare tutta la home), e il `.git` del suo repo, per i cambi di `.git/HEAD` che spengono
//! "sta scrivendo" (`activity`); se stanno già sotto una cartella osservata il watcher non le
//! aggiunge (`Watcher::add_root`), e ogni evento arriva una volta sola.
//!
//! `Live` svuota il watcher a ogni `poll`; gli eventi di quel risveglio restano in `drained` per
//! chi altro li legge (l'indice).

pub mod activity;
pub mod arriving;
pub mod below;
pub mod reload;

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant, SystemTime};

use rune::Waker;

use crate::doc::document::{Change, Document};
use crate::watch::{FsEvent, Watcher};

use activity::Activity;
use reload::{Commit, OwnWrites, Read};

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
    /// "Sta scrivendo": solo le ricariche esterne applicate lo accendono.
    activity: Activity,
    /// Il file aperto come lo riporta FSEvents (canonico).
    watched: Option<PathBuf>,
    /// L'hash di una lettura sospetta saltata: se la rilettura dà lo stesso, si applica.
    held: Option<u64>,
    recheck_at: Option<Instant>,
    /// Una rilettura da fare: `Some(true)` se è quella a tempo dopo una lettura sospetta.
    want: Option<bool>,
    /// La lettura in corso sul thread a parte.
    job: Option<Job>,
    waker: Option<Waker>,
    /// Gli eventi dell'ultimo `poll`, per l'indice.
    drained: Vec<FsEvent>,
    /// Nei test la lettura si aspetta subito, così l'orologio finto decide da solo.
    #[cfg(test)]
    sync: bool,
}

struct Job {
    rx: Receiver<io::Result<Read>>,
    timed: bool,
}

/// La radice del repo di `path`: la cartella più vicina, sopra il file, che contiene `.git/`.
/// Worktree e submodule (`.git` file) non si riconoscono: stesso limite del watcher.
fn repo_of(path: &Path) -> Option<PathBuf> {
    path.ancestors().skip(1).find(|d| d.join(".git").is_dir()).map(Path::to_path_buf)
}

/// Guarda la cartella di `path` e il `.git` del suo repo (per `.git/HEAD`), senza sottocartelle.
fn watch_roots(watcher: &mut Watcher, path: &Path) {
    if let Some(root) = root_of(path) {
        watcher.add_root(root, false);
    }
    if let Some(repo) = repo_of(path) {
        watcher.add_root(repo.join(".git"), false);
    }
}

/// La cartella da guardare per `path` (non ricorsiva).
fn root_of(path: &Path) -> Option<PathBuf> {
    path.parent().map(|p| if p.as_os_str().is_empty() { PathBuf::from(".") } else { p.to_owned() })
}

impl Live {
    /// Avvia il watcher (da `AppSpec::setup`, che ha il waker) sulle cartelle osservate `roots`,
    /// con le sottocartelle, e guarda il file già aperto.
    pub fn start(&mut self, waker: Waker, roots: Vec<PathBuf>) {
        let mut watcher = Watcher::start(roots, waker.clone());
        if let Some(path) = self.watched.as_deref() {
            watch_roots(&mut watcher, path);
        }
        self.watcher = Some(watcher);
        self.waker = Some(waker);
    }

    /// Il file aperto ora è `path`: si guarda la sua cartella e si dimentica lo stato del
    /// file di prima, lettura in corso compresa (era per l'altro documento).
    pub fn watch(&mut self, path: &Path) {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_owned());
        if let Some(w) = self.watcher.as_mut() {
            watch_roots(w, &canonical);
        }
        self.watched = Some(canonical);
        self.held = None;
        self.recheck_at = None;
        self.want = None;
        self.job = None;
    }

    /// Da chiamare a ogni `poll`: svuota il watcher (va fatto a ogni risveglio), raccoglie la
    /// lettura finita e ne avvia una se il file aperto è cambiato. Ritorna il cambio applicato e,
    /// se serve, fra quanto richiamarla.
    pub fn poll(&mut self, doc: Option<&mut Document>, now: Instant) -> (Option<Reloaded>, Option<Duration>) {
        let events = self.watcher.as_mut().map(|w| w.drain(now)).unwrap_or_default();
        // Un `Rescan` sopra il file aperto (eventi persi, cartella spostata): si rilegge, se il
        // contenuto è lo stesso la ricarica non cambia nulla.
        let changed = events.iter().any(|e| match e {
            FsEvent::Changed(p) => self.watched.as_ref() == Some(p),
            FsEvent::Rescan(dir) => self.watched.as_ref().is_some_and(|w| w.starts_with(dir)),
            _ => false,
        });
        self.note(&events, now);
        self.drained = events;
        self.step(doc, changed, now)
    }

    /// Gli eventi del watcher svuotati dall'ultimo `poll`.
    pub fn drained(&self) -> &[FsEvent] {
        &self.drained
    }

    /// Fra quanto il file aperto smette di essere "sta scrivendo"; `None` se non lo è.
    pub fn writing_left(&self, now: Instant) -> Option<Duration> {
        self.activity.left(self.watched.as_deref()?, now)
    }

    /// Gli eventi che non sono ricariche: un cambio di `.git/HEAD`.
    fn note(&mut self, events: &[FsEvent], now: Instant) {
        for e in events {
            if let FsEvent::GitHead(repo) = e {
                self.activity.git_head(repo, now);
            }
        }
    }

    /// `poll` senza il watcher: `changed` dice se è arrivato un `Changed` del file aperto.
    fn step(&mut self, doc: Option<&mut Document>, changed: bool, now: Instant) -> (Option<Reloaded>, Option<Duration>) {
        if self.recheck_at.is_some_and(|t| now >= t) {
            self.recheck_at = None;
            self.want = Some(true);
        } else if changed {
            self.want = Some(self.want.unwrap_or(false));
        }
        let Some(doc) = doc else { return (None, self.wait(now)) };
        let out = self.collect(doc, now);
        if self.job.is_none()
            && let Some(timed) = self.want.take()
        {
            self.spawn(doc, timed);
        }
        #[cfg(test)]
        let out = if self.sync && out.is_none() { self.collect(doc, now) } else { out };
        (out, self.wait(now))
    }

    fn wait(&self, now: Instant) -> Option<Duration> {
        self.recheck_at.map(|t| t.saturating_duration_since(now))
    }

    /// Legge il file su un thread a parte, a partire dai blocchi di adesso.
    fn spawn(&mut self, doc: &Document, timed: bool) {
        let (tx, rx) = mpsc::channel();
        let (path, snap, waker) = (doc.path.clone(), doc.snapshot(), self.waker.clone());
        std::thread::spawn(move || {
            let _ = tx.send(reload::read(&path, &snap));
            if let Some(wake) = waker {
                wake();
            }
        });
        self.job = Some(Job { rx, timed });
    }

    /// La lettura finita, se c'è, decisa e applicata.
    fn collect(&mut self, doc: &mut Document, now: Instant) -> Option<Reloaded> {
        let job = self.job.take()?;
        #[cfg(test)]
        let polled = if self.sync { job.rx.recv().map_err(|_| TryRecvError::Disconnected) } else { job.rx.try_recv() };
        #[cfg(not(test))]
        let polled = job.rx.try_recv();
        match polled {
            Ok(Ok(read)) => self.settle(doc, read, job.timed, now),
            // File assente o illeggibile durante un rename: il watcher lo riporta quando torna.
            Ok(Err(_)) | Err(TryRecvError::Disconnected) => None,
            Err(TryRecvError::Empty) => {
                self.job = Some(job);
                None
            }
        }
    }

    fn settle(&mut self, doc: &mut Document, read: Read, timed: bool, now: Instant) -> Option<Reloaded> {
        if read.suspect {
            // Una lettura sospetta vale solo alla rilettura a tempo, uguale a quella trattenuta.
            // Prima, anche un altro `Changed` con lo stesso contenuto aspetta: la scrittura può
            // non essere finita.
            if !(timed && self.held == Some(read.hash)) {
                if self.held != Some(read.hash) || self.recheck_at.is_none() {
                    self.held = Some(read.hash);
                    self.recheck_at = Some(now + SETTLE);
                }
                return None;
            }
        }
        self.held = None;
        self.recheck_at = None;
        let written = read.written;
        match reload::commit(doc, read, &self.own) {
            Commit::Applied(change) => {
                // L'unico punto che accende "sta scrivendo": mai per le scritture dell'app, le
                // letture saltate o quelle sospette trattenute (ritornate sopra).
                if let Some(path) = &self.watched {
                    self.activity.touch(path, now);
                }
                Some(Reloaded { at: now, written, change })
            }
            Commit::Skipped => None,
            Commit::Stale => {
                self.want = Some(self.want.unwrap_or(false));
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
        live.start(Arc::new(|| {}), Vec::new());
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

    /// Il file aperto sta in una sottocartella di una cartella osservata (ricorsiva): una
    /// scrittura lo ricarica una volta sola, e l'evento resta in `drained` per l'indice.
    #[test]
    fn a_file_under_a_watched_root_reloads_once() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        fs::create_dir_all(root.join("repo/docs")).unwrap();
        let path = root.join("repo/docs/a.md");
        fs::write(&path, "# a\n").unwrap();
        let mut live = Live::default();
        live.watch(&path);
        live.start(Arc::new(|| {}), vec![root.clone()]);
        let mut doc = Document::load(&path).unwrap();
        thread::sleep(Duration::from_millis(300));
        fs::write(&path, "# a\n\nb\n").unwrap();
        let mut seen = Vec::new();
        let mut reloads = 0;
        let mut deadline = Instant::now() + Duration::from_secs(4);
        while Instant::now() < deadline {
            if live.poll(Some(&mut doc), Instant::now()).0.is_some() {
                reloads += 1;
                // Dopo la prima, ancora 500 ms per vedere un'eventuale seconda ricarica.
                deadline = deadline.min(Instant::now() + Duration::from_millis(500));
            }
            seen.extend(live.drained().iter().cloned());
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

    /// Troncamento, poi un secondo `Changed` 60 ms dopo con lo stesso file vuoto: niente si
    /// applica prima della rilettura a 200 ms; il contenuto pieno che torna prima tiene gli id.
    #[test]
    fn a_second_changed_does_not_skip_the_settle_wait() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let long: String = (0..200).map(|n| format!("Paragrafo {n} di prova.\n\n")).collect();
        fs::write(&path, &long).unwrap();
        let mut doc = Document::load(&path).unwrap();
        let ids: Vec<u64> = doc.blocks.iter().map(|b| b.id).collect();
        let mut live = Live { sync: true, ..Live::default() };
        live.watch(&path);
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        fs::write(&path, "").unwrap();
        let (r, wait) = live.step(Some(&mut doc), true, at(0));
        assert!(r.is_none());
        assert_eq!(wait, Some(SETTLE));
        fs::write(&path, "").unwrap();
        let (r, wait) = live.step(Some(&mut doc), true, at(60));
        assert!(r.is_none(), "secondo Changed uguale prima della rilettura");
        assert_eq!(wait, Some(Duration::from_millis(140)), "la rilettura resta a 200 ms");
        assert!(live.step(Some(&mut doc), false, at(150)).0.is_none());
        assert_eq!(doc.blocks.len(), 200);
        let full = format!("{long}Nuovo in fondo.\n");
        fs::write(&path, &full).unwrap();
        let (r, _) = live.step(Some(&mut doc), true, at(180));
        assert_eq!(r.expect("contenuto pieno").change.added.len(), 1);
        assert_eq!(doc.blocks[..200].iter().map(|b| b.id).collect::<Vec<_>>(), ids, "id tenuti");
        assert!(live.step(Some(&mut doc), false, at(400)).0.is_none(), "nessuna rilettura in sospeso");
    }

    /// Un troncamento vero si applica alla rilettura a tempo, non prima.
    #[test]
    fn a_held_truncation_applies_only_on_the_timed_recheck() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let long: String = (0..200).map(|n| format!("Paragrafo {n} di prova.\n\n")).collect();
        fs::write(&path, &long).unwrap();
        let mut doc = Document::load(&path).unwrap();
        let mut live = Live { sync: true, ..Live::default() };
        live.watch(&path);
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        fs::write(&path, "").unwrap();
        assert!(live.step(Some(&mut doc), true, at(0)).0.is_none());
        assert!(live.step(Some(&mut doc), true, at(199)).0.is_none());
        assert!(live.step(Some(&mut doc), false, at(200)).0.is_some(), "alla rilettura a tempo");
        assert!(doc.blocks.is_empty());
    }

    fn sync_live(path: &Path) -> (Live, Document) {
        let doc = Document::load(path).unwrap();
        let mut live = Live { sync: true, ..Live::default() };
        live.watch(path);
        (live, doc)
    }

    /// Solo una ricarica esterna applicata accende "sta scrivendo": la rilettura di una
    /// scrittura dell'app no.
    #[test]
    fn own_writes_do_not_touch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n").unwrap();
        let (mut live, mut doc) = sync_live(&path);
        let t0 = Instant::now();
        // Stesso contenuto (un `touch` senza modifiche): lettura saltata, niente anello.
        fs::write(&path, "# a\n").unwrap();
        assert!(live.step(Some(&mut doc), true, t0).0.is_none());
        assert_eq!(live.writing_left(t0), None, "lettura saltata");
        let mine = "# a\n\nscritto dall'app\n";
        live.own.remember(&doc.path, crate::doc::ids::hash_str(mine));
        fs::write(&path, mine).unwrap();
        assert!(live.step(Some(&mut doc), true, t0).0.is_none());
        assert_eq!(live.writing_left(t0), None, "scrittura dell'app");
        fs::write(&path, "# a\n\ndell'agente\n").unwrap();
        assert!(live.step(Some(&mut doc), true, t0).0.is_some());
        assert_eq!(live.writing_left(t0), Some(activity::WRITING_WINDOW), "scrittura dell'agente");
    }

    /// Una lettura sospetta trattenuta non accende l'anello; lo accende quando si applica.
    #[test]
    fn a_held_suspect_read_does_not_touch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let long: String = (0..200).map(|n| format!("Paragrafo {n} di prova.\n\n")).collect();
        fs::write(&path, &long).unwrap();
        let (mut live, mut doc) = sync_live(&path);
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        fs::write(&path, "").unwrap();
        assert!(live.step(Some(&mut doc), true, at(0)).0.is_none());
        assert_eq!(live.writing_left(at(0)), None, "trattenuta");
        assert!(live.step(Some(&mut doc), false, at(200)).0.is_some());
        assert!(live.writing_left(at(200)).is_some(), "applicata alla rilettura");
    }

    /// Un cambio di `.git/HEAD` del repo del file spegne le ricariche dei 2 s attorno.
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
        assert!(live.step(Some(&mut doc), true, at).0.is_some(), "il documento si aggiorna");
        assert_eq!(live.writing_left(at), None, "ma l'anello resta spento");
    }

    #[test]
    fn repo_root_is_the_nearest_folder_with_git() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join(".git")).unwrap();
        fs::create_dir_all(dir.path().join("docs/sub")).unwrap();
        assert_eq!(repo_of(&dir.path().join("docs/sub/a.md")).as_deref(), Some(dir.path()));
        let other = tempfile::tempdir().unwrap();
        assert_eq!(repo_of(&other.path().join("a.md")), None);
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
