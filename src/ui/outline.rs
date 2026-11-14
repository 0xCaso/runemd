//! L'indice dei titoli, la colonna "In questo file" a destra del documento (DESIGN.md, "Indice
//! dei titoli"; mockup `scheda`): i titoli H1–H3 del file, uno per riga, rientrati per livello;
//! quello della sezione in cima alla vista in `fg` e peso 600, gli altri in `fg2`. Un clic porta
//! il titolo in cima alla vista.
//!
//! I titoli si calcolano una volta per documento e per ricarica ([`Cache`], con la versione delle
//! righe di `doc_view`), non a ogni frame: solo i blocchi che possono essere un titolo passano dal
//! parser. La lista è a righe virtuali, così un file con migliaia di titoli costruisce solo le
//! righe in vista.

use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use rune::{El, GrowthSpec, ScrollSpec, TextStyle, Tx, col, el, row, text};

use super::sidebar::{self, Hooks, Motion};
use super::theme::{
    HAIRLINE, OUTLINE_INDENT, OUTLINE_LABEL_GAP, OUTLINE_PAD_X, OUTLINE_ROW, OUTLINE_ROW_PAD, OUTLINE_TEXT, OUTLINE_W, Theme, UI_STRONG_WEIGHT, UI_WEIGHT,
};
use crate::app::{App, Tab};
use crate::doc::document::Document;

/// L'etichetta della colonna (mockup `scheda`).
pub const LABEL: &str = "In questo file";

/// Il livello più profondo che l'indice mostra.
const DEEPEST: u8 = 3;

/// Un titolo del documento: livello (1–3), testo, id del blocco e sua posizione nel documento
/// alla revisione da cui viene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub id: u64,
    pub index: usize,
}

/// I titoli H1–H3 di `doc` con la posizione del blocco. Un titolo è un blocco top-level (un `#`
/// dentro una fence sta nel blocco della fence, dentro una lista nel blocco della lista).
pub fn scan(doc: &Document) -> Vec<Heading> {
    doc.blocks
        .iter()
        .enumerate()
        // Il frontmatter (solo il primo blocco) non è un titolo, anche se finisce con `---`.
        .filter(|(i, b)| maybe_heading(&b.source) && !(*i == 0 && crate::doc::frontmatter::is_block(&b.source)))
        .filter_map(|(index, b)| {
            let (level, text) = heading(&b.source)?;
            (level <= DEEPEST).then_some(Heading { level, text, id: b.id, index })
        })
        .collect()
}

/// Il filtro veloce, prima del parser: un titolo ATX comincia con `#` dopo al massimo 3 spazi;
/// un titolo setext ha l'ultima riga fatta solo di `=` o di `-`.
fn maybe_heading(source: &str) -> bool {
    let lead = source.len() - source.trim_start_matches(' ').len();
    if lead <= 3 && source[lead..].starts_with('#') {
        return true;
    }
    // Dal fondo, senza scorrere il blocco: un paragrafo lungo non costa niente.
    let bytes = source.trim_end().as_bytes();
    let Some(&mark @ (b'=' | b'-')) = bytes.last() else { return false };
    let line = bytes.iter().rev().take_while(|&&c| c != b'\n').count();
    line < bytes.len() && bytes[bytes.len() - line..].iter().all(|&c| c == mark || c == b' ' || c == b'\t')
}

/// Livello e testo semplice del blocco, se il blocco è un titolo.
fn heading(source: &str) -> Option<(u8, String)> {
    plain_atx(source).or_else(|| parsed(source))
}

