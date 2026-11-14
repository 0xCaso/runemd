//! Modifica del sorgente (Cmd+E): il file intero in un campo di testo, salvato con Cmd+S o
//! tornando alla lettura.
//!
//! All'ingresso si ricorda l'hash del file ([`SourceEdit::base_hash`]); ogni salvataggio lo
//! confronta con il disco e, se è cambiato, non scrive ([`Conflict`]): mai sovrascrivere in
//! silenzio. La riga "Il file è cambiato su disco" è del task 21.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use rune::TextEdit;

use crate::app::{Mode, Tab};
use crate::doc::document::Document;
use crate::doc::ids::hash_str;
use crate::live::reload::OwnWrites;

/// Il sorgente in modifica di una scheda.
#[derive(Debug)]
pub struct SourceEdit {
    pub edit: TextEdit,
    /// L'hash del contenuto da cui parte la modifica (lo stesso `ids::hash_str` della ricarica):
    /// se il disco non ha più questo, qualcun altro ha scritto il file.
    pub base_hash: u64,
    /// Il testo è cambiato dall'ultimo salvataggio.
    pub dirty: bool,
}

/// Il file è cambiato su disco dopo l'ingresso (o l'ultimo salvataggio): niente è stato scritto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conflict;

/// Entra in modifica: il sorgente è il file com'è adesso su disco (il documento tiene solo i
/// blocchi, non le righe vuote fra l'uno e l'altro), con il cursore all'inizio e il fuoco nel
/// campo. Se il file non si legge (sparito durante un rename) si parte dai blocchi del documento.
pub fn enter(doc: &Document) -> SourceEdit {
    let (text, base_hash) = match disk(&doc.path) {
        Some((text, hash)) => (text, hash),
        None => (doc.blocks.iter().map(|b| &*b.source).collect::<Vec<_>>().join("\n\n") + "\n", doc.file_hash),
    };
    let mut edit = TextEdit::new(text);
    edit.set_caret(0);
    edit.focused = true;
    SourceEdit { edit, base_hash, dirty: false }
}

/// Salva il sorgente della scheda se il disco ha ancora il contenuto da cui è partita la
/// modifica; altrimenti non scrive e ritorna [`Conflict`]. Il salvataggio aggiorna anche il
/// documento, così la lettura mostra il testo salvato senza passare dalla ricarica: niente segno
/// dei blocchi cambiati, fuoco o "sta scrivendo". La rilettura che il watcher farà della nostra
/// scrittura si salta (`OwnWrites`). Fuori dalla modifica non fa niente.
pub fn save(tab: &mut Tab, own: &mut OwnWrites) -> Result<(), Conflict> {
    let Mode::Source(src) = &mut tab.mode else { return Ok(()) };
    let target = tab.follow.path().to_owned();
    if disk(&target).map(|(_, h)| h) != Some(src.base_hash) {
        return Err(Conflict);
    }
    let hash = hash_str(&src.edit.text);
    if hash != src.base_hash {
        if let Err(e) = write(&target, &src.edit.text) {
            // Non è un conflitto, ma niente è stato scritto: la modifica resta aperta.
            eprintln!("runemd: {}: {e}", target.display());
            return Err(Conflict);
        }
        // Sul thread della UI, prima di qualsiasi `poll`: la rilettura del watcher la trova.
        own.remember(&tab.doc.path, hash);
        tab.doc.update(&src.edit.text);
        src.base_hash = hash;
    }
    src.dirty = false;
    Ok(())
}

/// Esce dalla modifica: salva se ci sono modifiche, poi torna alla lettura. Con un [`Conflict`]
/// resta in modifica. Durante la modifica la ricarica era sospesa: si rilegge il file, così una
/// scrittura arrivata nel frattempo si vede (la nostra si salta).
pub fn leave(tab: &mut Tab, own: &mut OwnWrites) -> Result<(), Conflict> {
    if matches!(&tab.mode, Mode::Source(s) if s.dirty) {
        save(tab, own)?;
    }
    tab.mode = Mode::Read;
    tab.follow.reread();
    Ok(())
}

