//! Source editing (Cmd+E): the whole file, split into pieces with one field each
//! ([`blocks`]), saved with Cmd+S or when going back to reading.
//!
//! On entry the file hash is remembered ([`SourceEdit::base_hash`]); each save
//! compares it with the disk and, if it changed, does not write ([`SaveError::Conflict`]): never
//! overwrite silently. The tab then shows the "The file changed on disk" row
//! ([`Notice::Conflict`]) until "Reload" ([`reload`]) or "Keep mine"
//! ([`keep_mine`]) is chosen. A write error is not a conflict: the row says "Save
//! failed" ([`Notice::SaveFailed`]) and the changes stay.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

pub mod blocks;
pub mod new_file;
pub mod recovery;

use crate::app::{Mode, Tab};
use crate::doc::disk::read_disk;
use crate::doc::document::Document;
use crate::doc::ids::hash_str;
use crate::live::reload::OwnWrites;
use blocks::Pieces;

/// A tab's source being edited.
#[derive(Debug)]
pub struct SourceEdit {
    /// The text, in pieces joined with `"\n"` (one field per piece).
    pub pieces: Pieces,
    /// The hash of the content the edit starts from (the same `ids::hash_str` as the reload):
    /// if the disk no longer has it, someone else wrote the file.
    pub base_hash: u64,
    /// The text changed since the last save.
    pub dirty: bool,
    /// The row above the source after a save that did not write; it stays until a
    /// save succeeds or a choice is made (even with the tab in the background).
    pub notice: Option<Notice>,
    /// Cmd+W or middle click with unsaved changes: instead of `notice` the row asks
    /// "Save changes to …?" (Save, Don't save, Cancel). Each save removes it: if it
    /// does not write, the `notice` row comes back.
    pub asking: bool,
    /// The starting text was not UTF-8 (read with `from_utf8_lossy`): it is never written,
    /// not even with "Keep mine".
    pub lossy: bool,
}

/// Why the last save did not write: what the row above the source says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    /// "The file changed on disk", with "Reload" and "Keep mine".
    Conflict,
    /// "Save failed": a read or write error, not a conflict.
    SaveFailed,
    /// "The file is not UTF-8": the text has U+FFFD in place of the invalid
    /// bytes, and writing it would lose them.
    NotUtf8,
}

/// The save did not write.
#[derive(Debug)]
pub enum SaveError {
    /// The file changed on disk after entry (or the last save), or it is gone.
    Conflict,
    /// The file cannot be read or written (permissions, disk full).
    Io(io::Error),
    /// The file on disk, or the one the edit started from, is not UTF-8: it is not written.
    NotUtf8,
}

impl SourceEdit {
    /// The whole source text.
    pub fn text(&self) -> String {
        self.pieces.text()
    }

    /// Replaces the whole text (splits the pieces again, cursor at the start).
    #[cfg(test)]
    pub fn set_text(&mut self, text: &str) {
        self.pieces.set_text(text);
    }
}

/// The file can be edited: it reads and is UTF-8. One that is not stays in reading (Cmd+E,
/// Cmd+N on a name that exists): the source would rewrite it with U+FFFD in place of the invalid
/// bytes. A file that cannot be read is left to [`enter`].
pub fn editable(path: &Path) -> bool {
    !matches!(read_disk(path), Ok((_, _, false)))
}

/// The file on disk no longer has the content `base_hash` (the one the edit starts from, or of
/// our last save): someone else wrote it. A file that cannot be read does not count.
pub fn changed_on_disk(path: &Path, base_hash: u64) -> bool {
    read_disk(path).is_ok_and(|(_, h, _)| h != base_hash)
}

/// Enters editing: the source is the file as it is now on disk (the document holds only the
/// blocks, not the blank lines between them), with the cursor at the start and focus in the
/// field. If the file cannot be read (gone during a rename) it starts from the document's blocks.
pub fn enter(doc: &Document) -> SourceEdit {
    let (text, base_hash, utf8) = match read_disk(&doc.path) {
        Ok(d) => d,
        Err(_) => {
            let text = doc.blocks.iter().map(|b| &*b.source).collect::<Vec<_>>().join("\n\n") + "\n";
            let utf8 = !text.contains(char::REPLACEMENT_CHARACTER);
            (text, doc.file_hash, utf8)
        }
    };
    SourceEdit { pieces: Pieces::new(&text), base_hash, dirty: false, notice: None, asking: false, lossy: !utf8 }
}

