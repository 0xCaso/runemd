//! Paths and file names in the text (record 2026-11-16-paths-name-and-link): where they are in a
//! block, with or without backticks, and which file on disk they lead to.
//!
//! Recognition is cautious (constraint 5): a path ends with a file name with
//! an extension that has at least one letter (`v1.2`, `0.5`, `etc.` do not); without `/` it counts only with
//! a known extension and not as a library name (`Node.js`). URLs, links,
//! autolinks, HTML and code blocks are not touched (constraint 4).

use std::ops::Range;
use std::path::{Path, PathBuf};

/// A path found in a block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The source bytes it covers, backticks included.
    pub range: Range<usize>,
    /// The path as written, without `:line`.
    pub path: String,
    /// The line of `:line`, if there is one.
    pub line: Option<u32>,
    /// The last line of `:from-to`.
    pub to: Option<u32>,
    /// A folder-looking word or a link: if it leads nowhere it stays as it was written.
    pub quiet: bool,
    /// The destination of a `[text](destination)` link: only it is replaced, the text stays.
    pub link: bool,
}

impl Found {
    /// The path split into folders (with the last `/`) and file name.
    pub fn parts(&self) -> (&str, &str) {
        match self.path.rfind('/') {
            Some(i) => self.path.split_at(i + 1),
            None => ("", &self.path),
        }
    }
}

/// The extensions with which a name without `/` is a file.
const KNOWN: &[&str] = &[
    "md", "markdown", "mdx", "txt", "rs", "toml", "lock", "json", "jsonl", "yaml", "yml", "sh", "zsh", "py", "rb", "go", "c", "h", "cc", "cpp", "hpp", "swift", "kt", "java", "js", "mjs", "cjs", "ts", "tsx", "jsx", "css", "scss", "html", "svg", "png", "jpg", "jpeg", "gif", "webp", "pdf", "csv", "sql", "xml", "plist", "wgsl", "metal", "env", "ini", "cfg", "conf", "log", "diff", "patch",
];

