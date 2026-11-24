//! The JSON viewer (decision 2026-11-23-json-viewer, mockup `codice`): the file pretty-printed in
//! one code card with the json colours, line numbers, and a disclosure arrow on every `{` and `[`
//! line. A folded node shows `{…}` / `[…]`, its count of keys or items and a short preview; Alt
//! folds or unfolds the whole subtree; the header has Expand all, Collapse all and Copy. A JSONL
//! file is one foldable row per line. The lines are virtual rows: a long file mounts only what
//! shows.

use std::borrow::Cow;
use std::sync::Arc;

use rune::{Align, Color, El, ScrollSpec, TextStyle, col, el, row};
use rune_kit::MarkdownStyle;
use rune_kit::syntax::{Syntax, SyntaxTheme};

use crate::app::App;
use crate::doc::json::{self, Shown, Tree};

use super::theme::{ROW_PAD_X, Theme};

/// Mockup `codice` (`.cb-b`, `.ln`, `.no`, `.tw`): 21 pt lines inset 4 with radius 4, an 8 pt
/// right pad, 36 pt line numbers in 12 pt, a 22 pt arrow column with a 10 pt chevron, 10 under.
const LINE_H: f32 = 21.0;
const LINE_INSET: f32 = 4.0;
const LINE_PAD_R: f32 = 8.0;
const LINE_RADIUS: f32 = 4.0;
const NUMBER_W: f32 = 36.0;
const NUMBER_TEXT: f32 = 12.0;
const ARROW_W: f32 = 22.0;
const ARROW_ICON: f32 = 10.0;
const BODY_BOTTOM: f32 = 10.0;
/// The `…` pill of a folded node (`.pill`): 7 at the sides, radius 4, 3 apart from the brackets.
const PILL_PAD: f32 = 7.0;
const PILL_GAP: f32 = 3.0;
/// The count and preview after a node (`.ann`, `.ct`, `.pv`): 12 after the text, 10 apart.
const ANN_GAP: f32 = 12.0;
const ANN_TEXT: f32 = 12.0;
const ANN_WEIGHT: f32 = 500.0;
const PREVIEW_GAP: f32 = 10.0;
/// The header (the app's code header, `doc_view`): 32 high, 12 left, 4 right, 6 between.
const HEAD_H: f32 = 32.0;
const HEAD_PAD_L: f32 = 12.0;
const HEAD_PAD_R: f32 = 4.0;
const HEAD_GAP: f32 = 6.0;
const HEAD_ICON: f32 = 13.0;
/// Header text buttons (`.ghost`): 24 high, 7 at the sides, radius 6.
const GHOST_H: f32 = 24.0;
const GHOST_PAD: f32 = 7.0;
const GHOST_RADIUS: f32 = 6.0;
/// The card (the table box's quiet edge, record 2026-11-23-t3-paths-review): 1 pt `line`, radius 6.
const CARD_EDGE: f32 = 1.0;
const CARD_RADIUS: f32 = 6.0;

/// The colours and font of the card.
#[derive(Clone)]
struct Look {
    font: TextStyle,
    ink: Color,
    syntax: SyntaxTheme,
    fg: Color,
    fg2: Color,
    fg3: Color,
    sel: Color,
    line: Color,
    /// Two characters of the code font: one indent level.
    indent: f32,
}

