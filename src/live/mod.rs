//! Lettura live: i file aperti nelle schede si ricaricano quando un agente li scrive.
//!
//! `Live` tiene il watcher (task 9), "sta scrivendo" e le scritture dell'app, comuni a tutte le
//! schede; ogni scheda ha un [`Follow`] con la sua rilettura. Si rilegge un file a ogni `Changed`
//! del suo percorso e, dopo una lettura sospetta (file a metà scrittura), ancora una volta dopo
//! `SETTLE`. La lettura e la preparazione dell'aggiornamento (blocchi, hash, diff con id stabili)
//! girano su un thread a parte (`reload::read`); sul thread della UI resta `reload::commit`, che
//! sostituisce i blocchi. Il thread sveglia l'app con il `Waker` quando ha finito.
//!
//! Il watcher guarda con le sottocartelle le cartelle osservate (`index::roots`, task 14). Per
//! ogni file aperto si aggiungono la sua cartella, senza sottocartelle (un file in `~` non deve far
//! guardare tutta la home), e il `.git` del suo repo, per i cambi di `.git/HEAD` che spengono
//! "sta scrivendo" (`activity`); se stanno già sotto una cartella osservata il watcher non le
//! aggiunge (`Watcher::add_root`), e ogni evento arriva una volta sola.
//!
//! Le radici aggiunte per i file aperti si contano: quando l'ultima scheda che ne ha bisogno si
//! chiude escono dal watcher (`Live::unfollow`).
//!
//! `Live::drain` svuota il watcher a ogni `poll`; gli eventi di quel risveglio restano in
//! `drained` per le schede (`Live::poll`) e per l'indice.

pub mod activity;
pub mod arriving;
pub mod below;
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
    /// Le radici aggiunte per i file aperti (cartella del file, cartella di HEAD) e quante
    /// schede le usano: l'ultima che si chiude la toglie dal watcher.
    roots: BTreeMap<PathBuf, usize>,
    waker: Option<Waker>,
    /// Gli eventi dell'ultimo `drain`, per le schede e per l'indice.
    drained: Vec<FsEvent>,
    /// Nei test la lettura si aspetta subito, così l'orologio finto decide da solo.
    #[cfg(test)]
    sync: bool,
}

/// La rilettura di un file aperto (una scheda): ognuna ha la sua, con la sua lettura in corso.
pub struct Follow {
    /// Il file come lo riporta FSEvents (canonico).
    watched: PathBuf,
    /// Le radici che questo file ha chiesto al watcher (`Live::roots`).
    roots: Vec<PathBuf>,
    /// L'hash di una lettura sospetta saltata: se la rilettura dà lo stesso, si applica.
    held: Option<u64>,
    recheck_at: Option<Instant>,
    /// Una rilettura da fare: `Some(true)` se è quella a tempo dopo una lettura sospetta.
    want: Option<bool>,
    /// La lettura in corso sul thread a parte.
    job: Option<Job>,
}

impl Follow {
    /// Il file seguito, canonico (come lo riportano FSEvents e `Activity`).
    pub fn path(&self) -> &Path {
        &self.watched
    }

    /// Rilegge il file al prossimo `poll`: dopo la modifica (Cmd+E), che sospende la ricarica e
    /// perde i suoi eventi.
    pub fn reread(&mut self) {
        self.want = Some(self.want.unwrap_or(false));
    }
}

struct Job {
    rx: Receiver<io::Result<Read>>,
    timed: bool,
}

/// Le cartelle da guardare per `path`, senza sottocartelle: la sua cartella e quella del file
/// HEAD del suo repo, `.git/` in un repo, `<repo principale>/.git/worktrees/<nome>/` in un
/// worktree (`repo::head_path`).
fn roots_for(path: &Path) -> Vec<PathBuf> {
    root_of(path).into_iter().chain(head_dir(path)).collect()
}

/// La cartella del file HEAD del repo di `path`; `None` fuori da un repo.
fn head_dir(path: &Path) -> Option<PathBuf> {
    crate::repo::head_path(&crate::repo::of(path).root)?.parent().map(Path::to_path_buf)
}