/// The paths of `source`, a top-level block, in order. Nothing in code blocks and
/// in HTML.
pub fn find(source: &str) -> Vec<Found> {
    let first = source.trim_start_matches([' ', '\n']);
    if first.starts_with("```") || first.starts_with("~~~") || first.starts_with('<') || source.starts_with("    ") || source.starts_with('\t') {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut start = 0;
    for line in source.split_inclusive('\n') {
        if !is_reference(line) {
            in_line(line, start, &mut out);
        }
        start += line.len();
    }
    out
}

/// A reference definition `[x]: url`: not touched.
fn is_reference(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with('[') && t.find("]:").is_some()
}

fn in_line(line: &str, at: usize, out: &mut Vec<Found>) {
    let b = line.as_bytes();
    let mut i = 0;
    let mut word = None;
    let flush = |word: &mut Option<usize>, end: usize, out: &mut Vec<Found>| {
        if let Some(s) = word.take()
            && let Some(t) = token(&line[s..end], false)
        {
            out.push(Found { range: at + s + t.range.start..at + s + t.range.end, path: t.path, line: t.line, to: t.to, quiet: t.quiet, link: false });
        }
    };
    while i < b.len() {
        let c = b[i];
        match c {
            b'`' => {
                flush(&mut word, i, out);
                let run = b[i..].iter().take_while(|&&x| x == b'`').count();
                let fence = &line[i..i + run];
                match line[i + run..].find(fence) {
                    Some(j) => {
                        let inner = &line[i + run..i + run + j];
                        if let Some(t) = token(inner.trim(), true)
                            && t.range.len() == inner.trim().len()
                        {
                            out.push(Found { range: at + i..at + i + run + j + run, path: t.path, line: t.line, to: t.to, quiet: t.quiet, link: false });
                        }
                        i += run + j + run;
                    }
                    None => i += run,
                }
                continue;
            }
            // A link (`[text](destination)`): its text stays as it is, a local destination
            // is found (images are left alone).
            b'[' => {
                flush(&mut word, i, out);
                let (end, dest) = skip_link(line, i);
                if let Some(d) = dest.filter(|_| i == 0 || b[i - 1] != b'!')
                    && let Some(f) = destination(&line[d.clone()])
                {
                    out.push(Found { range: at + d.start..at + d.end, ..f });
                }
                i = end;
                continue;
            }
            // An autolink or inline HTML.
            b'<' => {
                flush(&mut word, i, out);
                i = line[i..].find('>').map_or(i + 1, |j| i + j + 1);
                continue;
            }
            b' ' | b'\t' | b'\n' | b'\r' | b'|' => flush(&mut word, i, out),
            _ => {
                if word.is_none() {
                    word = Some(i);
                }
            }
        }
        i += 1;
    }
    flush(&mut word, b.len(), out);
}

/// After the link that starts at `i` (`[`), or right after `[` if it is not a link, and the bytes
/// of its destination (title excluded).
fn skip_link(line: &str, i: usize) -> (usize, Option<Range<usize>>) {
    let Some(close) = line[i..].find("](") else { return (i + 1, None) };
    let dest = i + close + 2;
    match line[dest..].find(')') {
        Some(j) => {
            let inner = &line[dest..dest + j];
            let lead = inner.len() - inner.trim_start().len();
            let len = if inner[lead..].starts_with('<') { inner[lead..].find('>').map_or(0, |k| k + 1) } else { inner[lead..].find(' ').unwrap_or(inner.len() - lead) };
            (dest + j + 1, (len > 0).then(|| dest + lead..dest + lead + len))
        }
        None => (i + 1, None),
    }
}

/// A local link destination (`docs/a.md`, `<b c.md>`, `a.rs#L9`): not a URL, `#heading` or
/// `mailto:`. The range is filled in by the caller.
fn destination(dest: &str) -> Option<Found> {
    let d = dest.strip_prefix('<').and_then(|d| d.strip_suffix('>')).unwrap_or(dest);
    if d.is_empty() || d.starts_with('#') || d.contains(':') {
        return None;
    }
    let (path, line) = match d.split_once('#') {
        Some((p, frag)) => (p, frag.strip_prefix('L').and_then(|n| n.parse().ok())),
        None => (d, None),
    };
    (!path.is_empty()).then(|| Found { range: 0..0, path: path.to_owned(), line, to: None, quiet: true, link: true })
}

/// A path inside a word ([`token`]).
struct Token {
    /// The bytes it covers, without the punctuation around it, with `:line`.
    range: Range<usize>,
    path: String,
    line: Option<u32>,
    to: Option<u32>,
    quiet: bool,
}

/// The path inside a word; `code` if the word is inline code.
fn token(word: &str, code: bool) -> Option<Token> {
    if word.contains("://") || word.to_ascii_lowercase().starts_with("www.") {
        return None;
    }
    let lead = word.len() - word.trim_start_matches(['(', '"', '\'', '*', '“', '«']).len();
    let mut end = word.len();
    while let Some(c) = word[lead..end].chars().next_back() {
        if matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | ')' | '"' | '\'' | '*' | '”' | '»') {
            end -= c.len_utf8();
        } else {
            break;
        }
    }
    let body = &word[lead..end];
    let lines = |n: &str| -> Option<(u32, Option<u32>)> {
        let (a, b) = match n.split_once('-') {
            Some((a, b)) => (a, Some(b)),
            None => (n, None),
        };
        let a = a.parse().ok().filter(|_| a.bytes().all(|c| c.is_ascii_digit()))?;
        match b {
            Some(b) if b.bytes().all(|c| c.is_ascii_digit()) => Some((a, Some(b.parse().ok()?))),
            Some(_) => None,
            None => Some((a, None)),
        }
    };
    let (core, line, to) = match body.rsplit_once(':').and_then(|(p, n)| Some((p, lines(n)?))) {
        Some((p, (a, b))) => (p, Some(a), b),
        None => (body, None, None),
    };
    let quiet = if is_path(core) {
        false
    } else if is_folder(core, code) {
        true
    } else {
        return None;
    };
    Some(Token { range: lead..end, path: core.to_owned(), line, to, quiet })
}

/// `s` looks like a folder: in inline code with a `/`, in the text ending with `/` or starting
/// with `~/` or `/`. It counts only if it exists.
fn is_folder(s: &str, code: bool) -> bool {
    if !plain(s) || !s.bytes().any(|b| b.is_ascii_alphanumeric()) {
        return false;
    }
    (code && s.contains('/')) || s.ends_with('/') || s.starts_with("~/") || s.starts_with('/')
}

/// Only path characters, no `//`, `~` only as `~/` at the start.
fn plain(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/' | b'~' | b'+' | b'@')) && !s.contains("//") && !(s.contains('~') && !s.starts_with("~/")) && !s[1..].contains('~')
}

/// `s` is a path or a file name.
fn is_path(s: &str) -> bool {
    if !plain(s) {
        return false;
    }
    let name = s.rsplit('/').next().unwrap_or(s);
    let Some((stem, ext)) = name.rsplit_once('.') else { return false };
    let stem_ok = stem.bytes().any(|b| b.is_ascii_alphanumeric());
    let ext_ok = (1..=8).contains(&ext.len()) && ext.bytes().all(|b| b.is_ascii_alphanumeric()) && ext.bytes().any(|b| b.is_ascii_alphabetic());
    if !stem_ok || !ext_ok {
        return false;
    }
    if s.contains('/') {
        return true;
    }
    // Without `/`: a known extension, a name with at least one letter, and not a capitalized
    // library name (`Node.js`, `Vue.js`).
    let lib = stem.chars().next().is_some_and(char::is_uppercase) && stem.chars().all(char::is_alphabetic) && matches!(ext, "js" | "ts");
    KNOWN.contains(&ext) && stem.bytes().any(|b| b.is_ascii_alphabetic()) && !lib
}

