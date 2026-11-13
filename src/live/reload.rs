//! Rilettura del file aperto: quando un agente lo scrive, il documento si aggiorna con
//! `Document::update` (id stabili, quindi la riga in lettura resta ferma).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::doc::document::{Change, Document};
use crate::doc::ids;

/// Gli hash dei contenuti che l'app stessa ha scritto, per percorso: la loro rilettura non è
/// una scrittura di un agente.
#[derive(Debug, Default)]
pub struct OwnWrites {
    hashes: HashMap<PathBuf, u64>,
}

impl OwnWrites {
    /// Da chiamare dopo che l'app ha scritto `hash` in `path` (task 20, salvataggio).
    #[cfg_attr(not(test), allow(dead_code))] // chiamato dal salvataggio (task 20); oggi solo dai test
    pub fn remember(&mut self, path: &Path, hash: u64) {
        self.hashes.insert(path.to_owned(), hash);
    }

    pub fn is_own(&self, path: &Path, hash: u64) -> bool {
        self.hashes.get(path) == Some(&hash)
    }
}

/// Una lettura più corta di questa frazione della precedente (1/10) è sospetta: chi riscrive
/// sullo stesso inode (Codex, `docs/agent-writes.md`) tronca il file e poi lo scrive, e una
/// lettura nel mezzo vede il file vuoto o a metà.
const SHORT_READ_DIVISOR: usize = 10;

/// Aggiorna `doc` con il contenuto letto dal disco. `None` se non c'è niente da mostrare: stesso
/// contenuto, una scrittura dell'app stessa, o una lettura sospetta ([`suspect`]) che il
/// chiamante rilegge più tardi.
pub fn apply(doc: &mut Document, new_source: &str, own_writes: &OwnWrites) -> Option<Change> {
    if suspect(doc, new_source) {
        return None;
    }
    apply_settled(doc, new_source, own_writes)
}

/// Come [`apply`], ma accetta anche una lettura corta: per la seconda lettura uguale di un
/// contenuto sospetto, quando il file non sta più cambiando (un agente l'ha davvero accorciato).
pub fn apply_settled(doc: &mut Document, new_source: &str, own_writes: &OwnWrites) -> Option<Change> {
    let hash = ids::hash_str(new_source);
    if hash == doc.file_hash || own_writes.is_own(&doc.path, hash) {
        return None;
    }
    Some(doc.update(new_source))
}

/// La lettura è molto più corta del documento che si mostra: forse il file è a metà scrittura.
pub fn suspect(doc: &Document, new_source: &str) -> bool {
    let before: usize = doc.blocks.iter().map(|b| b.source.len()).sum();
    new_source.len() * SHORT_READ_DIVISOR < before
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

    #[test]
    fn same_content_is_no_change() {
        let mut doc = Document::from_source(Path::new("a.md"), "# a\n");
        assert_eq!(apply(&mut doc, "# a\n", &OwnWrites::default()), None);
        assert_eq!(doc.revision, 0);
    }

    #[test]
    fn a_short_read_that_holds_still_is_applied() {
        let mut doc = Document::from_source(Path::new("a.md"), &ten_kb());
        assert!(suspect(&doc, "# corto\n"));
        assert!(apply_settled(&mut doc, "# corto\n", &OwnWrites::default()).is_some());
        assert_eq!(doc.blocks.len(), 1);
    }
}
