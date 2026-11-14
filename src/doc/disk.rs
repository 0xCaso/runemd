//! Lettura di un file Markdown dal disco, uguale per tutti: UTF-8 lossy, come la ricarica.

use std::io;
use std::path::Path;

use super::ids::hash_str;

/// Il testo del file e se era UTF-8 valido (altrimenti i byte non validi sono U+FFFD).
pub fn read_lossy(path: &Path) -> io::Result<(String, bool)> {
    let bytes = std::fs::read(path)?;
    Ok(match String::from_utf8(bytes) {
        Ok(text) => (text, true),
        Err(e) => (String::from_utf8_lossy(e.as_bytes()).into_owned(), false),
    })
}

/// Come [`read_lossy`], più l'hash del testo letto.
pub fn read_disk(path: &Path) -> io::Result<(String, u64, bool)> {
    let (text, utf8) = read_lossy(path)?;
    let hash = hash_str(&text);
    Ok((text, hash, utf8))
}
