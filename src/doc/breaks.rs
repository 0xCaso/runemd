//! Where long words can break (paths, names_with_dashes, file.extension):
//! the source of a block with a zero-width space (U+200B, a break point for the
//! layout) after `/ - _ .` between two letters or digits and, only as a last resort, inside the stretches
//! without break points wider than a line. The real source does not change: this text goes
//! only to drawing; copy, index and Find read the block's source.
//!
//! Fenced code, HTML, link destinations and reference definitions are not
//! touched. A long bare URL becomes an explicit link `[text](<url>)` with the U+200B only
//! in the text: the autolink would also take the U+200B and the address would not open.

use std::ops::Range;

/// The zero-width space: a break point that is not seen.
pub const ZWSP: char = '\u{200B}';

/// Words shorter than this (in characters) stay as they are: they always fit on one line.
const MIN_WORD: usize = 16;

/// Where `source` can break in more places. `widest(i)` is the maximum width of a line, in pt,
/// for a block of kind `i` ([`Kind`]); `advance(text, kind)` the width of `text` on one line.
/// `None` if nothing is needed.
pub fn soft_breaks(source: &str, limit: impl Fn(Kind) -> f32, advance: impl Fn(&str, Kind) -> f32) -> Option<Breaks> {
    let first = source.trim_start_matches([' ', '\n']);
    if first.starts_with("```") || first.starts_with("~~~") || first.starts_with('<') || source.starts_with("    ") || source.starts_with('\t') {
        return None;
    }
    let mut ins = Vec::new();
    let mut start = 0;
    for line in source.split_inclusive('\n') {
        let kind = Kind::of(line);
        if !is_reference(line) {
            line_breaks(line, start, kind, &limit, &advance, &mut ins);
        }
        start += line.len();
    }
    if ins.is_empty() {
        return None;
    }
    let mut text = String::with_capacity(source.len() + ins.iter().map(|(_, s)| s.len()).sum::<usize>());
    let mut from = 0;
    for (p, s) in &ins {
        text.push_str(&source[from..*p]);
        text.push_str(s);
        from = *p;
    }
    text.push_str(&source[from..]);
    Some(Breaks { text, ins })
}

/// The text to draw and what was added to it: U+200B, and for a long bare URL the `[` and the
/// `](<url>)` of an explicit link with its `\` (source bytes, in order: each one sits before
/// the byte given).
#[derive(Debug, PartialEq)]
pub struct Breaks {
    pub text: String,
    ins: Vec<(usize, String)>,
}

impl Breaks {
    /// `r`, a range of the source, in the text to draw. An addition at the edge stays out.
    pub fn map(&self, r: &Range<usize>) -> Range<usize> {
        let shift = |o: usize, inclusive: bool| -> usize {
            self.ins.iter().take_while(|(p, _)| if inclusive { *p <= o } else { *p < o }).map(|(_, s)| s.len()).sum()
        };
        r.start + shift(r.start, true)..r.end + shift(r.end, false)
    }
}

/// The kind of line that decides the font and width of the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Heading of level 1..=6.
    Heading(u8),
    /// Table row: a cell is narrower than the column.
    Cell,
    /// Paragraph, list, quote.
    Body,
}

impl Kind {
    pub fn of(line: &str) -> Kind {
        let t = line.trim_start();
        let hashes = t.bytes().take_while(|&b| b == b'#').count();
        if (1..=6).contains(&hashes) && t[hashes..].starts_with([' ', '\t', '\n']) {
            Kind::Heading(hashes as u8)
        } else if t.starts_with('|') {
            Kind::Cell
        } else {
            Kind::Body
        }
    }
}

/// `[label]: destination`, a reference definition.
fn is_reference(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with('[') && t.find(']').is_some_and(|i| t[i + 1..].starts_with(':'))
}

