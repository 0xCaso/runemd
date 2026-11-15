//! Le note a piè di pagina (`[^1]` nel testo, `[^1]: …` in un blocco a sé). Il kit non le accende:
//! la vista mette al posto del riferimento il numero in alto (cifre in apice, in ordine di primo
//! riferimento, come GitHub), non disegna i blocchi delle definizioni dove sono e le elenca in
//! fondo al file. Un riferimento senza definizione resta com'è scritto. Il sorgente non cambia.

use std::collections::HashMap;
use std::ops::Range;

/// Le note di un documento.
#[derive(Debug, Default, PartialEq)]
pub struct Notes {
    /// Il numero di ogni etichetta che ha una definizione e almeno un riferimento.
    numbers: HashMap<String, usize>,
    /// Il testo delle note, nell'ordine dei numeri (la prima è la 1).
    pub list: Vec<String>,
}

impl Notes {
    /// Le note dei blocchi `sources`, in ordine.
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

    /// `source` con i numeri in apice al posto dei riferimenti che hanno una nota. `None` se non
    /// ce n'è nessuno.
    pub fn shown(&self, source: &str) -> Option<Edited> {
        if self.is_empty() {
            return None;
        }
        let edits: Vec<(Range<usize>, String)> = references(source).into_iter().filter_map(|(r, label)| self.numbers.get(label).map(|&n| (r, superscript(n)))).collect();
        (!edits.is_empty()).then(|| Edited::new(source, edits))
    }
}

/// Un testo con dei tratti del sorgente sostituiti.
#[derive(Debug, PartialEq)]
pub struct Edited {
    pub text: String,
    edits: Vec<(Range<usize>, String)>,
}

impl Edited {
    fn new(source: &str, edits: Vec<(Range<usize>, String)>) -> Edited {
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

    /// `r`, un intervallo del sorgente, nel testo: ciò che cade dentro un tratto sostituito
    /// prende tutta la sostituzione.
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

/// `n` in cifre in apice.
fn superscript(n: usize) -> String {
    n.to_string().chars().map(|c| ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'][c as usize - '0' as usize]).collect()
}

/// Se il blocco è fatto di definizioni (`[^etichetta]: testo`, con le righe che seguono), le sue
/// note: (etichetta, testo).
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

/// `[^etichetta]:` all'inizio di `t`: l'etichetta e il resto.
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

/// I riferimenti `[^etichetta]` di un blocco, fuori dal codice: (dove, etichetta).
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

    const DOC: [&str; 4] = ["Prima[^b] e poi[^a], ancora[^b], e[^manca].", "[^a]: Nota A.\n[^b]: Nota B\n  su due righe.", "`[^a]` in codice", "```\n[^a]\n```"];

    #[test]
    fn notes_are_numbered_by_first_reference() {
        let n = Notes::of(DOC.iter().copied());
        assert_eq!(n.list, ["Nota B su due righe.", "Nota A."]);
        assert_eq!(n.shown(DOC[0]).unwrap().text, "Prima¹ e poi², ancora¹, e[^manca].");
        assert_eq!(n.shown(DOC[2]), None, "in codice resta");
        assert_eq!(n.shown(DOC[3]), None);
        assert!(definitions(DOC[1]).is_some());
        assert!(definitions(DOC[0]).is_none());
        assert_eq!(Notes::of(["niente note"].into_iter()), Notes::default());
        assert_eq!(superscript(12), "¹²");
    }

    #[test]
    fn search_ranges_follow_the_numbers() {
        let n = Notes::of(DOC.iter().copied());
        let e = n.shown(DOC[0]).unwrap();
        let src = DOC[0];
        let r = src.find("poi").unwrap()..src.find("poi").unwrap() + 3;
        assert_eq!(&e.text[e.map(&r)], "poi");
        let r = src.find("ancora").unwrap()..src.len();
        assert_eq!(&e.text[e.map(&r)], "ancora¹, e[^manca].");
        assert_eq!(&e.text[e.map(&(5..9))], "¹", "dentro il riferimento");
    }
}