/// Where relative paths are looked up: the file's folder and the root of its repo.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Places {
    pub dir: PathBuf,
    pub root: Option<PathBuf>,
    pub home: Option<PathBuf>,
}

impl Places {
    /// The places of file `file` (repo from `repo::of`, home from `$HOME`).
    pub fn of(file: &Path) -> Places {
        let dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
        let repo = crate::repo::of(file).root;
        let root = (repo != dir || dir.join(".git").exists()).then_some(repo);
        Places { dir, root, home: std::env::var_os("HOME").map(PathBuf::from) }
    }

    /// The file `path` leads to: absolute, `~/`, relative to the file's folder or to the
    /// repo root, in this order. `None` if it is not an existing file.
    pub fn resolve(&self, path: &str) -> Option<PathBuf> {
        let tries: Vec<PathBuf> = if let Some(rest) = path.strip_prefix("~/") {
            self.home.iter().map(|h| h.join(rest)).collect()
        } else if Path::new(path).is_absolute() {
            vec![PathBuf::from(path)]
        } else {
            std::iter::once(self.dir.join(path)).chain(self.root.iter().map(|r| r.join(path))).collect()
        };
        tries.into_iter().find(|p| p.is_file())
    }
}

/// What a path leads to ([`Places::find`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub path: PathBuf,
    pub dir: bool,
    pub len: u64,
}

impl Places {
    /// The file or folder `path` leads to, in the order of [`Places::resolve`].
    pub fn find(&self, path: &str) -> Option<Hit> {
        let path = path.strip_suffix('/').filter(|p| !p.is_empty()).unwrap_or(path);
        let tries: Vec<PathBuf> = if let Some(rest) = path.strip_prefix("~/") {
            self.home.iter().map(|h| h.join(rest)).collect()
        } else if Path::new(path).is_absolute() {
            vec![PathBuf::from(path)]
        } else {
            std::iter::once(self.dir.join(path)).chain(self.root.iter().map(|r| r.join(path))).collect()
        };
        tries.into_iter().find_map(|p| {
            let m = std::fs::metadata(&p).ok()?;
            Some(Hit { dir: m.is_dir(), len: m.len(), path: p })
        })
    }

    /// `path` with the home folder written `~`.
    pub fn short(&self, path: &Path) -> String {
        match self.home.as_deref().and_then(|h| path.strip_prefix(h).ok()) {
            Some(rest) => format!("~/{}", rest.display()),
            None => path.display().to_string(),
        }
    }
}

/// For files that appear in a document with the same name, the shortest folders that tell them
/// apart (T3 Code's `fileLinks.ts`): the last parent folders, at least two.
#[derive(Debug, Clone, Default)]
pub struct Labels(std::collections::HashMap<PathBuf, String>);

impl Labels {
    /// The labels of the paths in `blocks` (link destinations excluded: their text is the author's).
    pub fn of<'a>(blocks: impl Iterator<Item = &'a str>, places: &Places) -> Labels {
        let mut by_name: std::collections::HashMap<std::ffi::OsString, Vec<PathBuf>> = Default::default();
        for f in blocks.flat_map(find).filter(|f| !f.link) {
            let Some(hit) = places.find(&f.path) else { continue };
            let Some(name) = hit.path.file_name().map(|n| n.to_owned()) else { continue };
            let same = by_name.entry(name).or_default();
            if !same.contains(&hit.path) {
                same.push(hit.path);
            }
        }
        let mut out = std::collections::HashMap::new();
        for same in by_name.values().filter(|v| v.len() > 1) {
            let all: Vec<&Path> = same.iter().map(PathBuf::as_path).collect();
            for p in &all {
                out.insert(p.to_path_buf(), label_of(p, &all));
            }
        }
        Labels(out)
    }

    pub fn get(&self, path: &Path) -> Option<&str> {
        self.0.get(path).map(String::as_str)
    }
}

/// The folders that tell `path` apart from `all` (it among them): the fewest last parent folders
/// that no other has, at least two.
fn label_of(path: &Path, all: &[&Path]) -> String {
    let parents = |p: &Path| -> Vec<String> { p.parent().map(|d| d.iter().filter(|c| *c != "/").map(|c| c.to_string_lossy().into_owned()).collect()).unwrap_or_default() };
    let mine = parents(path);
    let others: Vec<Vec<String>> = all.iter().filter(|p| **p != path).map(|p| parents(p)).collect();
    let tail = |v: &[String], d: usize| v[v.len().saturating_sub(d)..].to_vec();
    let unique = (1..=mine.len()).find(|&d| others.iter().all(|o| tail(o, d) != tail(&mine, d))).unwrap_or(mine.len());
    tail(&mine, unique.max(2).min(mine.len())).join("/")
}