/// The break points of a line (`start`: where it begins in the source).
fn line_breaks(line: &str, start: usize, kind: Kind, limit: &impl Fn(Kind) -> f32, advance: &impl Fn(&str, Kind) -> f32, out: &mut Vec<(usize, String)>) {
    let mut word_start = None;
    for (i, c) in line.char_indices().chain(std::iter::once((line.len(), ' '))) {
        match (c.is_whitespace(), word_start) {
            (false, None) => word_start = Some(i),
            (true, Some(w)) => {
                word_breaks(&line[w..i], start + w, kind, limit, advance, out);
                word_start = None;
            }
            _ => {}
        }
    }
}

/// The break points of a word (`start`: where it begins in the source). A bare URL becomes an
/// explicit link `[text](<url>)`: the U+200B go only in the text, the destination stays intact
/// (Rune Kit's autolink would also take the U+200B).
fn word_breaks(word: &str, start: usize, kind: Kind, limit: &impl Fn(Kind) -> f32, advance: &impl Fn(&str, Kind) -> f32, out: &mut Vec<(usize, String)>) {
    if word.chars().count() < MIN_WORD {
        return;
    }
    if let Some((r, www)) = bare_url(word) {
        let url = &word[r.clone()];
        let at = start + r.start;
        let cuts = cuts(url, &[], kind, limit, advance);
        if cuts.is_empty() {
            return;
        }
        out.push((at, "[".to_owned()));
        let mut cuts = cuts.into_iter().peekable();
        for (i, c) in url.char_indices() {
            if cuts.next_if_eq(&i).is_some() {
                out.push((at + i, ZWSP.to_string()));
            }
            if needs_escape(c) {
                out.push((at + i, "\\".to_owned()));
            }
        }
        let dest: String = url.chars().flat_map(|c| (needs_escape(c) && matches!(c, '\\' | '&')).then_some('\\').into_iter().chain([c])).collect();
        out.push((start + r.end, format!("](<{}{dest}>)", if www { "http://" } else { "" })));
        return;
    }
    let protected = protected(word);
    out.extend(cuts(word, &protected, kind, limit, advance).into_iter().map(|i| (start + i, ZWSP.to_string())));
}

/// The break points inside `word` (bytes of the word, in order), outside the `protected` stretches.
fn cuts(word: &str, protected: &[Range<usize>], kind: Kind, limit: &impl Fn(Kind) -> f32, advance: &impl Fn(&str, Kind) -> f32) -> Vec<usize> {
    let free = |i: usize| !protected.iter().any(|r| r.contains(&i));
    let chars: Vec<(usize, char)> = word.char_indices().collect();
    // After `/ - _ .` between two letters or digits.
    let mut cuts: Vec<usize> = Vec::new();
    for k in 1..chars.len().saturating_sub(1) {
        let (i, c) = chars[k];
        if matches!(c, '/' | '-' | '_' | '.') && chars[k - 1].1.is_alphanumeric() && chars[k + 1].1.is_alphanumeric() && free(i) {
            cuts.push(chars[k + 1].0);
        }
    }
    // Last resort: the stretches between two break points wider than the line, split where
    // they fill it.
    let max = limit(kind);
    let mut all = Vec::new();
    let mut from = 0;
    for end in cuts.iter().copied().chain(std::iter::once(word.len())) {
        let part = &word[from..end];
        if free(from) && advance(part, kind) > max {
            let mut line_start = from;
            for (i, _) in part.char_indices().skip(1).map(|(i, c)| (from + i, c)) {
                if !free(i) {
                    break;
                }
                if advance(&word[line_start..word[i..].chars().next().map_or(i, |c| i + c.len_utf8())], kind) > max {
                    all.push(i);
                    line_start = i;
                }
            }
        }
        if end < word.len() {
            all.push(end);
        }
        from = end;
    }
    all
}

/// The characters that must be written with `\` in the link text to stay literal.
pub(crate) fn needs_escape(c: char) -> bool {
    matches!(c, '\\' | '[' | ']' | '*' | '_' | '`' | '~' | '&')
}

