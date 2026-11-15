//! Registri e piani (record 2026-11-16-tasks-sections-and-columns, con 2026-11-16-a-capo-solo-
//! blocchi-etichetta): i paragrafi fatti di righe a etichetta (`Task 1:`, `Ruling:`) tengono i
//! loro a capo e l'etichetta in grassetto; i titoli `Task N` mostrano il numero e l'avanzamento.
//!
//! Tutto il resto resta Markdown standard: un paragrafo che non è un blocco a etichetta non
//! cambia (vincolo 3 del record degli a capo).

use std::collections::HashMap;
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

impl Line {
    /// Il testo dopo l'etichetta (e i suoi spazi), o la riga intera se non ne ha.
    pub fn rest(&self, source: &str) -> Range<usize> {
        match &self.label {
            Some(l) => {
                let after = &source[l.end..self.range.end];
                l.end + (after.len() - after.trim_start().len())..self.range.end
            }
            None => self.range.clone(),
        }
    }

    /// L'etichetta senza i `:` finali, per la colonna delle etichette.
    pub fn name(&self) -> Option<Range<usize>> {
        self.label.clone().map(|l| l.start..l.end - 1)
    }
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

/// Un titolo `Task N` (vincolo 3): il numero e il titolo dopo `Task N:` nel sorgente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskHead {
    pub level: u8,
    pub number: String,
    /// I byte del sorgente prima del titolo (`### Task 12: `), che non si disegnano.
    pub prefix: Range<usize>,
    pub hashes: Range<usize>,
}

/// `source` è un titolo `Task N: titolo` (anche `Task N —`, `Task N.`).
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

/// L'avanzamento di ogni titolo `Task N` per indice del blocco: caselle `[x]` e caselle in
/// tutto fra il titolo e il titolo seguente di livello uguale o più alto. Solo i titoli con
/// almeno una casella.
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
        assert_eq!(&reg[lines[2].rest(reg)], "misurato di nuovo: 118 fps.");
        assert_eq!(&reg[lines[0].rest(reg)], "BASE root.");
        assert_eq!(&reg[lines[1].name().unwrap()], "Ruling");
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

    #[test]
    fn task_headings_give_number_and_title() {
        let h = task_head("### Task 12: Fuoco per il testo").unwrap();
        assert_eq!((h.level, h.number.as_str()), (3, "12"));
        assert_eq!(&"### Task 12: Fuoco per il testo"[h.prefix.end..], "Fuoco per il testo");
        assert!(task_head("### Task 12").is_none());
        assert!(task_head("### Tasks").is_none());
        assert!(task_head("Task 1: x").is_none());
        assert_eq!(task_head("## Task 3 — Aprire").map(|h| h.number), Some("3".into()));
    }

    #[test]
    fn progress_counts_the_task_boxes() {
        let blocks = ["## Piano", "### Task 1: Uno", "**Files:**", "- [x] a\n- [x] b\n- [ ] c\n- [ ] d", "### Task 2: Due", "- [ ] e", "## Altro", "- [x] f", "### Task 3: Tre", "testo"];
        let p = progress(blocks.into_iter());
        assert_eq!(p.get(&1), Some(&(2, 4)));
        assert_eq!(p.get(&4), Some(&(0, 1)));
        assert_eq!(p.get(&8), None, "senza caselle niente avanzamento");
        assert_eq!(p.len(), 2);
    }
}
