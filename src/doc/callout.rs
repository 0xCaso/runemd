//! GitHub callouts: a quote that starts with a `> [!NOTE]` line (or `TIP`, `IMPORTANT`,
//! `WARNING`, `CAUTION`, any case). The kit does not recognize them: the view removes that line from the
//! text it draws and puts in its place the label in words with the icon. The source does not
//! change: copy, index and Find read the block's source.

use std::ops::Range;

/// The kind of callout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl Kind {
    /// The label in words.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Note => "Note",
            Kind::Tip => "Tip",
            Kind::Important => "Important",
            Kind::Warning => "Warning",
            Kind::Caution => "Caution",
        }
    }
}

/// A callout: the kind and the text to draw, that is the quote without the marker line.
#[derive(Debug, PartialEq)]
pub struct Callout {
    pub kind: Kind,
    pub text: String,
    /// The marker line in the source, newline included.
    cut: Range<usize>,
}

impl Callout {
    /// `r`, a range of the source, in the text to draw: what falls in the marker line
    /// shrinks to its place.
    pub fn map(&self, r: &Range<usize>) -> Range<usize> {
        let at = |o: usize| if o <= self.cut.start { o } else { o.saturating_sub(self.cut.len()).max(self.cut.start) };
        at(r.start)..at(r.end)
    }
}

/// The callout of block `source`, if its first line is `> [!KIND]` on its own.
pub fn of(source: &str) -> Option<Callout> {
    let line_end = source.find('\n').map_or(source.len(), |i| i + 1);
    let line = source[..line_end].trim_end();
    let rest = line.trim_start_matches(' ').strip_prefix('>')?.trim();
    let name = rest.strip_prefix("[!")?.strip_suffix(']')?;
    let kind = match name.to_ascii_uppercase().as_str() {
        "NOTE" => Kind::Note,
        "TIP" => Kind::Tip,
        "IMPORTANT" => Kind::Important,
        "WARNING" => Kind::Warning,
        "CAUTION" => Kind::Caution,
        _ => return None,
    };
    let text = source[line_end..].to_owned();
    Some(Callout { kind, text, cut: 0..line_end })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_marker_line_goes_and_the_quote_stays() {
        let c = of("> [!NOTE]\n> A note in GitHub style.").unwrap();
        assert_eq!((c.kind, c.text.as_str()), (Kind::Note, "> A note in GitHub style."));
        assert_eq!(of("> [!warning]\n> x").unwrap().kind, Kind::Warning);
        assert_eq!(of(">[!CAUTION]").unwrap().text, "");
        assert_eq!(of("> [!MEMO]\n> x"), None, "unknown kind");
        assert_eq!(of("> [!NOTE] and text\n> x"), None, "the marker goes alone on the line");
        assert_eq!(of("[!NOTE]\n> x"), None, "outside a quote");
    }

    #[test]
    fn search_ranges_skip_the_marker() {
        let src = "> [!TIP]\n> A suggestion.";
        let c = of(src).unwrap();
        let r = src.find("A s").unwrap()..src.len();
        assert_eq!(&c.text[c.map(&r)], "A suggestion.");
        assert_eq!(c.map(&(3..6)), 0..0, "inside the marker");
    }
}