/// The front tab's JSON file as the viewer draws it. `bottom` is the room under the page (the fire).
pub fn view(s: &App, style: &MarkdownStyle, theme: &Theme, column: f32, bottom: f32) -> El {
    let tab = s.tab().expect("json view without a tab");
    let doc = &tab.doc;
    let tree = doc.json.clone().expect("json view without a tree");
    let (fill, font, ink, syntax) = style.code_look();
    let look = Look { font, ink, syntax, fg: theme.fg, fg2: theme.fg2, fg3: theme.fg3, sel: theme.sel, line: theme.line, indent: rune::text::advance("  ", font) };
    let rows_now = tab.json_rows();
    let id = doc.id;
    let extra = if tree.jsonl { records(tree.roots.len()) } else { super::file_view::lines(tree.lines.len()) };
    let head = super::file_view::head(&doc.path, Some(extra), theme.fg, theme.fg3);
    let body = {
        let look = look.clone();
        col().key("json-rows").w_full().pb(BODY_BOTTOM).selection_group(true).growth(rune::GrowthSpec::default())
            .virtual_rows(
                rows_now.len(),
                LINE_H,
                move |s: &App, i| Cow::Owned(match s.tab().and_then(|t| t.json_rows().get(i).copied()) {
                    Some(r) => format!("j{}", r.line),
                    None => format!("j-{i}"),
                }),
                move |s: &App, i| {
                    let Some(tab) = s.tab().filter(|t| t.doc.id == id) else { return el() };
                    let (Some(tree), Some(r)) = (tab.doc.json.as_ref(), tab.json_rows().get(i).copied()) else { return el() };
                    line_row(tree, r, &look, id)
                },
            )
            .virtual_rows_version((id, doc.revision, tab.folds_generation(), theme.dark))
    };
    let card = col().key("json-card").w_full().fill(fill).border(CARD_EDGE, theme.line).border_layout().radius(CARD_RADIUS).child(header(&tree, &doc.path, id, &look)).child(body);
    let page = super::theme::column(col().w_full(), column).self_center().px(ROW_PAD_X).child(head).child(card);
    let scroll = col().key("doc").w_full().h_full().scroll(ScrollSpec { stick_bottom: false }).child(col().w_full().pt(super::doc_view::TOP).pb(bottom.max(BODY_BOTTOM)).child(page));
    el().key(format!("json-{id:x}")).w_full().grow(1.0).min_h(0.0).child(scroll)
}

/// "1 record", "25 records".
fn records(n: usize) -> String {
    if n == 1 { "1 record".to_owned() } else { format!("{n} records") }
}

/// The card's header: icon and `json`/`jsonl`, then Expand all, Collapse all and Copy.
fn header(tree: &Tree, path: &std::path::Path, id: u64, look: &Look) -> El {
    let label = if tree.jsonl { "jsonl" } else { "json" };
    let icon = rune::icon(&super::path_icons::JSON, HEAD_ICON).color(look.fg3);
    let name = rune::text(label).font(TextStyle::new(ANN_TEXT, ANN_WEIGHT)).color(look.fg3).selectable(false);
    let ghost = |key: &str, text: &str| {
        rune::row().key(key).h(GHOST_H).px(GHOST_PAD).radius(GHOST_RADIUS).center_y().role("button").label(text)
            .child(rune::text(text.to_owned()).font(TextStyle::new(ANN_TEXT, ANN_WEIGHT)).color(look.fg2).selectable(false)).hover(look.sel)
    };
    let expand = ghost("expand-all", "Expand all").on_press(move |s: &mut App| s.fold_all_json(id, false));
    let collapse = ghost("collapse-all", "Collapse all").on_press(move |s: &mut App| s.fold_all_json(id, true));
    let file = path.to_owned();
    let chrome = super::doc_view::ChromeColors { label: look.fg3, button: look.fg2, hover: look.sel };
    let copy = super::doc_view::head_button("copy", "Copy File", &super::path_icons::COPY, chrome, false).on_press_tx(move |_: &mut App, tx: &mut rune::Tx| {
        if let Ok(text) = std::fs::read_to_string(&file) {
            tx.copy(text);
        }
    });
    let rule = el().w(1.0).h(14.0).mx(2.0).fill(look.line);
    row().key("json-head").w_full().h(HEAD_H).pl(HEAD_PAD_L).pr(HEAD_PAD_R).gap(HEAD_GAP).center_y()
        .child(icon).child(name).child(el().grow(1.0)).child(expand).child(collapse).child(rule).child(copy)
}

