//! The heading outline, the "In this file" column right of the document (DESIGN.md, "Heading
//! outline"; mockup `scheda`): the file's H1–H3 headings, one per row, indented by level;
//! the one of the section at the top of the view in `fg` and weight 600, the others in `fg2`. A click brings
//! the heading to the top of the view.
//!
//! Headings are computed once per document and per reload ([`Cache`], with the version of the
//! `doc_view` rows), not every frame: only blocks that can be a heading go through the
//! parser. The list uses virtual rows, so a file with thousands of headings builds only the
//! rows in view.

use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use rune::{El, GrowthSpec, ScrollSpec, TextStyle, Tx, col, el, row, text};

use super::sidebar::{self, Hooks, Motion};
use super::theme::{
    HAIRLINE, OUTLINE_INDENT, OUTLINE_LABEL_GAP, OUTLINE_PAD_X, OUTLINE_ROW, OUTLINE_ROW_PAD, OUTLINE_TEXT, Theme, UI_STRONG_WEIGHT, UI_WEIGHT,
};
use crate::app::{App, Tab};
use crate::doc::document::Document;

/// The column label (mockup `scheda`).
pub const LABEL: &str = "In this file";

/// The deepest level the outline shows.
const DEEPEST: u8 = 3;

/// A document heading: level (1–3), text, block id and its position in the document
/// at the revision it comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Heading {
    pub level: u8,
    pub text: String,
    pub id: u64,
    pub index: usize,
}

/// The H1–H3 headings of `doc` with the block position. A heading is a top-level block (a `#`
/// inside a fence is in the fence's block, inside a list in the list's block).
pub fn scan(doc: &Document) -> Vec<Heading> {
    doc.blocks
        .iter()
        .enumerate()
        // The frontmatter (only the first block) is not a heading, even if it ends with `---`.
        .filter(|(i, b)| maybe_heading(&b.source) && !(*i == 0 && crate::doc::frontmatter::is_block(&b.source)))
        .filter_map(|(index, b)| {
            let (level, text) = heading(&b.source)?;
            (level <= DEEPEST).then_some(Heading { level, text, id: b.id, index })
        })
        .collect()
}

/// The fast filter, before the parser: an ATX heading starts with `#` after at most 3 spaces;
/// a setext heading has a last line made only of `=` or `-`.
fn maybe_heading(source: &str) -> bool {
    let lead = source.len() - source.trim_start_matches(' ').len();
    if lead <= 3 && source[lead..].starts_with('#') {
        return true;
    }
    // From the end, without walking the block: a long paragraph costs nothing.
    let bytes = source.trim_end().as_bytes();
    let Some(&mark @ (b'=' | b'-')) = bytes.last() else { return false };
    let line = bytes.iter().rev().take_while(|&&c| c != b'\n').count();
    line < bytes.len() && bytes[bytes.len() - line..].iter().all(|&c| c == mark || c == b' ' || c == b'\t')
}

/// Level and plain text of the block, if the block is a heading.
fn heading(source: &str) -> Option<(u8, String)> {
    plain_atx(source).or_else(|| parsed(source))
}

