//! Dove possono andare a capo le parole lunghe (percorsi, nomi_con_trattini, file.estensione):
//! il sorgente di un blocco con uno spazio di larghezza zero (U+200B, un punto di a capo per il
//! layout) dopo `/ - _ .` fra due lettere o cifre e, solo come ultima risorsa, dentro i tratti
//! senza punti di a capo più larghi di una riga. Il sorgente vero non cambia: questo testo va
//! solo al disegno; copia, indice e Cerca leggono il sorgente del blocco.
//!
//! Non si tocca il codice a blocchi, l'HTML, le destinazioni dei link e le definizioni dei
//! riferimenti. Un URL nudo lungo diventa un link esplicito `[testo](<url>)` con gli U+200B solo
//! nel testo: il link automatico prenderebbe anche gli U+200B e l'indirizzo non si aprirebbe.

use std::ops::Range;

/// Lo spazio di larghezza zero: un punto di a capo che non si vede.
pub const ZWSP: char = '\u{200B}';

/// Le parole più corte di così (in caratteri) restano come sono: stanno sempre in una riga.
const MIN_WORD: usize = 16;

/// Dove `source` può andare a capo in più. `widest(i)` è la larghezza massima di una riga, in pt,
/// per un blocco di tipo `i` ([`Kind`]); `advance(text, kind)` la larghezza di `text` su una riga.
/// `None` se non serve nulla.
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

/// Il testo da disegnare e cosa vi è stato aggiunto: U+200B, e per un URL nudo lungo il `[` e il
/// `](<url>)` di un link esplicito con le sue `\` (byte del sorgente, in ordine: ognuno sta prima
/// del byte indicato).
#[derive(Debug, PartialEq)]
pub struct Breaks {
    pub text: String,
    ins: Vec<(usize, String)>,
}

impl Breaks {
    /// `r`, un intervallo del sorgente, nel testo da disegnare. Un'aggiunta al bordo resta fuori.
    pub fn map(&self, r: &Range<usize>) -> Range<usize> {
        let shift = |o: usize, inclusive: bool| -> usize {
            self.ins.iter().take_while(|(p, _)| if inclusive { *p <= o } else { *p < o }).map(|(_, s)| s.len()).sum()
        };
        r.start + shift(r.start, true)..r.end + shift(r.end, false)
    }
}

/// Il tipo di riga che decide font e larghezza della riga.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Titolo di livello 1..=6.
    Heading(u8),
    /// Riga di tabella: una cella è più stretta della colonna.
    Cell,
    /// Paragrafo, lista, citazione.
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

/// `[etichetta]: destinazione`, una definizione di riferimento.
fn is_reference(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with('[') && t.find(']').is_some_and(|i| t[i + 1..].starts_with(':'))
}

/// I punti di a capo di una riga (`start`: dove comincia nel sorgente).
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

/// I punti di a capo di una parola (`start`: dove comincia nel sorgente). Un URL nudo diventa un
/// link esplicito `[testo](<url>)`: gli U+200B vanno solo nel testo, la destinazione resta intatta
/// (il link automatico di Rune Kit prenderebbe anche gli U+200B).
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

