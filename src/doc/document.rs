//! The open document: blocks with ids stable from one read to the next.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{blocks, ids};
use super::select_all::SelectAllFlag;
use crate::open::kind::FileKind;

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
    /// The point of the last edit, for the "is writing" caret (`live::caret::locate`).
    pub at: Option<crate::live::caret::At>,
}

/// The next document id: a single one for the whole app, never reused.
static NEXT_DOC: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub struct Document {
    /// Unique in the app: every `Document` read takes a new one, even the same file reopened.
    /// Row keys use it (two tabs do not share the heights Rune measured) and
    /// `commit` rejects an update prepared for another document.
    pub id: u64,
    pub path: PathBuf,
    pub revision: u64,
    pub blocks: Vec<Block>,
    pub file_hash: u64,
    /// The footnotes of the blocks (`doc::footnotes`), redone at every revision.
    pub notes: Arc<super::footnotes::Notes>,
    /// The progress ("2 of 4") of the `Task N` headings by block id (`doc::labels::progress`),
    /// redone at every revision.
    pub tasks: Arc<std::collections::HashMap<u64, (u32, u32)>>,
    /// Whether the current selection is a Cmd+A (`ui::doc_view`); shared with the view's closures.
    pub select_all: Arc<SelectAllFlag>,
    /// What the file is drawn as (`open::kind`): its blocks are the Markdown [`shown`] makes of it.
    pub kind: FileKind,
    next_id: u64,
}

impl Document {
    /// Reads the file; non-UTF-8 bytes become U+FFFD. An image is not read: its view decodes it.
    pub fn load(path: &Path) -> io::Result<Document> {
        let kind = crate::open::kind::of(path);
        if kind == FileKind::Image {
            std::fs::metadata(path)?;
            return Ok(Self::from_source_as(path, "", kind));
        }
        let (source, _) = super::disk::read_lossy(path)?;
        Ok(Self::from_source_as(path, &source, kind))
    }

    /// A document of `source`, of the kind its name and text say.
    pub fn from_source(path: &Path, source: &str) -> Document {
        let kind = crate::open::kind::of_name(&path.to_string_lossy(), || Some(source.as_bytes().to_vec()));
        Self::from_source_as(path, source, kind)
    }

    pub fn from_source_as(path: &Path, source: &str, kind: FileKind) -> Document {
        let mut doc = Document {
            id: NEXT_DOC.fetch_add(1, Ordering::Relaxed),
            path: path.to_owned(),
            revision: 0,
            blocks: Vec::new(),
            file_hash: ids::hash_str(source),
            notes: Arc::default(),
            tasks: Arc::default(),
            select_all: Arc::default(),
            kind,
            next_id: 1,
        };
        let shown = shown(kind, source);
        let (blocks, _) = ids::assign(&[], ids::hash_sources(blocks::split(&shown)), &mut doc.next_id);
        doc.blocks = blocks;
        doc.notes = Arc::new(super::footnotes::Notes::of(doc.blocks.iter().map(|b| &*b.source)));
        doc.tasks = doc.tasks_now();
        doc
    }

    fn tasks_now(&self) -> Arc<std::collections::HashMap<u64, (u32, u32)>> {
        let by_index = super::labels::progress(self.blocks.iter().map(|b| &*b.source));
        Arc::new(by_index.into_iter().map(|(i, p)| (self.blocks[i].id, p)).collect())
    }

    pub fn update(&mut self, source: &str) -> Change {
        self.commit(self.snapshot().prepare(source)).expect("prepared from the current revision")
    }

    /// What it takes to prepare an update on another thread (`Snapshot::prepare`).
    pub fn snapshot(&self) -> Snapshot {
        Snapshot { doc: self.id, revision: self.revision, file_hash: self.file_hash, blocks: self.blocks.clone(), next_id: self.next_id, kind: self.kind }
    }

    /// Applies a prepared update. `None` if the document changed after the snapshot
    /// (the caller reads again) or if it was prepared for another document.
    pub fn commit(&mut self, p: Prepared) -> Option<Change> {
        if p.doc != self.id || p.revision != self.revision {
            return None;
        }
        self.blocks = p.blocks;
        self.notes = Arc::new(super::footnotes::Notes::of(self.blocks.iter().map(|b| &*b.source)));
        self.tasks = self.tasks_now();
        self.file_hash = p.file_hash;
        self.next_id = p.next_id;
        self.revision += 1;
        Some(p.change)
    }
}