/// The common case without the parser: a one-line ATX heading, with no inline marks (`*`, `` ` ``,
/// links, entities, escapes). The rest goes through pulldown-cmark, which strips the marks. With this and the
/// setext filter from the end, 16 MB of Markdown (6960 headings) are scanned in ~1.8 ms instead of
/// ~4.7 (`outline_cost`).
fn plain_atx(source: &str) -> Option<(u8, String)> {
    let line = source.trim_start_matches(' ');
    let hashes = line.bytes().take_while(|&c| c == b'#').count();
    let rest = &line[hashes..];
    let simple = |c: char| !matches!(c, '*' | '_' | '`' | '[' | ']' | '<' | '>' | '&' | '\\' | '~' | '!' | '\n' | '\r' | '|');
    if !(1..=6).contains(&hashes) || !(rest.is_empty() || rest.starts_with([' ', '\t'])) || !rest.chars().all(simple) {
        return None;
    }
    // The closing sequence (`## Title ##`) is stripped only if preceded by a space.
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

/// The last heading at or above block `start` (the first visible one): the section being
/// read. `None` above the first heading.
pub fn active(items: &[Heading], start: usize) -> Option<usize> {
    items.partition_point(|h| h.index <= start).checked_sub(1)
}

/// A document's headings, computed on the first request and again only when the document
/// changes (`doc_view::rows_version`: another document, reload, other content).
#[derive(Default)]
pub struct Cache {
    cell: RefCell<Option<(Version, Rc<[Heading]>)>>,
}

/// `doc_view::rows_version` of the document the headings come from.
type Version = (u64, u64, u64);

impl Cache {
    /// Whether the headings have been computed (tests: with the outline hidden they are not computed).
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

/// The key of the row for heading `block` in tab `doc` (Rune keys are global:
/// the document id keeps them apart between tabs).
pub fn row_key(doc: u64, block: u64) -> String {
    format!("o-{doc:x}-{block:x}")
}

/// The "In this file" column of tab `tab`, `None` if the file has no H1–H3 headings. A line
/// of 0.5 pt in `line` on the left, `width` wide (`OUTLINE_W` by default, then dragged); the label in small UI `fg3` aligned
/// with the first document row; headings indent by `OUTLINE_INDENT` per level below the
/// file's highest. The list scrolls on its own if it is taller than the window.
pub fn view(tab: &Tab, hooks: Hooks<App>, width: f32, theme: &Theme, reduced: bool, hover: bool) -> Option<El> {
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
        .w(width - HAIRLINE)
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
    // In an outline taller than the window the highlighted row must not leave the view. Rune's
    // virtual rows have no `reveal_selected` (it only works for `virtualize` with children): an
    // empty placeholder with the highlighted heading's key is born on every change, and its first
    // layout brings that heading into the outline's view (`scroll_into_view_in` mounts even a
    // virtual row that was not built).
    if let Some(h) = on.and_then(|i| keys.get(i)) {
        let target = row_key(doc, h.id);
        let scope = format!("outline-list-{doc:x}");
        let marker = el().key(format!("outline-reveal-{doc:x}-{:x}", h.id)).pin(Some(0.0), Some(0.0), None, None).w(0.0).h(0.0).a11y_hidden();
        outline = outline.child(marker.on_layout_tx(move |_: &mut App, _, tx: &mut Tx| tx.scroll_into_view_in(scope.clone(), target.clone())));
    }
    Some(outline)
}

/// An outline row: `OUTLINE_ROW` tall, 12.5 pt text truncated; hover `sel` and scale 0.98
/// when pressed, like the sidebar rows.
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
        let src = "# One\n\ntext\n\n## Two\n\n### Three\n\n#### Four\n\n## Five\n";
        let d = doc(src);
        let got = scan(&d);
        assert_eq!(got.iter().map(|h| (h.level, h.text.as_str())).collect::<Vec<_>>(), [(1, "One"), (2, "Two"), (3, "Three"), (2, "Five")]);
        let ids: Vec<u64> = [0, 2, 3, 5].iter().map(|&i| d.blocks[i].id).collect();
        assert_eq!(got.iter().map(|h| h.id).collect::<Vec<_>>(), ids, "heading block ids");
    }

    #[test]
    fn a_hash_inside_a_fence_is_not_a_heading() {
        let src = "# Real\n\n```sh\n# comment\n## other\n```\n\n~~~\n# also\n~~~\n";
        assert_eq!(levels_and_text(src), [(1, "Real".to_owned())]);
    }

    #[test]
    fn frontmatter_is_not_a_heading() {
        let src = "---\ntitle: My file\ndescription: One line\n---\n\n# Real title\n";
        assert_eq!(levels_and_text(src), [(1, "Real title".to_owned())]);
    }

    #[test]
    fn setext_inline_code_and_emphasis_give_plain_text() {
        let src = "Setext title\n===\n\nBelow\n---\n\n## With `code` and *emphasis*\n\n#hashtag is not a heading\n";
        assert_eq!(levels_and_text(src), [(1, "Setext title".to_owned()), (2, "Below".to_owned()), (2, "With code and emphasis".to_owned())]);
    }

    /// The fast case gives the same text as the parser.
    #[test]
    fn the_fast_path_agrees_with_the_parser() {
        for src in [
            "# One",
            "## Two ##",
            "### Three #",
            "  # Indented",
            "## C# e F#",
            "##   spaces   in   between  ",
            "#\tTab",
            "## Title#",
            "####### seven",
            "#empty",
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
        let d = doc("intro\n\n# A\n\none\n\ntwo\n\n## B\n\nthree\n");
        let items = scan(&d);
        assert_eq!(active(&items, 0), None, "above the first heading");
        assert_eq!(active(&items, 1), Some(0));
        assert_eq!(active(&items, 3), Some(0));
        assert_eq!(active(&items, 4), Some(1));
        assert_eq!(active(&items, 99), Some(1));
    }

    #[test]
    fn the_cache_scans_again_only_when_the_document_changes() {
        let mut d = doc("# A\n\ntext\n");
        let cache = Cache::default();
        let first = cache.get(&d);
        assert!(Rc::ptr_eq(&first, &cache.get(&d)), "same revision: no new scan");
        d.update("# A\n\ntext\n\n## B\n");
        let second = cache.get(&d);
        assert_eq!(second.len(), 2, "after the reload");
        assert_eq!(second[0].id, first[0].id, "stable id");
    }

    // --- the column in the app, on a headless Rune runtime ---

    use rune::{Env, Key, KeyEvent, Mods, Runtime};
    use rune_kit::MarkdownStyle;

    const WIDE: (f32, f32) = (1280.0, 800.0);

    fn paragraph(n: usize) -> String {
        let words = ["line", "reader", "block", "scroll", "again", "frame", "text", "column"];
        (0..30 + (n * 7) % 40).map(|k| words[(n + k) % words.len()]).collect::<Vec<_>>().join(" ")
    }

    /// 12 H2 sections with 6 paragraphs each, an H3 in the even sections, under an H1.
    fn sample() -> String {
        let mut out = vec!["# Document".to_owned()];
        for s in 0..12 {
            out.push(format!("## Section {s}"));
            for p in 0..6 {
                if p == 3 && s % 2 == 0 {
                    out.push(format!("### Part {s}"));
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
        panic!("the view does not settle");
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

    /// The key of an outline element of the front tab: `outline`, `outline-line`...
    fn ok(rt: &Runtime<App>, name: &str) -> String {
        format!("{name}-{:x}", rt.state.tab().unwrap().doc.id)
    }

    fn keyed(rt: &Runtime<App>, key: &str) -> Option<[f32; 4]> {
        rt.elements().into_iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).map(|e| e.rect)
    }

    /// The text of the headings drawn in `fg` (the highlighted one).
    fn highlighted(rt: &Runtime<App>) -> Vec<String> {
        let fg = Theme::for_mode(true).fg;
        rt.elements().into_iter().filter(|e| !e.exiting && e.path.contains("outline-rows-") && e.text_color == Some(fg)).filter_map(|e| e.text).collect()
    }

    fn click(rt: &mut Runtime<App>, key: &str) {
        let r = keyed(rt, key).unwrap_or_else(|| panic!("row {key}"));
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

    /// With a wide window the column sits right of the document, `OUTLINE_W` wide, with the
    /// line on the left; the document column stays `COLUMN` wide.
    #[test]
    fn a_wide_window_shows_the_outline_beside_the_document() {
        let rt = open_at(&sample(), WIDE);
        let outline = keyed(&rt, &ok(&rt, "outline")).expect("outline");
        assert_eq!(outline[2], super::super::theme::OUTLINE_W);
        let line = keyed(&rt, &ok(&rt, "outline-line")).unwrap();
        assert_eq!((line[0], line[2]), (outline[0], HAIRLINE));
        let doc = keyed(&rt, "doc").unwrap();
        assert!((doc[0] + doc[2] - outline[0]).abs() < 0.5, "right of the document: {doc:?} {outline:?}");
        assert_eq!(keyed(&rt, "rows").unwrap()[2], super::super::theme::COLUMN);
        assert!(rt.elements().iter().any(|e| e.text.as_deref() == Some(LABEL)));
        assert_eq!(highlighted(&rt), ["Document"], "at the top: the file's heading");
    }

    /// Drags from the middle of strip `key` by `dx` points, in steps of 10, and releases.
    fn drag_edge(rt: &mut Runtime<App>, key: &str, dx: f32) {
        let e = keyed(rt, key).expect("edge");
        let (x, y) = (e[0] + e[2] / 2.0, e[1] + e[3] / 2.0);
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        let steps = (dx.abs() / 10.0).ceil().max(1.0) as usize;
        for k in 1..=steps {
            rt.pointer_move(x + dx * k as f32 / steps as f32, y);
            rt.frame(1.0 / 120.0);
        }
        rt.pointer_up(x + dx, y);
        settle(rt);
    }

    /// The sidebar and outline edges can be dragged: the width follows, is remembered and snaps to a
    /// remembered width or to the default one when you pass near it; the document stays
    /// between the two.
    #[test]
    fn dragging_the_edges_resizes_sidebar_and_outline_and_snaps() {
        let mut rt = open_at(&sample(), (1400.0, 800.0));
        assert_eq!(keyed(&rt, "sidebar").unwrap()[2], 248.0);
        drag_edge(&mut rt, "sidebar-edge", 60.0);
        assert_eq!(keyed(&rt, "sidebar").unwrap()[2], 308.0);
        assert_eq!(rt.state.settings.sidebar.learned, vec![308.0]);
        // Back to the default (snaps to 248 from 252), then near 308: it snaps there.
        drag_edge(&mut rt, "sidebar-edge", -56.0);
        assert_eq!(rt.state.settings.sidebar.width, 248.0);
        drag_edge(&mut rt, "sidebar-edge", 54.0);
        assert_eq!(keyed(&rt, "sidebar").unwrap()[2], 308.0, "snaps to the remembered width");
        // The outline: dragging left widens it.
        drag_edge(&mut rt, "outline-edge", -40.0);
        let outline = keyed(&rt, &ok(&rt, "outline")).unwrap();
        assert_eq!(outline[2], 236.0);
        assert_eq!(rt.state.settings.outline.width, 236.0);
        let doc = keyed(&rt, "doc").unwrap();
        assert!((doc[0] + doc[2] - outline[0]).abs() < 0.5, "the document reaches the outline");
        // Never so much that the outline hides: the threshold moves with the widths.
        drag_edge(&mut rt, "outline-edge", -400.0);
        assert!(keyed(&rt, &ok(&rt, "outline")).is_some());
        assert!(rt.state.outline_min_window() <= 1400.0);
        // Double click: the default again.
        let e = keyed(&rt, "outline-edge").unwrap();
        let (x, y) = (e[0] + e[2] / 2.0, e[1] + 40.0);
        for _ in 0..2 {
            rt.pointer_move(x, y);
            rt.pointer_down(x, y);
            rt.pointer_up(x, y);
        }
        settle(&mut rt);
        assert_eq!(rt.state.settings.outline.width, super::super::theme::OUTLINE_W);
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
        let rt = open_at("only text\n\nmore text\n", WIDE);
        assert!(keyed(&rt, &ok(&rt, "outline")).is_none());
    }

    /// A click on a far heading (row not mounted) brings it to the top of the view and highlights it.
    #[test]
    fn a_click_scrolls_the_heading_to_the_top_and_highlights_it() {
        let mut rt = open_at(&sample(), WIDE);
        let (key, block) = heading_key(&rt, "Section 9");
        click(&mut rt, &key);
        let tab = rt.state.tab().unwrap();
        assert_eq!(tab.visible_first, block, "the heading is the first visible block");
        let i = tab.doc.blocks.iter().position(|b| b.id == block).unwrap();
        let row = keyed(&rt, &crate::ui::doc_view::row_key(i, &tab.doc)).unwrap();
        let top = keyed(&rt, "doc").unwrap()[1];
        assert!((row[1] - top).abs() <= 1.0, "at the top: {row:?} view {top}");
        assert_eq!(highlighted(&rt), ["Section 9"]);
    }

    /// Document scroll moves the highlight to the section at the top, even inside an H3.
    #[test]
    fn scrolling_moves_the_highlight() {
        let mut rt = open_at(&sample(), WIDE);
        let mut seen = Vec::new();
        for step in 1..60 {
            rt.update(|_, tx| tx.scroll_to_instant("doc", 0.0, step as f32 * 250.0));
            settle(&mut rt);
            let h = highlighted(&rt);
            assert_eq!(h.len(), 1, "a single highlighted heading: {h:?}");
            let tab = rt.state.tab().unwrap();
            let items = tab.outline.get(&tab.doc);
            // At the bottom the last visible heading wins, otherwise the section at the top.
            let shown = if tab.at_bottom { tab.visible_last } else { tab.visible_first };
            let i = tab.doc.blocks.iter().position(|b| b.id == shown).unwrap();
            let want = &items[active(&items, i).unwrap()].text;
            assert_eq!(&h[0], want, "at scroll {step}");
            if seen.last() != Some(want) {
                seen.push(want.clone());
            }
        }
        assert!(seen.iter().any(|t| t.starts_with("Part")), "H3s too: {seen:?}");
        assert!(seen.len() > 10, "{seen:?}");
    }

    /// A reload that adds a heading above the view: the highlight stays on the section being
    /// read (it follows the block id) and the new heading enters the outline.
    #[test]
    fn a_live_reload_keeps_the_highlight_and_adds_new_headings() {
        let mut rt = open_at(&sample(), WIDE);
        let (key, _) = heading_key(&rt, "Section 6");
        click(&mut rt, &key);
        let src = format!("## New at the top\n\n{}", sample());
        rt.update(|s, _| {
            let change = s.tab_mut().unwrap().doc.update(&src);
            s.tab_mut().unwrap().note_reload(&change, 1.0);
        });
        settle(&mut rt);
        assert_eq!(highlighted(&rt), ["Section 6"]);
        let tab = rt.state.tab().unwrap();
        assert_eq!(tab.outline.get(&tab.doc)[0].text, "New at the top", "the new heading, at the top");
    }

    /// In the source (Cmd+E) there is no outline: its clicks would lead to blocks the source
    /// does not have, and the text changes while you type.
    #[test]
    fn the_source_view_hides_the_outline() {
        let mut rt = open_at(&sample(), WIDE);
        rt.key(KeyEvent { key: Key::Command('e'), mods: Mods { cmd: true, logo: true, ..Mods::default() } });
        settle(&mut rt);
        assert!(matches!(rt.state.tab().unwrap().mode, crate::app::Mode::Source(_)));
        assert!(keyed(&rt, &ok(&rt, "outline")).is_none());
    }

    /// Indents: H1 on the left, H2 indented by `OUTLINE_INDENT`, H3 by twice that.
    #[test]
    fn levels_indent_by_ten() {
        let rt = open_at(&sample(), WIDE);
        let x = |t: &str| rt.elements().into_iter().find(|e| e.path.contains("outline-rows-") && e.text.as_deref() == Some(t)).unwrap().rect[0];
        assert_eq!(x("Section 0") - x("Document"), OUTLINE_INDENT);
        assert_eq!(x("Part 0") - x("Document"), 2.0 * OUTLINE_INDENT);
    }

    /// An outline taller than the window: scrolling the document to the bottom, the highlighted
    /// heading enters the visible part of the outline.
    #[test]
    fn the_highlight_stays_in_view_in_a_long_outline() {
        let src: String = (0..80).map(|n| format!("## Title {n}\n\n{}\n\n", paragraph(n))).collect();
        let mut rt = open_at(&src, WIDE);
        rt.update(|_, tx| tx.scroll_to_instant("doc", 0.0, 1.0e9));
        settle(&mut rt);
        let fg = Theme::for_mode(true).fg;
        let els = rt.elements();
        let on = els.iter().find(|e| !e.exiting && e.path.contains("outline-rows-") && e.text_color == Some(fg)).expect("highlight mounted");
        let doc = rt.state.tab().unwrap().doc.id;
        let list = keyed(&rt, &format!("outline-list-{doc:x}")).unwrap();
        assert!(on.rect[1] >= list[1] - 0.5 && on.rect[1] + on.rect[3] <= list[1] + list[3] + 0.5, "{:?} outside {list:?}", on.rect);
    }

    /// The cost of the heading scan, paid on open and on every reload (once,
    /// then the cache): `fixtures/big.md` (1 MB) and the same repeated up to 16 MB.
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
            eprintln!("{name}: {} bytes, {} blocks, {} headings, scan p50 {:.2} ms, max {:.2} ms", src.len(), d.blocks.len(), scan(&d).len(), ms[10], ms[19]);
        }
    }

    /// The first visible block (id) and its offset from the top edge of the document view.
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

    /// The window goes below and above 1128 pt: the outline disappears and comes back, the document stays
    /// where it was (same first block, same offset) and so does its view state.
    #[test]
    fn resizing_across_the_threshold_keeps_the_document_in_place() {
        let mut rt = open_at(&sample(), WIDE);
        rt.update(|_, tx| tx.scroll_to_instant("doc", 0.0, 3000.0));
        settle(&mut rt);
        let before = anchor(&rt);
        assert!(before.1 < -1.0, "the edge cuts the first block: {before:?}");
        let min = super::super::theme::OUTLINE_MIN_WINDOW;
        for (w, shown) in [(min - 30.0, false), (min + 150.0, true), (min - 1.0, false), (WIDE.0, true)] {
            resize(&mut rt, w);
            assert_eq!(keyed(&rt, &ok(&rt, "outline")).is_some(), shown, "outline at {w}");
            let now = anchor(&rt);
            assert_eq!(now.0, before.0, "at {w}: first block");
            assert!((now.1 - before.1).abs() <= 1.0, "at {w}: offset {} → {}", before.1, now.1);
        }
    }

    /// A file that ends with short sections: at the bottom, scrolling cannot bring them to the top.
    fn short_tail() -> String {
        format!("{}\n\n## Penultimate\n\nshort\n\n## Last\n\nshort\n", sample().trim_end())
    }

    /// With the scroll at the bottom the last visible heading is highlighted.
    #[test]
    fn at_the_bottom_the_last_visible_heading_is_highlighted() {
        let mut rt = open_at(&short_tail(), WIDE);
        rt.update(|_, tx| tx.scroll_to_instant("doc", 0.0, 1.0e9));
        settle(&mut rt);
        assert_eq!(highlighted(&rt), ["Last"]);
        // Higher up (no longer at the bottom) the section-at-the-top rule is back.
        rt.update(|_, tx| tx.scroll_to_instant("doc", 0.0, 3000.0));
        settle(&mut rt);
        assert_ne!(highlighted(&rt), ["Last"]);
    }

    /// A file that fits in the window is not "at the bottom": the first heading stays highlighted.
    #[test]
    fn a_file_that_fits_highlights_its_first_heading() {
        let rt = open_at("# One\n\nshort\n\n## Two\n\nshort\n", WIDE);
        assert_eq!(highlighted(&rt), ["One"]);
    }

    /// A click on a heading that cannot reach the top: the clicked one is highlighted, until
    /// you scroll by hand.
    #[test]
    fn a_clicked_heading_is_highlighted_even_if_it_cannot_reach_the_top() {
        let mut rt = open_at(&short_tail(), WIDE);
        let (key, _) = heading_key(&rt, "Penultimate");
        // the outline row goes into view before the click
        let doc = rt.state.tab().unwrap().doc.id;
        rt.update(|_, tx| tx.scroll_into_view_in(format!("outline-list-{doc:x}"), key.clone()));
        settle(&mut rt);
        click(&mut rt, &key);
        assert_eq!(highlighted(&rt), ["Penultimate"]);
        rt.update(|_, tx| tx.scroll_to_instant("doc", 0.0, 2000.0));
        settle(&mut rt);
        assert_ne!(highlighted(&rt), ["Penultimate"], "a scroll by hand goes back to the rule");
    }

    /// With the outline hidden (narrow window) scrolling does not compute the headings.
    #[test]
    fn a_hidden_outline_costs_nothing_while_scrolling() {
        let mut rt = open_at(&short_tail(), (1000.0, 800.0));
        for step in 1..20 {
            rt.update(|_, tx| tx.scroll_to_instant("doc", 0.0, step as f32 * 400.0));
            settle(&mut rt);
        }
        rt.update(|_, tx| tx.scroll_to_instant("doc", 0.0, 1.0e9));
        settle(&mut rt);
        assert!(!rt.state.tab().unwrap().outline.scanned(), "headings computed with the outline hidden");
        resize(&mut rt, WIDE.0);
        assert_eq!(highlighted(&rt), ["Last"], "and when the outline appears it is already right");
    }
}
