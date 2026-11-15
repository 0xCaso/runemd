//! The scan of the watched folders: every Markdown file with its mtime.
//!
//! The walk is the watcher's (`watch::walk`): same skipped folders, no symbolic
//! link to folders followed (a cycle would never end). A link to a file is in, with
//! the mtime of the file it points to.

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver};

use rune::Waker;

use super::{Entry, mtime_of};

/// All the Markdown files below `roots`, subfolders included, sorted by path. Read
/// errors ignored.
pub fn scan(roots: &[PathBuf]) -> Vec<Entry> {
    let mut paths = Vec::new();
    for root in roots {
        crate::watch::walk(root, true, &mut paths);
    }
    // Sorted and without duplicates here, on the scan thread: the UI thread only merges
    // them (`Index::finish_scan`). Duplicates only with roots one inside the other.
    paths.sort();
    paths.dedup();
    paths.into_iter().filter_map(|path| Some(Entry { mtime: mtime_of(&path)?, path })).collect()
}

/// Whether macOS (or the permissions) lets the watched folders be read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Access {
    #[default]
    Ok,
    /// The first folder that cannot be read because of permissions (on Desktop, macOS consent).
    Denied(PathBuf),
}

/// Can the roots be read? Only `PermissionDenied` counts: a root that does not exist is not
/// a permissions problem. Call it on the scan thread.
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

/// The result of a background scan: the entries and the permissions outcome.
pub type Scanned = (Vec<Entry>, Access);

/// `scan` on its own thread: the result (with the permissions outcome) arrives on the channel and the
/// thread wakes the app. The first frame does not wait for it.
pub fn scan_in_background(roots: Vec<PathBuf>, waker: Waker) -> Receiver<Scanned> {
    let (tx, rx) = mpsc::channel();
    let spawned = std::thread::Builder::new().name("runemd-index".into()).spawn(move || {
        let access = access(&roots);
        let _ = tx.send((scan(&roots), access));
        waker();
    });
    if let Err(e) = spawned {
        eprintln!("runemd: index scan not started: {e}");
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
        assert_eq!(missing, Access::Ok, "a missing root is not a permissions problem");
    }
}