/// The bare URL of a word, as Rune Kit's autolink finds it (GFM): at the start of the
/// word or after `( * _ ~`, with a domain that has a dot, without trailing punctuation. `true`
/// if it starts with `www.`. `None` inside code, links and quoted text.
fn bare_url(word: &str) -> Option<(Range<usize>, bool)> {
    let lead = word.len() - word.trim_start_matches(['(', '*', '_', '~']).len();
    let rest = &word[lead..];
    let prefix = ["https://", "http://", "www."].into_iter().find(|p| rest.get(..p.len()).is_some_and(|s| s.eq_ignore_ascii_case(p)))?;
    if word[..lead].ends_with('(') && lead > 1 && word[..lead - 1].ends_with(']') {
        return None;
    }
    let mut end = word.len();
    loop {
        let body = &word[lead + prefix.len()..end];
        let last = body.chars().next_back()?;
        let unbalanced = |o: char, c: char| last == c && body.matches(c).count() > body.matches(o).count();
        if matches!(last, '?' | '!' | '.' | ',' | ':' | ';' | '*' | '_' | '~' | '\'' | '"') || unbalanced('(', ')') || unbalanced('[', ']') {
            end -= last.len_utf8();
        } else {
            break;
        }
    }
    let body = &word[lead + prefix.len()..end];
    let domain = body.split(['/', '?', '#']).next().unwrap_or("");
    let ok = domain.contains('.') && !body.contains(['<', '>', '`']) && !word[end..].contains("](");
    ok.then_some((lead..end, prefix == "www."))
}