/// Line `r.line` of `tree`: number, arrow, indent guides, the highlighted text and, on a node, its
/// count (and, folded, its preview).
fn line_row(tree: &Tree, r: Shown, look: &Look, id: u64) -> El {
    let line = &tree.lines[r.line];
    let number = rune::text(line.number.map(|n| n.to_string()).unwrap_or_default())
        .font(TextStyle { size: NUMBER_TEXT, line_height: LINE_H, ..look.font }).color(look.fg3).align(Align::End).w(NUMBER_W).selectable(false);
    let arrow = match &line.node {
        Some(_) => {
            let chevron = rune::icon(&rune_kit::glyphs::CHEVRON_DOWN, ARROW_ICON).color(look.fg3).icon_rotation(if r.folded { -std::f32::consts::FRAC_PI_2 } else { 0.0 });
            let label = if r.folded { "Unfold" } else { "Fold" };
            let at = r.line;
            el().key("arrow").w(ARROW_W).h(LINE_H).center().radius(LINE_RADIUS).role("button").label(label).help(format!("{label} (⌥-click: with everything inside)"))
                .child(chevron).hover(look.sel)
                .on_press_tx(move |s: &mut App, tx: &mut rune::Tx| s.fold_json(id, at, tx.mods().alt))
        }
        None => el().w(ARROW_W).h(LINE_H),
    };
    let guides = row().h(LINE_H).children((0..line.depth).map(|_| el().w(look.indent).h(LINE_H).child(el().ml(look.indent / 4.0).w(1.0).h(LINE_H).fill(look.line))));
    // Each piece is coloured from the line's own tokens; `key` holds only in the line's first piece.
    let code = |t: &str, key: usize| -> El {
        let spans = json::tokens(t, key).into_iter().map(|(r, k)| {
            let mut sp = rune::Span::new(r.start, r.end);
            sp.color = Some(look.syntax.color(match k {
                json::Token::Key => Syntax::Property,
                json::Token::String => Syntax::String,
                json::Token::Number => Syntax::Number,
                json::Token::Word => Syntax::Keyword,
                json::Token::Punctuation => Syntax::Punctuation,
            }));
            sp
        }).collect();
        rune::rich(t.to_owned(), spans).font(TextStyle { line_height: LINE_H, ..look.font }).color(look.ink).selectable(true)
    };
    let ann = |node: &json::Node, folded: bool| {
        let count = rune::text(node.count_label()).font(TextStyle::new(ANN_TEXT, ANN_WEIGHT)).color(look.fg3).selectable(false);
        let preview = node.preview.clone().filter(|_| folded).map(|p| rune::text(p).font(TextStyle { line_height: LINE_H, ..look.font }).color(look.fg2).selectable(false).min_w(0.0));
        row().ml(ANN_GAP).gap(PREVIEW_GAP).center_y().min_w(0.0).children(preview).child(count)
    };
    let body = match (&line.node, r.folded) {
        (Some(node), true) => {
            let (open, close) = node.brackets();
            let comma = if tree.lines[node.close].text.ends_with(',') { "," } else { "" };
            let at = r.line;
            let pill = el().key("pill").px(PILL_PAD).mx(PILL_GAP).radius(LINE_RADIUS).fill(look.sel).border(0.5, look.line).role("button").label("Unfold")
                .child(rune::text("…").font(TextStyle { weight: 600.0, line_height: 17.0, ..look.font }).color(look.fg).selectable(false))
                .hover(look.sel)
                .on_press_tx(move |s: &mut App, tx: &mut rune::Tx| s.fold_json(id, at, tx.mods().alt));
            row().center_y().min_w(0.0)
                .child(code(&format!("{}{open}", &line.text[..node.prefix]), line.key))
                .child(pill)
                .child(code(&format!("{close}{comma}"), 0))
                .child(ann(node, true))
        }
        (Some(node), false) => row().center_y().min_w(0.0).child(code(&line.text, line.key)).child(ann(node, false)),
        // A long value wraps under itself.
        (None, _) => row().center_y().min_w(0.0).child(code(&line.text, line.key).wrap().grow(1.0).min_w(0.0)),
    };
    row().w_full().min_h(LINE_H).mx(LINE_INSET).pr(LINE_PAD_R).radius(LINE_RADIUS).center_y()
        .child(number).child(arrow).child(guides).child(body.grow(1.0).min_w(0.0))
}

