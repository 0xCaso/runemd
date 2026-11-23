//! JSON and JSONL files as foldable lines (decision 2026-11-23-json-viewer, mockup `codice`): the
//! file pretty-printed with a 2-space indent, one [`Line`] per printed line, and a [`Node`] on each
//! line that opens a non-empty `{` or `[`. Keys, strings and numbers keep the text they have in the
//! file (no reordering, no float rounding). [`Folds`] is a tab's fold state, [`visible`] the lines
//! it leaves.

use std::collections::HashSet;
use std::fmt;

/// A parsed JSON file, or a JSONL file with one root per line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tree {
    pub lines: Vec<Line>,
    /// The first line of each root value.
    pub roots: Vec<usize>,
    pub jsonl: bool,
}

/// A printed line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// Indent level (2 spaces each).
    pub depth: u16,
    /// The line without its indent, comma included (`"seen": {`, `429,`, `},`).
    pub text: String,
    /// The bytes of the key at the start of [`Line::text`] (`"seen"`), 0 without one.
    pub key: usize,
    /// The non-empty object or array this line opens.
    pub node: Option<Node>,
    /// The number in the gutter: the printed line (JSON), the file line on a root (JSONL).
    pub number: Option<u32>,
}

/// A non-empty object or array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    /// The index of its closing line.
    pub close: usize,
    /// The bytes of [`Line::text`] before the bracket (`"seen": `).
    pub prefix: usize,
    pub object: bool,
    /// Keys or items.
    pub count: usize,
    /// Its place in the file (`/accounts/3/seen`; a JSONL root starts with its line, `12/`), the
    /// key of its fold state: it holds across reloads while the place exists.
    pub path: String,
    /// The first string value among its children, shown beside it when folded (mockup: `marta.rossi`).
    pub preview: Option<String>,
}

impl Node {
    /// "5 keys", "1 item".
    pub fn count_label(&self) -> String {
        let (one, many) = if self.object { ("key", "keys") } else { ("item", "items") };
        format!("{} {}", self.count, if self.count == 1 { one } else { many })
    }

    /// `{` or `[`, and the closing one.
    pub fn brackets(&self) -> (char, char) {
        if self.object { ('{', '}') } else { ('[', ']') }
    }
}

/// Where and why the file is not JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    /// From 1, in the file.
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, column {}: {}", self.line, self.column, self.message)
    }
}

/// Deeper nesting is refused (the parser recurses).
const MAX_DEPTH: u16 = 512;
/// Characters of a folded node's preview.
const PREVIEW_CHARS: usize = 40;

/// Parses a JSON file.
pub fn parse(text: &str) -> Result<Tree, Error> {
    let mut p = Parser::new(text, 0);
    p.ws();
    if p.at >= p.b.len() {
        return Err(p.error("the file is empty"));
    }
    p.value(String::new(), 0, String::new())?;
    p.ws();
    if p.at < p.b.len() {
        return Err(p.error("more text after the JSON value"));
    }
    let mut lines = p.lines;
    for (i, l) in lines.iter_mut().enumerate() {
        l.number = Some(i as u32 + 1);
    }
    Ok(Tree { lines, roots: vec![0], jsonl: false })
}

/// Parses a JSONL file: each non-blank line is a root, numbered with its file line.
pub fn parse_lines(text: &str) -> Result<Tree, Error> {
    let mut lines = Vec::new();
    let mut roots = Vec::new();
    for (n, raw) in text.split('\n').enumerate() {
        let raw = raw.strip_suffix('\r').unwrap_or(raw);
        if raw.trim().is_empty() {
            continue;
        }
        let mut p = Parser::new(raw, n);
        p.ws();
        p.value(String::new(), 0, format!("{}/", n + 1))?;
        p.ws();
        if p.at < p.b.len() {
            return Err(p.error("more text after the JSON value"));
        }
        let first = lines.len();
        roots.push(first);
        for (k, mut l) in p.lines.into_iter().enumerate() {
            l.number = (k == 0).then_some(n as u32 + 1);
            if let Some(node) = &mut l.node {
                node.close += first;
            }
            lines.push(l);
        }
    }
    if roots.is_empty() {
        return Err(Error { line: 1, column: 1, message: "the file is empty".to_owned() });
    }
    Ok(Tree { lines, roots, jsonl: true })
}