/// I punti di a capo dentro `word` (byte della parola, in ordine), fuori dai tratti `protected`.
fn cuts(word: &str, protected: &[Range<usize>], kind: Kind, limit: &impl Fn(Kind) -> f32, advance: &impl Fn(&str, Kind) -> f32) -> Vec<usize> {
    let free = |i: usize| !protected.iter().any(|r| r.contains(&i));
    let chars: Vec<(usize, char)> = word.char_indices().collect();
    // Dopo `/ - _ .` fra due lettere o cifre.
    let mut cuts: Vec<usize> = Vec::new();
    for k in 1..chars.len().saturating_sub(1) {
        let (i, c) = chars[k];
        if matches!(c, '/' | '-' | '_' | '.') && chars[k - 1].1.is_alphanumeric() && chars[k + 1].1.is_alphanumeric() && free(i) {
            cuts.push(chars[k + 1].0);
        }
    }
    // Ultima risorsa: i tratti fra due punti di a capo più larghi della riga, spezzati dove
    // arrivano a riempirla.
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

/// I caratteri che nel testo del link vanno scritti con `\` per restare letterali.
pub(crate) fn needs_escape(c: char) -> bool {
    matches!(c, '\\' | '[' | ']' | '*' | '_' | '`' | '~' | '&')
}

/// L'URL nudo di una parola, come lo trova il link automatico di Rune Kit (GFM): all'inizio della
/// parola o dopo `( * _ ~`, con un dominio che ha un punto, senza la punteggiatura in coda. `true`
/// se comincia con `www.`. `None` dentro codice, link e citazioni fra virgolette.
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

/// Le parti di una parola che non si spezzano: URL nudi, destinazioni dei link `](…)`, link
/// automatici `<…>`.
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

    /// Ogni carattere largo 10 pt, righe di 200 pt.
    fn breaks(source: &str) -> Option<Breaks> {
        soft_breaks(source, |_| 200.0, |t, _| t.chars().count() as f32 * 10.0)
    }

    fn shown(source: &str) -> String {
        breaks(source).map_or(source.to_owned(), |b| b.text.replace(ZWSP, "|"))
    }

    #[test]
    fn a_long_path_breaks_after_each_separator() {
        assert_eq!(shown("# docs/decisions/ordine-lavori_dopo.v1"), "# docs/|decisions/|ordine-|lavori_|dopo.|v1");
    }

    #[test]
    fn short_words_and_plain_text_stay_as_they_are() {
        assert_eq!(breaks("Un paragrafo normale, con parole corte e a/b."), None);
        assert_eq!(breaks("src/main.rs"), None, "sotto MIN_WORD caratteri");
    }

    #[test]
    fn separators_at_the_edges_or_doubled_are_left_alone() {
        assert_eq!(shown("~/Desktop//GitHub/runemd-"), "~/Desktop//GitHub/|runemd-");
        assert_eq!(shown("__dunder_nome_lungo__ x"), "__dunder_|nome_|lungo__ x");
    }

    #[test]
    fn a_run_wider_than_a_line_breaks_anywhere_as_a_last_resort() {
        // 45 caratteri senza separatori: 450 pt, righe di 200 pt.
        assert_eq!(shown("abcdefghijklmnopqrstuvwxyabcdefghijklmnopqrst"), "abcdefghijklmnopqrst|uvwxyabcdefghijklmno|pqrst");
        // Il tratto dopo "/" sta in una riga: si spezza solo lì.
        assert_eq!(shown("abcdefgh/ijklmnopq"), "abcdefgh/|ijklmnopq");
    }

    #[test]
    fn code_html_urls_and_link_targets_are_not_touched() {
        assert_eq!(breaks("```\nsrc/ui/doc_view/molto/lungo.rs\n```"), None);
        assert_eq!(breaks("    src/ui/doc_view/molto/lungo.rs"), None);
        assert_eq!(breaks("<div>src/ui/doc_view/molto/lungo.rs</div>"), None);
        assert_eq!(shown("[docs/decisions/x](https://example.com/a/b/c-d)"), "[docs/|decisions/|x](https://example.com/a/b/c-d)");
        assert_eq!(breaks("[rif]: https://example.com/un/percorso/lungo.html"), None);
    }

    /// Un URL nudo va a capo come il resto ma resta un link solo: diventa un link esplicito con
    /// la destinazione intatta e gli U+200B solo nel testo.
    #[test]
    fn a_bare_url_wraps_and_stays_one_whole_link() {
        let url = "https://github.com/anomalyco/rune/blob/384c2991e582418ba529835279c5876ac3915e02/crates/rune-kit/src/markdown/render.rs";
        for source in [format!("Link lungo: {url}"), format!("Vedi ({url}), qui."), format!("- **{url}**")] {
            let b = breaks(&source).expect("un URL lungo va a capo");
            assert!(b.text.contains(ZWSP), "{}", b.text);
            let links: Vec<(String, String)> = links_of(&b.text);
            assert_eq!(links.len(), 1, "un link solo: {}", b.text);
            assert_eq!(links[0].0, url, "destinazione intatta");
            assert_eq!(links[0].1.replace(ZWSP, ""), url, "testo uguale all'URL");
        }
        // Un URL corto, in codice o già dentro un link non cambia.
        assert_eq!(breaks("Vedi https://a.it/b qui"), None);
        assert_eq!(breaks(&format!("`{url}`")), None);
        assert_eq!(links_of(&shown_raw(&format!("[testo]({url})")))[0].0, url);
        // I segni di Cerca seguono anche il `[` aggiunto davanti.
        let b = breaks(&format!("x {url}")).unwrap();
        assert_eq!(b.text[b.map(&(2..7))].replace(ZWSP, ""), "https");
    }

    fn shown_raw(source: &str) -> String {
        breaks(source).map_or(source.to_owned(), |b| b.text)
    }

    /// I link di `text` come li vede il parser: (destinazione, testo).
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
        assert_eq!(shown("| a | docs/decisions/lungo |"), "| a | docs/|decisions/|lungo |");
        assert_eq!(shown("- crates/rune/src/text.rs"), "- crates/|rune/|src/|text.|rs");
    }

    #[test]
    fn search_ranges_follow_the_inserted_breaks() {
        let b = breaks("docs/decisions/lungo").unwrap();
        assert_eq!(b.text, "docs/\u{200B}decisions/\u{200B}lungo");
        // "decisions" nel sorgente: 5..14.
        assert_eq!(&b.text[b.map(&(5..14))], "decisions");
        // "s/l": attraversa un punto di a capo.
        assert_eq!(b.text[b.map(&(13..16))].replace(ZWSP, ""), "s/l");
        assert_eq!(&b.text[b.map(&(0..4))], "docs");
    }
}