/// Il caso comune senza parser: un titolo ATX su una riga, senza segni inline (`*`, `` ` ``,
/// link, entità, escape). Il resto passa da pulldown-cmark, che toglie i segni. Con questo e il
/// filtro setext dal fondo, 16 MB di Markdown (6960 titoli) si scandiscono in ~1,8 ms invece di
/// ~4,7 (`outline_cost`).
fn plain_atx(source: &str) -> Option<(u8, String)> {
    let line = source.trim_start_matches(' ');
    let hashes = line.bytes().take_while(|&c| c == b'#').count();
    let rest = &line[hashes..];
    let simple = |c: char| !matches!(c, '*' | '_' | '`' | '[' | ']' | '<' | '>' | '&' | '\\' | '~' | '!' | '\n' | '\r' | '|');
    if !(1..=6).contains(&hashes) || !(rest.is_empty() || rest.starts_with([' ', '\t'])) || !rest.chars().all(simple) {
        return None;
    }
    // La sequenza di chiusura (`## Titolo ##`) va tolta solo se preceduta da uno spazio.
    let rest = rest.trim();
    let closed = rest.trim_end_matches('#');
    let text = if closed.is_empty() || closed.ends_with([' ', '\t']) { closed.trim_end() } else { rest };
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some((hashes as u8, text))
}

fn parsed(source: &str) -> Option<(u8, String)> {
    let mut events = Parser::new_ext(source, Options::ENABLE_STRIKETHROUGH);
    let Some(Event::Start(Tag::Heading { level, .. })) = events.next() else { return None };
    let mut out = String::new();
    for ev in events {
        match ev {
            Event::End(TagEnd::Heading(_)) => break,
            Event::Text(t) | Event::Code(t) => out.push_str(&t),
            Event::SoftBreak | Event::HardBreak => out.push(' '),
            _ => {}
        }
    }
    let level = match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    };
    let text = out.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some((level, text))
}

/// L'ultimo titolo che sta sopra o al blocco `start` (il primo visibile): la sezione che si sta
/// leggendo. `None` sopra il primo titolo.
pub fn active(items: &[Heading], start: usize) -> Option<usize> {
    items.partition_point(|h| h.index <= start).checked_sub(1)
}

/// I titoli di un documento, calcolati alla prima richiesta e di nuovo solo quando il documento
/// cambia (`doc_view::rows_version`: altro documento, ricarica, altro contenuto).
#[derive(Default)]
pub struct Cache {
    cell: RefCell<Option<(Version, Rc<[Heading]>)>>,
}

/// `doc_view::rows_version` del documento da cui vengono i titoli.
type Version = (u64, u64, u64);

impl Cache {
    /// Se i titoli sono già stati calcolati (i test: con l'indice nascosto non si calcolano).
    #[cfg(test)]
    pub fn scanned(&self) -> bool {
        self.cell.borrow().is_some()
    }

    pub fn get(&self, doc: &Document) -> Rc<[Heading]> {
        let version = super::doc_view::rows_version(doc);
        let mut cell = self.cell.borrow_mut();
        match &*cell {
            Some((v, items)) if *v == version => items.clone(),
            _ => {
                let items: Rc<[Heading]> = scan(doc).into();
                *cell = Some((version, items.clone()));
                items
            }
        }
    }
}

/// La chiave della riga del titolo `block` nella scheda `doc` (le chiavi di Rune sono globali:
/// l'id del documento le tiene separate fra le schede).
pub fn row_key(doc: u64, block: u64) -> String {
    format!("o-{doc:x}-{block:x}")
}