impl Tree {
    /// The file pretty-printed: 2-space indent, one line per [`Line`].
    pub fn pretty(&self) -> String {
        let mut out = String::new();
        for l in &self.lines {
            for _ in 0..l.depth {
                out.push_str("  ");
            }
            out.push_str(&l.text);
            out.push('\n');
        }
        out
    }

    /// The lines of the nodes inside node line `i` (not `i` itself), in order.
    fn inner_nodes(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        let end = self.lines[i].node.as_ref().map_or(i, |n| n.close);
        (i + 1..end).filter(|&k| self.lines[k].node.is_some())
    }
}

struct Parser<'a> {
    s: &'a str,
    b: &'a [u8],
    at: usize,
    /// The file line of byte 0 (JSONL), from 0.
    line0: usize,
    lines: Vec<Line>,
}

impl<'a> Parser<'a> {
    fn new(s: &'a str, line0: usize) -> Self {
        Parser { s, b: s.as_bytes(), at: 0, line0, lines: Vec::new() }
    }

    fn error(&self, message: &str) -> Error {
        let before = &self.s[..self.at.min(self.s.len())];
        let line = before.matches('\n').count();
        let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
        Error { line: self.line0 + line + 1, column, message: message.to_owned() }
    }

    fn ws(&mut self) {
        while self.at < self.b.len() && matches!(self.b[self.at], b' ' | b'\t' | b'\n' | b'\r') {
            self.at += 1;
        }
    }

    fn push(&mut self, depth: u16, text: String, key: usize) -> usize {
        self.lines.push(Line { depth, text, key, node: None, number: None });
        self.lines.len() - 1
    }

    /// A value whose first line starts with `prefix` (`"key": `), at `depth`, at place `path`.
    fn value(&mut self, prefix: String, depth: u16, path: String) -> Result<(), Error> {
        if depth > MAX_DEPTH {
            return Err(self.error("nested too deep"));
        }
        match self.b.get(self.at) {
            Some(b'{') => self.container(prefix, depth, path, true),
            Some(b'[') => self.container(prefix, depth, path, false),
            Some(b'"') => {
                let s = self.string()?;
                let key = key_len(&prefix);
                self.push(depth, prefix + s, key);
                Ok(())
            }
            Some(_) => {
                let t = self.scalar()?;
                let key = key_len(&prefix);
                self.push(depth, prefix + t, key);
                Ok(())
            }
            None => Err(self.error("the file ends where a value should be")),
        }
    }

