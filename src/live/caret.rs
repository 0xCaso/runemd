//! Dove sta il caret "sta scrivendo" (record docs/decisions/2026-11-15-caret-al-punto-modificato.md):
//! nel punto dell'ultima modifica, dal confronto fra i blocchi di prima e quelli di adesso
//! (lo stesso `ids::assign` che segna i blocchi cambiati).
//!
//! - Un blocco cambiato: al carattere, dopo l'ultima parola toccata (prefisso e suffisso comuni
//!   fra il sorgente vecchio e il nuovo).
//! - Un blocco aggiunto: alla sua fine, dopo l'ultima parola (vincolo 3).
//! - Solo blocchi tolti: alla fine del blocco prima di quelli tolti.
//! - Nessuna modifica: nessun punto nuovo.
//!
//! Con più blocchi toccati vale l'ultimo nell'ordine del file. L'offset è in byte del sorgente
//! del blocco e cade sempre dopo un carattere che si vede (non spazi né segni del Markdown come
//! `*`, `_`, `` ` ``, `~`, `#`, `>`, `|`): lì la vista mette il caret, alla destra di quel
//! carattere. Se nessun carattere prima si vede, `Side::Before` sul primo.
//!
//! Se nella vista quel carattere non ha un rect (l'URL di un link, per esempio), il caret va
//! alla fine del blocco cambiato: `At::end` (vincolo 2, "altrimenti alla fine del blocco").

use crate::doc::document::{Block, Change};

/// Il punto del caret: blocco, carattere a cui si appoggia, da che parte.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct At {
    /// L'id del blocco.
    pub block: u64,
    /// Il carattere su cui si appoggia il caret, in byte del sorgente del blocco.
    pub anchor: std::ops::Range<usize>,
    pub side: Side,
    /// Il ripiego quando `anchor` non ha un rect nella vista (vincolo 2: un carattere che non si
    /// vede, come l'URL di un link): l'ultimo carattere del blocco che si vede, saltando la
    /// destinazione di un link in fondo. Il caret ci sta a destra.
    pub end: std::ops::Range<usize>,
}

/// Il caret sta a destra del carattere (il caso comune) o a sinistra (modifica all'inizio).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    After,
    Before,
}

/// I caratteri del Markdown che di solito non si vedono: il caret non ci si appoggia.
fn is_syntax(c: char) -> bool {
    matches!(c, '*' | '_' | '`' | '~' | '#' | '>' | '|')
}

fn visible(c: char) -> bool {
    !c.is_whitespace() && !is_syntax(c)
}

/// Il punto del caret dopo un aggiornamento da `old` a `new` con il suo `change`.
pub fn locate(old: &[Block], new: &[Block], change: &Change) -> Option<At> {
    let touched = |b: &&Block| change.added.contains(&b.id) || change.changed.contains(&b.id);
    if let Some(b) = new.iter().rev().find(touched) {
        let end = match old.iter().find(|o| o.id == b.id) {
            Some(o) => changed_end(&o.source, &b.source),
            None => b.source.len(),
        };
        return anchor(b, end);
    }
    // Solo blocchi tolti: alla fine del blocco sopra l'ultimo tolto che ne ha uno.
    let removed = old.iter().rposition(|o| change.removed.contains(&o.id))?;
    let above = old[..removed].iter().rev().find_map(|o| new.iter().find(|b| b.id == o.id))?;
    anchor(above, above.source.len())
}

