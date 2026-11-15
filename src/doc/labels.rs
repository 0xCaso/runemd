//! Registri e piani (record 2026-11-16-tasks-sezioni-e-colonne, con 2026-11-16-a-capo-solo-
//! blocchi-etichetta): i paragrafi fatti di righe a etichetta (`Task 1:`, `Ruling:`) tengono i
//! loro a capo e l'etichetta in grassetto; i titoli `Task N` mostrano il numero e l'avanzamento.
//!
//! Tutto il resto resta Markdown standard: un paragrafo che non è un blocco a etichetta non
//! cambia (vincolo 3 del record degli a capo).

use std::ops::Range;

use super::footnotes::Edited;

/// Una riga di un blocco a etichetta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    /// La riga nel sorgente del blocco, senza `\n`.
    pub range: Range<usize>,
    /// L'etichetta nel sorgente, con i `:`; `None` per una riga senza etichetta.
    pub label: Option<Range<usize>>,
}

/// Le parole di un'etichetta: lettere e cifre, con `( ) - _ / . # '` dentro.
fn word_ok(w: &str) -> bool {
    !w.is_empty() && w.chars().all(|c| c.is_alphanumeric() || "()-_/.#'".contains(c)) && w.chars().any(char::is_alphanumeric)
}

/// Fine (dopo i `:`) di un segmento `parole:` che comincia in `at` di `line`, con le sue parole,
/// se i `:` sono seguiti da uno spazio o dalla fine della riga.
fn segment(line: &str, at: usize) -> Option<(usize, usize)> {
    let colon = at + line[at..].find(':')?;
    if !matches!(line.as_bytes().get(colon + 1), None | Some(b' ' | b'\t')) {
        return None;
    }
    let words: Vec<&str> = line[at..colon].split(' ').collect();
    words.iter().all(|w| word_ok(w)).then_some((colon + 1, words.len()))
}

/// L'etichetta a inizio di `line` (vincolo 2): da 1 a 3 parole e `:`, la prima con la maiuscola
/// o una cifra; più un secondo segmento di 1–2 parole con `:` subito dopo (`Task 1: minor
/// (deferred):`, mockup t-mix). I byte che copre, `:` compresi.
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

/// Se `source`, un blocco top-level, è un paragrafo a etichetta: più della metà delle righe, e
/// almeno 2, cominciano con un'etichetta (vincolo 2 del record degli a capo). Le sue righe.
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
    /// Nei test: spegne i blocchi a etichetta, per confrontare con com'era prima.
    pub static OFF: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Un paragrafo semplice: niente titolo, lista, citazione, tabella, codice, HTML, e nessuna riga
/// che lo diventa.
fn is_paragraph(source: &str) -> bool {
    source.split('\n').all(|l| {
        let t = l.trim_start();
        let list = t.starts_with(['-', '*', '+']) && t[1..].starts_with(' ');
        let ordered = t.bytes().take_while(u8::is_ascii_digit).count() > 0 && t.trim_start_matches(|c: char| c.is_ascii_digit()).starts_with([')', '.']);
        !(l.starts_with("    ") || l.starts_with('\t') || list || ordered || t.starts_with(['#', '>', '|', '<']) || t.starts_with("```") || t.starts_with("~~~") || t.starts_with("==") || t.starts_with("--"))
    })
}

/// Il blocco come si disegna su una colonna: ogni a capo è un a capo (`\` a fine riga) ed
/// etichette in grassetto.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn label(l: &str) -> Option<&str> {
        label_of(l).map(|r| &l[r])
    }

    #[test]
    fn labels_are_short_and_start_the_line() {
        assert_eq!(label("Task 1: BASE root (nessun commit)"), Some("Task 1:"));
        assert_eq!(label("Ruling: keychain dedicato"), Some("Ruling:"));
        assert_eq!(label("Task 1: minor (deferred): Makefile senza .PHONY"), Some("Task 1: minor (deferred):"));
        assert_eq!(label("Task 1: complete (commits root..0095282, review clean)"), Some("Task 1:"));
        assert_eq!(label("Report BLOCKED: scroll"), Some("Report BLOCKED:"));
        assert_eq!(label("Task 3: review (session wer-team) — Important 1: open_paths"), Some("Task 3:"));
        assert_eq!(label("Spec: docs/spec.md."), Some("Spec:"));
    }

    #[test]
    fn sentences_are_not_labels() {
        assert_eq!(label("misurato di nuovo dopo lo sblocco dello schermo: 118 fps."), None);
        assert_eq!(label("Questo è un paragrafo lungo che dice: qualcosa"), None);
        assert_eq!(label("vedi https://example.com"), None);
        assert_eq!(label("Vedi https://example.com"), None);
        assert_eq!(label("nota: minuscola"), None);
        assert_eq!(label("**Files:**"), None);
        assert_eq!(label("Ore 10:30 arrivo"), None);
    }

    #[test]
    fn a_block_needs_most_lines_labelled() {
        let reg = "Task 1: BASE root.\nRuling: keychain.\nmisurato di nuovo: 118 fps.\nTask 1: complete";
        let lines = block(reg).expect("registro");
        assert_eq!(lines.iter().filter(|l| l.label.is_some()).count(), 3);
        assert!(block("Task 1: uno").is_none(), "una riga sola");
        assert!(block("Task 1: uno\naltro testo\nancora testo").is_none(), "metà o meno");
        assert!(block("Task 1: uno\nRuling: due\naltro\naltro").is_none(), "metà esatta");
        assert!(block("- Task 1: uno\n- Ruling: due").is_none(), "lista");
        assert!(block("Un paragrafo andato\na capo a mano.").is_none());
    }

    #[test]
    fn one_column_keeps_breaks_and_bolds_labels() {
        let s = "Task 1: uno\nRuling: due\naltro";
        let e = one_column(s, &block(s).unwrap());
        assert_eq!(e.text, "**Task 1:** uno\\\n**Ruling:** due\\\naltro");
        // Un segno di Cerca sull'etichetta prende anche i `**` (il kit li toglie dal disegno).
        assert_eq!(&e.text[e.map(&(12..18))], "**Ruling");
    }
}