/// Saves the tab's source if the disk still has the content the edit started
/// from; otherwise it does not write and returns [`SaveError::Conflict`]. An error does not write and
/// lights the row ([`SourceEdit::notice`]); a successful save turns it off. It always removes the
/// close question ([`SourceEdit::asking`]). The save also updates the
/// document, so reading shows the saved text without going through the reload: no
/// changed-block mark, fire or "writing". The reread the watcher will do of our
/// write is skipped (`OwnWrites`). Outside editing it does nothing.
pub fn save(tab: &mut Tab, own: &mut OwnWrites) -> Result<(), SaveError> {
    let result = try_save(tab, own);
    if let Mode::Source(src) = &mut tab.mode {
        src.asking = false;
        src.notice = match &result {
            Ok(()) => None,
            Err(SaveError::Conflict) => Some(Notice::Conflict),
            Err(SaveError::Io(_)) => Some(Notice::SaveFailed),
            Err(SaveError::NotUtf8) => Some(Notice::NotUtf8),
        };
    }
    if let Err(SaveError::Io(e)) = &result {
        eprintln!("runemd: {}: save failed: {e}", tab.doc.path.display());
    }
    result
}

fn try_save(tab: &mut Tab, own: &mut OwnWrites) -> Result<(), SaveError> {
    let Mode::Source(src) = &mut tab.mode else { return Ok(()) };
    if src.lossy {
        return Err(SaveError::NotUtf8);
    }
    let target = tab.follow.path().to_owned();
    match read_disk(&target) {
        // The comparison first: an external change (even non-UTF-8) is a conflict resolved
        // in place. The non-UTF-8 refusal stays for the unchanged file that would lose bytes.
        Ok((_, h, _)) if h != src.base_hash => return Err(SaveError::Conflict),
        Ok((_, _, false)) => return Err(SaveError::NotUtf8),
        Ok(_) => {}
        // Gone (a rename halfway, or deleted): it changed on disk.
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

/// The save at quit did not write: the source text goes into a recovery copy in
/// `dir` ([`recovery::write`]) and the tab has no more changes to save, so a second hook
/// of the same quit does not write another copy. The tab's file is not touched.
pub fn recover(tab: &mut Tab, dir: &Path, stamp: &str) -> io::Result<std::path::PathBuf> {
    let Mode::Source(src) = &mut tab.mode else { return Err(io::Error::other("not editing")) };
    let copy = recovery::write(dir, &tab.doc.path, &src.text(), stamp)?;
    src.dirty = false;
    Ok(copy)
}

/// "Keep mine": writes the source text over what is on disk, without comparing,
/// and makes it the new starting point. Like a save, the watcher's reread is an
/// app write (no marks, fire or dither). The tab stays in the source.
pub fn keep_mine(tab: &mut Tab, own: &mut OwnWrites) -> io::Result<()> {
    if let Mode::Source(src) = &mut tab.mode
        && src.lossy
    {
        src.notice = Some(Notice::NotUtf8);
        return Err(io::Error::new(io::ErrorKind::InvalidData, "the file is not UTF-8"));
    }
    let result = commit(tab, own);
    if let Mode::Source(src) = &mut tab.mode {
        match &result {
            Ok(()) => (src.dirty, src.notice) = (false, None),
            Err(e) => {
                eprintln!("runemd: {}: save failed: {e}", tab.doc.path.display());
                src.notice = Some(Notice::SaveFailed);
            }
        }
    }
    result
}

/// "Reload": discards the changes and starts again from the file as it is now on disk, still in the
/// source (the row belongs to the source; Cmd+E then goes back to reading). The cursor stays where it was, if
/// the new text reaches that far. It writes nothing. If the file cannot be read (deleted) there is
/// nothing to reread: the changes and the row stay, and only "Keep mine" recreates it.
pub fn reload(tab: &mut Tab) {
    let Mode::Source(src) = &mut tab.mode else { return };
    if read_disk(&tab.doc.path).is_err() {
        return;
    }
    let caret = src.pieces.caret();
    let mut fresh = enter(&tab.doc);
    fresh.pieces.set_caret(caret);
    **src = fresh;
}

/// Writes the source text to the tab's file, updates the document and `base_hash`.
fn commit(tab: &mut Tab, own: &mut OwnWrites) -> io::Result<()> {
    let Mode::Source(src) = &mut tab.mode else { return Ok(()) };
    let text = src.text();
    let hash = hash_str(&text);
    write(tab.follow.path(), &text)?;
    // On the UI thread, before any `poll`: the watcher's reread finds it.
    own.remember(&tab.doc.path, hash);
    tab.doc.update(&text);
    src.base_hash = hash;
    Ok(())
}

/// Leaves editing: saves if there are changes, then goes back to reading. If the save
/// does not write it stays in editing, with the row. During editing the reload was suspended: the file is reread, so a
/// write that arrived in the meantime shows (ours is skipped).
pub fn leave(tab: &mut Tab, own: &mut OwnWrites) -> Result<(), SaveError> {
    if matches!(&tab.mode, Mode::Source(s) if s.dirty) {
        save(tab, own)?;
    }
    tab.mode = Mode::Read;
    tab.follow.reread();
    Ok(())
}

/// Writes `text` to `path` with a file next to it and a rename: whoever reads (our reload, an
/// agent) sees the file before or after, never halfway. The new file takes the permissions of the old one,
/// if it still exists ("Keep mine" on a deleted file recreates it).
fn write(path: &Path, text: &str) -> io::Result<()> {
    let name = path.file_name().ok_or_else(|| io::Error::other("path without a name"))?;
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
            Mode::Read => panic!("not editing"),
        }
    }

    fn mtime(path: &Path) -> std::time::SystemTime {
        fs::metadata(path).unwrap().modified().unwrap()
    }

    #[test]
    fn leave_without_changes_does_not_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n\none\n").unwrap();
        // An mtime in the past: a write would bring it to now.
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        fs::File::options().write(true).open(&path).unwrap().set_modified(old).unwrap();
        let mut tab = tab_on(&path);
        tab.mode = Mode::Source(Box::new(enter(&tab.doc)));
        assert_eq!(source(&mut tab).text(), "# a\n\none\n", "the whole source of the file");
        let mut own = OwnWrites::default();
        assert!(leave(&mut tab, &mut own).is_ok());
        assert!(matches!(tab.mode, Mode::Read), "back to reading");
        assert_eq!(mtime(&path), old, "file not written");
        assert_eq!(fs::read_to_string(&path).unwrap(), "# a\n\none\n");
    }

    /// Typed and then deleted: `dirty` stays true but the text is the starting one, nothing is written.
    #[test]
    fn leave_with_the_text_back_to_the_start_does_not_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n\none\n").unwrap();
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        fs::File::options().write(true).open(&path).unwrap().set_modified(old).unwrap();
        let mut tab = tab_on(&path);
        tab.mode = Mode::Source(Box::new(enter(&tab.doc)));
        let s = source(&mut tab);
        s.pieces.current().insert("X");
        s.dirty = true;
        s.pieces.current().set_selection(0, 1);
        s.pieces.current().insert("");
        assert_eq!(s.text(), "# a\n\none\n");
        assert!(s.dirty);
        assert!(leave(&mut tab, &mut OwnWrites::default()).is_ok());
        assert!(matches!(tab.mode, Mode::Read));
        assert_eq!(mtime(&path), old, "file not written");
    }

    #[test]
    fn save_writes_and_remembers_hash() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n\none\n").unwrap();
        let mut tab = tab_on(&path);
        tab.mode = Mode::Source(Box::new(enter(&tab.doc)));
        let mine = "# a\n\none\n\ndue\n";
        let s = source(&mut tab);
        s.set_text(mine);
        s.dirty = true;
        let mut own = OwnWrites::default();
        assert!(save(&mut tab, &mut own).is_ok());
        assert_eq!(fs::read_to_string(&path).unwrap(), mine);
        let hash = hash_str(mine);
        assert!(own.is_own(&tab.doc.path, hash), "the reread will be an app write");
        let s = source(&mut tab);
        assert_eq!((s.base_hash, s.dirty), (hash, false), "the new starting point");
        // Reading shows the saved text, without going through the reload.
        assert_eq!(tab.doc.file_hash, hash);
        assert_eq!(tab.doc.blocks.len(), 3);
    }

    /// Running as root (euid 0): a read-only folder does not stop writes, and the tests
    /// that rely on 0o555 are skipped. The folder just created belongs to the euid.
    pub(crate) fn as_root(dir: &Path) -> bool {
        use std::os::unix::fs::MetadataExt;
        let root = fs::metadata(dir).unwrap().uid() == 0;
        if root {
            eprintln!("skipped: as root a 0o555 folder does not stop writes");
        }
        root
    }

    /// Puts the tab in the source with `mine` as the edit.
    fn editing(path: &Path, mine: &str) -> Tab {
        let mut tab = tab_on(path);
        tab.mode = Mode::Source(Box::new(enter(&tab.doc)));
        let s = source(&mut tab);
        s.set_text(mine);
        s.dirty = true;
        tab
    }

    /// Two external writes during editing: each save finds the conflict, the row
    /// stays until a choice is made and the disk keeps the agent's text.
    #[test]
    fn two_external_writes_keep_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n").unwrap();
        let mut tab = editing(&path, "# a\n\nmine\n");
        let mut own = OwnWrites::default();
        fs::write(&path, "# a\n\nprima\n").unwrap();
        assert!(matches!(save(&mut tab, &mut own), Err(SaveError::Conflict)));
        assert_eq!(source(&mut tab).notice, Some(Notice::Conflict), "the row shows");
        fs::write(&path, "# a\n\nsecond\n").unwrap();
        assert!(matches!(save(&mut tab, &mut own), Err(SaveError::Conflict)));
        assert!(matches!(leave(&mut tab, &mut own), Err(SaveError::Conflict)));
        let s = source(&mut tab);
        assert_eq!(s.notice, Some(Notice::Conflict), "the row stays until a choice is made");
        assert!(s.dirty && s.text() == "# a\n\nmine\n", "the changes stay");
        assert_eq!(fs::read_to_string(&path).unwrap(), "# a\n\nsecond\n", "no write to disk");
    }

    /// "Reload": the changes are discarded and the source starts again from disk, without writing.
    #[test]
    fn reload_discards_edits_and_rereads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n").unwrap();
        let mut tab = editing(&path, "# a\n\nmine\n");
        let agent = "# a\n\nby the agent\n";
        fs::write(&path, agent).unwrap();
        let mut own = OwnWrites::default();
        assert!(save(&mut tab, &mut own).is_err());
        reload(&mut tab);
        let s = source(&mut tab);
        assert_eq!((s.text().as_str(), s.dirty, s.notice), (agent, false, None));
        assert_eq!(s.base_hash, hash_str(agent), "the new starting point");
        assert_eq!(fs::read_to_string(&path).unwrap(), agent);
        // Now it saves over the agent's text without a conflict.
        source(&mut tab).pieces.set_caret(0);
        source(&mut tab).pieces.current().insert("X");
        source(&mut tab).dirty = true;
        assert!(save(&mut tab, &mut own).is_ok());
        assert_eq!(fs::read_to_string(&path).unwrap(), format!("X{agent}"));
    }

    /// "Reload" with the file deleted: there is nothing to reread, so the changes and the
    /// row stay; only "Keep mine" recreates the file.
    #[test]
    fn reload_of_a_deleted_file_keeps_the_edits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n").unwrap();
        let mine = "# a\n\nmine\n";
        let mut tab = editing(&path, mine);
        fs::remove_file(&path).unwrap();
        let mut own = OwnWrites::default();
        assert!(matches!(save(&mut tab, &mut own), Err(SaveError::Conflict)));
        reload(&mut tab);
        let s = source(&mut tab);
        assert_eq!((s.text().as_str(), s.dirty, s.notice), (mine, true, Some(Notice::Conflict)));
        assert!(!path.exists(), "nothing written");
        assert!(keep_mine(&mut tab, &mut own).is_ok());
        assert_eq!(fs::read_to_string(&path).unwrap(), mine, "Keep mine recreates the file");
        assert_eq!(source(&mut tab).notice, None);
    }

    /// "Keep mine": writes the local text over the agent's, updates `base_hash` and
    /// remembers the write (the reread lights no marks, fire or dither).
    #[test]
    fn keep_mine_writes_and_rebases() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n").unwrap();
        let mine = "# a\n\nmine\n";
        let mut tab = editing(&path, mine);
        fs::write(&path, "# a\n\nby the agent\n").unwrap();
        let mut own = OwnWrites::default();
        assert!(save(&mut tab, &mut own).is_err());
        assert!(keep_mine(&mut tab, &mut own).is_ok());
        assert_eq!(fs::read_to_string(&path).unwrap(), mine);
        let hash = hash_str(mine);
        assert!(own.is_own(&tab.doc.path, hash), "the reread will be an app write");
        assert_eq!(tab.doc.file_hash, hash, "reading shows my text");
        let s = source(&mut tab);
        assert_eq!((s.base_hash, s.dirty, s.notice), (hash, false, None));
        assert!(matches!(tab.mode, Mode::Source(_)), "stays in the source");
    }

    /// A write error is not a conflict: no "The file changed on disk" row but
    /// "Save failed", and the changes stay.
    #[test]
    fn an_io_error_is_not_a_conflict() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        if as_root(dir.path()) {
            return;
        }
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n").unwrap();
        let mut tab = editing(&path, "# a\n\nmine\n");
        // Read-only folder: the file next to it for the rename cannot be created.
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
        let mut own = OwnWrites::default();
        let saved = save(&mut tab, &mut own);
        let kept = keep_mine(&mut tab, &mut own);
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(saved, Err(SaveError::Io(_))), "{saved:?}");
        assert!(kept.is_err());
        let s = source(&mut tab);
        assert_eq!(s.notice, Some(Notice::SaveFailed));
        assert!(s.dirty && s.text() == "# a\n\nmine\n", "the changes stay");
        assert_eq!(fs::read_to_string(&path).unwrap(), "# a\n");
        // When writing works again, the row goes away.
        assert!(save(&mut tab, &mut own).is_ok());
        assert_eq!(source(&mut tab).notice, None);
    }

    /// A file that is not UTF-8 is never saved (the source text would have U+FFFD in
    /// place of the invalid bytes), not even with "Keep mine": the bytes stay the same.
    #[test]
    fn save_refuses_a_file_that_is_not_utf8() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        let bytes = b"# a\n\ncaff\xe8 \xff\n".to_vec();
        fs::write(&path, &bytes).unwrap();
        let mut tab = tab_on(&path);
        tab.mode = Mode::Source(Box::new(enter(&tab.doc)));
        let s = source(&mut tab);
        let text = s.text();
        s.set_text(&format!("{text}X\n"));
        s.dirty = true;
        let mut own = OwnWrites::default();
        assert!(matches!(save(&mut tab, &mut own), Err(SaveError::NotUtf8)));
        assert_eq!(source(&mut tab).notice, Some(Notice::NotUtf8));
        assert!(keep_mine(&mut tab, &mut own).is_err());
        assert!(matches!(leave(&mut tab, &mut own), Err(SaveError::NotUtf8)));
        assert_eq!(fs::read(&path).unwrap(), bytes, "same bytes");
    }

    /// An agent writes non-UTF-8 bytes while editing: it is a conflict (Reload / Keep
    /// mine), not "editing disabled". Nothing is written.
    #[test]
    fn external_non_utf8_write_is_a_conflict() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n").unwrap();
        let mut tab = tab_on(&path);
        tab.mode = Mode::Source(Box::new(enter(&tab.doc)));
        let bytes = b"# a\n\ncaff\xe8 \xff\n".to_vec();
        fs::write(&path, &bytes).unwrap();
        let s = source(&mut tab);
        s.set_text("# a\n\nmine\n");
        s.dirty = true;
        let mut own = OwnWrites::default();
        assert!(matches!(save(&mut tab, &mut own), Err(SaveError::Conflict)));
        assert_eq!(source(&mut tab).notice, Some(Notice::Conflict));
        assert_eq!(fs::read(&path).unwrap(), bytes, "same bytes");
    }

    #[test]
    fn save_refuses_when_disk_changed() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.md");
        fs::write(&path, "# a\n").unwrap();
        let mut tab = tab_on(&path);
        tab.mode = Mode::Source(Box::new(enter(&tab.doc)));
        let agent = "# a\n\nby the agent\n";
        fs::write(&path, agent).unwrap();
        let s = source(&mut tab);
        s.set_text("# a\n\nmine\n");
        s.dirty = true;
        let mut own = OwnWrites::default();
        assert!(matches!(save(&mut tab, &mut own), Err(SaveError::Conflict)));
        assert_eq!(fs::read_to_string(&path).unwrap(), agent, "file on disk unchanged");
        assert!(source(&mut tab).dirty, "the changes stay");
        assert!(matches!(leave(&mut tab, &mut own), Err(SaveError::Conflict)));
        assert!(matches!(tab.mode, Mode::Source(_)), "stays in editing");
        assert_eq!(fs::read_to_string(&path).unwrap(), agent);
    }
}