/// The parts of a word that do not break: bare URLs, link destinations `](…)`,
/// autolinks `<…>`.
fn protected(word: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let lead = word.len() - word.trim_start_matches(['(', '[', '*', '_', '`', '"', '\'', '!']).len();
    let rest = &word[lead..];
    if rest.starts_with("http://") || rest.starts_with("https://") || rest.starts_with("www.") {
        out.push(0..word.len());
        return out;
    }
    let mut from = 0;
    while let Some(i) = word[from..].find("](") {
        let open = from + i + 1;
        let close = word[open..].find(')').map_or(word.len(), |j| open + j + 1);
        out.push(open..close);
        from = close;
        if from >= word.len() {
            break;
        }
    }
    let mut from = 0;
    while let Some(i) = word[from..].find('<') {
        let open = from + i;
        let Some(j) = word[open..].find('>') else { break };
        if word[open..open + j].contains([':', '@']) {
            out.push(open..open + j + 1);
        }
        from = open + j + 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character 10 pt wide, lines of 200 pt.
    fn breaks(source: &str) -> Option<Breaks> {
        soft_breaks(source, |_| 200.0, |t, _| t.chars().count() as f32 * 10.0)
    }

    fn shown(source: &str) -> String {
        breaks(source).map_or(source.to_owned(), |b| b.text.replace(ZWSP, "|"))
    }

    #[test]
    fn a_long_path_breaks_after_each_separator() {
        assert_eq!(shown("# docs/decisions/orders-worker_next.v1"), "# docs/|decisions/|orders-|worker_|next.|v1");
    }

    #[test]
    fn short_words_and_plain_text_stay_as_they_are() {
        assert_eq!(breaks("A normal paragraph, with short words and a/b."), None);
        assert_eq!(breaks("src/main.rs"), None, "below MIN_WORD characters");
    }

    #[test]
    fn separators_at_the_edges_or_doubled_are_left_alone() {
        assert_eq!(shown("~/Desktop//GitHub/runemd-"), "~/Desktop//GitHub/|runemd-");
        assert_eq!(shown("__dunder_name_large__ x"), "__dunder_|name_|large__ x");
    }

    #[test]
    fn a_run_wider_than_a_line_breaks_anywhere_as_a_last_resort() {
        // 45 characters without separators: 450 pt, lines of 200 pt.
        assert_eq!(shown("abcdefghijklmnopqrstuvwxyabcdefghijklmnopqrst"), "abcdefghijklmnopqrst|uvwxyabcdefghijklmno|pqrst");
        // The stretch after "/" fits on one line: it breaks only there.
        assert_eq!(shown("abcdefgh/ijklmnopq"), "abcdefgh/|ijklmnopq");
    }

    #[test]
    fn code_html_urls_and_link_targets_are_not_touched() {
        assert_eq!(breaks("```\nsrc/ui/doc_view/quite/large.rs\n```"), None);
        assert_eq!(breaks("    src/ui/doc_view/quite/large.rs"), None);
        assert_eq!(breaks("<div>src/ui/doc_view/quite/large.rs</div>"), None);
        assert_eq!(shown("[docs/decisions/x](https://example.com/a/b/c-d)"), "[docs/|decisions/|x](https://example.com/a/b/c-d)");
        assert_eq!(breaks("[ref]: https://example.com/an/longpath/large.html"), None);
    }

    /// A bare URL breaks like the rest but stays a single link: it becomes an explicit link with
    /// the destination intact and the U+200B only in the text.
    #[test]
    fn a_bare_url_wraps_and_stays_one_whole_link() {
        let url = "https://github.com/anomalyco/rune/blob/384c2991e582418ba529835279c5876ac3915e02/crates/rune-kit/src/markdown/render.rs";
        for source in [format!("Long link: {url}"), format!("See ({url}), here."), format!("- **{url}**")] {
            let b = breaks(&source).expect("a long URL breaks");
            assert!(b.text.contains(ZWSP), "{}", b.text);
            let links: Vec<(String, String)> = links_of(&b.text);
            assert_eq!(links.len(), 1, "a single link: {}", b.text);
            assert_eq!(links[0].0, url, "destination intact");
            assert_eq!(links[0].1.replace(ZWSP, ""), url, "text equal to the URL");
        }
        // A short URL, in code or already inside a link does not change.
        assert_eq!(breaks("See https://a.it/b here"), None);
        assert_eq!(breaks(&format!("`{url}`")), None);
        assert_eq!(links_of(&shown_raw(&format!("[text]({url})")))[0].0, url);
        // Find marks also follow the `[` added in front.
        let b = breaks(&format!("x {url}")).unwrap();
        assert_eq!(b.text[b.map(&(2..7))].replace(ZWSP, ""), "https");
    }

    fn shown_raw(source: &str) -> String {
        breaks(source).map_or(source.to_owned(), |b| b.text)
    }

    /// The links of `text` as the parser sees them: (destination, text).
    fn links_of(text: &str) -> Vec<(String, String)> {
        use pulldown_cmark::{Event, Parser, Tag, TagEnd};
        let mut out = Vec::new();
        let mut open: Option<(String, String)> = None;
        for e in Parser::new(text) {
            match e {
                Event::Start(Tag::Link { dest_url, .. }) => open = Some((dest_url.into_string(), String::new())),
                Event::Text(t) | Event::Code(t) => {
                    if let Some(o) = open.as_mut() {
                        o.1.push_str(&t);
                    }
                }
                Event::End(TagEnd::Link) => out.extend(open.take()),
                _ => {}
            }
        }
        out
    }

    #[test]
    fn cells_and_list_items_break_too() {
        assert_eq!(shown("| a | docs/decisions/large |"), "| a | docs/|decisions/|large |");
        assert_eq!(shown("- crates/rune/src/text.rs"), "- crates/|rune/|src/|text.|rs");
    }

    #[test]
    fn search_ranges_follow_the_inserted_breaks() {
        let b = breaks("docs/decisions/large").unwrap();
        assert_eq!(b.text, "docs/\u{200B}decisions/\u{200B}large");
        // "decisions" in the source: 5..14.
        assert_eq!(&b.text[b.map(&(5..14))], "decisions");
        // "s/l": crosses a break point.
        assert_eq!(b.text[b.map(&(13..16))].replace(ZWSP, ""), "s/l");
        assert_eq!(&b.text[b.map(&(0..4))], "docs");
    }
}
