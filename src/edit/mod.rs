//! Modifica del sorgente (Cmd+E): il file intero, diviso in pezzi con un campo ciascuno
//! ([`blocks`]), salvato con Cmd+S o tornando alla lettura.
//!
//! All'ingresso si ricorda l'hash del file ([`SourceEdit::base_hash`]); ogni salvataggio lo
//! confronta con il disco e, se è cambiato, non scrive ([`SaveError::Conflict`]): mai
//! sovrascrivere in silenzio. La scheda mostra allora la riga "Il file è cambiato su disco"
//! ([`Notice::Conflict`]) finché non si sceglie "Ricarica" ([`reload`]) o "Tieni il mio"
//! ([`keep_mine`]). Un errore di scrittura non è un conflitto: la riga dice "Salvataggio non
//! riuscito" ([`Notice::SaveFailed`]) e le modifiche restano.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

pub mod blocks;
pub mod new_file;

use crate::app::{Mode, Tab};
use crate::doc::document::Document;
use crate::doc::ids::hash_str;
use crate::live::reload::OwnWrites;
use blocks::Pieces;

/// Il sorgente in modifica di una scheda.
#[derive(Debug)]
pub struct SourceEdit {
    /// Il testo, in pezzi riuniti con `"\n"` (un campo per pezzo).
    pub pieces: Pieces,
    /// L'hash del contenuto da cui parte la modifica (lo stesso `ids::hash_str` della ricarica):
    /// se il disco non ha più questo, qualcun altro ha scritto il file.
    pub base_hash: u64,
    /// Il testo è cambiato dall'ultimo salvataggio.
    pub dirty: bool,
    /// La riga sopra il sorgente dopo un salvataggio che non ha scritto; resta finché un
    /// salvataggio riesce o si sceglie (anche con la scheda in secondo piano).
    pub notice: Option<Notice>,
}

/// Perché l'ultimo salvataggio non ha scritto: quello che la riga sopra il sorgente dice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    /// "Il file è cambiato su disco", con "Ricarica" e "Tieni il mio".
    Conflict,
    /// "Salvataggio non riuscito": un errore di lettura o scrittura, non un conflitto.
    SaveFailed,
}

/// Il salvataggio non ha scritto.
#[derive(Debug)]
pub enum SaveError {
    /// Il file è cambiato su disco dopo l'ingresso (o l'ultimo salvataggio), o non c'è più.
    Conflict,
    /// Il file non si legge o non si scrive (permessi, disco pieno).
    Io(io::Error),
}

impl SourceEdit {
    /// Il testo intero del sorgente.
    pub fn text(&self) -> String {
        self.pieces.text()
    }

    /// Sostituisce tutto il testo (ridivide i pezzi, cursore all'inizio).
    #[cfg(test)]
    pub fn set_text(&mut self, text: &str) {
        self.pieces.set_text(text);
    }
}

/// Entra in modifica: il sorgente è il file com'è adesso su disco (il documento tiene solo i
/// blocchi, non le righe vuote fra l'uno e l'altro), con il cursore all'inizio e il fuoco nel
/// campo. Se il file non si legge (sparito durante un rename) si parte dai blocchi del documento.
pub fn enter(doc: &Document) -> SourceEdit {
    let (text, base_hash) = match disk(&doc.path) {
        Ok((text, hash)) => (text, hash),
        Err(_) => (doc.blocks.iter().map(|b| &*b.source).collect::<Vec<_>>().join("\n\n") + "\n", doc.file_hash),
    };
    SourceEdit { pieces: Pieces::new(&text), base_hash, dirty: false, notice: None }
}

/// Salva il sorgente della scheda se il disco ha ancora il contenuto da cui è partita la
/// modifica; altrimenti non scrive e ritorna [`SaveError::Conflict`]. Un errore non scrive e
/// accende la riga ([`SourceEdit::notice`]); un salvataggio riuscito la spegne. Il salvataggio aggiorna anche il
/// documento, così la lettura mostra il testo salvato senza passare dalla ricarica: niente segno
/// dei blocchi cambiati, fuoco o "sta scrivendo". La rilettura che il watcher farà della nostra
/// scrittura si salta (`OwnWrites`). Fuori dalla modifica non fa niente.
pub fn save(tab: &mut Tab, own: &mut OwnWrites) -> Result<(), SaveError> {
    let result = try_save(tab, own);
    if let Mode::Source(src) = &mut tab.mode {
        src.notice = match &result {
            Ok(()) => None,
            Err(SaveError::Conflict) => Some(Notice::Conflict),
            Err(SaveError::Io(_)) => Some(Notice::SaveFailed),
        };
    }
    if let Err(SaveError::Io(e)) = &result {
        eprintln!("runemd: {}: salvataggio non riuscito: {e}", tab.doc.path.display());
    }
    result
}