/// Vero se `path` è cambiato da meno della finestra "sta scrivendo": un `Changed` che rilegge i
/// file noti dopo eventi persi (`Core::rescan`) non accende l'anello di file fermi da ore.
fn written_recently(path: &Path) -> bool {
    let modified = std::fs::metadata(path).and_then(|m| m.modified());
    modified.is_ok_and(|m| SystemTime::now().duration_since(m).unwrap_or_default() < activity::WRITING_WINDOW)
}

/// La cartella da guardare per `path` (non ricorsiva).
fn root_of(path: &Path) -> Option<PathBuf> {
    path.parent().map(|p| if p.as_os_str().is_empty() { PathBuf::from(".") } else { p.to_owned() })
}

impl Live {
    /// Avvia il watcher (da `AppSpec::setup`, che ha il waker) sulle cartelle osservate `roots`,
    /// con le sottocartelle, e guarda i file già aperti.
    pub fn start(&mut self, waker: Waker, roots: Vec<PathBuf>) {
        let mut watcher = Watcher::start(roots, waker.clone());
        for root in self.roots.keys() {
            watcher.add_root(root.clone(), false);
        }
        self.watcher = Some(watcher);
        self.waker = Some(waker);
    }

    /// Un file si apre in una scheda: si guardano la sua cartella e quella di HEAD (se non le
    /// guarda già un'altra scheda o una cartella osservata, `Watcher::add_root`).
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

    /// La scheda di `f` si chiude: le radici che nessun'altra scheda usa escono dal watcher (le
    /// cartelle osservate restano, `Watcher::remove_root`). La lettura in corso si perde con `f`.
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

    /// Le radici aggiunte per i file aperti (per i test e la misura della memoria).
    #[cfg(test)]
    pub fn open_roots(&self) -> Vec<PathBuf> {
        self.roots.keys().cloned().collect()
    }

    /// Da chiamare a ogni `poll`, prima di [`Live::poll`] per le schede: svuota il watcher (va
    /// fatto a ogni risveglio) e annota le scritture dei file non aperti. `open`: i file seguiti
    /// dalle schede, che passano solo dalla ricarica.
    pub fn drain(&mut self, open: &[&Path], now: Instant) {
        let events = self.watcher.as_mut().map(|w| w.drain(now)).unwrap_or_default();
        self.note(&events, open, now);
        self.drained = events;
    }

    /// Per una scheda, dopo [`Live::drain`]: raccoglie la lettura finita e ne avvia una se il
    /// suo file è cambiato. Ritorna il cambio applicato e, se serve, fra quanto richiamarla.
    pub fn poll(&mut self, f: &mut Follow, doc: &mut Document, now: Instant) -> (Option<Reloaded>, Option<Duration>) {
        // Un `Rescan` sopra il file (eventi persi, cartella spostata): si rilegge, se il
        // contenuto è lo stesso la ricarica non cambia nulla.
        let changed = self.drained.iter().any(|e| match e {
            FsEvent::Changed(p) => *p == f.watched,
            FsEvent::Rescan(dir) => f.watched.starts_with(dir),
            _ => false,
        });
        self.step(f, doc, changed, now)
    }

    /// Gli eventi del watcher svuotati dall'ultimo `drain`.
    pub fn drained(&self) -> &[FsEvent] {
        &self.drained
    }

    /// Fra quanto `path` (canonico) smette di essere "sta scrivendo"; `None` se non lo è.
    pub fn writing_left(&self, path: &Path, now: Instant) -> Option<Duration> {
        self.activity.left(path, now)
    }

    /// I file che "stanno scrivendo" a `now`, aperti compresi, e fra quanto smettono.
    pub fn writing(&self, now: Instant) -> Vec<(PathBuf, Duration)> {
        self.activity.writing(now).map(|(p, d)| (p.to_owned(), d)).collect()
    }