/// The rows a tab's JSON leaves visible, cached on the document's revision and the folds.
pub fn rows(tree: &Tree, folds: &json::Folds) -> Arc<[Shown]> {
    json::visible(tree, folds).into()
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use rune::{Env, Mods, Runtime};

    use crate::app::App;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/open-files").join(name)
    }

    fn open(paths: &[&str], dark: bool) -> Runtime<App> {
        let mut state = App::bare(crate::index::Library::load(Vec::new(), None));
        state.open_paths(paths.iter().map(|p| fixture(p)).collect());
        let mut rt = Runtime::new(state, crate::view, (1200.0, 900.0), Env::new(dark, 2.0));
        rt.frame(0.0);
        settle(&mut rt);
        rt
    }

    fn settle(rt: &mut Runtime<App>) {
        for _ in 0..2000 {
            if !rt.frame(1.0 / 120.0) {
                return;
            }
        }
    }

    fn texts(rt: &Runtime<App>) -> Vec<String> {
        rt.elements().into_iter().filter(|e| !e.exiting).filter_map(|e| e.text).collect()
    }

    /// The line index of node `path` in the front tab's tree.
    fn node(rt: &Runtime<App>, path: &str) -> usize {
        let tree = rt.state.tab().unwrap().doc.json.clone().unwrap();
        tree.lines.iter().position(|l| l.node.as_ref().is_some_and(|n| n.path == path)).unwrap()
    }

    fn click_arrow(rt: &mut Runtime<App>, line: usize, alt: bool) {
        let want = format!("/j{line}/");
        let r = rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some("arrow") && e.path.contains(&want)).unwrap_or_else(|| panic!("no arrow on line {line}")).rect;
        let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.physical_input(rune::PhysicalInput::Modifiers(Mods { alt, ..Mods::default() }));
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        settle(rt);
        rt.physical_input(rune::PhysicalInput::Modifiers(Mods::default()));
        settle(rt);
    }

    #[test]
    fn json_shows_as_a_pretty_printed_card_with_an_arrow_on_every_bracket_line() {
        let rt = open(&["state.json"], false);
        let els: Vec<_> = rt.elements().into_iter().filter(|e| !e.exiting).collect();
        assert!(els.iter().any(|e| e.key.as_deref() == Some("json-card")));
        let t = texts(&rt);
        for want in ["state.json", "118 lines", "json", "Expand all", "Collapse all", "{", "\"version\": 2,", "\"settings\": {", "5 keys", "7 keys", "6 items"] {
            assert!(t.iter().any(|x| x == want), "{want}: {t:?}");
        }
        // Every mounted `{` or `[` line has an arrow, and only those.
        let tree = rt.state.tab().unwrap().doc.json.clone().unwrap();
        let mounted: Vec<usize> = els.iter().filter_map(|e| e.key.as_deref()?.strip_prefix('j')?.parse().ok()).collect();
        assert!(mounted.len() > 20, "{mounted:?}");
        for &i in &mounted {
            let arrow = els.iter().any(|e| e.key.as_deref() == Some("arrow") && e.path.contains(&format!("/j{i}/")));
            assert_eq!(arrow, tree.lines[i].node.is_some(), "line {i}: {}", tree.lines[i].text);
        }
        // The arrows show the hand (feedback F1).
        assert!(els.iter().filter(|e| e.key.as_deref() == Some("arrow")).all(|e| e.hand));
    }

    #[test]
    fn a_click_folds_a_node_to_braces_and_count_and_alt_takes_the_subtree() {
        let mut rt = open(&["state.json"], true);
        let settings = node(&rt, "/settings");
        let all = rt.state.tab().unwrap().json_rows().len();
        click_arrow(&mut rt, settings, false);
        let close = rt.state.tab().unwrap().doc.json.as_ref().unwrap().lines[settings].node.as_ref().unwrap().close;
        assert_eq!(rt.state.tab().unwrap().json_rows().len(), all - (close - settings), "its inner lines and the close go");
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "\"settings\": {") && t.iter().any(|x| x == "…") && t.iter().any(|x| x == "},") && t.iter().any(|x| x == "5 keys"), "{t:?}");
        assert!(!t.iter().any(|x| x == "\"daily_cap\": 200,"), "the inside is hidden");
        click_arrow(&mut rt, settings, false);
        assert_eq!(rt.state.tab().unwrap().json_rows().len(), all);
        // Alt on the root folds everything; a plain click then opens only the root.
        click_arrow(&mut rt, 0, true);
        assert_eq!(rt.state.tab().unwrap().json_rows().len(), 1);
        assert!(texts(&rt).iter().any(|x| x == "7 keys"));
        click_arrow(&mut rt, 0, false);
        assert_eq!(rt.state.tab().unwrap().json_rows().len(), 9, "the root, its 7 keys folded or plain, its close");
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "x.com/following"), "a folded node's preview: {t:?}");
        assert!(t.iter().any(|x| x == "6 items"), "{t:?}");
    }

    #[test]
    fn fold_state_is_per_tab() {
        let mut rt = open(&["state.json", "runs-short.jsonl"], false);
        let tabs = |rt: &Runtime<App>| rt.state.tabs.iter().map(|t| t.json_rows().len()).collect::<Vec<_>>();
        let before = tabs(&rt);
        click_arrow(&mut rt, 0, false);
        let after = tabs(&rt);
        assert!(after[0] == before[0] && after[1] < before[1], "only the front tab folds: {before:?} → {after:?}");
        rt.update(|s, _| { s.select(0); });
        settle(&mut rt);
        assert_eq!(tabs(&rt), after);
    }

    #[test]
    fn jsonl_is_one_row_per_line_folded_when_long() {
        let rt = open(&["runs.jsonl"], false);
        let tab = rt.state.tab().unwrap();
        assert_eq!(tab.json_rows().len(), 25, "one row per line");
        assert!(tab.json_rows().iter().all(|r| r.folded));
        let t = texts(&rt);
        assert!(t.iter().any(|x| x == "25 records") && t.iter().any(|x| x == "jsonl"), "{t:?}");
        assert!(t.iter().filter(|x| *x == "6 keys").count() >= 20, "{t:?}");
        let short = open(&["runs-short.jsonl"], false);
        assert!(short.state.tab().unwrap().json_rows().iter().all(|r| !r.folded), "4 lines open unfolded");
    }

    #[test]
    fn invalid_json_is_raw_text_with_the_error_never_a_broken_tab() {
        let rt = open(&["broken.json"], false);
        let els = rt.elements();
        assert!(!els.iter().any(|e| e.key.as_deref() == Some("json-card")));
        assert_eq!(els.iter().filter(|e| e.role == "code").count(), 1, "the raw text in a code card");
        let t = texts(&rt);
        assert!(t.iter().any(|x| x.contains("Not valid JSON: line 47") && x.contains("the file ends inside a string")), "{t:?}");
        assert!(t.iter().any(|x| x.contains("\"giulia_bianchi\"")), "{t:?}");
        assert!(t.iter().any(|x| x == "47 lines"), "{t:?}");
        let _ = Path::new("");
    }
}