/// The kind of a path link, for its icon and color (mockup `colori`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Folder,
    Json,
    Rust,
    Markdown,
    Image,
    Shell,
    Other,
}

impl Kind {
    /// The kind of a `runemd-open:` destination: a folder ends with `/`.
    pub fn of(url: &str) -> Option<Kind> {
        let (path, _) = parse_target(url)?;
        if url.split('#').next().is_some_and(|u| u.ends_with('/')) {
            return Some(Kind::Folder);
        }
        Some(Kind::of_extension(&path.extension().map(|e| e.to_string_lossy()).unwrap_or_default()))
    }

    /// The kind of a file extension (`rs`, `JSON`…).
    fn of_extension(ext: &str) -> Kind {
        match ext.to_ascii_lowercase().as_str() {
            "json" | "jsonl" | "yaml" | "yml" => Kind::Json,
            "rs" => Kind::Rust,
            "md" | "markdown" | "mdx" | "txt" => Kind::Markdown,
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "pdf" => Kind::Image,
            "sh" | "zsh" | "bash" | "fish" => Kind::Shell,
            _ => Kind::Other,
        }
    }
}

/// What a code block's header shows for its fence's first word: a file name (`src/main.rs`)
/// as its base name with the extension's kind, else the language as written with its kind.
pub fn code_head(word: &str) -> (String, Kind) {
    let path = Path::new(word);
    match path.extension() {
        Some(ext) if word.contains(['.', '/']) => {
            let name = path.file_name().map_or_else(|| word.to_owned(), |n| n.to_string_lossy().into_owned());
            (name, Kind::of_extension(&ext.to_string_lossy()))
        }
        _ => {
            let kind = match word.to_ascii_lowercase().as_str() {
                "sh" | "bash" | "zsh" | "shell" | "console" | "fish" => Kind::Shell,
                "rust" => Kind::Rust,
                "jsonc" => Kind::Json,
                lang => Kind::of_extension(lang),
            };
            (word.to_owned(), kind)
        }
    }
}

/// A size as Finder writes it, with one decimal from KB on.
pub fn size(len: u64) -> String {
    match len {
        n if n < 1024 => format!("{n} B"),
        n if n < 1024 * 1024 => format!("{:.1} KB", n as f64 / 1024.0),
        n if n < 1024 * 1024 * 1024 => format!("{:.1} MB", n as f64 / (1024.0 * 1024.0)),
        n => format!("{:.1} GB", n as f64 / (1024.0 * 1024.0 * 1024.0)),
    }
}

/// `source` as it is drawn (decision 2026-11-23-t3-style-paths): a path to an existing file or
/// folder becomes a Markdown link to [`open_target`] with only the base name as its text, then
/// ` L88` and the telling folders of [`Labels`] in [`Shown::dims`]; a local link destination that
/// exists becomes the same target and keeps its text. A file path that does not exist becomes
/// inline code (monospaced, not a link: no hand cursor or click), with folders and `:line` in
/// [`Shown::dims`]; a folder or link that does not exist stays as it was written. `None` if there
/// is no path.
pub fn shown(source: &str, places: &Places, labels: &Labels) -> Option<Shown> {
    let found = find(source);
    if found.is_empty() {
        return None;
    }
    let esc = |t: &str| -> String {
        let mut o = String::with_capacity(t.len());
        for c in t.chars() {
            if super::breaks::needs_escape(c) {
                o.push('\\');
            }
            o.push(c);
        }
        o
    };
    let mut names = Vec::new();
    let mut dims = Vec::new();
    let mut helps = std::collections::HashMap::new();
    let mut missing = false;
    let mut shift = 0isize;
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    for f in &found {
        let at = (f.range.start as isize + shift) as usize;
        let text = match places.find(&f.path) {
            Some(hit) => {
                let target = if hit.dir { format!("{OPEN}{}/", hit.path.display()) } else { open_target(&hit.path, f.line) };
                let short = places.short(&hit.path);
                // Only the full path, for files and folders (Matteo, Waggle c_856970ee; ledger `2026-11-24-link-hint`).
                let help = if hit.dir { format!("{short}/") } else { short };
                helps.insert(target.clone(), help);
                if f.link {
                    format!("<{target}>")
                } else {
                    let name = esc(&hit.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| f.path.clone()));
                    let mut suffix = String::new();
                    match (f.line, f.to) {
                        (Some(a), Some(b)) => suffix.push_str(&format!(" L{a}–{b}")),
                        (Some(a), None) => suffix.push_str(&format!(" L{a}")),
                        _ => {}
                    }
                    if let Some(l) = labels.get(&hit.path) {
                        suffix.push(' ');
                        suffix.push_str(&esc(l));
                    }
                    names.push(at + 1..at + 1 + name.len());
                    dims.push(at + 1 + name.len()..at + 1 + name.len() + suffix.len());
                    format!("[{name}{suffix}](<{target}>)")
                }
            }
            None if f.quiet => continue,
            None => {
                missing = true;
                let (dir, name) = f.parts();
                let line = &source[f.range.clone()].trim_matches('`')[f.path.len()..];
                let n = at + 1 + dir.len();
                dims.push(at + 1..n);
                names.push(n..n + name.len());
                dims.push(n + name.len()..n + name.len() + line.len());
                format!("`{dir}{name}{line}`")
            }
        };
        shift += text.len() as isize - f.range.len() as isize;
        edits.push((f.range.clone(), text));
    }
    if edits.is_empty() {
        return None;
    }
    // Other inline code in the block: then the paths that do not exist keep the code
    // pill, which the kit gives to all the code of a block.
    let mut rest = String::new();
    let mut from = 0;
    for (r, _) in &edits {
        rest.push_str(&source[from..r.start]);
        from = r.end;
    }
    rest.push_str(&source[from..]);
    let bare = missing && !rest.contains('`');
    dims.retain(|r| !r.is_empty());
    Some(Shown { edited: super::footnotes::Edited::new(source, edits), names, dims, helps, bare })
}

