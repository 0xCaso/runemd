//! Records and plans (record 2026-11-16-tasks-sections-and-columns, with
//! 2026-11-16-wrap-only-label-blocks): paragraphs made of labelled lines (`Task 1:`, `Ruling:`) keep their
//! line breaks and the label in bold; `Task N` headings show the number and the progress.
//!
//! Everything else stays standard Markdown: a paragraph that is not a labelled block does not
//! change (constraint 3 of the line-break record).

use std::collections::HashMap;
use std::ops::Range;

use super::footnotes::Edited;

/// A line of a labelled block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// The line in the block's source, without `\n`.
    pub range: Range<usize>,
    /// The label in the source, with the `:`; `None` for a line without a label.
    pub label: Option<Range<usize>>,
}

impl Line {
    /// The text after the label (and its spaces), or the whole line if it has none.
    pub fn rest(&self, source: &str) -> Range<usize> {
        match &self.label {
            Some(l) => {
                let after = &source[l.end..self.range.end];
                l.end + (after.len() - after.trim_start().len())..self.range.end
            }
            None => self.range.clone(),
        }
    }

    /// The label without the trailing `:`, for the labels column.
    pub fn name(&self) -> Option<Range<usize>> {
        self.label.clone().map(|l| l.start..l.end - 1)
    }
}

/// The words of a label: letters and digits, with `( ) - _ / . # '` inside.
fn word_ok(w: &str) -> bool {
    !w.is_empty() && w.chars().all(|c| c.is_alphanumeric() || "()-_/.#'".contains(c)) && w.chars().any(char::is_alphanumeric)
}

/// End (after the `:`) of a `words:` segment that starts at `at` of `line`, with its words,
/// if the `:` is followed by a space or by the end of the line.
fn segment(line: &str, at: usize) -> Option<(usize, usize)> {
    let colon = at + line[at..].find(':')?;
    if !matches!(line.as_bytes().get(colon + 1), None | Some(b' ' | b'\t')) {
        return None;
    }
    let words: Vec<&str> = line[at..colon].split(' ').collect();
    words.iter().all(|w| word_ok(w)).then_some((colon + 1, words.len()))
}

/// The label at the start of `line` (constraint 2): from 1 to 3 words and `:`, the first capitalized
/// or a digit; plus a second segment of 1–2 words with `:` right after (`Task 1: minor
/// (deferred):`, mockup t-mix). The bytes it covers, `:` included.
pub fn label_of(line: &str) -> Option<Range<usize>> {
    let first = line.chars().next()?;
    if !(first.is_uppercase() || first.is_ascii_digit()) {
        return None;
    }
    let (end, n) = segment(line, 0)?;
    if !(1..=3).contains(&n) {
        return None;
    }
    let next = end + (line[end..].len() - line[end..].trim_start().len());
    match segment(line, next) {
        Some((end2, m)) if next < line.len() && m <= 2 && n + m <= 5 => Some(0..end2),
        _ => Some(0..end),
    }
}

/// Whether `source`, a top-level block, is a labelled paragraph: more than half of the lines, and
/// at least 2, start with a label (constraint 2 of the line-break record). Its lines.
pub fn block(source: &str) -> Option<Vec<Line>> {
    #[cfg(test)]
    if OFF.with(std::cell::Cell::get) {
        return None;
    }
    if !source.contains('\n') || !is_paragraph(source) {
        return None;
    }
    let mut lines = Vec::new();
    let mut at = 0;
    for l in source.split('\n') {
        lines.push(Line { range: at..at + l.len(), label: label_of(l).map(|r| at + r.start..at + r.end) });
        at += l.len() + 1;
    }
    let labeled = lines.iter().filter(|l| l.label.is_some()).count();
    (labeled >= 2 && labeled * 2 > lines.len()).then_some(lines)
}

