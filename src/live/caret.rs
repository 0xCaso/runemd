//! Where the "writing" caret sits (record docs/decisions/2026-11-15-caret-at-the-edit-point.md):
//! at the point of the last edit, from comparing the blocks before with the current ones
//! (the same `ids::assign` that marks the changed blocks).
//!
//! - A changed block: at the character, after the last word touched (common prefix and suffix
//!   between the old and the new source).
//! - An added block: at its end, after the last word (constraint 3).
//! - Only removed blocks: at the end of the block before the removed ones.
//! - No edit: no new point.
//!
//! With several blocks touched the last one in file order wins. The offset is in bytes of the block's
//! source and always falls after a visible character (not spaces or Markdown marks like
//! `*`, `_`, `` ` ``, `~`, `#`, `>`, `|`): there the view puts the caret, to the right of that
//! character. If no character before it is visible, `Side::Before` on the first one.
//!
//! If that character has no rect in the view (the URL of a link, for example), the caret goes
//! to the end of the changed block: `At::end` (constraint 2, "otherwise at the end of the block").

use crate::doc::document::{Block, Change};

/// The caret point: block, character it rests on, which side.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct At {
    /// The block id.
    pub block: u64,
    /// The character the caret rests on, in bytes of the block's source.
    pub anchor: std::ops::Range<usize>,
    pub side: Side,
    /// The fallback when `anchor` has no rect in the view (constraint 2: a character that does not
    /// show, like the URL of a link): the last visible character of the block, skipping the
    /// destination of a link at the end. The caret sits to its right.
    pub end: std::ops::Range<usize>,
}

/// The caret sits to the right of the character (the common case) or to the left (edit at the start).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    After,
    Before,
}

/// The Markdown characters that usually do not show: the caret does not rest on them.
fn is_syntax(c: char) -> bool {
    matches!(c, '*' | '_' | '`' | '~' | '#' | '>' | '|')
}

fn visible(c: char) -> bool {
    !c.is_whitespace() && !is_syntax(c)
}

/// The caret point after an update from `old` to `new` with its `change`.
pub fn locate(old: &[Block], new: &[Block], change: &Change) -> Option<At> {
    let touched = |b: &&Block| change.added.contains(&b.id) || change.changed.contains(&b.id);
    if let Some(b) = new.iter().rev().find(touched) {
        let end = match old.iter().find(|o| o.id == b.id) {
            Some(o) => changed_end(&o.source, &b.source),
            None => b.source.len(),
        };
        return anchor(b, end);
    }
    // Only removed blocks: at the end of the block above the last removed one that has one.
    let removed = old.iter().rposition(|o| change.removed.contains(&o.id))?;
    let above = old[..removed].iter().rev().find_map(|o| new.iter().find(|b| b.id == o.id))?;
    anchor(above, above.source.len())
}

/// Where the changed part ends in `new` compared with `old`, in bytes of `new`, extended to the end
/// of the word (letters and digits) it falls in.
fn changed_end(old: &str, new: &str) -> usize {
    let prefix = common_prefix(old, new);
    let suffix = common_suffix(&old[prefix..], &new[prefix..]);
    let end = new.len() - suffix;
    // Inside a word: up to its end.
    let rest = &new[end..];
    let inside = new[..end].chars().next_back().is_some_and(char::is_alphanumeric);
    if inside { end + rest.find(|c: char| !c.is_alphanumeric()).unwrap_or(rest.len()) } else { end }
}

fn common_prefix(a: &str, b: &str) -> usize {
    a.char_indices().zip(b.chars()).find(|((_, x), y)| x != y).map_or(a.len().min(b.len()), |((i, _), _)| i)
}

fn common_suffix(a: &str, b: &str) -> usize {
    a.chars().rev().zip(b.chars().rev()).take_while(|(x, y)| x == y).map(|(x, _)| x.len_utf8()).sum()
}

/// The caret for `b` at end `end`: after the last visible character before `end`, or before
/// the first visible one of the block.
fn anchor(b: &Block, end: usize) -> Option<At> {
    let s = &*b.source;
    let end = end.min(s.len());
    if let Some((i, c)) = s[..end].char_indices().rev().find(|(_, c)| visible(*c)) {
        return Some(At { block: b.id, anchor: i..i + c.len_utf8(), side: Side::After, end: block_end(s) });
    }
    let (i, c) = s.char_indices().find(|(_, c)| visible(*c))?;
    let anchor = i..i + c.len_utf8();
    Some(At { block: b.id, end: block_end(s), anchor, side: Side::Before })
}