/// Il file su disco come lo legge la ricarica (`from_utf8_lossy`) e il suo hash.
fn disk(path: &Path) -> Option<(String, u64)> {
    let bytes = fs::read(path).ok()?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let hash = hash_str(&text);
    Some((text, hash))
}

/// Scrive `text` in `path` con un file accanto e un rename: chi legge (la nostra ricarica, un
/// agente) vede il file prima o dopo, mai a metà. Il file nuovo prende i permessi del vecchio.
fn write(path: &Path, text: &str) -> io::Result<()> {
    let name = path.file_name().ok_or_else(|| io::Error::other("percorso senza nome"))?;
    let mut tmp_name = OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(".runemd-save");
    let tmp = path.with_file_name(tmp_name);
    let perms = fs::metadata(path)?.permissions();
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        fs::set_permissions(&tmp, perms)?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live::Live;

    fn tab_on(path: &Path) -> Tab {
        let mut live = Live::default();
        Tab::new(Document::load(path).unwrap(), live.follow(path))
    }

    fn source(tab: &mut Tab) -> &mut SourceEdit {
        match &mut tab.mode {
            Mode::Source(s) => s,
            Mode::Read => panic!("non in modifica"),
        }
    }

    fn mtime(path: &Path) -> std::time::SystemTime {
        fs::metadata(path).unwrap().modified().unwrap()
    }

    #[test]
    fn leave_without_changes_does_not_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n\nuno\n").unwrap();
        // Un mtime nel passato: una scrittura lo porterebbe ad adesso.
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        fs::File::options().write(true).open(&path).unwrap().set_modified(old).unwrap();
        let mut tab = tab_on(&path);
        tab.mode = Mode::Source(Box::new(enter(&tab.doc)));
        assert_eq!(source(&mut tab).edit.text, "# a\n\nuno\n", "il sorgente intero del file");
        let mut own = OwnWrites::default();
        assert_eq!(leave(&mut tab, &mut own), Ok(()));
        assert!(matches!(tab.mode, Mode::Read), "torna alla lettura");
        assert_eq!(mtime(&path), old, "file non scritto");
        assert_eq!(fs::read_to_string(&path).unwrap(), "# a\n\nuno\n");
    }

    #[test]
    fn save_writes_and_remembers_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n\nuno\n").unwrap();
        let mut tab = tab_on(&path);
        tab.mode = Mode::Source(Box::new(enter(&tab.doc)));
        let mine = "# a\n\nuno\n\ndue\n";
        let s = source(&mut tab);
        s.edit = TextEdit::new(mine);
        s.dirty = true;
        let mut own = OwnWrites::default();
        assert_eq!(save(&mut tab, &mut own), Ok(()));
        assert_eq!(fs::read_to_string(&path).unwrap(), mine);
        let hash = hash_str(mine);
        assert!(own.is_own(&tab.doc.path, hash), "la rilettura sarà una scrittura dell'app");
        let s = source(&mut tab);
        assert_eq!((s.base_hash, s.dirty), (hash, false), "il nuovo punto di partenza");
        // La lettura mostra il testo salvato, senza passare dalla ricarica.
        assert_eq!(tab.doc.file_hash, hash);
        assert_eq!(tab.doc.blocks.len(), 3);
    }

    #[test]
    fn save_refuses_when_disk_changed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n").unwrap();
        let mut tab = tab_on(&path);
        tab.mode = Mode::Source(Box::new(enter(&tab.doc)));
        let agent = "# a\n\ndell'agente\n";
        fs::write(&path, agent).unwrap();
        let s = source(&mut tab);
        s.edit = TextEdit::new("# a\n\nmio\n");
        s.dirty = true;
        let mut own = OwnWrites::default();
        assert_eq!(save(&mut tab, &mut own), Err(Conflict));
        assert_eq!(fs::read_to_string(&path).unwrap(), agent, "file su disco invariato");
        assert!(source(&mut tab).dirty, "le modifiche restano");
        assert_eq!(leave(&mut tab, &mut own), Err(Conflict));
        assert!(matches!(tab.mode, Mode::Source(_)), "resta in modifica");
        assert_eq!(fs::read_to_string(&path).unwrap(), agent);
    }
}
