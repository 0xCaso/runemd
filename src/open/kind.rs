//! What a path opens as (decision 2026-11-23-open-non-markdown): Markdown as today, JSON and
//! JSONL in the JSON viewer, other text files as one highlighted code card, images in an image
//! view, and everything else (PDF, binaries, unknown non-text) with the system app, no tab.

use std::io::Read;
use std::path::Path;

/// How a file opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Markdown,
    Json,
    Jsonl,
    /// A text file drawn as one code card, with the highlighter's language when it has one.
    Code(Option<&'static str>),
    Image,
    /// Opened with `open`: no tab.
    System,
}

impl FileKind {
    /// Whether the file gets a tab in the app.
    pub fn in_app(self) -> bool {
        self != FileKind::System
    }
}

/// Markdown, as before (the `.txt` of the old list stays prose).
const MARKDOWN: &[&str] = &["md", "markdown", "mdx", "txt"];
/// Images the decoder draws (rune: PNG, JPEG, GIF, WebP, BMP, ICO; SVG with resvg). HEIC, TIFF
/// and AVIF have no decoder in this build: they go to the system ([`SYSTEM`]).
const IMAGES: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico"];
/// Known non-text files: never sniffed, always the system app.
const SYSTEM: &[&str] = &[
    "pdf", "heic", "heif", "tif", "tiff", "avif", "psd", "zip", "gz", "tgz", "bz2", "xz", "7z", "rar", "dmg", "pkg", "app", "exe", "dll",
    "dylib", "so", "a", "o", "class", "jar", "wasm", "bin", "dat", "db", "sqlite", "sqlite3", "mp3", "m4a", "wav", "aiff", "flac", "ogg",
    "mp4", "mov", "m4v", "avi", "mkv", "webm", "ttf", "otf", "woff", "woff2", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "key", "pages",
    "numbers", "sketch", "fig", "icns",
];
/// Text files the highlighter draws plain: still a code card.
const PLAIN_TEXT: &[&str] = &[
    "csv", "tsv", "log", "env", "ini", "cfg", "conf", "config", "lock", "properties", "gitignore", "gitattributes", "editorconfig", "plist",
    "out", "err", "rst", "adoc", "tex", "srt", "vtt",
];
/// How much of an unknown file is read to tell text from binary.
const SNIFF: usize = 8 * 1024;

/// What `path` opens as; reads the first bytes of the file when the name alone cannot say.
pub fn of(path: &Path) -> FileKind {
    of_name(&path.to_string_lossy(), || head(path))
}

/// [`of`] from the file name, with `head` giving the file's first bytes (`None` if unreadable)
/// for the kinds that need a look inside: known text extensions (a binary `.plist`) and
/// unknown ones.
pub fn of_name(name: &str, head: impl FnOnce() -> Option<Vec<u8>>) -> FileKind {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let ext = base.rsplit_once('.').map(|(stem, e)| if stem.is_empty() { &base[1..] } else { e }).unwrap_or("").to_ascii_lowercase();
    let ext = ext.as_str();
    if MARKDOWN.contains(&ext) {
        return FileKind::Markdown;
    }
    if IMAGES.contains(&ext) {
        return FileKind::Image;
    }
    if SYSTEM.contains(&ext) {
        return FileKind::System;
    }
    let text = match ext {
        "json" => Some(FileKind::Json),
        "jsonl" | "ndjson" => Some(FileKind::Jsonl),
        _ => match rune_kit::syntax::lang_for_path(base) {
            Some(lang) => Some(FileKind::Code(Some(lang))),
            None if PLAIN_TEXT.contains(&ext) => Some(FileKind::Code(None)),
            None => None,
        },
    };
    match (text, head().map(|b| is_text(&b))) {
        // Binary bytes win over the name (`Info.plist` in binary form).
        (_, Some(false)) => FileKind::System,
        (Some(kind), _) => kind,
        (None, Some(true)) => FileKind::Code(None),
        // Unknown and unreadable: the system says what it is.
        (None, None) => FileKind::System,
    }
}

