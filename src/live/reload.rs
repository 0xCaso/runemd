//! Rilettura del file aperto: quando un agente lo scrive, il documento si aggiorna con
//! `Document::update` (id stabili, quindi la riga in lettura resta ferma).

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::SystemTime;

use crate::doc::document::{Change, Document, Prepared, Snapshot};
use crate::doc::ids;

/// Gli hash dei contenuti che l'app stessa ha scritto, per percorso: la loro rilettura non è
/// una scrittura di un agente. Ogni voce vale una volta sola: dopo l'app scrive X, un agente Y,
/// poi di nuovo X, quella X è dell'agente.
#[derive(Debug, Default)]
pub struct OwnWrites {
    hashes: Mutex<HashMap<PathBuf, u64>>,
}

impl OwnWrites {
    /// Da chiamare dopo che l'app ha scritto `hash` in `path` (task 20, salvataggio).
    #[cfg_attr(not(test), allow(dead_code))] // chiamato dal salvataggio (task 20); oggi solo dai test
    pub fn remember(&mut self, path: &Path, hash: u64) {
        self.map().insert(path.to_owned(), hash);
    }

    /// Se `hash` è l'ultima scrittura dell'app in `path` non ancora riletta. Non consuma la voce.
    #[cfg_attr(not(test), allow(dead_code))] // letto dal salvataggio (task 20); oggi solo dai test
    pub fn is_own(&self, path: &Path, hash: u64) -> bool {
        self.map().get(path) == Some(&hash)
    }