#[cfg(test)]
thread_local! {
    /// In tests: turns labelled blocks off, to compare with how it was before.
    pub static OFF: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// A plain paragraph: no heading, list, quote, table, code, HTML, and no line
/// that becomes one.
fn is_paragraph(source: &str) -> bool {
    source.split('\n').all(|l| {
        let t = l.trim_start();
        let list = t.starts_with(['-', '*', '+']) && t[1..].starts_with(' ');
        let ordered = t.bytes().take_while(u8::is_ascii_digit).count() > 0 && t.trim_start_matches(|c: char| c.is_ascii_digit()).starts_with([')', '.']);
        !(l.starts_with("    ") || l.starts_with('\t') || list || ordered || t.starts_with(['#', '>', '|', '<']) || t.starts_with("```") || t.starts_with("~~~") || t.starts_with("==") || t.starts_with("--"))
    })
}

/// The block as it is drawn in one column: every line break is a line break (`\` at the end of the line) and
/// labels in bold.
pub fn one_column(source: &str, lines: &[Line]) -> Edited {
    let mut edits = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if let Some(r) = &l.label {
            edits.push((r.start..r.start, "**".to_owned()));
            edits.push((r.end..r.end, "**".to_owned()));
        }
        if i + 1 < lines.len() {
            edits.push((l.range.end..l.range.end, "\\".to_owned()));
        }
    }
    Edited::new(source, edits)
}

/// A `Task N` heading (constraint 3): the number and the title after `Task N:` in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskHead {
    pub level: u8,
    pub number: String,
    /// The source bytes before the title (`### Task 12: `), which are not drawn.
    pub prefix: Range<usize>,
    pub hashes: Range<usize>,
}

/// `source` is a `Task N: title` heading (also `Task N —`, `Task N.`).
pub fn task_head(source: &str) -> Option<TaskHead> {
    if source.contains('\n') {
        return None;
    }
    let hashes = source.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&hashes) {
        return None;
    }
    let after = source[hashes..].strip_prefix(' ')?;
    let start = hashes + 1;
    let rest = after.strip_prefix("Task ")?;
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let number = rest[..digits].to_owned();
    let tail = &rest[digits..];
    let sep = tail.len() - tail.trim_start_matches([':', '.', ' ', '—', '–', '-']).len();
    if sep == 0 || tail[sep..].trim().is_empty() {
        return None;
    }
    let end = start + "Task ".len() + digits + sep;
    Some(TaskHead { level: hashes as u8, number, prefix: 0..end, hashes: 0..hashes })
}

