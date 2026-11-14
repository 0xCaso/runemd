//! Il documento aperto: blocchi con id stabili tra una lettura e l'altra.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{blocks, ids};
use super::select_all::SelectAllFlag;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub id: u64,
    pub source: Arc<str>,
    pub hash: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Change {
    pub added: Vec<u64>,
    pub changed: Vec<u64>,
    pub removed: Vec<u64>,
}

/// Il prossimo id di documento: uno solo per tutta l'app, mai riusato.
static NEXT_DOC: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub struct Document {
    /// Unico nell'app: ogni `Document` letto ne prende uno nuovo, anche lo stesso file riaperto.
    /// Le chiavi delle righe lo usano (due schede non condividono le altezze misurate da Rune) e
    /// `commit` rifiuta un aggiornamento preparato per un altro documento.
    pub id: u64,
    pub path: PathBuf,
    pub revision: u64,
    pub blocks: Vec<Block>,
    pub file_hash: u64,
    /// L'ultimo blocco del file è ancora aperto (`live::arriving::last_block_open`): il sorgente
    /// intero non resta, quindi si calcola a ogni lettura.
    pub last_open: bool,
    /// Se la selezione corrente è un Cmd+A (`ui::doc_view`); condiviso con le closure della vista.
    pub select_all: Arc<SelectAllFlag>,
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
            id: NEXT_DOC.fetch_add(1, Ordering::Relaxed),
            path: path.to_owned(),
            revision: 0,
            blocks: Vec::new(),
            file_hash: ids::hash_str(source),
            last_open: crate::live::arriving::last_block_open(source),
            select_all: Arc::default(),
            next_id: 1,
        };
        let (blocks, _) = ids::assign(&[], ids::hash_sources(blocks::split(source)), &mut doc.next_id);
        doc.blocks = blocks;
        doc
    }

    pub fn update(&mut self, source: &str) -> Change {
        self.commit(self.snapshot().prepare(source)).expect("preparato dalla revisione corrente")
    }

    /// Quanto serve per preparare un aggiornamento su un altro thread (`Snapshot::prepare`).
    pub fn snapshot(&self) -> Snapshot {
        Snapshot { doc: self.id, revision: self.revision, file_hash: self.file_hash, blocks: self.blocks.clone(), next_id: self.next_id }
    }

    /// Applica un aggiornamento preparato. `None` se il documento è cambiato dopo lo snapshot
    /// (il chiamante rilegge) o se è stato preparato per un altro documento.
    pub fn commit(&mut self, p: Prepared) -> Option<Change> {
        if p.doc != self.id || p.revision != self.revision {
            return None;
        }
        self.blocks = p.blocks;
        self.file_hash = p.file_hash;
        self.last_open = p.last_open;
        self.next_id = p.next_id;
        self.revision += 1;
        Some(p.change)
    }
}

/// I blocchi di un `Document` a una revisione; si sposta su un altro thread (`Block.source` è
/// `Arc<str>`).
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// L'id del `Document` da cui viene.
    doc: u64,
    revision: u64,
    pub file_hash: u64,
    blocks: Vec<Block>,
    next_id: u64,
}

impl Snapshot {
    /// La lunghezza del sorgente dei blocchi (senza le righe vuote tra un blocco e l'altro).
    pub fn source_len(&self) -> usize {
        self.blocks.iter().map(|b| b.source.len()).sum()
    }

    /// Il lavoro di `Document::update` che non tocca il documento: divisione in blocchi, hash e
    /// diff con id stabili.
    pub fn prepare(&self, source: &str) -> Prepared {
        let mut next_id = self.next_id;
        let (blocks, change) = ids::assign(&self.blocks, ids::hash_sources(blocks::split(source)), &mut next_id);
        Prepared { doc: self.doc, revision: self.revision, blocks, change, file_hash: ids::hash_str(source), last_open: crate::live::arriving::last_block_open(source), next_id }
    }
}

/// Un aggiornamento pronto per `Document::commit`.
#[derive(Debug)]
pub struct Prepared {
    doc: u64,
    revision: u64,
    blocks: Vec<Block>,
    change: Change,
    file_hash: u64,
    last_open: bool,
    next_id: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Una ricarica preparata per la scheda A non entra mai nella scheda B, anche con la stessa
    /// revisione e lo stesso percorso (una scheda chiusa e un'altra aperta sullo stesso file).
    #[test]
    fn a_reload_prepared_for_another_document_is_refused() {
        let p = Path::new("/r/a.md");
        let a = Document::from_source(p, "# a\n");
        let mut b = Document::from_source(p, "# a\n");
        assert_ne!(a.id, b.id, "id unici");
        let prepared = a.snapshot().prepare("# a\n\nnuovo\n");
        assert!(b.commit(prepared).is_none());
        assert_eq!(b.revision, 0);
    }
}