    fn container(&mut self, prefix: String, depth: u16, path: String, object: bool) -> Result<(), Error> {
        let (open, close) = if object { ('{', '}') } else { ('[', ']') };
        self.at += 1;
        self.ws();
        if self.b.get(self.at) == Some(&(close as u8)) {
            self.at += 1;
            self.push(depth, format!("{prefix}{open}{close}"), key_len(&prefix));
            return Ok(());
        }
        let at = prefix.len();
        let line = self.push(depth, format!("{prefix}{open}"), key_len(&prefix));
        let mut count = 0;
        let mut preview = None;
        loop {
            self.ws();
            let (child_prefix, child_path) = if object {
                if self.b.get(self.at) != Some(&b'"') {
                    return Err(self.error(if self.at >= self.b.len() { "the file ends inside an object" } else { "expected a key in quotes" }));
                }
                let key = self.string()?;
                self.ws();
                if self.b.get(self.at) != Some(&b':') {
                    return Err(self.error("expected `:` after the key"));
                }
                self.at += 1;
                self.ws();
                (format!("{key}: "), format!("{path}/{}", &key[1..key.len() - 1]))
            } else {
                (String::new(), format!("{path}/{count}"))
            };
            let first = self.lines.len();
            self.value(child_prefix, depth + 1, child_path)?;
            if preview.is_none() && self.lines.len() == first + 1 {
                preview = string_value(&self.lines[first].text);
            }
            count += 1;
            self.ws();
            match self.b.get(self.at) {
                Some(b',') => {
                    self.at += 1;
                    if let Some(l) = self.lines.last_mut() {
                        l.text.push(',');
                    }
                }
                Some(&c) if c == close as u8 => {
                    self.at += 1;
                    break;
                }
                None => return Err(self.error(if object { "the file ends inside an object" } else { "the file ends inside an array" })),
                Some(_) => return Err(self.error(if object { "expected `,` or `}`" } else { "expected `,` or `]`" })),
            }
        }
        let end = self.push(depth, close.to_string(), 0);
        self.lines[line].node = Some(Node { close: end, prefix: at, object, count, path, preview });
        Ok(())
    }

    /// A string, quotes included, as written.
    fn string(&mut self) -> Result<&'a str, Error> {
        let start = self.at;
        self.at += 1;
        loop {
            match self.b.get(self.at) {
                None => return Err(self.error("the file ends inside a string")),
                Some(b'"') => {
                    self.at += 1;
                    return Ok(&self.s[start..self.at]);
                }
                Some(b'\\') => {
                    match self.b.get(self.at + 1) {
                        Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => self.at += 2,
                        Some(b'u') if self.b.get(self.at + 2..self.at + 6).is_some_and(|h| h.iter().all(u8::is_ascii_hexdigit)) => self.at += 6,
                        _ => return Err(self.error("a bad escape in a string")),
                    }
                }
                Some(&c) if c < 0x20 => return Err(self.error("a control character in a string")),
                Some(_) => self.at += 1,
            }
        }
    }

    /// A number, `true`, `false` or `null`, as written.
    fn scalar(&mut self) -> Result<&'a str, Error> {
        let start = self.at;
        for word in ["true", "false", "null"] {
            if self.s[self.at..].starts_with(word) {
                self.at += word.len();
                return Ok(word);
            }
        }
        let b = self.b;
        let digits = |at: &mut usize| {
            let s = *at;
            while *at < b.len() && b[*at].is_ascii_digit() {
                *at += 1;
            }
            *at > s
        };
        let mut at = self.at;
        if b.get(at) == Some(&b'-') {
            at += 1;
        }
        let int_start = at;
        if !digits(&mut at) || (b[int_start] == b'0' && at - int_start > 1) {
            return Err(self.error("expected a value"));
        }
        if b.get(at) == Some(&b'.') {
            at += 1;
            if !digits(&mut at) {
                self.at = at;
                return Err(self.error("a bad number"));
            }
        }
        if matches!(b.get(at), Some(b'e' | b'E')) {
            at += 1;
            if matches!(b.get(at), Some(b'+' | b'-')) {
                at += 1;
            }
            if !digits(&mut at) {
                self.at = at;
                return Err(self.error("a bad number"));
            }
        }
        self.at = at;
        Ok(&self.s[start..at])
    }
}

/// The key's bytes in a `"key": ` prefix.
fn key_len(prefix: &str) -> usize {
    prefix.len().saturating_sub(2)
}

/// What a piece of a printed line is, for its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Token {
    Key,
    String,
    Number,
    /// `true`, `false`, `null`.
    Word,
    Punctuation,
}