/// The drawn text of a block with paths ([`shown`]).
#[derive(Debug)]
pub struct Shown {
    pub edited: super::footnotes::Edited,
    /// The bytes of each path's name in the new text, for the name color.
    pub names: Vec<Range<usize>>,
    /// `L88` and telling folders of existing paths, folders and `:line` of those that do not
    /// exist (inline code), in `fg-3`.
    pub dims: Vec<Range<usize>>,
    /// The popover of each target: the full path (`~` for home) and, on a second row, the
    /// size and the hint.
    pub helps: std::collections::HashMap<String, String>,
    /// The block's only inline code is paths that do not exist: the view draws it
    /// without the pill (constraint 1).
    pub bare: bool,
}

/// The destination of a link to an existing file: `runemd-open:` plus the absolute path and,
/// if there is one, `#L` and the line.
pub const OPEN: &str = "runemd-open:";

/// The destination for `file` at line `line`.
pub fn open_target(file: &Path, line: Option<u32>) -> String {
    match line {
        Some(n) => format!("{OPEN}{}#L{n}", file.display()),
        None => format!("{OPEN}{}", file.display()),
    }
}

/// The file and line of a `runemd-open:` destination.
pub fn parse_target(url: &str) -> Option<(PathBuf, Option<u32>)> {
    let rest = url.strip_prefix(OPEN)?;
    match rest.rsplit_once("#L") {
        Some((p, n)) if n.bytes().all(|b| b.is_ascii_digit()) && !n.is_empty() => Some((PathBuf::from(p), n.parse().ok())),
        _ => Some((PathBuf::from(rest), None)),
    }
}

/// `path` as copied by "Copy Relative Path": from `base` (the repo root, or the document's
/// folder), or the full path when it is not below it.
pub fn relative(path: &Path, base: &Path) -> String {
    let path = path.to_string_lossy();
    let path = Path::new(path.strip_suffix('/').unwrap_or(&path));
    path.strip_prefix(base).ok().filter(|r| !r.as_os_str().is_empty()).unwrap_or(path).display().to_string()
}

/// Opens `path` with `open`: in its default app, or in Finder with `reveal`. Tests record the
/// call ([`system_opened`]) instead of launching an app.
pub fn open_with_system(path: &Path, reveal: bool) {
    #[cfg(test)]
    {
        SYSTEM_OPENED.with_borrow_mut(|v| v.push((path.to_owned(), reveal)));
        return;
    }
    #[cfg_attr(test, allow(unreachable_code))]
    let mut cmd = std::process::Command::new("/usr/bin/open");
    if reveal {
        cmd.arg("-R");
    }
    if let Err(e) = cmd.arg(path).spawn() {
        eprintln!("runemd: open {}: {e}", path.display());
    }
}

#[cfg(test)]
thread_local! { static SYSTEM_OPENED: std::cell::RefCell<Vec<(PathBuf, bool)>> = const { std::cell::RefCell::new(Vec::new()) }; }

/// The paths [`open_with_system`] was asked to open on this test thread, emptied.
#[cfg(test)]
pub fn system_opened() -> Vec<(PathBuf, bool)> {
    SYSTEM_OPENED.with_borrow_mut(std::mem::take)
}