/// The last visible character of `s`, before the link destinations at the end
/// (`[text](url)` ends on "text"). The block has one: [`anchor`] guarantees it.
fn block_end(s: &str) -> std::ops::Range<usize> {
    let mut end = s.trim_end().len();
    while let Some(i) = s[..end].ends_with(')').then(|| s[..end].rfind("](")).flatten() {
        end = i;
    }
    let last = |upto: usize| s[..upto].char_indices().rev().find(|(_, c)| visible(*c)).map(|(i, c)| i..i + c.len_utf8());
    last(end).or_else(|| last(s.len())).unwrap_or(0..0)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::doc::document::Document;

    /// The point after rewriting the file from `a` to `b`, as text: the block and the bytes up to the
    /// caret (including the character it rests on).
    fn caret(a: &str, b: &str) -> Option<(String, Side)> {
        let mut doc = Document::from_source(Path::new("/r/a.md"), a);
        let old = doc.blocks.clone();
        let change = doc.update(b);
        let at = locate(&old, &doc.blocks, &change)?;
        let block = doc.blocks.iter().find(|x| x.id == at.block).unwrap();
        let upto = match at.side {
            Side::After => &block.source[..at.anchor.end],
            Side::Before => &block.source[..at.anchor.start],
        };
        Some((upto.to_owned(), at.side))
    }

    #[test]
    fn an_append_at_the_end_puts_it_after_the_last_word() {
        let got = caret("# T\n\nAn agent", "# T\n\nAn agent writes when");
        assert_eq!(got, Some(("An agent writes when".to_owned(), Side::After)));
        let got = caret("# T\n\nFirst.\n", "# T\n\nFirst.\n\nSecond paragraph");
        assert_eq!(got, Some(("Second paragraph".to_owned(), Side::After)), "new block at the end");
    }

    #[test]
    fn an_edit_mid_file_puts_it_after_the_changed_word() {
        let a = "# T\n\nThe cat sleeps on the sofa.\n\nEnd of file.\n";
        let b = "# T\n\nThe dog sleeps on the sofa.\n\nEnd of file.\n";
        assert_eq!(caret(a, b), Some(("The dog".to_owned(), Side::After)));
        // A word changed in the middle: the caret goes to the end of the word.
        let b = "# T\n\nThe cart sleeps on the sofa.\n\nEnd of file.\n";
        assert_eq!(caret(a, b), Some(("The cart".to_owned(), Side::After)));
    }

    #[test]
    fn with_several_blocks_touched_the_last_one_wins() {
        let a = "One.\n\nTwo.\n\nThree.\n";
        let b = "One bis.\n\nTwo.\n\nThree bis.\n";
        assert_eq!(caret(a, b), Some(("Three bis".to_owned(), Side::After)));
    }

    #[test]
    fn a_deletion_inside_a_block_puts_it_at_the_cut() {
        let a = "The cat really sleeps.\n\nEnd.\n";
        let b = "The cat sleeps.\n\nEnd.\n";
        assert_eq!(caret(a, b), Some(("The cat".to_owned(), Side::After)));
    }

    #[test]
    fn a_removed_block_puts_it_at_the_end_of_the_block_above() {
        let a = "One.\n\nTwo.\n\nThree.\n";
        let b = "One.\n\nThree.\n";
        assert_eq!(caret(a, b), Some(("One.".to_owned(), Side::After)));
        assert_eq!(caret("One.\n", ""), None, "no block above");
    }

    #[test]
    fn an_unchanged_file_has_no_new_point() {
        assert_eq!(caret("# T\n\nText.\n", "# T\n\nText.\n"), None);
    }

    #[test]
    fn markdown_marks_and_spaces_are_skipped() {
        assert_eq!(caret("Text.\n", "Text **bold**\n"), Some(("Text **bold".to_owned(), Side::After)));
        let got = caret("Text.\n", "Text.\n\n```rs\nfn a() {}\n```\n");
        assert_eq!(got, Some(("```rs\nfn a() {}".to_owned(), Side::After)), "after the code, not on the fence");
    }

    #[test]
    fn an_edit_at_the_start_puts_it_before_the_first_letter() {
        assert_eq!(caret("First text.\n", "text.\n"), Some((String::new(), Side::Before)));
        assert_eq!(caret("Text one.\n", "**Text one.\n"), Some(("**".to_owned(), Side::Before)), "a mark added in front");
    }

    /// The end of the block for when the caret character has no rect (constraint 2): the last
    /// visible character, skipping the destination of a link at the end.
    #[test]
    fn the_fallback_is_the_last_visible_character_of_the_block() {
        let end = |a: &str, b: &str| {
            let mut doc = Document::from_source(Path::new("/r/a.md"), a);
            let old = doc.blocks.clone();
            let change = doc.update(b);
            let at = locate(&old, &doc.blocks, &change).unwrap();
            let block = doc.blocks.iter().find(|x| x.id == at.block).unwrap();
            block.source[..at.end.end].to_owned()
        };
        assert_eq!(end("The cat sleeps.\n", "The dog sleeps.\n"), "The dog sleeps.");
        assert_eq!(end("See [the page](http://a.it/x).\n", "See [the page](http://a.it/y)\n"), "See [the page");
        assert_eq!(end("Text [one](u) and [two](http://a/b).\n", "Text [one](u) and [two](http://a/c)"), "Text [one](u) and [two");
    }

    #[test]
    fn multibyte_text_stays_on_char_boundaries() {
        assert_eq!(caret("Café.\n", "Café crème"), Some(("Café crème".to_owned(), Side::After)));
    }
}
