//! La scansione delle cartelle osservate: ogni Markdown con il suo mtime.
//!
//! Il percorso è quello del watcher (`watch::walk`): stesse cartelle saltate, nessun link
//! simbolico a cartelle seguito (un ciclo non finirebbe mai). Un link a un file entra, con
//! l'mtime del file a cui punta.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

use rune::Waker;

use super::{Entry, mtime_of};

/// Tutti i Markdown sotto `roots`, sottocartelle comprese, ordinati per percorso. Errori di
/// lettura ignorati.
pub fn scan(roots: &[PathBuf]) -> Vec<Entry> {
    let mut paths = Vec::new();
    for root in roots {
        crate::watch::walk(root, true, &mut paths);
    }
    // Ordinate e senza doppioni qui, sul thread della scansione: il thread della UI le fonde
    // soltanto (`Index::finish_scan`). Doppioni solo con radici una dentro l'altra.
    paths.sort();
    paths.dedup();
    paths.into_iter().filter_map(|path| Some(Entry { mtime: mtime_of(&path)?, path })).collect()
}

/// Se macOS (o i permessi) lasciano leggere le cartelle osservate.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Access {
    #[default]
    Ok,
    /// La prima cartella che non si può leggere per permessi (su Desktop, il consenso di macOS).
    Denied(PathBuf),
}

/// Le radici si possono leggere? Solo `PermissionDenied` conta: una radice che non esiste non è
/// un problema di permessi. Va chiamata sul thread della scansione.
pub fn access(roots: &[PathBuf]) -> Access {
    for root in roots {
        if let Err(e) = std::fs::read_dir(root)
            && e.kind() == std::io::ErrorKind::PermissionDenied
        {
            return Access::Denied(root.clone());
        }
    }
    Access::Ok
}

/// Il risultato di una scansione in background: le voci e l'esito dei permessi.
pub type Scanned = (Vec<Entry>, Access);

/// `scan` su un thread a parte: il risultato (con l'esito dei permessi) arriva sul canale e il
/// thread sveglia l'app. Il primo frame non lo aspetta.
pub fn scan_in_background(roots: Vec<PathBuf>, waker: Waker) -> Receiver<Scanned> {
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new().name("runemd-index".into()).spawn(move || {
        let access = access(&roots);
        let _ = tx.send((scan(&roots), access));
        waker();
    });
    if let Err(e) = spawned {
        eprintln!("runemd: scansione dell'indice non avviata: {e}");
    }
    rx
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[test]
    fn permission_denied_is_reported() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap().join("Desktop");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("a.md"), "x").unwrap();
        assert_eq!(access(std::slice::from_ref(&root)), Access::Ok);
        let missing = access(&[root.join("nope")]);
        fs::set_permissions(&root, fs::Permissions::from_mode(0o000)).unwrap();
        let denied = access(std::slice::from_ref(&root));
        fs::set_permissions(&root, fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(denied, Access::Denied(root));
        assert_eq!(missing, Access::Ok, "una radice assente non è un problema di permessi");
    }
}
