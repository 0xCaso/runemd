//! Footnotes (`[^1]` in the text, `[^1]: …` in a block of their own). The kit does not turn them on:
//! the view puts the raised number in place of the reference (superscript digits, in order of first
//! reference, like GitHub), does not draw the definition blocks where they are and lists them at
//! the end of the file. A reference without a definition stays as written. The source does not change.

use std::collections::HashMap;
use std::ops::Range;

/// The id of the notes block at the end (`Notes::block`).
pub const BLOCK_ID: u64 = u64::MAX;

/// The key of the notes column at the end, which also acts as a row for the Find marks.
pub const KEY: &str = "footnotes";

/// The notes of a document.
#[derive(Debug, Default, PartialEq)]
pub struct Notes {
    /// The number of every label that has a definition and at least one reference.
    numbers: HashMap<String, usize>,
    /// The text of the notes, in number order (the first is 1).
    pub list: Vec<String>,
}

impl Notes {
    /// The notes of the blocks `sources`, in order.
    pub fn of<'a>(sources: impl Iterator<Item = &'a str> + Clone) -> Notes {
        if !sources.clone().any(|s| s.contains("[^")) {
            return Notes::default();
        }
        let mut bodies: HashMap<String, String> = HashMap::new();
        for s in sources.clone() {
            for (label, body) in definitions(s).unwrap_or_default() {
                bodies.entry(label).or_insert(body);
            }
        }
        let mut notes = Notes::default();
        for s in sources.filter(|s| definitions(s).is_none()) {
            for (_, label) in references(s) {
                if let Some(body) = bodies.get(label)
                    && !notes.numbers.contains_key(label)
                {
                    notes.list.push(body.clone());
                    notes.numbers.insert(label.to_owned(), notes.list.len());
                }
            }
        }
        notes
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    /// The notes as they are drawn at the end of the file (a numbered list), in a block with an id that
    /// no block of the document has: Find finds them there and the view puts the marks there. `None` if there
    /// are none.
    pub fn block(&self) -> Option<super::document::Block> {
        if self.is_empty() {
            return None;
        }
        let source: String = self.list.iter().enumerate().map(|(i, n)| format!("{}. {n}\n", i + 1)).collect();
        Some(super::document::Block { id: BLOCK_ID, hash: super::ids::hash_str(&source), source: source.into() })
    }

    /// `source` with superscript numbers in place of the references that have a note. `None` if
    /// there are none.
    pub fn shown(&self, source: &str) -> Option<Edited> {
        if self.is_empty() {
            return None;
        }
        let edits: Vec<(Range<usize>, String)> = references(source).into_iter().filter_map(|(r, label)| self.numbers.get(label).map(|&n| (r, superscript(n)))).collect();
        (!edits.is_empty()).then(|| Edited::new(source, edits))
    }
}

/// A text with some stretches of the source replaced.
#[derive(Debug, PartialEq)]
pub struct Edited {
    pub text: String,
    edits: Vec<(Range<usize>, String)>,
}

impl Edited {
    pub fn new(source: &str, edits: Vec<(Range<usize>, String)>) -> Edited {
        let mut text = String::with_capacity(source.len());
        let mut from = 0;
        for (r, s) in &edits {
            text.push_str(&source[from..r.start]);
            text.push_str(s);
            from = r.end;
        }
        text.push_str(&source[from..]);
        Edited { text, edits }
    }

    /// `r`, a range of the source, in the text: what falls inside a replaced stretch
    /// takes the whole replacement.
    pub fn map(&self, r: &Range<usize>) -> Range<usize> {
        let at = |o: usize, end: bool| {
            let mut shift = 0isize;
            for (e, s) in &self.edits {
                if o <= e.start {
                    break;
                }
                if o < e.end {
                    let base = (e.start as isize + shift) as usize;
                    return if end { base + s.len() } else { base };
                }
                shift += s.len() as isize - e.len() as isize;
            }
            (o as isize + shift) as usize
        };
        at(r.start, false)..at(r.end, true).max(at(r.start, false))
    }
}

/// `n` in superscript digits.
fn superscript(n: usize) -> String {
    n.to_string().chars().map(|c| ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'][c as usize - '0' as usize]).collect()
}

/// If the block is made of definitions (`[^label]: text`, with the lines that follow), its
/// notes: (label, text).
pub fn definitions(source: &str) -> Option<Vec<(String, String)>> {
    let mut out: Vec<(String, String)> = Vec::new();
    for line in source.lines() {
        let t = line.trim_start();
        match def_head(t) {
            Some((label, rest)) => out.push((label.to_owned(), rest.trim().to_owned())),
            None => {
                let last = out.last_mut()?;
                if !t.is_empty() {
                    last.1.push(' ');
                    last.1.push_str(t.trim_end());
                }
            }
        }
    }
    (!out.is_empty()).then_some(out)
}

/// `[^label]:` at the start of `t`: the label and the rest.
fn def_head(t: &str) -> Option<(&str, &str)> {
    let rest = t.strip_prefix("[^")?;
    let end = rest.find(']')?;
    let label = &rest[..end];
    let after = rest[end + 1..].strip_prefix(':')?;
    valid(label).then_some((label, after))
}

fn valid(label: &str) -> bool {
    !label.is_empty() && !label.contains(|c: char| c.is_whitespace() || c == '[' || c == '^')
}

/// The `[^label]` references of a block, outside code: (where, label).
fn references(source: &str) -> Vec<(Range<usize>, &str)> {
    let first = source.trim_start_matches([' ', '\n']);
    if first.starts_with("```") || first.starts_with("~~~") || source.starts_with("    ") || source.starts_with('\t') || !source.contains("[^") {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut code = false;
    let mut i = 0;
    let b = source.as_bytes();
    while i < b.len() {
        match b[i] {
            b'`' => code = !code,
            b'\\' => i += 1,
            b'[' if !code && b.get(i + 1) == Some(&b'^') => {
                if let Some(end) = source[i + 2..].find(']').map(|j| i + 2 + j) {
                    let label = &source[i + 2..end];
                    if valid(label) && b.get(end + 1) != Some(&b':') {
                        out.push((i..end + 1, label));
                        i = end;
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: [&str; 4] = ["First[^b] and now[^a], again[^b], so[^missing].", "[^a]: Note A.\n[^b]: Note B\n  on two lines.", "`[^a]` in code", "```\n[^a]\n```"];

    #[test]
    fn notes_are_numbered_by_first_reference() {
        let n = Notes::of(DOC.iter().copied());
        assert_eq!(n.list, ["Note B on two lines.", "Note A."]);
        assert_eq!(n.shown(DOC[0]).unwrap().text, "First¹ and now², again¹, so[^missing].");
        assert_eq!(n.shown(DOC[2]), None, "in code it stays");
        assert_eq!(n.shown(DOC[3]), None);
        assert!(definitions(DOC[1]).is_some());
        assert!(definitions(DOC[0]).is_none());
        assert_eq!(Notes::of(["no notes"].into_iter()), Notes::default());
        assert_eq!(superscript(12), "¹²");
    }

    #[test]
    fn search_ranges_follow_the_numbers() {
        let n = Notes::of(DOC.iter().copied());
        let e = n.shown(DOC[0]).unwrap();
        let src = DOC[0];
        let r = src.find("now").unwrap()..src.find("now").unwrap() + 3;
        assert_eq!(&e.text[e.map(&r)], "now");
        let r = src.find("again").unwrap()..src.len();
        assert_eq!(&e.text[e.map(&r)], "again¹, so[^missing].");
        assert_eq!(&e.text[e.map(&(5..9))], "¹", "inside the reference");
    }
}