/// The coloured pieces of `text`, whose first `key` bytes are a key: the lexer of the code card
/// sees each line alone and cannot tell a key from a string.
pub fn tokens(text: &str, key: usize) -> Vec<(std::ops::Range<usize>, Token)> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    if key > 0 && key <= b.len() {
        out.push((0..key, Token::Key));
        i = key;
    }
    while i < b.len() {
        let start = i;
        let kind = match b[i] {
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i = (i + 1).min(b.len());
                Token::String
            }
            b'-' | b'0'..=b'9' => {
                while i < b.len() && matches!(b[i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                    i += 1;
                }
                Token::Number
            }
            b't' | b'f' | b'n' => {
                while i < b.len() && b[i].is_ascii_alphabetic() {
                    i += 1;
                }
                Token::Word
            }
            b'{' | b'}' | b'[' | b']' | b',' | b':' => {
                i += 1;
                Token::Punctuation
            }
            _ => {
                i += text[i..].chars().next().map_or(1, char::len_utf8);
                continue;
            }
        };
        out.push((start..i, kind));
    }
    out
}

/// The string of a `"key": "value"` or `"value"` line, unquoted and cut short, for a preview.
fn string_value(text: &str) -> Option<String> {
    let t = text.strip_suffix(',').unwrap_or(text);
    let v = match t.find("\": \"") {
        Some(i) if t.starts_with('"') => &t[i + 3..],
        _ => t,
    };
    let v = v.strip_prefix('"')?.strip_suffix('"')?;
    if v.is_empty() {
        return None;
    }
    let mut out: String = v.chars().take(PREVIEW_CHARS).collect();
    if v.chars().count() > PREVIEW_CHARS {
        out.push('…');
    }
    Some(out)
}

/// A tab's fold state: the nodes whose state differs from the default. By default every node is
/// open, except a JSONL file's roots when it has more than [`JSONL_FOLDED`] of them.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Folds {
    flipped: HashSet<String>,
    roots_folded: bool,
    /// Bumped on every change: the view's cache of the visible lines keys on it.
    pub generation: u64,
}

/// A JSONL file with more lines than this opens with every line folded.
pub const JSONL_FOLDED: usize = 20;

impl Folds {
    /// The state a tab of `tree` opens with.
    pub fn new(tree: &Tree) -> Folds {
        Folds { roots_folded: tree.jsonl && tree.roots.len() > JSONL_FOLDED, ..Folds::default() }
    }

    fn default_folded(&self, line: &Line) -> bool {
        self.roots_folded && line.depth == 0
    }

    /// Whether node line `i` is folded.
    pub fn is_folded(&self, tree: &Tree, i: usize) -> bool {
        let line = &tree.lines[i];
        line.node.as_ref().is_some_and(|n| self.default_folded(line) != self.flipped.contains(&n.path))
    }

    fn set(&mut self, tree: &Tree, i: usize, folded: bool) {
        let line = &tree.lines[i];
        let Some(n) = &line.node else { return };
        if folded == self.default_folded(line) {
            self.flipped.remove(&n.path);
        } else {
            self.flipped.insert(n.path.clone());
        }
    }

    /// A click on node line `i`'s arrow: it folds or unfolds; with `subtree` (Alt), every node
    /// inside it takes the same state.
    pub fn toggle(&mut self, tree: &Tree, i: usize, subtree: bool) {
        if tree.lines.get(i).and_then(|l| l.node.as_ref()).is_none() {
            return;
        }
        let folded = !self.is_folded(tree, i);
        self.set(tree, i, folded);
        if subtree {
            for k in tree.inner_nodes(i).collect::<Vec<_>>() {
                self.set(tree, k, folded);
            }
        }
        self.generation += 1;
    }

    /// Expand all / Collapse all.
    pub fn set_all(&mut self, tree: &Tree, folded: bool) {
        for i in 0..tree.lines.len() {
            self.set(tree, i, folded);
        }
        self.generation += 1;
    }
}

/// A line the view draws: line `line` of the tree, folded or not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    pub line: usize,
    pub folded: bool,
}

