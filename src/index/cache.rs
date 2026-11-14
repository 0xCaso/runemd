//! La cache dell'indice su disco: un JSON con percorso e mtime (secondi e nanosecondi dall'epoca)
//! di ogni voce. Si scrive in un file temporaneo accanto e poi si rinomina: chi legge trova il
//! file vecchio o quello nuovo, mai uno a metà.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use super::Entry;

/// Cambia quando cambia il formato: una cache di un'altra versione si scarta.
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

/// Le voci salvate in `path`; `None` se il file manca, non si legge o è di un'altra versione.
pub fn load(path: &Path) -> Option<Vec<Entry>> {
    let bytes = std::fs::read(path).ok()?;
    let file: File = serde_json::from_slice(&bytes).ok()?;
    if file.version != VERSION {
        return None;
    }
    Some(file.entries.into_iter().map(|i| Entry { path: i.path, mtime: UNIX_EPOCH + Duration::new(i.secs, i.nanos) }).collect())
}

/// Scrive `entries` in `path` (cartella creata se manca) in modo atomico. Le voci con mtime
/// prima del 1970 o percorsi non UTF-8 (JSON non li rappresenta) si saltano.
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
    // Un nome per scrittura: due salvataggi non si pestano il file temporaneo.
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

