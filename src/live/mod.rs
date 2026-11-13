//! Lettura live: il file aperto si ricarica quando un agente lo scrive.
//!
//! `Live` tiene il watcher (task 9) e decide quando rileggere il file aperto: a ogni `Changed`
//! del suo percorso e, dopo una lettura sospetta (file a metà scrittura), ancora una volta dopo
//! `SETTLE`. La lettura e la preparazione dell'aggiornamento (blocchi, hash, diff con id stabili)
//! girano su un thread a parte (`reload::read`); sul thread della UI resta `reload::commit`, che
//! sostituisce i blocchi. Il thread sveglia l'app con il `Waker` quando ha finito.
//!
//! Fino al task 14 si guarda solo la cartella del file aperto, senza sottocartelle: un file in
//! `~` non deve far guardare tutta la home.

pub mod reload;

use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::{Duration, Instant, SystemTime};

use rune::Waker;

use crate::doc::document::{Change, Document};
use crate::watch::{FsEvent, Watcher};

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
    /// Nei test la lettura si aspetta subito, così l'orologio finto decide da solo.
    #[cfg(test)]
    sync: bool,
}

struct Job {
    rx: Receiver<io::Result<Read>>,
    timed: bool,
}

/// La cartella da guardare per `path` (non ricorsiva).
fn root_of(path: &Path) -> Option<PathBuf> {
    path.parent().map(|p| if p.as_os_str().is_empty() { PathBuf::from(".") } else { p.to_owned() })
}

impl Live {
    /// Avvia il watcher (da `AppSpec::setup`, che ha il waker) e guarda il file già aperto.
    pub fn start(&mut self, waker: Waker) {
        let mut watcher = Watcher::start(Vec::new(), waker.clone());
        if let Some(root) = self.watched.as_deref().and_then(root_of) {
            watcher.add_root(root, false);
        }
        self.watcher = Some(watcher);
        self.waker = Some(waker);
    }

    /// Il file aperto ora è `path`: si guarda la sua cartella e si dimentica lo stato del
    /// file di prima, lettura in corso compresa (era per l'altro documento).
    pub fn watch(&mut self, path: &Path) {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_owned());
        if let (Some(w), Some(root)) = (self.watcher.as_mut(), root_of(&canonical)) {
            w.add_root(root, false);
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
        let changed = events.iter().any(|e| matches!(e, FsEvent::Changed(p) if self.watched.as_ref() == Some(p)));
        self.step(doc, changed, now)
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
            Commit::Applied(change) => Some(Reloaded { at: now, written, change }),
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