    /// Gli eventi che non sono ricariche dei file aperti: un cambio di `.git/HEAD`, o un altro
    /// file scritto da poco (lo scrive un agente: l'app scrive solo i file aperti).
    fn note(&mut self, events: &[FsEvent], open: &[&Path], now: Instant) {
        for e in events {
            match e {
                FsEvent::GitHead(repo) => self.activity.git_head(repo, now),
                FsEvent::Changed(p) if !open.contains(&p.as_path()) && written_recently(p) => self.activity.touch(p, now),
                _ => {}
            }
        }
    }

    /// `poll` senza il watcher: `changed` dice se è arrivato un `Changed` del file di `f`.
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

    /// Legge il file su un thread a parte, a partire dai blocchi di adesso.
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

    /// La lettura finita, se c'è, decisa e applicata.
    fn collect(&mut self, f: &mut Follow, doc: &mut Document, now: Instant) -> Option<Reloaded> {
        let job = f.job.take()?;
        #[cfg(test)]
        let polled = if self.sync { job.rx.recv().map_err(|_| TryRecvError::Disconnected) } else { job.rx.try_recv() };
        #[cfg(not(test))]
        let polled = job.rx.try_recv();
        match polled {
            Ok(Ok(read)) => self.settle(f, doc, read, job.timed, now),
            // File assente o illeggibile durante un rename: il watcher lo riporta quando torna.
            Ok(Err(_)) | Err(TryRecvError::Disconnected) => None,
            Err(TryRecvError::Empty) => {
                f.job = Some(job);
                None
            }
        }
    }

    fn settle(&mut self, f: &mut Follow, doc: &mut Document, read: Read, timed: bool, now: Instant) -> Option<Reloaded> {
        if read.suspect {
            // Una lettura sospetta vale solo alla rilettura a tempo, uguale a quella trattenuta.
            // Prima, anche un altro `Changed` con lo stesso contenuto aspetta: la scrittura può
            // non essere finita.
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
                // L'unico punto che accende "sta scrivendo" per un file aperto: mai per le
                // scritture dell'app, le letture saltate o quelle sospette trattenute.
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

    /// `Live` con una scheda sola, come l'app prima delle schede.
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

    /// Fa girare `poll` come l'app (ogni ~16 ms) finché arriva un ricaricamento o scade `window`.
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
                // Dopo la prima, ancora 500 ms per vedere un'eventuale seconda ricarica.
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
            let (r, wait) = live.poll(&mut doc, Instant::now());
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
        let mut live = One::new(Live { sync: true, ..Live::default() }, &path);
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        fs::write(&path, "").unwrap();
        let (r, wait) = live.step(&mut doc, true, at(0));
        assert!(r.is_none());
        assert_eq!(wait, Some(SETTLE));
        fs::write(&path, "").unwrap();
        let (r, wait) = live.step(&mut doc, true, at(60));
        assert!(r.is_none(), "secondo Changed uguale prima della rilettura");
        assert_eq!(wait, Some(Duration::from_millis(140)), "la rilettura resta a 200 ms");
        assert!(live.step(&mut doc, false, at(150)).0.is_none());
        assert_eq!(doc.blocks.len(), 200);
        let full = format!("{long}Nuovo in fondo.\n");
        fs::write(&path, &full).unwrap();
        let (r, _) = live.step(&mut doc, true, at(180));
        assert_eq!(r.expect("contenuto pieno").change.added.len(), 1);
        assert_eq!(doc.blocks[..200].iter().map(|b| b.id).collect::<Vec<_>>(), ids, "id tenuti");
        assert!(live.step(&mut doc, false, at(400)).0.is_none(), "nessuna rilettura in sospeso");
    }

    /// Un troncamento vero si applica alla rilettura a tempo, non prima.
    #[test]
    fn a_held_truncation_applies_only_on_the_timed_recheck() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let long: String = (0..200).map(|n| format!("Paragrafo {n} di prova.\n\n")).collect();
        fs::write(&path, &long).unwrap();
        let mut doc = Document::load(&path).unwrap();
        let mut live = One::new(Live { sync: true, ..Live::default() }, &path);
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        fs::write(&path, "").unwrap();
        assert!(live.step(&mut doc, true, at(0)).0.is_none());
        assert!(live.step(&mut doc, true, at(199)).0.is_none());
        assert!(live.step(&mut doc, false, at(200)).0.is_some(), "alla rilettura a tempo");
        assert!(doc.blocks.is_empty());
    }