/// The first [`SNIFF`] bytes of `path`.
fn head(path: &Path) -> Option<Vec<u8>> {
    let mut buf = Vec::with_capacity(SNIFF);
    std::fs::File::open(path).ok()?.take(SNIFF as u64).read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// Text: no NUL byte and valid UTF-8, allowing a character cut at the end of the sample.
pub fn is_text(bytes: &[u8]) -> bool {
    if bytes.contains(&0) {
        return false;
    }
    match std::str::from_utf8(bytes) {
        Ok(_) => true,
        // `error_len() == None`: the bytes end inside a character, cut by the sample.
        Err(e) => e.error_len().is_none() && bytes.len() - e.valid_up_to() < 4,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(name: &str, bytes: &[u8]) -> FileKind {
        of_name(name, || Some(bytes.to_vec()))
    }

    #[test]
    fn markdown_stays_markdown() {
        assert_eq!(kind("/a/README.md", b"# hi"), FileKind::Markdown);
        assert_eq!(kind("notes.MARKDOWN", b""), FileKind::Markdown);
        assert_eq!(kind("todo.txt", b"x"), FileKind::Markdown);
        // Never sniffed: a Markdown file is opened as before, whatever its bytes.
        assert_eq!(of_name("a.md", || panic!("markdown is not sniffed")), FileKind::Markdown);
    }

    #[test]
    fn json_and_jsonl() {
        assert_eq!(kind("data/state.json", b"{}"), FileKind::Json);
        assert_eq!(kind("events.jsonl", b"{}\n{}"), FileKind::Jsonl);
        assert_eq!(kind("events.NDJSON", b"{}"), FileKind::Jsonl);
    }

    #[test]
    fn code_files_carry_their_language() {
        assert_eq!(kind("deploy.sh", b"#!/bin/sh\n# comment\n"), FileKind::Code(Some("zsh")));
        assert_eq!(kind("src/main.rs", b"fn main() {}"), FileKind::Code(Some("rust")));
        assert_eq!(kind("Cargo.toml", b"[package]"), FileKind::Code(Some("toml")));
        assert_eq!(kind("ci.yaml", b"a: 1"), FileKind::Code(Some("yaml")));
        assert_eq!(kind("Makefile", b"all:\n"), FileKind::Code(Some("makefile")));
        assert_eq!(kind("data.csv", b"a,b\n1,2\n"), FileKind::Code(None));
        assert_eq!(kind(".env", b"A=1\n"), FileKind::Code(None));
    }

    #[test]
    fn images() {
        for name in ["a.png", "b.JPG", "c.jpeg", "d.gif", "e.webp", "f.svg"] {
            assert_eq!(of_name(name, || panic!("images are not sniffed")), FileKind::Image, "{name}");
        }
    }

    #[test]
    fn pdf_heic_and_binaries_go_to_the_system() {
        for name in ["paper.pdf", "photo.HEIC", "a.zip", "song.mp3", "deck.key"] {
            assert_eq!(of_name(name, || panic!("known kinds are not sniffed")), FileKind::System, "{name}");
        }
        assert!(!FileKind::System.in_app());
    }

    #[test]
    fn unknown_extensions_are_sniffed() {
        assert_eq!(kind("notes.weird", b"just some text\n"), FileKind::Code(None));
        assert_eq!(kind("LICENSE", b"MIT License\n"), FileKind::Code(None));
        assert_eq!(kind("blob.weird", b"abc\0def"), FileKind::System, "a NUL byte");
        assert_eq!(kind("blob.weird", &[0xff, 0xfe, 0x41]), FileKind::System, "invalid UTF-8");
        assert_eq!(of_name("gone.weird", || None), FileKind::System, "unreadable");
        // A known text extension with binary bytes (a binary plist).
        assert_eq!(kind("Info.plist", b"bplist00\0\x01"), FileKind::System);
    }

    #[test]
    fn a_character_cut_by_the_sample_is_still_text() {
        let mut bytes = "caffè".repeat(3).into_bytes();
        bytes.pop();
        assert!(is_text(&bytes));
        assert!(!is_text(&[b'a', 0xe8, b'b']));
    }
}