/// The progress of every `Task N` heading by block index: `[x]` boxes and boxes in
/// all between the heading and the next heading of equal or higher level. Only the headings with
/// at least one box.
pub fn progress<'a>(blocks: impl Iterator<Item = &'a str>) -> HashMap<usize, (u32, u32)> {
    let mut out = HashMap::new();
    let mut current: Option<(usize, u8)> = None;
    for (i, b) in blocks.enumerate() {
        let level = match super::breaks::Kind::of(b) {
            super::breaks::Kind::Heading(n) if !b.contains('\n') => Some(n),
            _ => None,
        };
        if let Some(n) = level {
            if current.is_some_and(|(_, l)| n <= l) {
                current = None;
            }
            if let Some(t) = task_head(b) {
                current = Some((i, t.level));
            }
            continue;
        }
        let Some((at, _)) = current else { continue };
        for l in b.lines() {
            let t = l.trim_start();
            let Some(rest) = t.strip_prefix(['-', '*', '+']).and_then(|r| r.strip_prefix(' ')) else { continue };
            let done = rest.starts_with("[x]") || rest.starts_with("[X]");
            if done || rest.starts_with("[ ]") {
                let e = out.entry(at).or_insert((0, 0));
                e.0 += done as u32;
                e.1 += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(l: &str) -> Option<&str> {
        label_of(l).map(|r| &l[r])
    }

    #[test]
    fn labels_are_short_and_start_the_line() {
        assert_eq!(label("Task 1: BASE root (no commit)"), Some("Task 1:"));
        assert_eq!(label("Ruling: dedicated keychain"), Some("Ruling:"));
        assert_eq!(label("Task 1: minor (deferred): Makefile without .PHONY"), Some("Task 1: minor (deferred):"));
        assert_eq!(label("Task 1: complete (commits root..0095282, review clean)"), Some("Task 1:"));
        assert_eq!(label("Report BLOCKED: scroll"), Some("Report BLOCKED:"));
        assert_eq!(label("Task 3: review (session wer-team) — Important 1: open_paths"), Some("Task 3:"));
        assert_eq!(label("Spec: docs/spec.md."), Some("Spec:"));
    }

    #[test]
    fn sentences_are_not_labels() {
        assert_eq!(label("measured again after the screen was unlocked: 118 fps."), None);
        assert_eq!(label("This is a long paragraph that says: something"), None);
        assert_eq!(label("see https://example.com"), None);
        assert_eq!(label("See https://example.com"), None);
        assert_eq!(label("note: lowercase"), None);
        assert_eq!(label("**Files:**"), None);
        assert_eq!(label("At 10:30 we land"), None);
    }

    #[test]
    fn a_block_needs_most_lines_labelled() {
        let reg = "Task 1: BASE root.\nRuling: keychain.\nmeasured again: 118 fps.\nTask 1: complete";
        let lines = block(reg).expect("record");
        assert_eq!(lines.iter().filter(|l| l.label.is_some()).count(), 3);
        assert_eq!(&reg[lines[2].rest(reg)], "measured again: 118 fps.");
        assert_eq!(&reg[lines[0].rest(reg)], "BASE root.");
        assert_eq!(&reg[lines[1].name().unwrap()], "Ruling");
        assert!(block("Task 1: one").is_none(), "a single line");
        assert!(block("Task 1: one\nother text\nmore text").is_none(), "half or less");
        assert!(block("Task 1: one\nRuling: two\nother\nother").is_none(), "exactly half");
        assert!(block("- Task 1: one\n- Ruling: two").is_none(), "list");
        assert!(block("A paragraph broken\nby hand.").is_none());
    }

    #[test]
    fn one_column_keeps_breaks_and_bolds_labels() {
        let s = "Task 1: one\nRuling: two\nother";
        let e = one_column(s, &block(s).unwrap());
        assert_eq!(e.text, "**Task 1:** one\\\n**Ruling:** two\\\nother");
        // A Find mark on the label also takes the `**` (the kit removes them from the drawing).
        assert_eq!(&e.text[e.map(&(12..18))], "**Ruling");
    }

    #[test]
    fn task_headings_give_number_and_title() {
        let h = task_head("### Task 12: Fire for the text").unwrap();
        assert_eq!((h.level, h.number.as_str()), (3, "12"));
        assert_eq!(&"### Task 12: Fire for the text"[h.prefix.end..], "Fire for the text");
        assert!(task_head("### Task 12").is_none());
        assert!(task_head("### Tasks").is_none());
        assert!(task_head("Task 1: x").is_none());
        assert_eq!(task_head("## Task 3 — Open").map(|h| h.number), Some("3".into()));
    }

    #[test]
    fn progress_counts_the_task_boxes() {
        let blocks = ["## Plan", "### Task 1: One", "**Files:**", "- [x] a\n- [x] b\n- [ ] c\n- [ ] d", "### Task 2: Two", "- [ ] e", "## Other", "- [x] f", "### Task 3: Three", "text"];
        let p = progress(blocks.into_iter());
        assert_eq!(p.get(&1), Some(&(2, 4)));
        assert_eq!(p.get(&4), Some(&(0, 1)));
        assert_eq!(p.get(&8), None, "no boxes, no progress");
        assert_eq!(p.len(), 2);
    }
}