    fn sync_live(path: &Path) -> (One, Document) {
        let doc = Document::load(path).unwrap();
        (One::new(Live { sync: true, ..Live::default() }, path), doc)
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
        assert!(live.step(&mut doc, true, t0).0.is_none());
        assert_eq!(live.writing_left(t0), None, "lettura saltata");
        let mine = "# a\n\nscritto dall'app\n";
        live.live.own.remember(&doc.path, crate::doc::ids::hash_str(mine));
        fs::write(&path, mine).unwrap();
        assert!(live.step(&mut doc, true, t0).0.is_none());
        assert_eq!(live.writing_left(t0), None, "scrittura dell'app");
        fs::write(&path, "# a\n\ndell'agente\n").unwrap();
        assert!(live.step(&mut doc, true, t0).0.is_some());
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
        assert!(live.step(&mut doc, true, at(0)).0.is_none());
        assert_eq!(live.writing_left(at(0)), None, "trattenuta");
        assert!(live.step(&mut doc, false, at(200)).0.is_some());
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
        assert!(live.step(&mut doc, true, at).0.is_some(), "il documento si aggiorna");
        assert_eq!(live.writing_left(at), None, "ma l'anello resta spento");
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

    /// Un altro file scritto da poco "sta scrivendo" (righe di Recenti e albero); uno fermo da
    /// ore riportato da un rescan no; il file aperto passa solo dalla ricarica.
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

    /// Due schede: la scrittura di `b` ricarica solo `b`, e il suo anello si accende solo per lei.
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
        fs::write(&b, "# b\n\nnuovo\n").unwrap();
        live.drained = vec![FsEvent::Changed(fb.watched.clone())];
        assert!(live.poll(&mut fa, &mut da, t0).0.is_none());
        assert!(live.poll(&mut fb, &mut db, t0).0.is_some());
        assert_eq!((da.revision, db.revision), (0, 1));
        assert!(live.writing_left(fb.path(), t0).is_some());
        assert!(live.writing_left(fa.path(), t0).is_none());
    }

    /// Review Focus 1 per ogni scheda, non solo l'attiva: il file di una scheda sparisce (letto
    /// fallisce, il documento resta com'era) e torna entro 2 s: la scheda si ricarica.
    #[test]
    fn a_deleted_file_that_comes_back_reloads_its_tab() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.md"), dir.path().join("b.md"));
        fs::write(&a, "# a\n").unwrap();
        fs::write(&b, "# b\n\nuno\n").unwrap();
        let mut live = Live { sync: true, ..Live::default() };
        let (mut fa, mut fb) = (live.follow(&a), live.follow(&b));
        let (mut da, mut db) = (Document::load(&a).unwrap(), Document::load(&b).unwrap());
        let t0 = Instant::now();
        let watched = fb.watched.clone();
        fs::remove_file(&b).unwrap();
        live.drained = vec![FsEvent::Changed(watched.clone())];
        assert!(live.poll(&mut fb, &mut db, t0).0.is_none(), "file assente");
        assert_eq!(db.blocks.len(), 2, "la scheda tiene l'ultimo contenuto");
        fs::write(&b, "# b\n\nuno\n\ndue\n").unwrap();
        let t1 = t0 + Duration::from_millis(1500);
        live.drained = vec![FsEvent::Changed(watched)];
        assert!(live.poll(&mut fa, &mut da, t1).0.is_none());
        assert_eq!(live.poll(&mut fb, &mut db, t1).0.expect("torna").change.added.len(), 1);
    }

    /// Le radici dei file aperti si contano: si tolgono quando l'ultima scheda che le usa si chiude.
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
        assert_eq!(live.open_roots(), vec![root.clone(), root.join("x")], "b usa ancora la cartella");
        live.unfollow(fc);
        assert_eq!(live.open_roots(), vec![root.clone()]);
        live.unfollow(fb);
        assert!(live.open_roots().is_empty());
    }
}