/// The lines left by `folds`: a folded node is its own line, with its inside and closing line gone.
pub fn visible(tree: &Tree, folds: &Folds) -> Vec<Shown> {
    let mut out = Vec::with_capacity(tree.lines.len());
    let mut i = 0;
    while i < tree.lines.len() {
        let folded = folds.is_folded(tree, i);
        out.push(Shown { line: i, folded });
        i = match (&tree.lines[i].node, folded) {
            (Some(n), true) => n.close + 1,
            _ => i + 1,
        };
    }
    out
}

/// A folded node's text: `"seen": {…},` (the view draws the `…` as a pill between the two parts).
#[cfg(test)]
pub fn folded_text(tree: &Tree, i: usize) -> Option<String> {
    let line = &tree.lines[i];
    let n = line.node.as_ref()?;
    let (open, close) = n.brackets();
    let comma = if tree.lines[n.close].text.ends_with(',') { "," } else { "" };
    Some(format!("{}{open}…{close}{comma}", &line.text[..n.prefix]))
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATE: &str = r#"{"version": 2, "source": {"url": "x.com/following", "page": 3},
        "accounts": [{"handle": "marta.rossi", "seen": {"at": "2026-11-12", "list": {"id": 1}}}, {"handle": "luca"}],
        "empty": {}, "none": [], "ratio": 1.10, "big": 12345678901234567890}"#;

    fn texts(t: &Tree) -> Vec<String> {
        t.pretty().lines().map(str::to_owned).collect()
    }

    #[test]
    fn json_is_pretty_printed_with_two_spaces_keeping_order_and_numbers() {
        let t = parse(STATE).unwrap();
        let p = texts(&t);
        assert_eq!(p[0], "{");
        assert_eq!(p[1], "  \"version\": 2,");
        assert_eq!(p[2], "  \"source\": {");
        assert_eq!(p[3], "    \"url\": \"x.com/following\",");
        assert_eq!(p[5], "  },");
        assert!(p.contains(&"  \"empty\": {},".to_owned()) && p.contains(&"  \"none\": [],".to_owned()), "empty nodes stay on one line");
        assert!(p.contains(&"  \"ratio\": 1.10,".to_owned()), "numbers as written");
        assert!(p.contains(&"  \"big\": 12345678901234567890".to_owned()));
        assert_eq!(p.last().unwrap(), "}");
        assert_eq!(t.lines.iter().map(|l| l.number).collect::<Vec<_>>(), (1..=t.lines.len() as u32).map(Some).collect::<Vec<_>>());
    }

    #[test]
    fn every_open_bracket_line_is_a_node_with_its_close_and_count() {
        let t = parse(STATE).unwrap();
        let nodes: Vec<(usize, &Node)> = t.lines.iter().enumerate().filter_map(|(i, l)| l.node.as_ref().map(|n| (i, n))).collect();
        let opening = t.lines.iter().filter(|l| l.text.ends_with('{') || l.text.ends_with('[')).count();
        assert_eq!(nodes.len(), opening, "an arrow on every {{ and [ line");
        let root = &t.lines[0].node.as_ref().unwrap();
        assert_eq!((root.close, root.count, root.count_label()), (t.lines.len() - 1, 7, "7 keys".to_owned()));
        let accounts = nodes.iter().find(|(_, n)| n.path == "/accounts").unwrap();
        assert_eq!(accounts.1.count_label(), "2 items");
        assert_eq!(t.lines[accounts.1.close].text, "],");
        let source = nodes.iter().find(|(_, n)| n.path == "/source").unwrap();
        assert_eq!((source.1.count_label(), source.1.preview.as_deref()), ("2 keys".to_owned(), Some("x.com/following")));
        let first = nodes.iter().find(|(_, n)| n.path == "/accounts/0").unwrap();
        assert_eq!(first.1.preview.as_deref(), Some("marta.rossi"));
        assert!(nodes.iter().any(|(_, n)| n.path == "/accounts/0/seen/list" && n.count_label() == "1 key"));
    }

    #[test]
    fn folding_hides_the_inside_and_shows_braces_and_count() {
        let t = parse(STATE).unwrap();
        let mut f = Folds::new(&t);
        let all = visible(&t, &f);
        assert_eq!(all.len(), t.lines.len(), "everything open by default");
        let src = t.lines.iter().position(|l| l.node.as_ref().is_some_and(|n| n.path == "/source")).unwrap();
        f.toggle(&t, src, false);
        let v = visible(&t, &f);
        assert_eq!(v.len(), t.lines.len() - 3, "the two keys and the closing line go");
        assert_eq!(v[src], Shown { line: src, folded: true });
        assert_eq!(v[src + 1].line, src + 4, "the next line after the close");
        assert_eq!(folded_text(&t, src).unwrap(), "\"source\": {…},");
        let accounts = t.lines.iter().position(|l| l.node.as_ref().is_some_and(|n| n.path == "/accounts")).unwrap();
        assert_eq!(folded_text(&t, accounts).unwrap(), "\"accounts\": […],");
        assert_eq!(folded_text(&t, 0).unwrap(), "{…}");
        f.toggle(&t, src, false);
        assert_eq!(visible(&t, &f), all, "unfolds back");
        assert!(f.generation >= 2);
    }

    #[test]
    fn alt_toggles_the_whole_subtree() {
        let t = parse(STATE).unwrap();
        let mut f = Folds::new(&t);
        let accounts = t.lines.iter().position(|l| l.node.as_ref().is_some_and(|n| n.path == "/accounts")).unwrap();
        f.toggle(&t, accounts, true);
        let inner: Vec<usize> = t.inner_nodes(accounts).collect();
        assert_eq!(inner.len(), 4, "two accounts, seen, list");
        assert!(f.is_folded(&t, accounts) && inner.iter().all(|&i| f.is_folded(&t, i)), "folded with all its children");
        // A plain click opens only the node: its children stay folded.
        f.toggle(&t, accounts, false);
        let v = visible(&t, &f);
        assert!(v.iter().any(|s| s.line == inner[0] && s.folded));
        // Alt opens everything inside.
        f.toggle(&t, accounts, false);
        f.toggle(&t, accounts, true);
        assert!(!f.is_folded(&t, accounts) && inner.iter().all(|&i| !f.is_folded(&t, i)));
        f.set_all(&t, true);
        assert_eq!(visible(&t, &f), vec![Shown { line: 0, folded: true }], "Collapse all");
        f.set_all(&t, false);
        assert_eq!(visible(&t, &f).len(), t.lines.len(), "Expand all");
    }

    #[test]
    fn fold_state_follows_the_place_across_a_reload() {
        let t = parse(STATE).unwrap();
        let mut f = Folds::new(&t);
        let at = |t: &Tree, p: &str| t.lines.iter().position(|l| l.node.as_ref().is_some_and(|n| n.path == p)).unwrap();
        f.toggle(&t, at(&t, "/accounts"), false);
        let t2 = parse(&STATE.replace("\"version\": 2,", "\"version\": 2, \"new\": [1, 2],")).unwrap();
        assert!(f.is_folded(&t2, at(&t2, "/accounts")), "the same node, moved down");
        assert!(!f.is_folded(&t2, at(&t2, "/new")));
    }

    fn jsonl(n: usize) -> String {
        (1..=n).map(|i| format!("{{\"run\": {i}, \"status\": \"ok\", \"steps\": [{{\"name\": \"build\"}}]}}\n")).collect()
    }

    #[test]
    fn jsonl_is_one_foldable_row_per_line_numbered_by_file_line() {
        let t = parse_lines("{\"a\": 1}\n\n[1, 2]\n\"x\"\n").unwrap();
        assert_eq!(t.roots, vec![0, 3, 7]);
        assert_eq!(t.lines[0].number, Some(1));
        assert_eq!(t.lines[3].number, Some(3), "file line 3, after a blank line");
        assert_eq!(t.lines[7].number, Some(4));
        assert_eq!(t.lines[1].number, None, "inner lines have no number");
        assert_eq!(t.lines[3].node.as_ref().unwrap().close, 6);
        assert_eq!(t.lines[0].node.as_ref().unwrap().path, "1/");
        let f = Folds::new(&t);
        assert_eq!(visible(&t, &f).len(), t.lines.len(), "a short file opens unfolded");
    }

    #[test]
    fn a_long_jsonl_opens_folded_one_row_per_line() {
        let t = parse_lines(&jsonl(JSONL_FOLDED + 5)).unwrap();
        let mut f = Folds::new(&t);
        let v = visible(&t, &f);
        assert_eq!(v.len(), JSONL_FOLDED + 5, "one row per line");
        assert!(v.iter().all(|s| s.folded));
        assert_eq!(folded_text(&t, t.roots[0]).unwrap(), "{…}");
        assert_eq!(t.lines[t.roots[0]].node.as_ref().unwrap().count_label(), "3 keys");
        f.toggle(&t, t.roots[2], false);
        assert_eq!(visible(&t, &f).len(), JSONL_FOLDED + 5 + 8, "line 3 opens: its 9 printed lines");
        let short = parse_lines(&jsonl(JSONL_FOLDED)).unwrap();
        assert!(visible(&short, &Folds::new(&short)).iter().all(|s| !s.folded), "20 lines stay open");
    }

    #[test]
    fn a_line_tells_its_key_from_its_strings() {
        let t = parse(r#"{"name": "Luca", "n": -1.5e3, "ok": true, "x": [null]}"#).unwrap();
        let line = |k: usize| (&t.lines[k].text, t.lines[k].key);
        let (text, key) = line(1);
        let got: Vec<(&str, Token)> = tokens(text, key).into_iter().map(|(r, k)| (&text[r], k)).collect();
        assert_eq!(got, vec![("\"name\"", Token::Key), (":", Token::Punctuation), ("\"Luca\"", Token::String), (",", Token::Punctuation)]);
        let (text, key) = line(2);
        assert_eq!(tokens(text, key).iter().map(|(r, k)| (&text[r.clone()], *k)).nth(2), Some(("-1.5e3", Token::Number)));
        let (text, key) = line(3);
        assert_eq!(tokens(text, key).iter().map(|(r, k)| (&text[r.clone()], *k)).nth(2), Some(("true", Token::Word)));
        assert_eq!(t.lines[5].key, 0, "an array item has no key");
    }

    #[test]
    fn invalid_json_says_where() {
        let truncated = "{\n  \"version\": 2,\n  \"accounts\": [\n    {\"handle\": \"marta\"";
        let e = parse(truncated).unwrap_err();
        assert_eq!((e.line, e.message.as_str()), (4, "the file ends inside an object"));
        let e = parse("{\"a\": 1,}").unwrap_err();
        assert_eq!((e.line, e.column, e.message.as_str()), (1, 9, "expected a key in quotes"));
        assert_eq!(parse("").unwrap_err().message, "the file is empty");
        assert_eq!(parse("{} x").unwrap_err().message, "more text after the JSON value");
        assert_eq!(parse("[01]").unwrap_err().message, "expected a value");
        assert_eq!(parse("\"a\\qb\"").unwrap_err().message, "a bad escape in a string");
        assert_eq!(parse(&"[".repeat(600)).unwrap_err().message, "nested too deep");
        let e = parse_lines("{\"a\": 1}\n{\"b\": \n").unwrap_err();
        assert_eq!(e.line, 2, "JSONL errors name the file line");
        assert_eq!(e.to_string(), "line 2, column 7: the file ends where a value should be");
    }
}
