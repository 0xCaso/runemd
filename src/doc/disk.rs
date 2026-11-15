//! Reading a Markdown file from disk, the same for everyone: lossy UTF-8, like the reload.

use std::io;
use std::path::Path;

use super::ids::hash_str;

/// The file's text and whether it was valid UTF-8 (otherwise the invalid bytes are U+FFFD).
pub fn read_lossy(path: &Path) -> io::Result<(String, bool)> {
    let bytes = std::fs::read(path)?;
    Ok(match String::from_utf8(bytes) {
        Ok(text) => (text, true),
        Err(e) => (String::from_utf8_lossy(e.as_bytes()).into_owned(), false),
    })
}

/// Like [`read_lossy`], plus the hash of the text read.
pub fn read_disk(path: &Path) -> io::Result<(String, u64, bool)> {
    let (text, utf8) = read_lossy(path)?;
    let hash = hash_str(&text);
    Ok((text, hash, utf8))
}
