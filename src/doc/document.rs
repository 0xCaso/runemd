//! Il documento aperto: blocchi con id stabili tra una lettura e l'altra.

use std::io;
use std::path::{Path, PathBuf};

use super::{blocks, ids};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub id: u64,
    pub source: String,
    pub hash: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Change {
    pub added: Vec<u64>,
    pub changed: Vec<u64>,
    pub removed: Vec<u64>,
}

#[derive(Debug)]
pub struct Document {
    pub path: PathBuf,
    pub revision: u64,
    pub blocks: Vec<Block>,
    pub file_hash: u64,
    next_id: u64,
}

impl Document {
    /// Legge il file; i byte non UTF-8 diventano U+FFFD.
    pub fn load(path: &Path) -> io::Result<Document> {
        let bytes = std::fs::read(path)?;
        Ok(Self::from_source(path, &String::from_utf8_lossy(&bytes)))
    }

    pub fn from_source(path: &Path, source: &str) -> Document {
        let mut doc = Document {
            path: path.to_owned(),
            revision: 0,
            blocks: Vec::new(),
            file_hash: ids::hash_str(source),
            next_id: 1,
        };
        let (blocks, _) = ids::assign(&[], ids::hash_sources(blocks::split(source)), &mut doc.next_id);
        doc.blocks = blocks;
        doc
    }

    pub fn update(&mut self, source: &str) -> Change {
        let new = ids::hash_sources(blocks::split(source));
        let (blocks, change) = ids::assign(&self.blocks, new, &mut self.next_id);
        self.blocks = blocks;
        self.file_hash = ids::hash_str(source);
        self.revision += 1;
        change
    }
}