/// The blocks of a `Document` at a revision; it moves to another thread (`Block.source` is
/// `Arc<str>`).
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// The id of the `Document` it comes from.
    doc: u64,
    revision: u64,
    pub file_hash: u64,
    blocks: Vec<Block>,
    next_id: u64,
    kind: FileKind,
}

impl Snapshot {
    /// The length of the blocks' source (without the empty lines between one block and the next).
    pub fn source_len(&self) -> usize {
        self.blocks.iter().map(|b| b.source.len()).sum()
    }

    /// The work of `Document::update` that does not touch the document: splitting into blocks, hashes and
    /// diff with stable ids, and the point of the last edit.
    pub fn prepare(&self, source: &str) -> Prepared {
        let mut next_id = self.next_id;
        let shown = shown(self.kind, source);
        let (blocks, mut change) = ids::assign(&self.blocks, ids::hash_sources(blocks::split(&shown)), &mut next_id);
        change.at = crate::live::caret::locate(&self.blocks, &blocks, &change);
        Prepared { doc: self.doc, revision: self.revision, blocks, change, file_hash: ids::hash_str(source), next_id }
    }
}

/// The Markdown a file of `kind` is drawn from: Markdown as it is; a text file as one fenced code
/// block in its language, so nothing in it is read as Markdown (a `# comment` stays a comment);
/// nothing for an image, which its own view draws.
pub fn shown(kind: FileKind, source: &str) -> std::borrow::Cow<'_, str> {
    match kind {
        FileKind::Markdown => source.into(),
        FileKind::Image => "".into(),
        FileKind::Json | FileKind::Jsonl => fenced(Some("json"), source).into(),
        FileKind::Code(lang) => fenced(lang, source).into(),
        FileKind::System => fenced(None, source).into(),
    }
}

/// `code` in a backtick fence longer than any backtick run inside it, in language `lang`
/// (`text` without one: the kit's plain code).
pub fn fenced(lang: Option<&str>, code: &str) -> String {
    let longest = code.as_bytes().split(|&b| b != b'`').map(<[u8]>::len).max().unwrap_or(0);
    let fence = "`".repeat((longest + 1).max(3));
    let code = code.strip_suffix('\n').unwrap_or(code);
    format!("{fence}{}\n{code}\n{fence}\n", lang.unwrap_or("text"))
}

/// An update ready for `Document::commit`.
#[derive(Debug)]
pub struct Prepared {
    doc: u64,
    revision: u64,
    blocks: Vec<Block>,
    change: Change,
    file_hash: u64,
    next_id: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A code file is one fenced block in its language: no Markdown inside it, even a `# comment`
    /// line, a fence or a blank line; a reload keeps it so (decision 2026-11-23-open-non-markdown).
    #[test]
    fn a_code_file_is_one_code_block() {
        let sh = "#!/bin/sh\n# Deploy the site\n\necho '```'\n\n- not a list\n";
        let mut doc = Document::from_source(Path::new("/r/deploy.sh"), sh);
        assert_eq!(doc.kind, FileKind::Code(Some("zsh")));
        assert_eq!(doc.blocks.len(), 1, "{:?}", doc.blocks);
        assert_eq!(&*doc.blocks[0].source, "````zsh\n#!/bin/sh\n# Deploy the site\n\necho '```'\n\n- not a list\n````");
        assert_eq!(doc.file_hash, ids::hash_str(sh), "the hash is the file's, for the reload");
        doc.update("# only a comment\n\n# another\n");
        assert_eq!(doc.blocks.len(), 1);
        assert!(doc.blocks[0].source.starts_with("```zsh\n# only a comment"));
        let md = Document::from_source(Path::new("/r/a.md"), "# Title\n\nText\n");
        assert_eq!(md.blocks.len(), 2, "Markdown unchanged");
    }

    /// A reload prepared for tab A never enters tab B, even with the same
    /// revision and the same path (a tab closed and another opened on the same file).
    #[test]
    fn a_reload_prepared_for_another_document_is_refused() {
        let p = Path::new("/r/a.md");
        let a = Document::from_source(p, "# a\n");
        let mut b = Document::from_source(p, "# a\n");
        assert_ne!(a.id, b.id, "unique ids");
        let prepared = a.snapshot().prepare("# a\n\nfresh\n");
        assert!(b.commit(prepared).is_none());
        assert_eq!(b.revision, 0);
    }
}