    /// Come `is_own`, e consuma la voce: la rilettura della scrittura dell'app.
    fn take(&self, path: &Path, hash: u64) -> bool {
        let mut map = self.map();
        let own = map.get(path) == Some(&hash);
        if own {
            map.remove(path);
        }
        own
    }

    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<PathBuf, u64>> {
        self.hashes.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Una lettura più corta di questa frazione della precedente (1/10) è sospetta: chi riscrive
/// sullo stesso inode (Codex, `docs/agent-writes.md`) tronca il file e poi lo scrive, e una
/// lettura nel mezzo vede il file vuoto o a metà.
const SHORT_READ_DIVISOR: usize = 10;

/// Aggiorna `doc` con il contenuto letto dal disco. `None` se non c'è niente da mostrare: stesso
/// contenuto, una scrittura dell'app stessa, o una lettura sospetta ([`suspect`]) che il
/// chiamante rilegge più tardi. Sincrona: l'app usa [`read`] su un altro thread e [`commit`].
#[cfg_attr(not(test), allow(dead_code))] // l'app passa da `read` + `commit`; usata dai test
pub fn apply(doc: &mut Document, new_source: &str, own_writes: &OwnWrites) -> Option<Change> {
    if suspect(doc.snapshot().source_len(), new_source) {
        return None;
    }
    apply_settled(doc, new_source, own_writes)
}

/// Come [`apply`], ma accetta anche una lettura corta: per la seconda lettura uguale di un
/// contenuto sospetto, quando il file non sta più cambiando (un agente l'ha davvero accorciato).
#[cfg_attr(not(test), allow(dead_code))]
pub fn apply_settled(doc: &mut Document, new_source: &str, own_writes: &OwnWrites) -> Option<Change> {
    let hash = ids::hash_str(new_source);
    if own_writes.take(&doc.path, hash) || hash == doc.file_hash {
        return None;
    }
    Some(doc.update(new_source))
}

/// La lettura è molto più corta del documento che si mostra (`before` byte di sorgente): forse
/// il file è a metà scrittura.
pub fn suspect(before: usize, new_source: &str) -> bool {
    new_source.len() * SHORT_READ_DIVISOR < before
}

/// Una lettura del file preparata fuori dal thread della UI.
#[derive(Debug)]
pub struct Read {
    pub hash: u64,
    pub suspect: bool,
    /// L'mtime del file letto: quando è stato scritto (sonda di latenza).
    pub written: Option<SystemTime>,
    /// `None` se il contenuto è quello dello snapshot.
    prepared: Option<Prepared>,
}

/// Legge `path` e prepara l'aggiornamento di `snap`: tutto il lavoro pesante (lettura, UTF-8,
/// hash, blocchi, diff), da fare su un altro thread.
pub fn read(path: &Path, snap: &Snapshot) -> io::Result<Read> {
    let bytes = std::fs::read(path)?;
    let written = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let source = String::from_utf8_lossy(&bytes);
    let hash = ids::hash_str(&source);
    let prepared = (hash != snap.file_hash).then(|| snap.prepare(&source));
    Ok(Read { hash, suspect: suspect(snap.source_len(), &source), written, prepared })
}

pub enum Commit {
    Applied(Change),
    /// Stesso contenuto o scrittura dell'app: niente da mostrare.
    Skipped,
    /// Il documento è cambiato dopo lo snapshot: va riletto.
    Stale,
}

/// Il passo di [`read`] sul thread della UI: scarta le scritture dell'app e applica il resto.
pub fn commit(doc: &mut Document, read: Read, own_writes: &OwnWrites) -> Commit {
    if own_writes.take(&doc.path, read.hash) || read.hash == doc.file_hash {
        return Commit::Skipped;
    }
    let Some(prepared) = read.prepared else { return Commit::Stale };
    doc.commit(prepared).map_or(Commit::Stale, Commit::Applied)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::doc::ids::hash_str;

    fn ten_kb() -> String {
        (0..100).map(|n| format!("Paragrafo {n}: {}", "testo ".repeat(16))).collect::<Vec<_>>().join("\n\n")
    }

    #[test]
    fn own_write_is_ignored() {
        let p = Path::new("/tmp/runemd-own.md");
        let mut doc = Document::from_source(p, "# a\n");
        let mut own = OwnWrites::default();
        let mine = "# a\n\nscritto dall'app\n";
        own.remember(p, hash_str(mine));
        assert_eq!(apply(&mut doc, mine, &own), None);
        assert_eq!(doc.revision, 0, "il documento non si tocca");
    }

    #[test]
    fn empty_read_during_write_is_skipped() {
        let src = ten_kb();
        assert!(src.len() >= 10_000);
        let mut doc = Document::from_source(Path::new("a.md"), &src);
        assert_eq!(apply(&mut doc, "", &OwnWrites::default()), None);
        assert_eq!(doc.blocks.len(), 100);
    }

    #[test]
    fn much_shorter_read_is_skipped() {
        let src = ten_kb();
        let mut doc = Document::from_source(Path::new("a.md"), &src);
        let partial = &src[..src.len() / 20];
        assert_eq!(apply(&mut doc, partial, &OwnWrites::default()), None);
        assert_eq!(doc.revision, 0);
    }

    #[test]
    fn an_agent_write_updates_the_document() {
        let mut doc = Document::from_source(Path::new("a.md"), "# a\n\nb\n");
        let first = doc.blocks[0].id;
        let change = apply(&mut doc, "# a\n\nb\n\nc nuovo\n", &OwnWrites::default()).expect("cambio");
        assert_eq!(change.added.len(), 1);
        assert_eq!(doc.blocks[0].id, first);
        assert_eq!(doc.revision, 1);
    }

    /// L'app scrive X, un agente Y, poi di nuovo X: la seconda X è dell'agente.
    #[test]
    fn an_own_write_is_ignored_only_once() {
        let p = Path::new("/tmp/runemd-own-once.md");
        let (x, y) = ("# a\n\nX dell'app\n", "# a\n\nY dell'agente\n");
        let mut doc = Document::from_source(p, "# a\n");
        let mut own = OwnWrites::default();
        own.remember(p, hash_str(x));
        assert_eq!(apply(&mut doc, x, &own), None, "la rilettura della scrittura dell'app");
        assert!(!own.is_own(p, hash_str(x)), "voce consumata");
        assert!(apply(&mut doc, y, &own).is_some());
        assert!(apply(&mut doc, x, &own).is_some(), "X riscritta dall'agente");
    }

    /// Anche quando il documento mostra già il contenuto scritto dall'app, la rilettura consuma
    /// la voce.
    #[test]
    fn an_own_write_already_shown_is_consumed() {
        let p = Path::new("/tmp/runemd-own-shown.md");
        let x = "# a\n\nX\n";
        let mut doc = Document::from_source(p, x);
        let mut own = OwnWrites::default();
        own.remember(p, hash_str(x));
        assert_eq!(apply(&mut doc, x, &own), None);
        assert!(!own.is_own(p, hash_str(x)));
    }

    #[test]
    fn same_content_is_no_change() {
        let mut doc = Document::from_source(Path::new("a.md"), "# a\n");
        assert_eq!(apply(&mut doc, "# a\n", &OwnWrites::default()), None);
        assert_eq!(doc.revision, 0);
    }

    #[test]
    fn a_short_read_that_holds_still_is_applied() {
        let mut doc = Document::from_source(Path::new("a.md"), &ten_kb());
        assert!(suspect(doc.snapshot().source_len(), "# corto\n"));
        assert!(apply_settled(&mut doc, "# corto\n", &OwnWrites::default()).is_some());
        assert_eq!(doc.blocks.len(), 1);
    }
}

#[cfg(test)]
mod cost {
    //! Costo di una ricarica di `fixtures/big.md`. `tutto`: lettura, UTF-8, hash e
    //! `Document::update` (split, hash dei blocchi, diff Myers) come se fossero sul thread della
    //! UI. `ui`: la parte che ci resta, `snapshot` + `commit`; `worker`: `read`.
    //! `cargo test --profile perf reload_cost -- --ignored --nocapture`

    use std::path::Path;
    use std::time::Instant;

    use super::*;

    fn stats(ms: &mut [f64]) -> String {
        ms.sort_by(f64::total_cmp);
        let pct = |p: f64| ms[((ms.len() - 1) as f64 * p).round() as usize];
        format!("n={} p50={:.2} ms p95={:.2} ms max={:.2} ms", ms.len(), pct(0.5), pct(0.95), pct(1.0))
    }

    fn measure(case: &str, before: &str, after: &str) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.md");
        std::fs::write(&path, after).unwrap();
        let (mut all, mut ui, mut worker) = (Vec::new(), Vec::new(), Vec::new());
        let ms = |t: Instant| t.elapsed().as_secs_f64() * 1000.0;
        for _ in 0..40 {
            let mut doc = Document::from_source(&path, before);
            let t = Instant::now();
            let source = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned();
            assert!(apply(&mut doc, &source, &OwnWrites::default()).is_some());
            all.push(ms(t));

            let mut doc = Document::from_source(&path, before);
            let t = Instant::now();
            let snap = doc.snapshot();
            let ui_snap = ms(t);
            let t = Instant::now();
            let r = read(Path::new(&path), &snap).unwrap();
            worker.push(ms(t));
            let t = Instant::now();
            assert!(matches!(commit(&mut doc, r, &OwnWrites::default()), Commit::Applied(_)));
            ui.push(ui_snap + ms(t));
        }
        eprintln!("reload_cost {case} ({} byte): tutto {}", after.len(), stats(&mut all));
        eprintln!("reload_cost {case}: ui (snapshot+commit) {}", stats(&mut ui));
        eprintln!("reload_cost {case}: worker (read) {}", stats(&mut worker));
    }

    #[test]
    #[ignore]
    fn reload_cost() {
        let big = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/big.md")).unwrap();
        measure("append", &big, &format!("{big}\n\nUn paragrafo nuovo in fondo.\n"));
        // Ogni blocco cambia: nessun hash in comune, il caso peggiore per Myers.
        let all: String = crate::doc::blocks::split(&big).iter().map(|b| format!("{b} x")).collect::<Vec<_>>().join("\n\n");
        measure("riscrittura totale", &big, &all);
    }
}