/// Line `line` (from 1) of `text`, the whole file, falls in block number…: the last block that
/// starts before or on that line. `blocks` are the sources of the blocks, in order, each one a
/// piece of `text`.
pub fn block_of_line<'a>(text: &str, blocks: impl Iterator<Item = &'a str>, line: u32) -> usize {
    let mut from = 0;
    let mut found = 0;
    for (i, b) in blocks.enumerate() {
        let Some(off) = text[from..].find(b).map(|o| from + o) else { continue };
        let first = text[..off].matches('\n').count() as u32 + 1;
        if first > line {
            break;
        }
        found = i;
        from = off + b.len();
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(s: &str) -> Vec<String> {
        find(s).into_iter().map(|f| f.path).collect()
    }

    #[test]
    fn paths_with_and_without_backticks_are_found() {
        let s = "I moved the computation to src/live/below.rs:88 and updated `src/ui/doc_view.rs`. The proof is in ~/Desktop/GitHub/runemd/docs/proof/fire-dark.png.";
        let f = find(s);
        assert_eq!(paths(s), ["src/live/below.rs", "src/ui/doc_view.rs", "~/Desktop/GitHub/runemd/docs/proof/fire-dark.png"]);
        assert_eq!(f[0].line, Some(88));
        assert_eq!(&s[f[0].range.clone()], "src/live/below.rs:88");
        assert_eq!(&s[f[1].range.clone()], "`src/ui/doc_view.rs`");
        assert_eq!(f[0].parts(), ("src/live/", "below.rs"));
    }

    #[test]
    fn bare_file_names_need_a_known_extension() {
        assert_eq!(paths("Modify: `Cargo.toml`, README.md and bench/fire_settings.sh, then /tmp/a.log"), ["Cargo.toml", "README.md", "bench/fire_settings.sh", "/tmp/a.log"]);
        assert_eq!(paths("- Create: src/fx/fire.rs, src/live/below.rs"), ["src/fx/fire.rs", "src/live/below.rs"]);
        assert_eq!(paths("| src/fx/fire.rs | 412 |"), ["src/fx/fire.rs"]);
    }

    #[test]
    fn ordinary_words_with_a_dot_stay_text() {
        let s = "Text stays: etc., v1.2, 0.5 s, Node.js, Vue.js, e.g. this, 3.5x, example.com and https://github.com/anomalyco/rune/blob/main/README.md (already a link), www.foo.dev/a.md.";
        assert_eq!(paths(s), Vec::<String>::new());
        let links = find("[the file](src/a.rs) and <src/b.rs> and ![x](img/c.png)");
        assert_eq!(links.iter().map(|f| (f.path.as_str(), f.link)).collect::<Vec<_>>(), [("src/a.rs", true)], "only a link's destination, not autolinks or images");
        assert!(paths("and/or km/h 1/2 a//b.rs ~x/a.rs").is_empty());
        assert!(paths("`cargo test --profile perf`").is_empty(), "code with spaces is not a path");
    }

    #[test]
    fn code_blocks_and_references_are_left_alone() {
        assert!(paths("```\ncat src/live/below.rs\n```\n").is_empty());
        assert!(paths("    cat src/live/below.rs\n").is_empty());
        assert!(paths("[x]: docs/a.md\n").is_empty());
    }

    fn tree() -> (PathBuf, Places) {
        let t = std::env::temp_dir().join(format!("runemd-paths-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&t);
        std::fs::create_dir_all(t.join("repo/.git")).unwrap();
        std::fs::create_dir_all(t.join("repo/docs/plan")).unwrap();
        std::fs::create_dir_all(t.join("repo/src/live")).unwrap();
        std::fs::create_dir_all(t.join("home/notes")).unwrap();
        std::fs::write(t.join("repo/docs/plan/progress.md"), "x").unwrap();
        std::fs::write(t.join("repo/docs/plan/beside.md"), "x").unwrap();
        std::fs::write(t.join("repo/src/live/below.rs"), "x").unwrap();
        std::fs::write(t.join("home/notes/a.md"), "x").unwrap();
        let places = Places { home: Some(t.join("home")), ..Places::of(&t.join("repo/docs/plan/progress.md")) };
        (t, places)
    }

    #[test]
    fn paths_resolve_beside_the_file_in_the_repo_absolute_and_home() {
        let (t, p) = tree();
        assert_eq!(p.root.as_deref(), Some(t.join("repo").as_path()));
        assert_eq!(p.resolve("beside.md"), Some(t.join("repo/docs/plan/beside.md")));
        assert_eq!(p.resolve("src/live/below.rs"), Some(t.join("repo/src/live/below.rs")));
        let abs = t.join("repo/src/live/below.rs");
        assert_eq!(p.resolve(abs.to_str().unwrap()), Some(abs.clone()));
        assert_eq!(p.resolve("~/notes/a.md"), Some(t.join("home/notes/a.md")));
        assert_eq!(p.resolve("src/old/gone.rs"), None);
        assert_eq!(p.resolve("src/live"), None, "a folder is not a file");
        std::fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn shown_text_makes_each_path_a_link_with_its_name() {
        let (t, p) = tree();
        let s = "See `src/live/below.rs` and src/old/fire_x.rs:3.";
        let sh = shown(s, &p, &Labels::default()).unwrap();
        let (e, names) = (&sh.edited, &sh.names);
        let file = t.join("repo/src/live/below.rs");
        assert_eq!(e.text, format!("See [below.rs](<{OPEN}{}>) and `src/old/fire_x.rs:3`.", file.display()));
        let got: Vec<&str> = names.iter().map(|r| &e.text[r.clone()]).collect();
        assert_eq!(got, ["below.rs", "fire_x.rs"]);
        let dims: Vec<&str> = sh.dims.iter().map(|r| &e.text[r.clone()]).collect();
        assert_eq!(dims, ["src/old/", ":3"]);
        assert!(sh.bare, "no other code in the block");
        assert!(!shown("src/old/a.rs and `cargo test`", &p, &Labels::default()).unwrap().bare);
        assert!(shown("nothing here, etc.", &p, &Labels::default()).is_none());
        std::fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn folders_count_only_where_they_look_like_folders() {
        let s = "In `src/live` and docs/plan/ and ~/notes, not and/or, `feat/x-y`, src or km/h.";
        let f = find(s);
        assert_eq!(paths(s), ["src/live", "docs/plan/", "~/notes", "feat/x-y"]);
        assert!(f.iter().all(|f| f.quiet), "a folder that does not exist stays as it was written");
        assert!(!find("`src/a.rs`")[0].quiet);
    }

    #[test]
    fn line_ranges_are_read() {
        let f = find("See src/a.rs:42-64 and `b.rs:7`.");
        assert_eq!((f[0].line, f[0].to), (Some(42), Some(64)));
        assert_eq!((f[1].line, f[1].to), (Some(7), None));
    }

    #[test]
    fn local_link_destinations_are_found_but_not_images_or_urls() {
        let s = "[the plan](docs/plan/beside.md) and [x](#top), [y](https://a.dev/b.md), ![p](img/c.png), [z](<src/live/below.rs#L9>).";
        let f = find(s);
        assert_eq!(paths(s), ["docs/plan/beside.md", "src/live/below.rs"]);
        assert!(f.iter().all(|f| f.link && f.quiet));
        assert_eq!(&s[f[0].range.clone()], "docs/plan/beside.md");
        assert_eq!(&s[f[1].range.clone()], "<src/live/below.rs#L9>");
        assert_eq!(f[1].line, Some(9));
    }

    #[test]
    fn same_names_show_the_shortest_folder_that_tells_them_apart() {
        let (t, p) = tree();
        std::fs::create_dir_all(t.join("repo/src/ui")).unwrap();
        std::fs::write(t.join("repo/src/live/mod.rs"), "x").unwrap();
        std::fs::write(t.join("repo/src/ui/mod.rs"), "x").unwrap();
        let labels = Labels::of(["`src/live/mod.rs` and src/ui/mod.rs", "src/live/below.rs, src/live/mod.rs again"].into_iter(), &p);
        assert_eq!(labels.get(&t.join("repo/src/live/mod.rs")), Some("src/live"));
        assert_eq!(labels.get(&t.join("repo/src/ui/mod.rs")), Some("src/ui"));
        assert_eq!(labels.get(&t.join("repo/src/live/below.rs")), None, "a name used once needs no folder");
        std::fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn the_depth_grows_until_the_folders_differ() {
        let a = Path::new("/x/a/src/mod.rs");
        let b = Path::new("/x/b/src/mod.rs");
        assert_eq!(label_of(a, &[a, b]), "a/src");
        assert_eq!(label_of(Path::new("/r/live/mod.rs"), &[Path::new("/r/live/mod.rs"), Path::new("/r/ui/mod.rs")]), "r/live");
    }

    #[test]
    fn existing_paths_show_the_name_and_a_dim_suffix() {
        let (t, p) = tree();
        std::fs::create_dir_all(t.join("repo/src/ui")).unwrap();
        std::fs::write(t.join("repo/src/live/mod.rs"), "x").unwrap();
        std::fs::write(t.join("repo/src/ui/mod.rs"), "x").unwrap();
        let s = "At src/live/below.rs:88, in `src/live/mod.rs` and `src/ui/mod.rs`, folder `src/live`, gone `src/old`, [plan](beside.md).";
        let labels = Labels::of(std::iter::once(s), &p);
        let sh = shown(s, &p, &labels).unwrap();
        let e = &sh.edited.text;
        let below = t.join("repo/src/live/below.rs");
        let live = t.join("repo/src/live");
        assert!(e.starts_with(&format!("At [below.rs L88](<{OPEN}{}#L88>), in [mod.rs src/live](<", below.display())), "{e}");
        assert!(e.contains(&format!("folder [live](<{OPEN}{}/>)", live.display())), "{e}");
        assert!(e.contains("gone `src/old`"), "a folder that does not exist stays code: {e}");
        assert!(e.contains(&format!("[plan](<{OPEN}{}>)", t.join("repo/docs/plan/beside.md").display())), "{e}");
        let names: Vec<&str> = sh.names.iter().map(|r| &e[r.clone()]).collect();
        assert_eq!(names, ["below.rs", "mod.rs", "mod.rs", "live"]);
        let dims: Vec<&str> = sh.dims.iter().map(|r| &e[r.clone()]).collect();
        assert_eq!(dims, [" L88", " src/live", " src/ui"]);
        assert!(!sh.bare, "other code in the block keeps its pill");
        std::fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn the_popover_has_only_the_full_path() {
        let (t, p) = tree();
        std::fs::write(t.join("home/notes/big.json"), vec![b'x'; 2458]).unwrap();
        let sh = shown("`~/notes/big.json` and `~/notes`", &p, &Labels::default()).unwrap();
        let file = t.join("home/notes/big.json");
        let help = sh.helps.get(&open_target(&file, None)).unwrap();
        assert_eq!(help, "~/notes/big.json", "only the full path");
        let dir = sh.helps.get(&format!("{OPEN}{}/", t.join("home/notes").display())).unwrap();
        assert_eq!(dir, "~/notes/", "only the path for a folder too");
        assert_eq!(size(812), "812 B");
        assert_eq!(size(3 * 1024 * 1024 + 100), "3.0 MB");
        std::fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn the_kind_comes_from_the_target() {
        assert_eq!(Kind::of("runemd-open:/a/b/"), Some(Kind::Folder));
        assert_eq!(Kind::of("runemd-open:/a/b.rs#L3"), Some(Kind::Rust));
        assert_eq!(Kind::of("runemd-open:/a/x.yml"), Some(Kind::Json));
        assert_eq!(Kind::of("runemd-open:/a/Makefile"), Some(Kind::Other));
        assert_eq!(Kind::of("https://a.dev"), None);
    }

    #[test]
    fn relative_paths_start_at_the_base() {
        assert_eq!(relative(Path::new("/r/src/a.rs"), Path::new("/r")), "src/a.rs");
        assert_eq!(relative(Path::new("/r/src/"), Path::new("/r")), "src");
        assert_eq!(relative(Path::new("/elsewhere/a.rs"), Path::new("/r")), "/elsewhere/a.rs");
        assert_eq!(relative(Path::new("/r"), Path::new("/r")), "/r");
    }

    #[test]
    fn targets_round_trip_with_the_line() {
        let t = open_target(Path::new("/a/b.rs"), Some(88));
        assert_eq!(parse_target(&t), Some((PathBuf::from("/a/b.rs"), Some(88))));
        assert_eq!(parse_target(&open_target(Path::new("/a/b.rs"), None)), Some((PathBuf::from("/a/b.rs"), None)));
        assert_eq!(parse_target("https://x.dev"), None);
    }

    #[test]
    fn a_line_falls_in_its_block() {
        let text = "# T\n\none\ntwo\n\nsix\n";
        let blocks = ["# T", "one\ntwo", "six"];
        assert_eq!(block_of_line(text, blocks.into_iter(), 1), 0);
        assert_eq!(block_of_line(text, blocks.into_iter(), 4), 1);
        assert_eq!(block_of_line(text, blocks.into_iter(), 5), 1);
        assert_eq!(block_of_line(text, blocks.into_iter(), 6), 2);
        assert_eq!(block_of_line(text, blocks.into_iter(), 99), 2);
    }

    #[test]
    fn a_code_head_names_the_language_or_the_file() {
        assert_eq!(code_head("rust"), ("rust".to_owned(), Kind::Rust));
        assert_eq!(code_head("bash"), ("bash".to_owned(), Kind::Shell));
        assert_eq!(code_head("json"), ("json".to_owned(), Kind::Json));
        assert_eq!(code_head("python"), ("python".to_owned(), Kind::Other));
        assert_eq!(code_head("src/ui/doc_view.rs"), ("doc_view.rs".to_owned(), Kind::Rust));
        assert_eq!(code_head("Cargo.toml"), ("Cargo.toml".to_owned(), Kind::Other));
        assert_eq!(code_head("config.yaml"), ("config.yaml".to_owned(), Kind::Json));
    }
}