/// Dove finisce la parte cambiata in `new` rispetto a `old`, in byte di `new`, estesa alla fine
/// della parola (lettere e cifre) in cui cade.
fn changed_end(old: &str, new: &str) -> usize {
    let prefix = common_prefix(old, new);
    let suffix = common_suffix(&old[prefix..], &new[prefix..]);
    let end = new.len() - suffix;
    // Dentro una parola: fino alla sua fine.
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

/// Il caret per `b` alla fine `end`: dopo l'ultimo carattere che si vede prima di `end`, o prima
/// del primo che si vede del blocco.
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

/// L'ultimo carattere che si vede di `s`, prima delle destinazioni di link in fondo
/// (`[testo](url)` finisce su "testo"). Il blocco ne ha uno: lo garantisce [`anchor`].
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

    /// Il punto dopo aver riscritto il file da `a` a `b`, come testo: il blocco e i byte fino al
    /// caret (compreso il carattere a cui si appoggia).
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
        let got = caret("# T\n\nUn agente", "# T\n\nUn agente scrive quando");
        assert_eq!(got, Some(("Un agente scrive quando".to_owned(), Side::After)));
        let got = caret("# T\n\nPrimo.\n", "# T\n\nPrimo.\n\nSecondo paragrafo");
        assert_eq!(got, Some(("Secondo paragrafo".to_owned(), Side::After)), "blocco nuovo in fondo");
    }

    #[test]
    fn an_edit_mid_file_puts_it_after_the_changed_word() {
        let a = "# T\n\nIl gatto dorme sul divano.\n\nFine del file.\n";
        let b = "# T\n\nIl cane dorme sul divano.\n\nFine del file.\n";
        assert_eq!(caret(a, b), Some(("Il cane".to_owned(), Side::After)));
        // Una parola cambiata a metà: il caret va alla fine della parola.
        let b = "# T\n\nIl gattino dorme sul divano.\n\nFine del file.\n";
        assert_eq!(caret(a, b), Some(("Il gattino".to_owned(), Side::After)));
    }

    #[test]
    fn with_several_blocks_touched_the_last_one_wins() {
        let a = "Uno.\n\nDue.\n\nTre.\n";
        let b = "Uno bis.\n\nDue.\n\nTre bis.\n";
        assert_eq!(caret(a, b), Some(("Tre bis".to_owned(), Side::After)));
    }

    #[test]
    fn a_deletion_inside_a_block_puts_it_at_the_cut() {
        let a = "Il gatto grande dorme.\n\nFine.\n";
        let b = "Il gatto dorme.\n\nFine.\n";
        assert_eq!(caret(a, b), Some(("Il gatto".to_owned(), Side::After)));
    }

    #[test]
    fn a_removed_block_puts_it_at_the_end_of_the_block_above() {
        let a = "Uno.\n\nDue.\n\nTre.\n";
        let b = "Uno.\n\nTre.\n";
        assert_eq!(caret(a, b), Some(("Uno.".to_owned(), Side::After)));
        assert_eq!(caret("Uno.\n", ""), None, "nessun blocco sopra");
    }

    #[test]
    fn an_unchanged_file_has_no_new_point() {
        assert_eq!(caret("# T\n\nTesto.\n", "# T\n\nTesto.\n"), None);
    }

    #[test]
    fn markdown_marks_and_spaces_are_skipped() {
        assert_eq!(caret("Testo.\n", "Testo **forte**\n"), Some(("Testo **forte".to_owned(), Side::After)));
        let got = caret("Testo.\n", "Testo.\n\n```rs\nfn a() {}\n```\n");
        assert_eq!(got, Some(("```rs\nfn a() {}".to_owned(), Side::After)), "dopo il codice, non sulla fence");
    }

    #[test]
    fn an_edit_at_the_start_puts_it_before_the_first_letter() {
        assert_eq!(caret("Primo testo.\n", "testo.\n"), Some((String::new(), Side::Before)));
        assert_eq!(caret("Testo uno.\n", "**Testo uno.\n"), Some(("**".to_owned(), Side::Before)), "un segno aggiunto davanti");
    }

    /// La fine del blocco per quando il carattere del caret non ha un rect (vincolo 2): l'ultimo
    /// carattere che si vede, saltando la destinazione di un link in fondo.
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
        assert_eq!(end("Il gatto dorme.\n", "Il cane dorme.\n"), "Il cane dorme.");
        assert_eq!(end("Vedi [la pagina](http://a.it/x).\n", "Vedi [la pagina](http://a.it/y)\n"), "Vedi [la pagina");
        assert_eq!(end("Testo [uno](u) e [due](http://a/b).\n", "Testo [uno](u) e [due](http://a/c)"), "Testo [uno](u) e [due");
    }

    #[test]
    fn multibyte_text_stays_on_char_boundaries() {
        assert_eq!(caret("Città.\n", "Città più"), Some(("Città più".to_owned(), Side::After)));
    }
}