fn try_save(tab: &mut Tab, own: &mut OwnWrites) -> Result<(), SaveError> {
    let Mode::Source(src) = &mut tab.mode else { return Ok(()) };
    let target = tab.follow.path().to_owned();
    match disk(&target) {
        Ok((_, h)) if h == src.base_hash => {}
        Ok(_) => return Err(SaveError::Conflict),
        // Sparito (un rename a metà, o cancellato): è cambiato su disco.
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Err(SaveError::Conflict),
        Err(e) => return Err(SaveError::Io(e)),
    }
    if hash_str(&src.text()) != src.base_hash {
        commit(tab, own).map_err(SaveError::Io)?;
    }
    if let Mode::Source(src) = &mut tab.mode {
        src.dirty = false;
    }
    Ok(())
}

/// "Tieni il mio": scrive il testo del sorgente sopra quello che c'è su disco, senza confrontare,
/// e ne fa il nuovo punto di partenza. Come un salvataggio, la rilettura del watcher è una
/// scrittura dell'app (niente segni, fuoco o dither). La scheda resta nel sorgente.
pub fn keep_mine(tab: &mut Tab, own: &mut OwnWrites) -> io::Result<()> {
    let result = commit(tab, own);
    if let Mode::Source(src) = &mut tab.mode {
        match &result {
            Ok(()) => (src.dirty, src.notice) = (false, None),
            Err(e) => {
                eprintln!("runemd: {}: salvataggio non riuscito: {e}", tab.doc.path.display());
                src.notice = Some(Notice::SaveFailed);
            }
        }
    }
    result
}

/// "Ricarica": scarta le modifiche e riparte dal file com'è adesso su disco, sempre nel
/// sorgente (la riga è del sorgente; Cmd+E torna poi alla lettura). Il cursore resta dov'era, se
/// il testo nuovo arriva fin lì. Non scrive niente. Se il file non si legge (cancellato) non c'è
/// niente da rileggere: le modifiche e la riga restano, e solo "Tieni il mio" lo ricrea.
pub fn reload(tab: &mut Tab) {
    let Mode::Source(src) = &mut tab.mode else { return };
    if disk(&tab.doc.path).is_err() {
        return;
    }
    let caret = src.pieces.caret();
    let mut fresh = enter(&tab.doc);
    fresh.pieces.set_caret(caret);
    **src = fresh;
}

/// Scrive il testo del sorgente nel file della scheda, aggiorna il documento e `base_hash`.
fn commit(tab: &mut Tab, own: &mut OwnWrites) -> io::Result<()> {
    let Mode::Source(src) = &mut tab.mode else { return Ok(()) };
    let text = src.text();
    let hash = hash_str(&text);
    write(tab.follow.path(), &text)?;
    // Sul thread della UI, prima di qualsiasi `poll`: la rilettura del watcher la trova.
    own.remember(&tab.doc.path, hash);
    tab.doc.update(&text);
    src.base_hash = hash;
    Ok(())
}

/// Esce dalla modifica: salva se ci sono modifiche, poi torna alla lettura. Se il salvataggio
/// non scrive resta in modifica, con la riga. Durante la modifica la ricarica era sospesa: si rilegge il file, così una
/// scrittura arrivata nel frattempo si vede (la nostra si salta).
pub fn leave(tab: &mut Tab, own: &mut OwnWrites) -> Result<(), SaveError> {
    if matches!(&tab.mode, Mode::Source(s) if s.dirty) {
        save(tab, own)?;
    }
    tab.mode = Mode::Read;
    tab.follow.reread();
    Ok(())
}

/// Il file su disco come lo legge la ricarica (`from_utf8_lossy`) e il suo hash.
fn disk(path: &Path) -> io::Result<(String, u64)> {
    let bytes = fs::read(path)?;
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let hash = hash_str(&text);
    Ok((text, hash))
}