/// La colonna "In questo file" della scheda `tab`, `None` se il file non ha titoli H1–H3. Linea
/// di 0,5 pt in `line` a sinistra, larga `OUTLINE_W`; l'etichetta in UI piccola `fg3` allineata
/// alla prima riga del documento; i titoli rientrano di `OUTLINE_INDENT` per livello sotto il più
/// alto del file. La lista scorre da sola se è più alta della finestra.
pub fn view(tab: &Tab, hooks: Hooks<App>, theme: &Theme, reduced: bool, hover: bool) -> Option<El> {
    let items = tab.outline.get(&tab.doc);
    if items.is_empty() {
        return None;
    }
    let motion = Motion { reduced, hover };
    let doc = tab.doc.id;
    let on = tab.active_heading();
    let top = items.iter().map(|h| h.level).min().unwrap_or(1);
    let theme = *theme;
    let keys = items.clone();
    let row_keys = items.clone();
    let rows = col()
        .key(format!("outline-rows-{doc:x}"))
        .w_full()
        .growth(GrowthSpec::default())
        .role("list")
        .virtual_rows(
            items.len(),
            OUTLINE_ROW,
            move |_: &App, i| Cow::Owned(row_keys.get(i).map_or_else(|| format!("o-{doc:x}-i{i:x}"), |h| row_key(doc, h.id))),
            move |s: &App, i| match items.get(i) {
                Some(h) => heading_row(s, doc, h, top, on == Some(i), hooks, &theme, motion),
                None => el(),
            },
        )
        .virtual_rows_version(super::doc_view::rows_version(&tab.doc));
    let label = text(LABEL).key(format!("outline-label-{doc:x}")).font(sidebar::small_font()).color(theme.fg3).px(OUTLINE_ROW_PAD).pb(OUTLINE_LABEL_GAP);
    let list = col().key(format!("outline-list-{doc:x}")).w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(rows);
    let column = col()
        .w(OUTLINE_W - HAIRLINE)
        .h_full()
        .px(OUTLINE_PAD_X)
        .pt(super::doc_view::TOP + super::theme::ROW_PAD_Y)
        .pb(super::doc_view::TOP)
        .role("navigation")
        .label(LABEL)
        .child(label)
        .child(list);
    let mut outline =
        row().key(format!("outline-{doc:x}")).h_full().child(el().key(format!("outline-line-{doc:x}")).w(HAIRLINE).h_full().fill(theme.line)).child(column);
    // In un indice più alto della finestra l'evidenziato non deve uscire dalla vista. Le righe
    // virtuali di Rune non hanno `reveal_selected` (vale solo per `virtualize` con i figli): un
    // segnaposto vuoto con la chiave del titolo evidenziato nasce a ogni cambio, e il suo primo
    // layout porta quel titolo nella vista dell'indice (`scroll_into_view_in` monta anche una
    // riga virtuale non costruita).
    if let Some(h) = on.and_then(|i| keys.get(i)) {
        let target = row_key(doc, h.id);
        let scope = format!("outline-list-{doc:x}");
        let marker = el().key(format!("outline-reveal-{doc:x}-{:x}", h.id)).pin(Some(0.0), Some(0.0), None, None).w(0.0).h(0.0).a11y_hidden();
        outline = outline.child(marker.on_layout_tx(move |_: &mut App, _, tx: &mut Tx| tx.scroll_into_view_in(scope.clone(), target.clone())));
    }
    Some(outline)
}

