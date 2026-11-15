//! I callout di GitHub: una citazione che comincia con una riga `> [!NOTE]` (o `TIP`, `IMPORTANT`,
//! `WARNING`, `CAUTION`, maiuscole o no). Il kit non li riconosce: la vista toglie quella riga dal
//! testo che si disegna e mette al suo posto l'etichetta in parole con l'icona. Il sorgente non
//! cambia: copia, indice e Cerca leggono il sorgente del blocco.

use std::ops::Range;

/// Il tipo di callout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
}

impl Kind {
    /// L'etichetta in parole.
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

/// Un callout: il tipo e il testo da disegnare, cioè la citazione senza la riga del segno.
#[derive(Debug, PartialEq)]
pub struct Callout {
    pub kind: Kind,
    pub text: String,
    /// La riga del segno nel sorgente, a capo compreso.
    cut: Range<usize>,
}

impl Callout {
    /// `r`, un intervallo del sorgente, nel testo da disegnare: ciò che cade nella riga del segno
    /// si stringe al suo posto.
    pub fn map(&self, r: &Range<usize>) -> Range<usize> {
        let at = |o: usize| if o <= self.cut.start { o } else { o.saturating_sub(self.cut.len()).max(self.cut.start) };
        at(r.start)..at(r.end)
    }
}

/// Il callout del blocco `source`, se la sua prima riga è `> [!TIPO]` da sola.
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
        let c = of("> [!NOTE]\n> Una nota in stile GitHub.").unwrap();
        assert_eq!((c.kind, c.text.as_str()), (Kind::Note, "> Una nota in stile GitHub."));
        assert_eq!(of("> [!warning]\n> x").unwrap().kind, Kind::Warning);
        assert_eq!(of(">[!CAUTION]").unwrap().text, "");
        assert_eq!(of("> [!NOTA]\n> x"), None, "tipo sconosciuto");
        assert_eq!(of("> [!NOTE] e testo\n> x"), None, "il segno va da solo sulla riga");
        assert_eq!(of("[!NOTE]\n> x"), None, "fuori da una citazione");
    }

    #[test]
    fn search_ranges_skip_the_marker() {
        let src = "> [!TIP]\n> Un suggerimento.";
        let c = of(src).unwrap();
        let r = src.find("Un").unwrap()..src.len();
        assert_eq!(&c.text[c.map(&r)], "Un suggerimento.");
        assert_eq!(c.map(&(3..6)), 0..0, "dentro il segno");
    }
}