/// Scrive `text` in `path` con un file accanto e un rename: chi legge (la nostra ricarica, un
/// agente) vede il file prima o dopo, mai a metà. Il file nuovo prende i permessi del vecchio,
/// se c'è ancora ("Tieni il mio" su un file cancellato lo ricrea).
fn write(path: &Path, text: &str) -> io::Result<()> {
    let name = path.file_name().ok_or_else(|| io::Error::other("percorso senza nome"))?;
    let mut tmp_name = OsString::from(".");
    tmp_name.push(name);
    tmp_name.push(".runemd-save");
    let tmp = path.with_file_name(tmp_name);
    let perms = fs::metadata(path).ok().map(|m| m.permissions());
    let result = (|| {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        if let Some(perms) = perms {
            fs::set_permissions(&tmp, perms)?;
        }
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
pub(crate) mod tests {
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
        assert_eq!(source(&mut tab).text(), "# a\n\nuno\n", "il sorgente intero del file");
        let mut own = OwnWrites::default();
        assert!(leave(&mut tab, &mut own).is_ok());
        assert!(matches!(tab.mode, Mode::Read), "torna alla lettura");
        assert_eq!(mtime(&path), old, "file non scritto");
        assert_eq!(fs::read_to_string(&path).unwrap(), "# a\n\nuno\n");
    }

    /// Scritto e poi cancellato: `dirty` resta vero ma il testo è quello di partenza, niente si scrive.
    #[test]
    fn leave_with_the_text_back_to_the_start_does_not_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n\nuno\n").unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        fs::File::options().write(true).open(&path).unwrap().set_modified(old).unwrap();
        let mut tab = tab_on(&path);
        tab.mode = Mode::Source(Box::new(enter(&tab.doc)));
        let s = source(&mut tab);
        s.pieces.current().insert("X");
        s.dirty = true;
        s.pieces.current().set_selection(0, 1);
        s.pieces.current().insert("");
        assert_eq!(s.text(), "# a\n\nuno\n");
        assert!(s.dirty);
        assert!(leave(&mut tab, &mut OwnWrites::default()).is_ok());
        assert!(matches!(tab.mode, Mode::Read));
        assert_eq!(mtime(&path), old, "file non scritto");
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
        s.set_text(mine);
        s.dirty = true;
        let mut own = OwnWrites::default();
        assert!(save(&mut tab, &mut own).is_ok());
        assert_eq!(fs::read_to_string(&path).unwrap(), mine);
        let hash = hash_str(mine);
        assert!(own.is_own(&tab.doc.path, hash), "la rilettura sarà una scrittura dell'app");
        let s = source(&mut tab);
        assert_eq!((s.base_hash, s.dirty), (hash, false), "il nuovo punto di partenza");
        // La lettura mostra il testo salvato, senza passare dalla ricarica.
        assert_eq!(tab.doc.file_hash, hash);
        assert_eq!(tab.doc.blocks.len(), 3);
    }

    /// Si gira come root (euid 0): una cartella in sola lettura non ferma le scritture, e i test
    /// che contano su 0o555 si saltano. La cartella appena creata è dell'euid.
    pub(crate) fn as_root(dir: &Path) -> bool {
        use std::os::unix::fs::MetadataExt;
        let root = fs::metadata(dir).unwrap().uid() == 0;
        if root {
            eprintln!("saltato: come root una cartella 0o555 non ferma le scritture");
        }
        root
    }

    /// Mette la scheda nel sorgente con `mine` come modifica.
    fn editing(path: &Path, mine: &str) -> Tab {
        let mut tab = tab_on(path);
        tab.mode = Mode::Source(Box::new(enter(&tab.doc)));
        let s = source(&mut tab);
        s.set_text(mine);
        s.dirty = true;
        tab
    }

    /// Due scritture esterne durante la modifica: ogni salvataggio trova il conflitto, la riga
    /// resta finché non si sceglie e il disco resta quello dell'agente (review focus 4).
    #[test]
    fn two_external_writes_keep_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n").unwrap();
        let mut tab = editing(&path, "# a\n\nmio\n");
        let mut own = OwnWrites::default();
        fs::write(&path, "# a\n\nprima\n").unwrap();
        assert!(matches!(save(&mut tab, &mut own), Err(SaveError::Conflict)));
        assert_eq!(source(&mut tab).notice, Some(Notice::Conflict), "compare la riga");
        fs::write(&path, "# a\n\nseconda\n").unwrap();
        assert!(matches!(save(&mut tab, &mut own), Err(SaveError::Conflict)));
        assert!(matches!(leave(&mut tab, &mut own), Err(SaveError::Conflict)));
        let s = source(&mut tab);
        assert_eq!(s.notice, Some(Notice::Conflict), "la riga resta finché non si sceglie");
        assert!(s.dirty && s.text() == "# a\n\nmio\n", "le modifiche restano");
        assert_eq!(fs::read_to_string(&path).unwrap(), "# a\n\nseconda\n", "nessuna scrittura su disco");
    }

    /// "Ricarica": le modifiche si scartano e il sorgente riparte dal disco, senza scrivere.
    #[test]
    fn reload_discards_edits_and_rereads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n").unwrap();
        let mut tab = editing(&path, "# a\n\nmio\n");
        let agent = "# a\n\ndell'agente\n";
        fs::write(&path, agent).unwrap();
        let mut own = OwnWrites::default();
        assert!(save(&mut tab, &mut own).is_err());
        reload(&mut tab);
        let s = source(&mut tab);
        assert_eq!((s.text().as_str(), s.dirty, s.notice), (agent, false, None));
        assert_eq!(s.base_hash, hash_str(agent), "il nuovo punto di partenza");
        assert_eq!(fs::read_to_string(&path).unwrap(), agent);
        // Ora si salva sopra il testo dell'agente senza conflitto.
        source(&mut tab).pieces.set_caret(0);
        source(&mut tab).pieces.current().insert("X");
        source(&mut tab).dirty = true;
        assert!(save(&mut tab, &mut own).is_ok());
        assert_eq!(fs::read_to_string(&path).unwrap(), format!("X{agent}"));
    }

    /// "Ricarica" con il file cancellato: non c'è niente da rileggere, quindi le modifiche e la
    /// riga restano; solo "Tieni il mio" ricrea il file.
    #[test]
    fn reload_of_a_deleted_file_keeps_the_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n").unwrap();
        let mine = "# a\n\nmio\n";
        let mut tab = editing(&path, mine);
        fs::remove_file(&path).unwrap();
        let mut own = OwnWrites::default();
        assert!(matches!(save(&mut tab, &mut own), Err(SaveError::Conflict)));
        reload(&mut tab);
        let s = source(&mut tab);
        assert_eq!((s.text().as_str(), s.dirty, s.notice), (mine, true, Some(Notice::Conflict)));
        assert!(!path.exists(), "niente scritto");
        assert!(keep_mine(&mut tab, &mut own).is_ok());
        assert_eq!(fs::read_to_string(&path).unwrap(), mine, "Tieni il mio ricrea il file");
        assert_eq!(source(&mut tab).notice, None);
    }

    /// "Tieni il mio": scrive il testo locale sopra quello dell'agente, aggiorna `base_hash` e
    /// ricorda la scrittura (la rilettura non accende segni, fuoco o dither).
    #[test]
    fn keep_mine_writes_and_rebases() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n").unwrap();
        let mine = "# a\n\nmio\n";
        let mut tab = editing(&path, mine);
        fs::write(&path, "# a\n\ndell'agente\n").unwrap();
        let mut own = OwnWrites::default();
        assert!(save(&mut tab, &mut own).is_err());
        assert!(keep_mine(&mut tab, &mut own).is_ok());
        assert_eq!(fs::read_to_string(&path).unwrap(), mine);
        let hash = hash_str(mine);
        assert!(own.is_own(&tab.doc.path, hash), "la rilettura sarà una scrittura dell'app");
        assert_eq!(tab.doc.file_hash, hash, "la lettura mostra il mio testo");
        let s = source(&mut tab);
        assert_eq!((s.base_hash, s.dirty, s.notice), (hash, false, None));
        assert!(matches!(tab.mode, Mode::Source(_)), "resta nel sorgente");
    }

    /// Un errore di scrittura non è un conflitto: niente riga "Il file è cambiato su disco" ma
    /// "Salvataggio non riuscito", e le modifiche restano.
    #[test]
    fn an_io_error_is_not_a_conflict() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        if as_root(dir.path()) {
            return;
        }
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n").unwrap();
        let mut tab = editing(&path, "# a\n\nmio\n");
        // Cartella in sola lettura: il file accanto per il rename non si crea.
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
        let mut own = OwnWrites::default();
        let saved = save(&mut tab, &mut own);
        let kept = keep_mine(&mut tab, &mut own);
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(saved, Err(SaveError::Io(_))), "{saved:?}");
        assert!(kept.is_err());
        let s = source(&mut tab);
        assert_eq!(s.notice, Some(Notice::SaveFailed));
        assert!(s.dirty && s.text() == "# a\n\nmio\n", "le modifiche restano");
        assert_eq!(fs::read_to_string(&path).unwrap(), "# a\n");
        // Quando si torna a scrivere, la riga sparisce.
        assert!(save(&mut tab, &mut own).is_ok());
        assert_eq!(source(&mut tab).notice, None);
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
        s.set_text("# a\n\nmio\n");
        s.dirty = true;
        let mut own = OwnWrites::default();
        assert!(matches!(save(&mut tab, &mut own), Err(SaveError::Conflict)));
        assert_eq!(fs::read_to_string(&path).unwrap(), agent, "file su disco invariato");
        assert!(source(&mut tab).dirty, "le modifiche restano");
        assert!(matches!(leave(&mut tab, &mut own), Err(SaveError::Conflict)));
        assert!(matches!(tab.mode, Mode::Source(_)), "resta in modifica");
        assert_eq!(fs::read_to_string(&path).unwrap(), agent);
    }
}