/// Una riga dell'indice: alta `OUTLINE_ROW`, testo 12,5 pt troncato; hover `sel` e scala 0,98
/// premuta come le righe della sidebar.
#[allow(clippy::too_many_arguments)]
fn heading_row(s: &App, doc: u64, h: &Heading, top: u8, on: bool, hooks: Hooks<App>, theme: &Theme, motion: Motion) -> El {
    let key = row_key(doc, h.id);
    let font = TextStyle::new(OUTLINE_TEXT, if on { UI_STRONG_WEIGHT } else { UI_WEIGHT });
    let label = text(h.text.clone()).font(font).color(if on { theme.fg } else { theme.fg2 }).truncate().shrink().min_w(0.0).a11y_hidden();
    let block = h.id;
    let indent = f32::from(h.level.saturating_sub(top)) * OUTLINE_INDENT;
    let e = row()
        .w_full()
        .h(OUTLINE_ROW)
        .pl(OUTLINE_ROW_PAD + indent)
        .pr(OUTLINE_ROW_PAD)
        .center_y()
        .role("link")
        .label(h.text.clone())
        .focus_selected(on)
        .child(label)
        .on_press_tx(move |s: &mut App, tx: &mut Tx| s.go_to_heading(doc, block, tx));
    sidebar::pressable(e, key.clone(), (hooks.get)(s), hooks, theme, motion, true).key(key)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn doc(src: &str) -> Document {
        Document::from_source(Path::new("t.md"), src)
    }

    fn levels_and_text(src: &str) -> Vec<(u8, String)> {
        scan(&doc(src)).into_iter().map(|h| (h.level, h.text)).collect()
    }

    #[test]
    fn h1_to_h3_in_document_order_without_h4() {
        let src = "# Uno\n\ntesto\n\n## Due\n\n### Tre\n\n#### Quattro\n\n## Cinque\n";
        let d = doc(src);
        let got = scan(&d);
        assert_eq!(got.iter().map(|h| (h.level, h.text.as_str())).collect::<Vec<_>>(), [(1, "Uno"), (2, "Due"), (3, "Tre"), (2, "Cinque")]);
        let ids: Vec<u64> = [0, 2, 3, 5].iter().map(|&i| d.blocks[i].id).collect();
        assert_eq!(got.iter().map(|h| h.id).collect::<Vec<_>>(), ids, "id dei blocchi dei titoli");
    }

    #[test]
    fn a_hash_inside_a_fence_is_not_a_heading() {
        let src = "# Vero\n\n```sh\n# commento\n## altro\n```\n\n~~~\n# anche\n~~~\n";
        assert_eq!(levels_and_text(src), [(1, "Vero".to_owned())]);
    }

    #[test]
    fn frontmatter_is_not_a_heading() {
        let src = "---\ntitle: Il mio file\ndescription: Una riga\n---\n\n# Titolo vero\n";
        assert_eq!(levels_and_text(src), [(1, "Titolo vero".to_owned())]);
    }

    #[test]
    fn setext_inline_code_and_emphasis_give_plain_text() {
        let src = "Titolo setext\n===\n\nSotto\n---\n\n## Con `codice` e *enfasi*\n\n#hashtag non è un titolo\n";
        assert_eq!(levels_and_text(src), [(1, "Titolo setext".to_owned()), (2, "Sotto".to_owned()), (2, "Con codice e enfasi".to_owned())]);
    }

    /// Il caso veloce dà lo stesso testo del parser.
    #[test]
    fn the_fast_path_agrees_with_the_parser() {
        for src in [
            "# Uno",
            "## Due ##",
            "### Tre #",
            "  # Rientro",
            "## C# e F#",
            "##   spazi   in   mezzo  ",
            "#\tTab",
            "## Titolo#",
            "####### sette",
            "#vuoto",
            "#",
            "## #",
        ] {
            assert_eq!(plain_atx(src).or_else(|| parsed(src)), parsed(src), "{src:?}");
        }
    }

    #[test]
    fn a_rule_or_a_list_is_not_a_heading() {
        assert!(levels_and_text("---\n\n- a\n- b\n\n| a |\n| - |\n| 1 |\n").is_empty());
    }

    #[test]
    fn the_active_heading_is_the_last_one_above_the_first_visible_block() {
        let d = doc("intro\n\n# A\n\nuno\n\ndue\n\n## B\n\ntre\n");
        let items = scan(&d);
        assert_eq!(active(&items, 0), None, "sopra il primo titolo");
        assert_eq!(active(&items, 1), Some(0));
        assert_eq!(active(&items, 3), Some(0));
        assert_eq!(active(&items, 4), Some(1));
        assert_eq!(active(&items, 99), Some(1));
    }

    #[test]
    fn the_cache_scans_again_only_when_the_document_changes() {
        let mut d = doc("# A\n\ntesto\n");
        let cache = Cache::default();
        let first = cache.get(&d);
        assert!(Rc::ptr_eq(&first, &cache.get(&d)), "stessa revisione: niente nuova scansione");
        d.update("# A\n\ntesto\n\n## B\n");
        let second = cache.get(&d);
        assert_eq!(second.len(), 2, "dopo la ricarica");
        assert_eq!(second[0].id, first[0].id, "id stabile");
    }

    // --- la colonna nell'app, su un runtime Rune headless ---

    use rune::{Env, Key, KeyEvent, Mods, Runtime};
    use rune_kit::MarkdownStyle;

    const WIDE: (f32, f32) = (1280.0, 800.0);

    fn paragraph(n: usize) -> String {
        let words = ["riga", "lettore", "blocco", "scroll", "ancora", "frame", "testo", "colonna"];
        (0..30 + (n * 7) % 40).map(|k| words[(n + k) % words.len()]).collect::<Vec<_>>().join(" ")
    }

    /// 12 sezioni H2 con 6 paragrafi ciascuna, un H3 nelle sezioni pari, sotto un H1.
    fn sample() -> String {
        let mut out = vec!["# Documento".to_owned()];
        for s in 0..12 {
            out.push(format!("## Sezione {s}"));
            for p in 0..6 {
                if p == 3 && s % 2 == 0 {
                    out.push(format!("### Parte {s}"));
                }
                out.push(paragraph(s * 6 + p));
            }
        }
        out.join("\n\n") + "\n"
    }

    fn settle(rt: &mut Runtime<App>) {
        for _ in 0..4000 {
            if !rt.frame(1.0 / 120.0) {
                return;
            }
        }
        panic!("la vista non si ferma");
    }

    fn open_at(src: &str, viewport: (f32, f32)) -> Runtime<App> {
        let mut state = App::bare(crate::index::Library::load(Vec::new(), None));
        state.set_style(MarkdownStyle::neutral(true));
        state.push_tab(Document::from_source(Path::new("t.md"), src));
        let mut rt = Runtime::new(state, crate::view, viewport, Env::new(true, 2.0));
        rt.on_key = Some(crate::on_key);
        rt.frame(0.0);
        settle(&mut rt);
        rt
    }

    /// La chiave di un elemento dell'indice della scheda davanti: `outline`, `outline-line`...
    fn ok(rt: &Runtime<App>, name: &str) -> String {
        format!("{name}-{:x}", rt.state.tab().unwrap().doc.id)
    }

    fn keyed(rt: &Runtime<App>, key: &str) -> Option<[f32; 4]> {
        rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).map(|e| e.rect)
    }

    /// Il testo dei titoli disegnati in `fg` (l'evidenziato).
    fn highlighted(rt: &Runtime<App>) -> Vec<String> {
        let fg = Theme::for_mode(true).fg;
        rt.elements().into_iter().filter(|e| !e.exiting && e.path.contains("outline-rows-") && e.text_color == Some(fg)).filter_map(|e| e.text).collect()
    }

    fn click(rt: &mut Runtime<App>, key: &str) {
        let r = keyed(rt, key).unwrap_or_else(|| panic!("riga {key}"));
        let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.frame(1.0 / 60.0);
        rt.pointer_up(x, y);
        settle(rt);
    }

    fn heading_key(rt: &Runtime<App>, text: &str) -> (String, u64) {
        let tab = rt.state.tab().unwrap();
        let h = tab.outline.get(&tab.doc).iter().find(|h| h.text == text).cloned().unwrap();
        (row_key(tab.doc.id, h.id), h.id)
    }

    /// Con la finestra larga la colonna sta a destra del documento, larga `OUTLINE_W`, con la
    /// linea a sinistra; la colonna del documento resta larga `COLUMN`.
    #[test]
    fn a_wide_window_shows_the_outline_beside_the_document() {
        let rt = open_at(&sample(), WIDE);
        let outline = keyed(&rt, &ok(&rt, "outline")).expect("indice");
        assert_eq!(outline[2], OUTLINE_W);
        let line = keyed(&rt, &ok(&rt, "outline-line")).unwrap();
        assert_eq!((line[0], line[2]), (outline[0], HAIRLINE));
        let doc = keyed(&rt, "doc").unwrap();
        assert!((doc[0] + doc[2] - outline[0]).abs() < 0.5, "a destra del documento: {doc:?} {outline:?}");
        assert_eq!(keyed(&rt, "rows").unwrap()[2], super::super::theme::COLUMN);
        assert!(rt.elements().iter().any(|e| e.text.as_deref() == Some(LABEL)));
        assert_eq!(highlighted(&rt), ["Documento"], "in cima: il titolo del file");
    }

    #[test]
    fn a_window_narrower_than_sidebar_column_and_200_hides_it() {
        let w = super::super::theme::OUTLINE_MIN_WINDOW;
        assert_eq!(w, 248.0 + 680.0 + 200.0);
        let rt = open_at(&sample(), (w - 1.0, 800.0));
        assert!(keyed(&rt, &ok(&rt, "outline")).is_none());
        let rt = open_at(&sample(), (w, 800.0));
        assert!(keyed(&rt, &ok(&rt, "outline")).is_some());
    }

    #[test]
    fn a_file_without_headings_has_no_outline() {
        let rt = open_at("solo testo\n\naltro testo\n", WIDE);
        assert!(keyed(&rt, &ok(&rt, "outline")).is_none());
    }

    /// Un clic su un titolo lontano (riga non montata) lo porta in cima alla vista e lo evidenzia.
    #[test]
    fn a_click_scrolls_the_heading_to_the_top_and_highlights_it() {
        let mut rt = open_at(&sample(), WIDE);
        let (key, block) = heading_key(&rt, "Sezione 9");
        click(&mut rt, &key);
        let tab = rt.state.tab().unwrap();
        assert_eq!(tab.visible_first, block, "il titolo è il primo blocco visibile");
        let i = tab.doc.blocks.iter().position(|b| b.id == block).unwrap();
        let row = keyed(&rt, &crate::ui::doc_view::row_key(i, &tab.doc)).unwrap();
        let top = keyed(&rt, "doc").unwrap()[1];
        assert!((row[1] - top).abs() <= 1.0, "in cima: {row:?} vista {top}");
        assert_eq!(highlighted(&rt), ["Sezione 9"]);
    }

    /// Lo scroll del documento sposta l'evidenziato sulla sezione in cima, anche dentro un H3.
    #[test]
    fn scrolling_moves_the_highlight() {
        let mut rt = open_at(&sample(), WIDE);
        let mut seen = Vec::new();
        for step in 1..60 {
            rt.update(|_, tx| tx.scroll_to_instant("doc", 0.0, step as f32 * 250.0));
            settle(&mut rt);
            let h = highlighted(&rt);
            assert_eq!(h.len(), 1, "un solo titolo evidenziato: {h:?}");
            let tab = rt.state.tab().unwrap();
            let items = tab.outline.get(&tab.doc);
            // In fondo vale l'ultimo titolo che si vede, altrimenti la sezione in cima.
            let shown = if tab.at_bottom { tab.visible_last } else { tab.visible_first };
            let i = tab.doc.blocks.iter().position(|b| b.id == shown).unwrap();
            let want = &items[active(&items, i).unwrap()].text;
            assert_eq!(&h[0], want, "allo scroll {step}");
            if seen.last() != Some(want) {
                seen.push(want.clone());
            }
        }
        assert!(seen.iter().any(|t| t.starts_with("Parte")), "anche gli H3: {seen:?}");
        assert!(seen.len() > 10, "{seen:?}");
    }

    /// Una ricarica che aggiunge un titolo sopra la vista: l'evidenziato resta sulla sezione che
    /// si legge (segue l'id del blocco) e il titolo nuovo entra nell'indice.
    #[test]
    fn a_live_reload_keeps_the_highlight_and_adds_new_headings() {
        let mut rt = open_at(&sample(), WIDE);
        let (key, _) = heading_key(&rt, "Sezione 6");
        click(&mut rt, &key);
        let src = format!("## Nuova in testa\n\n{}", sample());
        rt.update(|s, _| {
            let change = s.tab_mut().unwrap().doc.update(&src);
            s.tab_mut().unwrap().note_reload(&change, 1.0);
        });
        settle(&mut rt);
        assert_eq!(highlighted(&rt), ["Sezione 6"]);
        let tab = rt.state.tab().unwrap();
        assert_eq!(tab.outline.get(&tab.doc)[0].text, "Nuova in testa", "il titolo nuovo, in testa");
    }

    /// Nel sorgente (Cmd+E) l'indice non c'è: i suoi clic porterebbero a blocchi che il sorgente
    /// non ha, e il testo cambia mentre si scrive.
    #[test]
    fn the_source_view_hides_the_outline() {
        let mut rt = open_at(&sample(), WIDE);
        rt.key(KeyEvent { key: Key::Command('e'), mods: Mods { cmd: true, logo: true, ..Mods::default() } });
        settle(&mut rt);
        assert!(matches!(rt.state.tab().unwrap().mode, crate::app::Mode::Source(_)));
        assert!(keyed(&rt, &ok(&rt, "outline")).is_none());
    }

    /// I rientri: H1 a sinistra, H2 rientrato di `OUTLINE_INDENT`, H3 del doppio.
    #[test]
    fn levels_indent_by_ten() {
        let rt = open_at(&sample(), WIDE);
        let x = |t: &str| rt.elements().into_iter().find(|e| e.path.contains("outline-rows-") && e.text.as_deref() == Some(t)).unwrap().rect[0];
        assert_eq!(x("Sezione 0") - x("Documento"), OUTLINE_INDENT);
        assert_eq!(x("Parte 0") - x("Documento"), 2.0 * OUTLINE_INDENT);
    }

    /// Un indice più alto della finestra: scorrendo il documento fino in fondo, il titolo
    /// evidenziato entra nella parte visibile dell'indice.
    #[test]
    fn the_highlight_stays_in_view_in_a_long_outline() {
        let src: String = (0..80).map(|n| format!("## Titolo {n}\n\n{}\n\n", paragraph(n))).collect();
        let mut rt = open_at(&src, WIDE);
        rt.update(|_, tx| tx.scroll_to_instant("doc", 0.0, 1.0e9));
        settle(&mut rt);
        let fg = Theme::for_mode(true).fg;
        let els = rt.elements();
        let on = els.iter().find(|e| !e.exiting && e.path.contains("outline-rows-") && e.text_color == Some(fg)).expect("evidenziato montato");
        let doc = rt.state.tab().unwrap().doc.id;
        let list = keyed(&rt, &format!("outline-list-{doc:x}")).unwrap();
        assert!(on.rect[1] >= list[1] - 0.5 && on.rect[1] + on.rect[3] <= list[1] + list[3] + 0.5, "{:?} fuori da {list:?}", on.rect);
    }

    /// Il costo della scansione dei titoli, che si paga all'apertura e a ogni ricarica (una volta,
    /// poi la cache): `fixtures/big.md` (1 MB) e lo stesso ripetuto fino a 16 MB.
    /// `cargo test --profile perf outline_cost -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn outline_cost() {
        let big = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/big.md")).unwrap();
        for (name, src) in [("big 1 MB", big.clone()), ("big ×16", big.repeat(16))] {
            let d = doc(&src);
            let mut ms: Vec<f64> = (0..20)
                .map(|_| {
                    let t = std::time::Instant::now();
                    std::hint::black_box(scan(&d));
                    t.elapsed().as_secs_f64() * 1000.0
                })
                .collect();
            ms.sort_by(f64::total_cmp);
            eprintln!("{name}: {} byte, {} blocchi, {} titoli, scan p50 {:.2} ms, max {:.2} ms", src.len(), d.blocks.len(), scan(&d).len(), ms[10], ms[19]);
        }
    }

    /// Il primo blocco visibile (id) e il suo scarto dal bordo alto della vista del documento.
    fn anchor(rt: &Runtime<App>) -> (u64, f32) {
        let tab = rt.state.tab().unwrap();
        let i = tab.doc.blocks.iter().position(|b| b.id == tab.visible_first).unwrap();
        let row = keyed(rt, &crate::ui::doc_view::row_key(i, &tab.doc)).unwrap();
        (tab.visible_first, row[1] - keyed(rt, "doc").unwrap()[1])
    }

    fn resize(rt: &mut Runtime<App>, w: f32) {
        rt.set_viewport((w, WIDE.1), 2.0);
        settle(rt);
    }

    /// La finestra passa sotto e sopra 1128 pt: l'indice sparisce e torna, il documento resta
    /// dov'era (stesso primo blocco, stesso scarto) e così lo stato della sua vista.
    #[test]
    fn resizing_across_the_threshold_keeps_the_document_in_place() {
        let mut rt = open_at(&sample(), WIDE);
        rt.update(|_, tx| tx.scroll_to_instant("doc", 0.0, 3000.0));
        settle(&mut rt);
        let before = anchor(&rt);
        assert!(before.1 < -1.0, "il bordo taglia il primo blocco: {before:?}");
        let min = super::super::theme::OUTLINE_MIN_WINDOW;
        for (w, shown) in [(min - 30.0, false), (min + 150.0, true), (min - 1.0, false), (WIDE.0, true)] {
            resize(&mut rt, w);
            assert_eq!(keyed(&rt, &ok(&rt, "outline")).is_some(), shown, "indice a {w}");
            let now = anchor(&rt);
            assert_eq!(now.0, before.0, "a {w}: primo blocco");
            assert!((now.1 - before.1).abs() <= 1.0, "a {w}: scarto {} → {}", before.1, now.1);
        }
    }

    /// Un file che finisce con sezioni corte: in fondo lo scroll non può portarle in cima.
    fn short_tail() -> String {
        format!("{}\n\n## Penultima\n\nbreve\n\n## Ultima\n\nbreve\n", sample().trim_end())
    }

    /// Con lo scroll in fondo è evidenziato l'ultimo titolo che si vede.
    #[test]
    fn at_the_bottom_the_last_visible_heading_is_highlighted() {
        let mut rt = open_at(&short_tail(), WIDE);
        rt.update(|_, tx| tx.scroll_to_instant("doc", 0.0, 1.0e9));
        settle(&mut rt);
        assert_eq!(highlighted(&rt), ["Ultima"]);
        // Più su (non più in fondo) torna la regola della sezione in cima.
        rt.update(|_, tx| tx.scroll_to_instant("doc", 0.0, 3000.0));
        settle(&mut rt);
        assert_ne!(highlighted(&rt), ["Ultima"]);
    }

    /// Un file che sta tutto nella finestra non è "in fondo": resta evidenziato il primo titolo.
    #[test]
    fn a_file_that_fits_highlights_its_first_heading() {
        let rt = open_at("# Uno\n\nbreve\n\n## Due\n\nbreve\n", WIDE);
        assert_eq!(highlighted(&rt), ["Uno"]);
    }

    /// Un clic su un titolo che non può arrivare in cima: è evidenziato quello cliccato, finché
    /// non si scorre a mano.
    #[test]
    fn a_clicked_heading_is_highlighted_even_if_it_cannot_reach_the_top() {
        let mut rt = open_at(&short_tail(), WIDE);
        let (key, _) = heading_key(&rt, "Penultima");
        // la riga dell'indice va in vista prima del clic
        let doc = rt.state.tab().unwrap().doc.id;
        rt.update(|_, tx| tx.scroll_into_view_in(format!("outline-list-{doc:x}"), key.clone()));
        settle(&mut rt);
        click(&mut rt, &key);
        assert_eq!(highlighted(&rt), ["Penultima"]);
        rt.update(|_, tx| tx.scroll_to_instant("doc", 0.0, 2000.0));
        settle(&mut rt);
        assert_ne!(highlighted(&rt), ["Penultima"], "uno scroll a mano torna alla regola");
    }

    /// Con l'indice nascosto (finestra stretta) lo scroll non calcola i titoli.
    #[test]
    fn a_hidden_outline_costs_nothing_while_scrolling() {
        let mut rt = open_at(&short_tail(), (1000.0, 800.0));
        for step in 1..20 {
            rt.update(|_, tx| tx.scroll_to_instant("doc", 0.0, step as f32 * 400.0));
            settle(&mut rt);
        }
        rt.update(|_, tx| tx.scroll_to_instant("doc", 0.0, 1.0e9));
        settle(&mut rt);
        assert!(!rt.state.tab().unwrap().outline.scanned(), "titoli calcolati con l'indice nascosto");
        resize(&mut rt, WIDE.0);
        assert_eq!(highlighted(&rt), ["Ultima"], "e quando l'indice compare è già giusto");
    }
}
