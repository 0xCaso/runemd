//! La scansione delle cartelle osservate: ogni Markdown con il suo mtime.
//!
//! Il percorso è quello del watcher (`watch::walk`): stesse cartelle saltate, nessun link
//! simbolico a cartelle seguito (un ciclo non finirebbe mai). Un link a un file entra, con
//! l'mtime del file a cui punta.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

use rune::Waker;

use super::{Entry, mtime_of};

/// Tutti i Markdown sotto `roots`, sottocartelle comprese. Errori di lettura ignorati.
pub fn scan(roots: &[PathBuf]) -> Vec<Entry> {
    let mut paths = Vec::new();
    for root in roots {
        crate::watch::walk(root, true, &mut paths);
    }
    paths.into_iter().filter_map(|path| Some(Entry { mtime: mtime_of(&path)?, path })).collect()
}

/// `scan` su un thread a parte: il risultato arriva sul canale e il thread sveglia l'app. Il
/// primo frame non lo aspetta.
pub fn scan_in_background(roots: Vec<PathBuf>, waker: Waker) -> Receiver<Vec<Entry>> {
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new().name("runemd-index".into()).spawn(move || {
        let _ = tx.send(scan(&roots));
        waker();
    });
    if let Err(e) = spawned {
        eprintln!("runemd: scansione dell'indice non avviata: {e}");
    }
    rx
}
