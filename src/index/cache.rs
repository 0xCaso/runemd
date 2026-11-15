//! The index cache on disk: a JSON with path and mtime (seconds and nanoseconds since the epoch)
//! of every entry. It is written to a temporary file next to it and then renamed: a reader finds the
//! old file or the new one, never a half-written one.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::Entry;

/// Changes when the format changes: a cache from another version is discarded.
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct File {
    version: u32,
    entries: Vec<Item>,
}

#[derive(Serialize, Deserialize)]
struct Item {
    path: PathBuf,
    secs: u64,
    nanos: u32,
}

/// The entries saved in `path`; `None` if the file is missing, cannot be read or is from another version.
pub fn load(path: &Path) -> Option<Vec<Entry>> {
    let bytes = std::fs::read(path).ok()?;
    let file: File = serde_json::from_slice(&bytes).ok()?;
    if file.version != VERSION {
        return None;
    }
    Some(file.entries.into_iter().map(|i| Entry { path: i.path, mtime: UNIX_EPOCH + Duration::new(i.secs, i.nanos) }).collect())
}

/// Writes `entries` to `path` (folder created if missing) atomically. Entries with an mtime
/// before 1970 or non-UTF-8 paths (JSON cannot represent them) are skipped.
pub fn save(path: &Path, entries: &[Entry]) -> io::Result<()> {
    let items = entries
        .iter()
        .filter(|e| e.path.to_str().is_some())
        .filter_map(|e| {
            let since = e.mtime.duration_since(UNIX_EPOCH).ok()?;
            Some(Item { path: e.path.clone(), secs: since.as_secs(), nanos: since.subsec_nanos() })
        })
        .collect();
    let json = serde_json::to_vec(&File { version: VERSION, entries: items }).map_err(io::Error::other)?;
    let dir = path.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    // One name per write: two saves do not step on each other's temporary file.
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let tmp = dir.join(format!(".{name}.{}.{}.tmp", std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed)));
    let written = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&json)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
}

