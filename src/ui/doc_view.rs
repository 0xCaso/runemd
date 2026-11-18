//! The document column: one top-level block per `virtual_rows` row inside a scroll.
//!
//! Rows are keyed by the block id, not the index: when the file changes,
//! Rune keeps the row at the top of the view still by key (`virtual_rows`: "inserting or
//! prepending rows keeps the reader's row in place"); if that block disappears, the anchor moves to the
//! first following block that still exists (`VirtualList::set_keys` → `first_visible`).

use std::borrow::Cow;
use std::collections::HashSet;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use rune::{Color, El, Family, GrowthSpec, Key, KeyEvent, ScrollSpec, SelectedText, TextSelection, TextStyle, col, el};
use rune_kit::MarkdownStyle;

use crate::doc::document::{Block, Document};
use crate::doc::select_all::{End, Ends, SelectAll, SelectAllFlag};

use super::theme::{CHANGED_MARK_W, CHANGED_MARK_X, R_ROW, ROW_PAD_X, ROW_PAD_Y, UI_SMALL, UI_WEIGHT};

/// Height of the space below the last row (the resting fire sits under it).
pub(crate) const BOTTOM: f32 = 48.0;
/// Space above the first row (the source starts here too).
pub(crate) const TOP: f32 = 24.0;

/// The live signals inside the column, which do not move the text.
pub struct Signals<S> {
    /// Whether block `id` has the "what changed" mark (left of the row, out of the flow).
    pub marked: fn(&S, u64) -> bool,
    /// Mark color (`theme.mark`).
    pub mark: Color,
    /// The "is writing" caret, if lit: in the row of the modified block, out of the flow.
    pub caret: Option<Caret>,
    /// The block's Find marks, whose row has the given key (`ui::find`).
    pub hits: fn(&S, &Block, &str) -> super::find::Marks,
    /// Where the view and the page of document `id` are (each from its `on_layout`): used to
    /// know whether the scroll is at the bottom. True if the view must run again.
    pub edge: fn(&mut S, u64, Edge, &mut rune::Tx) -> bool,
    /// The colors of the YAML frontmatter block: text (`fg2`) and fill.
    pub meta: Meta,
    /// The document fonts, to measure where long words break (`doc::breaks`).
    pub fonts: super::markdown_style::Fonts,
    /// A click on a document link (destination): `runemd-open:` paths open the file.
    pub open: fn(&mut S, &str, &mut rune::Tx),
    /// The text column is at least [`TWO_COLUMNS`] wide: label blocks go on two
    /// columns (`label_columns`).
    pub columns: bool,
    /// The space under the last row (`fx::fire::doc_bottom`): the resting fire sits there.
    pub bottom: f32,
}

/// From how wide the text column is (without the row margins) label blocks
/// go on two columns (constraint 4 of the logs record, mockup t-mix).
pub const TWO_COLUMNS: f32 = 480.0;

/// The "is writing" caret (record 2026-11-15-caret-at-the-edit-point): the character of the
/// row it leans on has a transparent highlight with key [`caret_key`], the row
/// its usual key ([`caret_row_key`]); from their rects in the last layout (`Cx::layout`, which scrolling does not
/// move) the view knows where to put it, in `place`. Before the first layout it does not show.
pub struct Caret {
    pub at: crate::live::caret::At,
    /// `[x, y, h]` in the row, in pt ([`caret_place`]).
    pub place: Option<[f32; 3]>,
    /// The clock it lit up at, and the theme and "reduce motion" of its slot.
    pub since: f64,
    pub dark: bool,
    pub reduced: bool,
}

/// The key of the caret character's highlight in document `doc`.
pub fn caret_key(doc: u64) -> String {
    format!("caret-{doc:x}")
}

/// The key of the highlight of the last character of the caret's block (`At::end`) in
/// document `doc`: if the caret character has no rect, the caret goes there.
pub fn caret_end_key(doc: u64) -> String {
    format!("caret-end-{doc:x}")
}

/// The row key of block `block` in document `doc` (the one from [`row_key`]).
pub fn caret_row_key(doc: u64, block: u64) -> String {
    format!("{doc:x}-{block:x}")
}

/// Space between the character and the caret, pt (mockup: `margin-left: 3px` on a bar of 2).
const CARET_GAP: f32 = 2.0;
/// Caret height relative to the character's text line (mockup: 1.1 em on a line of
/// about 1.5 em).
const CARET_LINE: f32 = 0.74;

/// Where the caret goes in the row, `[x, y, h]` in pt, from the character's `mark` rect and the
/// row's `row` rect (same points): right of the character, or left with `Side::Before`; as tall as the
/// letter, centered on the text line.
pub fn caret_place(mark: [f32; 4], row: [f32; 4], side: crate::live::caret::Side) -> [f32; 3] {
    let x = match side {
        crate::live::caret::Side::After => mark[0] + mark[2] + CARET_GAP,
        crate::live::caret::Side::Before => mark[0] - CARET_GAP - crate::fx::caret::WIDTH,
    } - row[0];
    let h = (mark[3] * CARET_LINE).round();
    [x, mark[1] - row[1] + (mark[3] - h) * 0.5, h]
}

/// Where the caret goes from the rects of the last layout: of its character `mark`, of the last character
/// of block `end` and of row `row`. Without `mark` (a character that does not show) it goes after `end`,
/// at the end of the changed block (record 2026-11-15-caret-at-the-edit-point, constraint 2).
pub fn caret_point(mark: Option<[f32; 4]>, end: Option<[f32; 4]>, row: Option<[f32; 4]>, side: crate::live::caret::Side) -> Option<[f32; 3]> {
    let row = row?;
    match mark {
        Some(m) => Some(caret_place(m, row, side)),
        None => end.map(|e| caret_place(e, row, crate::live::caret::Side::After)),
    }
}

/// The YAML frontmatter block in reading (DESIGN.md, "Frontmatter"): monospaced, small,
/// text `text` on `fill`, like a code block but quieter; and the callout colors.
#[derive(Clone, Copy, Debug)]
pub struct Meta {
    pub text: Color,
    pub fill: Color,
    /// Callouts (`doc::callout`): label (`fg`), icon (`fg2`) and quote line.
    pub label: Color,
    pub icon: Color,
    pub rule: Color,
    /// The line above the footnotes (`line`).
    pub line: Color,
    /// The color of headings 4, 5 and 6 (`body` like headings 1–3, `fg2`, `fg3`).
    pub headings: [Color; 3],
    /// Paths (`doc::paths`): file name (`fg`), folders, `:line` and dots (`fg3`), line
    /// on hover (`fg2`) and ground on hover (`sel`).
    pub path: PathColors,
}

/// The colors of paths in the text (record 2026-11-16-paths-name-and-link, mockup p-mix).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathColors {
    pub name: Color,
    pub dim: Color,
    pub hover_line: Color,
    pub hover_bg: Color,
}

/// A rectangle of the document view, in window points.
#[derive(Clone, Copy, Debug)]
pub enum Edge {
    /// The scrolling view (`doc`): it changes only with the window.
    Viewport([f32; 4]),
    /// The page inside the view: it moves on every scroll frame.
    Page([f32; 4]),
    /// A mounted row (block id): it moves on every scroll frame too.
    Row(u64, [f32; 4]),
}

/// Key of row `i`: document id + block id. Block ids restart from 1 in
/// every `Document`, the document id does not and is unique in the app: without it, a tab or a
/// reopened file would inherit the measured heights of another.
pub fn row_key(i: usize, doc: &Document) -> String {
    match doc.blocks.get(i) {
        Some(b) => format!("{:x}-{:x}", doc.id, b.id),
        // The `virtual_rows` closures run with the current state: if the document changed
        // after the last view, any unique key until the view runs again.
        None => format!("{:x}-i{i:x}", doc.id),
    }
}

/// What tells Rune whether the row keys changed: another document (`id`, even
/// if it is the same unchanged file), an update (`revision`), other content at the same
/// revision (`file_hash`).
pub fn rows_version(doc: &Document) -> (u64, u64, u64) {
    (doc.id, doc.revision, doc.file_hash)
}

/// The scrolling view of document `doc_of(s)`. The `virtual_rows` closures cannot
/// hold borrows: at layout they receive the window state and find the document again with the
/// same `doc_of`, so row count, version, keys and content all come from the same
/// document.
///
/// Each document has its own element (key `tab-<id>` around the `doc` scroll): a tab
/// that comes back to the front starts from a new scroll, not from the previous tab's.
///
/// `on_rows` receives the document id and the indices of the visible rows (Rune's
/// `on_visible_rows`: `start..end`, rows even partly in view) after the first layout and on every
/// change; it returns true if it changed state that the view reads.
pub fn view<S: 'static>(s: &S, doc_of: fn(&S) -> &Document, style: &MarkdownStyle, column: f32, on_rows: fn(&mut S, u64, Range<usize>) -> bool, signals: Signals<S>) -> El {
    let doc = doc_of(s);
    let generation = doc.id;
    let notes = doc.notes.block().map(|b| footnotes(&b, (signals.hits)(s, &b, crate::doc::footnotes::KEY), style, signals.fonts, signals.meta, column));
    let style = style.clone();
    let blocks: Arc<[(u64, Arc<str>)]> = doc.blocks.iter().map(|b| (b.id, b.source.clone())).collect();
    let file = File { path: doc.path.clone(), hash: doc.file_hash };
    let select_all = doc.select_all.clone();
    let flag = select_all.clone();
    let order = blocks.clone();
    let places = Arc::new(crate::doc::paths::Places::of(&doc.path));
    // The column is at most `column` wide ("Text width"; "Full" follows the panel).
    let rows = super::theme::column(col().key("rows").w_full(), column).self_center().selection_group(true)
        .growth(GrowthSpec::default())
        .selection_copy(move |sel| copy_source(sel, generation, &file, &select_all, &blocks))
        .on_selection_change(move |_: &mut S, ch, _| selection_changed(&flag, ch, generation, &order))
        .virtual_rows(
            doc.blocks.len(),
            60.0,
            move |s: &S, i| Cow::Owned(row_key(i, doc_of(s))),
            move |s: &S, i| {
                let row = match doc_of(s).blocks.get(i) {
                Some(b) if i == 0 && crate::doc::frontmatter::is_block(&b.source) => frontmatter_row(&b.source, signals.meta),
                // Footnote definitions are drawn at the bottom (`footnotes`).
                Some(b) if crate::doc::footnotes::definitions(&b.source).is_some() => el().w_full(),
                Some(b) => {
                    let doc = doc_of(s);
                    let mut hits = (signals.hits)(s, b, &row_key(i, doc));
                    let caret = signals.caret.as_ref().filter(|c| c.at.block == b.id);
                    if let Some(c) = caret {
                        let clear = rune_kit::markdown::Highlight::new().bg(Color::TRANSPARENT).ring(Color::TRANSPARENT, 1.0);
                        hits.push((vec![c.at.anchor.clone()], clear.clone().key(caret_key(doc.id))));
                        hits.push((vec![c.at.end.clone()], clear.key(caret_end_key(doc.id))));
                    }
                    let progress = doc.tasks.get(&b.id).copied();
                    let row = block_row(&b.source, &style, &doc.notes, &places, progress, signals.columns, signals.open, signals.fonts, signals.meta, (signals.marked)(s, b.id).then_some(signals.mark), hits);
                    match caret {
                        Some(c) => caret_row(row, c),
                        None => row,
                    }
                }
                None => el(),
                };
                // Where each row is: the one under the top edge holds when the text width changes.
                let Some(id) = doc_of(s).blocks.get(i).map(|b| b.id) else { return row };
                let edge = signals.edge;
                row.on_layout_tx(move |s: &mut S, r, tx: &mut rune::Tx| {
                    if !edge(s, generation, Edge::Row(id, r), tx) {
                        tx.live_only();
                    }
                })
            },
        )
        .virtual_rows_version(rows_version(doc))
        .on_visible_rows(move |s: &mut S, rows| on_rows(s, generation, rows));
    let bottom = col().w_full().h(signals.bottom);
    let edge = signals.edge;
    // The `on_layout`s only say where view and page are: the view runs again only if what it
    // shows changes (`live_only` otherwise), not on every scroll frame.
    let page = col().w_full().child(el().h(TOP)).child(rows).children(notes).child(bottom).on_layout_tx(move |s: &mut S, r, tx: &mut rune::Tx| {
        if !edge(s, generation, Edge::Page(r), tx) {
            tx.live_only();
        }
    });
    let scroll = col().key("doc").w_full().h_full().scroll(ScrollSpec { stick_bottom: false }).child(page).on_layout_tx(move |s: &mut S, r, tx: &mut rune::Tx| {
        if !edge(s, generation, Edge::Viewport(r), tx) {
            tx.live_only();
        }
    });
    el().key(format!("tab-{generation:x}")).w_full().grow(1.0).min_h(0.0).child(scroll)
}

/// The fire label: how many new blocks there are below the view (approved text, spec).
pub fn fire_label(n: usize) -> String {
    if n == 1 { "1 new paragraph below".to_owned() } else { format!("{n} new paragraphs below") }
}

/// The caps of the selection rebuilt out of view (rune `MAX_VIRTUAL_COPY_BYTES` and
/// `MAX_VIRTUAL_COPY_ROWS`, `pub(crate)` in Rune): past them, `TextSelection` stops before the
/// end. The row cap also counts rows without text (`---`).
const COPY_CAP_BYTES: usize = 4 * 1024 * 1024;
const COPY_CAP_ROWS: usize = 10_000;
/// Truncated text can stop a little before the cap (empty rows and the prefix between two pieces).
const COPY_CAP_SLACK: usize = 1024;

/// To call with every key the app receives (`AppSpec::on_key`, before Rune's
/// selection): a Cmd+A marks that the selection that follows is "the whole file".
///
/// Only the key Rune takes for itself counts: plain Cmd+A (like `selection_key`: no Shift,
/// Option, nor Control with Logo) and not repeated.
pub fn note_key(doc: &Document, key: &KeyEvent, repeat: bool) {
    let m = key.mods;
    let plain = m.cmd && !m.shift && !m.alt && !(m.ctrl && m.logo);
    if matches!(key.key, Key::Command('a')) && plain && !repeat && doc.select_all.get() == SelectAll::No {
        doc.select_all.set(SelectAll::Pending);
    }
}

/// The ends of `sel`; `None` if no selected text is in a document row.
fn ends(sel: &TextSelection, generation: u64, blocks: &[(u64, Arc<str>)]) -> Option<Ends> {
    let texts: Vec<_> = sel.texts().iter().filter(|t| !t.range().is_empty()).collect();
    let end = |t: &SelectedText| Some(End { row: row_id(t.path(), generation)?, range: t.range(), whole: t.is_whole() });
    let (first, last) = (end(texts.first()?)?, end(texts.last()?)?);
    let index = |row: u64| blocks.iter().position(|(id, _)| *id == row);
    let rows = match (index(first.row), index(last.row)) {
        (Some(a), Some(b)) => b.saturating_sub(a) + 1,
        _ => 0,
    };
    let capped = sel.text().len() + COPY_CAP_SLACK >= COPY_CAP_BYTES || rows >= COPY_CAP_ROWS;
    Some(Ends { first, last, capped })
}

/// Every selection change. A pending Cmd+A expires at the first change that is not Rune's: a
/// selection made with the pointer (`dragging`) or emptied. After a Cmd+A the first selection counts as
/// "all" and its ends are remembered. Rune then completes it with the rows it rebuilds, without
/// `started` (past the copy cap the length swings with the layout): it stays "all" as long as the
/// ends hold ([`End::holds`]). If the selection is truncated the last row seen is not the end and
/// is not compared. A click, an emptying, a new selection or one with a moved end
/// (Shift+click) remove it.
///
/// Leftover case: on a truncated selection (over 4 MiB or 10 000 rows) a Shift+click that
/// shortens the end without going below the cap does not show, and Cmd+C still copies the whole
/// file. `a_shift_trim_after_cmd_a_over_the_copy_cap_still_copies_the_file` pins it.
fn selection_changed(flag: &SelectAllFlag, change: &rune::SelectionChange, generation: u64, blocks: &[(u64, Arc<str>)]) {
    let now = change.selection().filter(|t| !t.text().is_empty()).and_then(|t| ends(t, generation, blocks));
    let next = match (flag.get(), now) {
        _ if change.cleared() || change.dragging() => SelectAll::No,
        (SelectAll::Pending, Some(ends)) => SelectAll::All(ends),
        (SelectAll::All(was), Some(now))
            if !change.started() && now.first.holds(&was.first) && ((was.capped && now.capped) || now.last.holds(&was.last)) =>
        {
            SelectAll::All(was)
        }
        _ => SelectAll::No,
    };
    flag.set(next);
}

/// What Cmd+C and the Copy menu copy: the Markdown source of the blocks the
/// selection touches, joined by an empty line, not the drawn text. Rows out of view have no
/// text in layout, but Rune rebuilds them for the selection and `sel.texts()` contains them (up to
/// 10 000 rows and 4 MiB per copy: past that, the selection arrives truncated and unmarked).
///
/// That is why "the whole file" is not inferred from the selection: the Cmd+A says it (`select_all`). Then
/// the file read from disk is copied, which keeps the extra empty lines between blocks.
fn copy_source(sel: &TextSelection, generation: u64, file: &File, select_all: &SelectAllFlag, blocks: &[(u64, Arc<str>)]) -> Option<String> {
    if matches!(select_all.get(), SelectAll::All(_)) && !blocks.is_empty() {
        return file.read().or_else(|| Some(join(blocks.iter().map(|(_, s)| &**s))));
    }
    let picked: HashSet<u64> = sel.texts().iter().filter(|t| !t.range().is_empty()).filter_map(|t| row_id(t.path(), generation)).collect();
    let index: Vec<usize> = blocks.iter().enumerate().filter(|(_, (id, _))| picked.contains(id)).map(|(i, _)| i).collect();
    let (&first, &last) = (index.first()?, index.last()?);
    Some(join(blocks[first..=last].iter().map(|(_, s)| &**s)))
}

fn join<'a>(sources: impl Iterator<Item = &'a str>) -> String {
    sources.collect::<Vec<_>>().join("\n\n")
}

/// The open file: it is read again from disk only if it is still the one the blocks come from.
#[derive(Clone)]
struct File {
    path: PathBuf,
    hash: u64,
}

impl File {
    fn read(&self) -> Option<String> {
        let (text, hash, _) = crate::doc::disk::read_disk(&self.path).ok()?;
        (hash == self.hash).then_some(text)
    }
}

/// The block id in a text's path: the `<document id>-<id>` segment of its row.
fn row_id(path: &str, generation: u64) -> Option<u64> {
    let prefix = format!("{generation:x}-");
    path.split('/').find_map(|seg| u64::from_str_radix(seg.strip_prefix(&prefix)?, 16).ok())
}

/// The YAML frontmatter: the text between the two `---` lines, as it is, in a metadata block
/// (`UI_SMALL` mono in `meta.text` on `meta.fill`, radius `R_ROW`). Selectable like the rest:
/// copying gives the block source, `---` included.
fn frontmatter_row(source: &str, meta: Meta) -> El {
    let body = source.split_once('\n').map_or("", |(_, rest)| rest);
    let body = body.rsplit_once('\n').map_or("", |(yaml, _)| yaml);
    let text = rune::text(body.to_owned()).font(TextStyle::new(UI_SMALL, UI_WEIGHT).family(Family::Mono)).color(meta.text).wrap().w_full().selectable(true);
    let block = col().key("frontmatter").w_full().fill(meta.fill).radius(R_ROW).px(FRONTMATTER_PAD_X).py(FRONTMATTER_PAD_Y).child(text);
    col().w_full().px(ROW_PAD_X).py(ROW_PAD_Y).child(block)
}

/// The inner margin of the frontmatter block.
const FRONTMATTER_PAD_X: f32 = 12.0;
const FRONTMATTER_PAD_Y: f32 = 10.0;

/// A document row, with the Find marks `hits`; with `mark` it has the "what changed" mark
/// in the left margin, out of the flow (the text does not move), as tall as the block's text.
#[allow(clippy::too_many_arguments)]
fn block_row<S: 'static>(source: &str, style: &MarkdownStyle, notes: &crate::doc::footnotes::Notes, places: &crate::doc::paths::Places, progress: Option<(u32, u32)>, columns: bool, open: fn(&mut S, &str, &mut rune::Tx), fonts: super::markdown_style::Fonts, meta: Meta, mark: Option<Color>, hits: super::find::Marks) -> El {
    // A label paragraph (logs, plans) keeps its line breaks, labels in bold
    // (`doc::labels`); with a wide column, labels and text on two columns. Other
    // paragraphs stay standard Markdown.
    let body = match crate::doc::labels::block(source) {
        Some(lines) if columns => label_columns(source, &lines, hits, fonts, |src, hits| block_body(src, style, notes, places, None, open, fonts, meta, hits)),
        Some(lines) => {
            let e = crate::doc::labels::one_column(source, &lines);
            block_body(&e.text, style, notes, places, None, open, fonts, meta, remap(hits, |r| e.map(r)))
        }
        None => block_body(source, style, notes, places, progress, open, fonts, meta, hits),
    };
    // Virtual rows want inner padding, not gaps between rows.
    let row = col().w_full().px(ROW_PAD_X).py(ROW_PAD_Y).child(body);
    match mark {
        Some(c) => row.child(el().key("changed").pin(Some(CHANGED_MARK_X), Some(ROW_PAD_Y), None, Some(ROW_PAD_Y)).w(CHANGED_MARK_W).fill(c)),
        None => row,
    }
}

/// The Find marks `hits` with each range passed through `f`.
fn remap(hits: super::find::Marks, f: impl Fn(&Range<usize>) -> Range<usize>) -> super::find::Marks {
    hits.into_iter().map(|(ranges, hl)| (ranges.iter().map(&f).collect(), hl)).collect()
}

/// The Find marks inside `r` of the source, counted from `r.start`.
fn clip(hits: &super::find::Marks, r: &Range<usize>) -> super::find::Marks {
    hits.iter()
        .filter_map(|(ranges, hl)| {
            let inside: Vec<Range<usize>> = ranges.iter().filter(|x| x.start < r.end && r.start < x.end || (x.is_empty() && r.contains(&x.start))).map(|x| x.start.max(r.start) - r.start..x.end.min(r.end) - r.start).collect();
            (!inside.is_empty()).then(|| (inside, hl.clone()))
        })
        .collect()
}

/// A label block on two columns (constraint 4 of the logs record, wide mockup t-mix):
/// on the left the labels in bold without the `:`, as wide as the widest and at most
/// 10 em, on the right the text, aligned row by row; a row without a label sits on the right.
fn label_columns(source: &str, lines: &[crate::doc::labels::Line], hits: super::find::Marks, fonts: super::markdown_style::Fonts, body: impl Fn(&str, super::find::Marks) -> El) -> El {
    let bold = TextStyle { weight: LABEL_WEIGHT, ..fonts.body };
    let widest = lines.iter().filter_map(|l| l.name()).map(|r| rune::text::advance(&source[r], bold)).fold(0.0, f32::max);
    let width = widest.min(LABEL_MAX_EM * fonts.body.size).ceil();
    col().w_full().children(lines.iter().map(|l| {
        let label = match l.name() {
            Some(r) => {
                let e = crate::doc::footnotes::Edited::new(&source[r.clone()], vec![(0..0, "**".to_owned()), (r.len()..r.len(), "**".to_owned())]);
                let hits = remap(clip(&hits, &r), |x| e.map(x));
                body(&e.text, hits)
            }
            None => el(),
        };
        let rest = l.rest(source);
        rune::row().w_full().gap(LABEL_GAP).child(el().w(width).child(label)).child(el().grow(1.0).min_w(0.0).child(body(&source[rest.clone()], clip(&hits, &rest))))
    }))
}

/// Weight of the labels in the column (the document's bold, `strong_weight`).
const LABEL_WEIGHT: f32 = 620.0;
/// The label column is at most 10 em wide (constraint 4).
const LABEL_MAX_EM: f32 = 10.0;
/// Between label and text.
const LABEL_GAP: f32 = 16.0;

/// The Markdown of a block (or of a piece of it) as it is drawn, with the Find marks `hits`.
#[allow(clippy::too_many_arguments)]
fn block_body<S: 'static>(source: &str, style: &MarkdownStyle, notes: &crate::doc::footnotes::Notes, places: &crate::doc::paths::Places, progress: Option<(u32, u32)>, open: fn(&mut S, &str, &mut rune::Tx), fonts: super::markdown_style::Fonts, meta: Meta, hits: super::find::Marks) -> El {
    // A `Task N` heading: the number outside the Markdown, in `fg3`, and the progress on the right.
    let task = crate::doc::labels::task_head(source);
    let untasked = task.as_ref().map(|t| crate::doc::footnotes::Edited::new(source, vec![(t.hashes.end + 1..t.prefix.end, String::new())]));
    let (source, hits) = match &untasked {
        Some(e) => (e.text.as_str(), hits.into_iter().map(|(ranges, hl)| (ranges.iter().map(|r| e.map(r)).collect(), hl)).collect()),
        None => (source, hits),
    };
    // A callout (`> [!NOTE]`) is drawn without the marker line, with the label above.
    let callout = crate::doc::callout::of(source);
    let (source, hits) = match &callout {
        Some(c) => (c.text.as_str(), hits.into_iter().map(|(ranges, hl)| (ranges.iter().map(|r| c.map(r)).collect(), hl)).collect()),
        None => (source, hits),
    };
    // Footnotes: the number in superscript in place of `[^1]`.
    let numbered = notes.shown(source);
    let (source, hits) = match &numbered {
        Some(e) => (e.text.as_str(), hits.into_iter().map(|(ranges, hl)| (ranges.iter().map(|r| e.map(r)).collect(), hl)).collect()),
        None => (source, hits),
    };
    // Paths: monospaced links with the file name in bold (`doc::paths`); the name
    // takes its color from a mark without a ground, like the Find ones.
    let linked = crate::doc::paths::shown(source, places);
    let (source, hits) = match &linked {
        Some(sh) => {
            let e = &sh.edited;
            let mut hits: super::find::Marks = hits.into_iter().map(|(ranges, hl)| (ranges.iter().map(|r| e.map(r)).collect(), hl)).collect();
            let paint = |c| rune_kit::markdown::Highlight::new().bg(Color::TRANSPARENT).ring(Color::TRANSPARENT, 1.0).fg(c);
            let marks = sh.names.iter().map(|r| (vec![r.clone()], paint(meta.path.name))).chain(sh.dims.iter().map(|r| (vec![r.clone()], paint(meta.path.dim))));
            hits.splice(0..0, marks);
            (e.text.as_str(), hits)
        }
        None => (source, hits),
    };
    // Long words break after `/ - _ .`, and anywhere only if wider than a line: the
    // drawn text has extra U+200Bs, the Find marks move with them.
    let (shown, hits) = match crate::doc::breaks::soft_breaks(source, line_limit, |t, kind| rune::text::advance(t, font_of(&fonts, kind))) {
        Some(b) => {
            let hits = hits.into_iter().map(|(ranges, hl)| (ranges.iter().map(|r| b.map(r)).collect(), hl)).collect();
            (b.text, hits)
        }
        None => (source.to_owned(), hits),
    };
    // `selectable` makes each block a selection_group of its own, which would stop a drag
    // at the block edge: the flag is removed and the single group on the rows applies.
    let level = heading_level(source);
    let style = match level {
        Some(n @ 4..=6) => Cow::Owned(style.clone().heading_font(4, fonts.headings[n as usize - 1]).muted(meta.headings[n as usize - 4])),
        _ => Cow::Borrowed(style),
    };
    // Paths that do not exist are inline code: without a pill if it is the only one in the block.
    let style = match &linked {
        Some(sh) if sh.bare => Cow::Owned(style.into_owned().inline_code_bg(Color::TRANSPARENT).inline_code_pad(0.0)),
        _ => style,
    };
    let md = rune_kit::Markdown::new(shown).highlights(hits).selectable(true);
    let md = if linked.is_some() { path_links(md, &meta, fonts) } else { md };
    // Every link opens (`App::open_link`): paths, URLs and `#heading`.
    let md = md.on_link_tx(move |s: &mut S, url, tx| open(s, url, tx));
    let body = md.render(&style).selection_group(false);
    let body = match callout {
        Some(c) => col().w_full().child(callout_head(c.kind, &meta)).child(body),
        None => body,
    };
    match task {
        Some(t) => task_heading(body, &t, progress, fonts, &meta),
        None => body,
    }
}

/// A `Task N` heading (mockup t-sezioni): the number in the heading font in `fg3` on the left, the
/// heading, and on the right "2 of 4" from the task's checkboxes, small in `fg3`, on the baseline of the
/// first line: a row as tall as the heading's, lowered by how much higher the small body's baseline
/// sits (`BASELINE_EM`).
fn task_heading(body: El, t: &crate::doc::labels::TaskHead, progress: Option<(u32, u32)>, fonts: super::markdown_style::Fonts, meta: &Meta) -> El {
    let font = fonts.headings[t.level as usize - 1];
    let number = rune::text(t.number.clone()).font(font).color(meta.path.dim).selectable(false);
    let small = super::sidebar::small_font().lh(font.line_height);
    let drop = (BASELINE_EM * (font.size - small.size)).max(0.0);
    let done = progress.map(|(d, n)| rune::text(format!("{d} of {n}")).font(small).color(meta.path.dim).selectable(false).pt(drop));
    rune::row().w_full().gap(TASK_GAP).child(number).child(el().grow(1.0).min_w(0.0).child(body)).children(done)
}

/// From mid-line to the baseline, per font size: (ascender − descender) / 2 of Inter (0.969 and
/// 0.242 per em). The text sits in the middle of its row, so two sizes on rows of equal height
/// have baselines `BASELINE_EM` apart times the size difference.
const BASELINE_EM: f32 = 0.363;

/// Between number, heading and progress of a `Task N` heading.
const TASK_GAP: f32 = 10.0;

/// The look of path links to files that exist (mockup p-mix): monospaced at 13.5 in
/// `fg3`, underlined in `fg3` (the kit has no dotted line) and on hover a `sel` ground, a
/// `fg2` line and "Open in RuneMD". Those that do not exist are not links (`doc::paths::shown`).
fn path_links(md: rune_kit::Markdown, meta: &Meta, fonts: super::markdown_style::Fonts) -> rune_kit::Markdown {
    use crate::doc::paths::OPEN;
    let c = meta.path;
    let size = super::theme::CODE_TEXT * fonts.body.size / super::theme::BODY_TEXT;
    let span = rune::text::Span { mono: true, size: Some(size), color: Some(c.dim), ..Default::default() };
    md.link_style(move |url| {
        if url.starts_with(OPEN) {
            Some((rune::text::Span { underline: Some(c.dim), ..span.clone() }, rune::text::LinkPaint::new().bg(c.hover_bg).underline(c.hover_line)))
        } else {
            None
        }
    })
    .link_help(|url| url.starts_with(OPEN).then(|| OPEN_HELP.to_owned()))
}

/// The hint on paths that open (mockup p-mix).
pub const OPEN_HELP: &str = "Open in RuneMD";

/// The level of a block that is an ATX heading (`#` … `######`). The kit folds levels from 4 down
/// into one, in `muted`: for their block the view gives them the font and color of the real level.
fn heading_level(source: &str) -> Option<u8> {
    match crate::doc::breaks::Kind::of(source) {
        crate::doc::breaks::Kind::Heading(n) if !source.contains('\n') => Some(n),
        _ => None,
    }
}

/// The footnotes at the end of the file: a line in `line`, then the numbered notes at 13.5 in
/// `fg2`, in the text column, with the Find marks.
fn footnotes(notes: &crate::doc::document::Block, hits: super::find::Marks, style: &MarkdownStyle, fonts: super::markdown_style::Fonts, meta: Meta, column: f32) -> El {
    let size = super::theme::CODE_TEXT * fonts.body.size / super::theme::BODY_TEXT;
    let font = TextStyle { size, line_height: size * super::theme::BODY_LEADING, ..fonts.body };
    let body = rune_kit::Markdown::new(notes.source.to_string()).highlights(hits).selectable(true).render(&style.clone().body_font(font).text(meta.text));
    let block = col().w_full().px(ROW_PAD_X).py(ROW_PAD_Y).gap(FOOTNOTES_GAP).child(el().w_full().h(super::theme::HAIRLINE).fill(meta.line)).child(body);
    super::theme::column(col().key(crate::doc::footnotes::KEY).w_full(), column).self_center().child(block)
}

/// Between the line and the notes.
const FOOTNOTES_GAP: f32 = 12.0;

/// A callout's label: icon 14 in `fg2` and word 13/600 in `fg`, on the quote
/// line (2 pt, 12 before the text, like the kit's), without colors (DESIGN.md).
fn callout_head(kind: crate::doc::callout::Kind, meta: &Meta) -> El {
    let label = rune::text(kind.label()).font(TextStyle::new(UI_SMALL, 600.0)).color(meta.label);
    let head = rune::row().gap(CALLOUT_GAP).center_y().pl(QUOTE_PAD).pb(CALLOUT_GAP).child(rune::icon(callout_icon(kind), super::theme::ICON).color(meta.icon)).child(label);
    rune::row().key("callout").w_full().child(el().w(QUOTE_RULE).h_full().radius(QUOTE_RULE / 2.0).fill(meta.rule)).child(head)
}

/// The kit's quote line and the space before the text (Rune Kit's `QuoteLook`).
const QUOTE_RULE: f32 = 2.0;
const QUOTE_PAD: f32 = 12.0;
/// Between icon and label, and below the label.
const CALLOUT_GAP: f32 = 6.0;

/// A callout's icon, drawn like the kit's (viewBox 16, stroke 1, round joins).
fn callout_icon(kind: crate::doc::callout::Kind) -> &'static rune::Icon {
    use crate::doc::callout::Kind;
    match kind {
        Kind::Note => &NOTE_ICON,
        Kind::Tip => &TIP_ICON,
        Kind::Important => &IMPORTANT_ICON,
        Kind::Warning => &WARNING_ICON,
        Kind::Caution => &CAUTION_ICON,
    }
}

/// Note: an "i" in a circle.
static NOTE_ICON: rune::Icon = rune::Icon {
    name: "runemd-note",
    view: 16.0,
    body: r#"<circle cx="8" cy="8" r="6" stroke="currentColor"/><path d="M8 7.2V11" stroke="currentColor" stroke-linecap="round"/><circle cx="8" cy="5.1" r="0.6" fill="currentColor"/>"#,
};
/// Tip: a light bulb.
static TIP_ICON: rune::Icon = rune::Icon {
    name: "runemd-tip",
    view: 16.0,
    body: r#"<path d="M6 11.5C6 10 4 9 4 6.5a4 4 0 0 1 8 0C12 9 10 10 10 11.5Z" stroke="currentColor" stroke-linejoin="round"/><path d="M6.3 13.5H9.7" stroke="currentColor" stroke-linecap="round"/>"#,
};
/// Important: a speech bubble with "!".
static IMPORTANT_ICON: rune::Icon = rune::Icon {
    name: "runemd-important",
    view: 16.0,
    body: r#"<path d="M3.2 2.5H12.8a1.2 1.2 0 0 1 1.2 1.2V10a1.2 1.2 0 0 1-1.2 1.2H7L4.5 13.5V11.2H3.2A1.2 1.2 0 0 1 2 10V3.7a1.2 1.2 0 0 1 1.2-1.2Z" stroke="currentColor" stroke-linejoin="round"/><path d="M8 4.8V7.6" stroke="currentColor" stroke-linecap="round"/><circle cx="8" cy="9.3" r="0.6" fill="currentColor"/>"#,
};
/// Warning: a triangle with "!".
static WARNING_ICON: rune::Icon = rune::Icon {
    name: "runemd-warning",
    view: 16.0,
    body: r#"<path d="M7.13 2.5a1 1 0 0 1 1.74 0l5.5 9.75a1 1 0 0 1-.87 1.5H2.5a1 1 0 0 1-.87-1.5Z" stroke="currentColor" stroke-linejoin="round"/><path d="M8 6.2V9" stroke="currentColor" stroke-linecap="round"/><circle cx="8" cy="11.1" r="0.6" fill="currentColor"/>"#,
};
/// Caution: an octagon with "!".
static CAUTION_ICON: rune::Icon = rune::Icon {
    name: "runemd-caution",
    view: 16.0,
    body: r#"<path d="M5.5 2H10.5L14 5.5V10.5L10.5 14H5.5L2 10.5V5.5Z" stroke="currentColor" stroke-linejoin="round"/><path d="M8 5V8.4" stroke="currentColor" stroke-linecap="round"/><circle cx="8" cy="10.6" r="0.6" fill="currentColor"/>"#,
};

/// The row of the block with the caret: the caret pinned at the point, out of the flow, so the text
/// does not move.
fn caret_row(row: El, c: &Caret) -> El {
    match c.place {
        Some([x, y, h]) => {
            let bar = el().key("caret").pin(Some(x), Some(y), None, None).w(crate::fx::caret::WIDTH).h(h);
            row.child(crate::fx::caret::slot(bar, c.since, c.dark, c.reduced))
        }
        None => row,
    }
}

/// The narrowest row a block of kind `kind` has with the narrowest column (`COLUMN`
/// minus the margins): a heading takes all of it, a paragraph leaves room for two list levels,
/// a cell for three columns. Past that, a word breaks anywhere (`doc::breaks`).
fn line_limit(kind: crate::doc::breaks::Kind) -> f32 {
    use crate::doc::breaks::Kind;
    let line = super::theme::COLUMN - 2.0 * ROW_PAD_X;
    match kind {
        Kind::Heading(_) => line,
        Kind::Body => line - 64.0,
        Kind::Cell => line / 3.0,
    }
}

/// The font of a block of kind `kind`.
fn font_of(fonts: &super::markdown_style::Fonts, kind: crate::doc::breaks::Kind) -> TextStyle {
    use crate::doc::breaks::Kind;
    match kind {
        Kind::Heading(n @ 1..=6) => fonts.headings[n as usize - 1],
        Kind::Heading(_) | Kind::Body => fonts.body,
        Kind::Cell => fonts.table,
    }
}

#[cfg(test)]
mod tests {
    //! The reading anchor on a headless Rune runtime: same view as the app (`crate::view`),
    //! `Document::update` as reloading will call it.

    use std::path::Path;

    use rune::{Env, Runtime};
    use rune_kit::MarkdownStyle;

    use super::row_key;
    use crate::app::App as Doc;
    use crate::doc::document::Document;

    const VIEWPORT: (f32, f32) = (900.0, 1000.0);

    fn paragraph(n: usize) -> String {
        let words = ["line", "readers", "blocks", "scroll", "anchor", "frame", "texts", "columns"];
        (0..12 + (n * 7) % 60).map(|k| words[(n + k) % words.len()]).collect::<Vec<_>>().join(" ")
    }

    /// 120 blocks of different heights: headings every 10, long paragraphs from 12 to 71 words.
    fn sample() -> Vec<String> {
        (0..120).map(|n| if n % 10 == 0 { format!("## Section {n}") } else { format!("{n}. {}", paragraph(n)).replacen(". ", " ", 1) }).collect()
    }

    fn join(blocks: &[String]) -> String {
        blocks.join("\n\n") + "\n"
    }

    fn settle(rt: &mut Runtime<Doc>) {
        for _ in 0..4000 {
            if !rt.frame(1.0 / 120.0) { return; }
        }
        panic!("the view does not settle");
    }

    fn open(source: &str) -> Runtime<Doc> {
        open_doc(Document::from_source(Path::new("t.md"), source))
    }

    fn open_doc(document: Document) -> Runtime<Doc> {
        let mut state = Doc::bare(crate::index::Library::load(Vec::new(), None));
        state.set_style(MarkdownStyle::neutral(true));
        state.push_tab(document);
        let mut rt = Runtime::new(state, crate::view, VIEWPORT, Env::new(true, 2.0));
        rt.on_key = Some(crate::on_key);
        rt.frame(0.0);
        settle(&mut rt);
        rt
    }

    fn scroll_to(rt: &mut Runtime<Doc>, y: f32) {
        rt.update(|_, tx| tx.scroll_to_instant("doc", 0.0, y));
        settle(rt);
    }

    /// The first visible block (id) and its offset from the top edge of the view, in pt.
    fn anchor(rt: &Runtime<Doc>) -> (u64, f32) {
        let els = rt.elements();
        let top = els.iter().find(|e| e.key.as_deref() == Some("doc")).expect("doc view").rect[1];
        let s = &rt.state;
        let doc = s.active_document();
        let (key, y) = els.iter()
            .filter(|e| !e.exiting && e.rect[1] + e.rect[3] > top + 0.5)
            .filter_map(|e| { let k = e.key.as_deref()?; e.path.ends_with(&format!("rows/{k}")).then_some((k, e.rect[1])) })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .expect("no visible row");
        let i = (0..doc.blocks.len()).find(|&i| row_key(i, doc) == key).expect("key without a block");
        (doc.blocks[i].id, y - top)
    }

    fn source(rt: &Runtime<Doc>) -> Vec<String> {
        rt.state.active_document().blocks.iter().map(|b| b.source.to_string()).collect()
    }

    fn index_of(rt: &Runtime<Doc>, id: u64) -> usize {
        rt.state.active_document().blocks.iter().position(|b| b.id == id).unwrap()
    }

    fn height(rt: &Runtime<Doc>, id: u64) -> f32 {
        let key = row_key(index_of(rt, id), rt.state.active_document());
        rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key.as_str())).unwrap().rect[3]
    }

    fn edit(rt: &mut Runtime<Doc>, blocks: &[String]) {
        let src = join(blocks);
        rt.update(|s, _| { s.tab_mut().unwrap().doc.update(&src); });
        settle(rt);
    }

    /// Opens the sample and scrolls to the middle, with the top edge halfway through a block.
    fn reading() -> (Runtime<Doc>, (u64, f32)) {
        let mut rt = open(&join(&sample()));
        scroll_to(&mut rt, 6000.0);
        let a = anchor(&rt);
        assert!(a.1 < -1.0, "the top edge cuts the first block: {a:?}");
        (rt, a)
    }

    fn assert_still(case: &str, before: (u64, f32), after: (u64, f32)) {
        eprintln!("{case}: before {before:?}, after {after:?}, drift {:.2} pt", after.1 - before.1);
        assert_eq!(before.0, after.0, "{case}: the first visible block changes");
        assert!((before.1 - after.1).abs() <= 1.0, "{case}: offset {} → {}", before.1, after.1);
    }

    #[test]
    fn insert_five_paragraphs_at_top_keeps_reading_position() {
        let (mut rt, before) = reading();
        let mut blocks: Vec<String> = (0..5).map(|n| format!("New {n}: {}", paragraph(n + 3))).collect();
        blocks.extend(source(&rt));
        edit(&mut rt, &blocks);
        assert_still("insert at the top", before, anchor(&rt));
    }

    #[test]
    fn lengthening_the_top_block_keeps_reading_position() {
        let (mut rt, before) = reading();
        let mut blocks = source(&rt);
        let i = index_of(&rt, before.0);
        blocks[i] = format!("{} {}", blocks[i], paragraph(40).repeat(3));
        let h0 = height(&rt, before.0);
        edit(&mut rt, &blocks);
        assert!(height(&rt, before.0) > h0 + 40.0, "the block grows: {h0} → {}", height(&rt, before.0));
        assert_eq!(rt.state.active_document().blocks[i].id, before.0, "in-place edit: same id");
        assert_still("grow the top block", before, anchor(&rt));
    }

    #[test]
    fn lengthening_the_last_block_keeps_reading_position() {
        let (mut rt, before) = reading();
        let mut blocks = source(&rt);
        let last = blocks.len() - 1;
        blocks[last] = format!("{} {}", blocks[last], paragraph(41).repeat(4));
        edit(&mut rt, &blocks);
        assert_still("lengthens the last block", before, anchor(&rt));
    }

    /// The top block becomes two blocks: new ids, the anchor moves to the next block.
    #[test]
    fn a_vanished_top_block_hands_the_anchor_to_the_next() {
        let (mut rt, before) = reading();
        let mut blocks = source(&rt);
        let i = index_of(&rt, before.0);
        let next = rt.state.active_document().blocks[i + 1].id;
        let rows = rt.elements();
        let top = rows.iter().find(|e| e.key.as_deref() == Some("doc")).unwrap().rect[1];
        let next_key = row_key(i + 1, rt.state.active_document());
        let next_y = rows.iter().find(|e| e.key.as_deref() == Some(next_key.as_str())).unwrap().rect[1] - top;
        blocks.splice(i..=i, [format!("Split A {}", paragraph(5)), format!("Split B {}", paragraph(6))]);
        edit(&mut rt, &blocks);
        let doc = rt.state.active_document();
        assert!(doc.blocks.iter().all(|b| b.id != before.0), "the top block is gone");
        let j = index_of(&rt, next);
        let key = row_key(j, doc);
        let els = rt.elements();
        let top = els.iter().find(|e| e.key.as_deref() == Some("doc")).unwrap().rect[1];
        let y = els.iter().find(|e| e.key.as_deref() == Some(key.as_str())).unwrap().rect[1] - top;
        eprintln!("block gone: next one before {next_y:.2}, after {y:.2}");
        assert!((y - next_y).abs() <= 1.0, "the next block moves: {next_y} → {y}");
    }

    #[test]
    fn an_empty_file_opens() {
        let mut rt = open("");
        assert!(rt.state.active_document().blocks.is_empty());
        edit(&mut rt, &["# Now it is here".to_owned()]);
        assert_eq!(anchor(&rt).1.round(), 24.0, "the first block below the top margin");
    }

    /// 500 KB with no empty lines is a single block: the "insert at the top" case keeps the line
    /// inside that block.
    #[test]
    fn a_500_kb_file_without_blank_lines_keeps_reading_position() {
        let line = "A long line of text with no empty lines between one and the next, reader block scroll again.\n";
        let big = line.repeat(500 * 1024 / line.len());
        assert!(big.len() >= 500 * 1000 && !big.contains("\n\n"));
        let mut rt = open(&big);
        assert_eq!(rt.state.active_document().blocks.len(), 1);
        scroll_to(&mut rt, 20000.0);
        let before = anchor(&rt);
        assert!(before.1 < -1000.0, "inside the block: {before:?}");
        let blocks: Vec<String> = (0..5).map(|n| format!("New {n}: {}", paragraph(n))).chain([big.trim_end().to_owned()]).collect();
        edit(&mut rt, &blocks);
        assert_still("500 KB, insert at the top", before, anchor(&rt));
    }

    /// The fire on the runtime: 3 blocks added at the end while reading at the top turn it on
    /// (the last visible one comes from `on_visible_rows`), scrolling to the end turns it off.
    #[test]
    fn appended_blocks_light_the_fire_until_scrolled_into_view() {
        let mut rt = open(&join(&sample()));
        assert!(rt.state.tab().unwrap().visible_last != 0, "the view reported the last visible row");
        let mut blocks = sample();
        blocks.extend((0..3).map(|n| format!("New at the end {n}: {}", paragraph(n))));
        let src = join(&blocks);
        rt.update(|s, _| {
            let change = s.tab_mut().unwrap().doc.update(&src);
            s.tab_mut().unwrap().note_reload(&change, 1.0);
        });
        settle(&mut rt);
        assert_eq!(rt.state.tab().unwrap().below.count(), 3);
        assert_eq!(rt.state.tab().unwrap().fire_since, Some(1.0));
        assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("fire")), "fire slot");
        scroll_to(&mut rt, 1.0e9);
        assert_eq!(rt.state.tab().unwrap().below.count(), 0);
        assert_eq!(rt.state.tab().unwrap().fire_since, None);
        assert!(!fire_drawn(&rt), "label off");
        assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("fire") && !e.exiting), "the fire stays, at rest");
    }

    /// The fire lit by new text: the label "N new paragraphs below". The flame
    /// slot is always there (at rest, low).
    fn fire_drawn(rt: &Runtime<Doc>) -> bool {
        rt.elements().iter().any(|e| e.key.as_deref() == Some("fire-text") && !e.exiting)
    }

    /// Always on: while reading the fire slot is there even with no new text, with no label;
    /// in the source (Cmd+E) and with the fire turned off in the settings it is not.
    #[test]
    fn the_fire_rests_at_the_panel_bottom_without_new_text() {
        let mut rt = open(&join(&sample()));
        let slot = |rt: &Runtime<Doc>| rt.elements().iter().any(|e| e.key.as_deref() == Some("fire") && !e.exiting);
        assert!(slot(&rt), "fire slot at rest");
        assert!(!fire_drawn(&rt), "no label at rest");
        assert!(rt.state.effects_lit(), "the compositor runs the instances");
        rt.update(|s, _| s.settings.fire = crate::settings::FireMode::Off);
        settle(&mut rt);
        assert!(!slot(&rt) && !fire_drawn(&rt), "fire off: no slot and no label");
    }

    /// At rest, with the window active and without "reduce motion", the flames ask for a
    /// wake-up every `fire::REST_FRAME`; still flames do not.
    #[test]
    fn the_resting_fire_is_paced_by_wakes_only_while_it_moves() {
        let mut rt = open(&join(&sample()));
        assert!(rt.state.fire_rests(rt.clock));
        rt.update(|s, _| s.window_active = false);
        assert!(!rt.state.fire_rests(rt.clock), "window not active: still");
        rt.update(|s, _| {
            s.window_active = true;
            s.motion_env = Some(true);
        });
        settle(&mut rt);
        assert!(!rt.state.fire_rests(rt.clock), "reduce motion: still");
        rt.update(|s, _| {
            s.motion_env = Some(false);
            s.tabs[0].fire_since = Some(0.0);
        });
        settle(&mut rt);
        assert!(!rt.state.fire_rests(rt.clock), "lit: the display link drives it");
    }

    /// A short document, all in view: a block added at the end enters the view at once.
    /// No frame must draw the fire or the label, not even the one right after the
    /// reload (Rune reports the new visible rows only after layout).
    #[test]
    fn a_block_appended_in_view_never_draws_the_fire() {
        let blocks: Vec<String> = (0..3).map(|n| format!("Breve {n}")).collect();
        let mut rt = open(&join(&blocks));
        assert!(rt.state.tab().unwrap().visible_last != 0);
        let mut more = blocks.clone();
        more.push("New at the end, already in view".to_owned());
        let src = join(&more);
        rt.update(|s, _| {
            let change = s.tab_mut().unwrap().doc.update(&src);
            s.tab_mut().unwrap().note_reload(&change, 1.0);
        });
        for f in 0..30 {
            assert!(!fire_drawn(&rt), "fire drawn at frame {f}");
            assert_eq!(rt.state.tab().unwrap().fire_since, None, "fire lit at frame {f}");
            rt.frame(1.0 / 60.0);
        }
        assert_eq!(rt.state.tab().unwrap().below.count(), 0);
    }

    /// A block changed below the view (same key, no new rows) lights the fire.
    #[test]
    fn a_block_changed_below_the_view_lights_the_fire() {
        let mut rt = open(&join(&sample()));
        let mut blocks = sample();
        let last = blocks.len() - 1;
        blocks[last] = format!("{} changed", blocks[last]);
        let src = join(&blocks);
        rt.update(|s, _| {
            let change = s.tab_mut().unwrap().doc.update(&src);
            assert_eq!(change.changed.len(), 1);
            s.tab_mut().unwrap().note_reload(&change, 1.0);
        });
        settle(&mut rt);
        assert_eq!(rt.state.tab().unwrap().below.count(), 1);
        assert_eq!(rt.state.tab().unwrap().fire_since, Some(1.0));
        assert!(fire_drawn(&rt));
    }

    /// The rects of the document rows (key → rect), in order.
    fn row_rects(rt: &Runtime<Doc>) -> Vec<(String, [f32; 4])> {
        let doc = rt.state.active_document();
        let keys: Vec<String> = (0..doc.blocks.len()).map(|i| row_key(i, doc)).collect();
        rt.elements().iter().filter(|e| !e.exiting).filter_map(|e| { let k = e.key.as_deref()?; keys.iter().any(|x| x == k).then(|| (k.to_owned(), e.rect)) }).collect()
    }

    fn keyed(rt: &Runtime<Doc>, key: &str) -> Vec<[f32; 4]> {
        rt.elements().iter().filter(|e| !e.exiting && e.key.as_deref() == Some(key)).map(|e| e.rect).collect()
    }

    /// Caret and mark lit on an edit in the middle of the file (record 2026-11-15-caret-at-the-edit-point,
    /// constraints 1–3): the rows stay where they were, no bar at the bottom, the 2 pt caret sits in the
    /// changed row right after the changed word, and the mark sits in the left margin.
    #[test]
    fn caret_and_changed_mark_do_not_move_the_text() {
        let mut rt = open("# Title\n\nThe cat sleeps on the sofa all day long.\n\nEnd.\n");
        let before = row_rects(&rt);
        assert_eq!(before.len(), 3);
        assert!(keyed(&rt, "caret").is_empty() && keyed(&rt, "changed").is_empty());
        rt.update(|s, _| {
            let tab = s.tab_mut().unwrap();
            let old = tab.doc.blocks.clone();
            let change = tab.doc.update("# Title\n\nThe dog sleeps on the sofa all day long.\n\nEnd.\n");
            let at = crate::live::caret::locate(&old, &tab.doc.blocks, &change);
            tab.changed.mark(&change.changed, std::time::Instant::now());
            tab.caret_step(at.as_ref(), true, 0.0);
        });
        settle(&mut rt);
        assert_eq!(row_rects(&rt).iter().map(|r| r.1).collect::<Vec<_>>(), before.iter().map(|r| r.1).collect::<Vec<_>>(), "nothing moves");
        assert!(keyed(&rt, "arriving").is_empty(), "no bar at the bottom");
        let caret = keyed(&rt, "caret");
        assert_eq!(caret.len(), 1, "one caret");
        let (c, row) = (caret[0], before[1].1);
        assert_eq!(c[2], crate::fx::caret::WIDTH);
        assert!(c[1] > row[1] && c[1] + c[3] < row[1] + row[3], "in the changed row: {c:?} {row:?}");
        // After "The dog": right of the highlighted character, left of the middle of the row.
        let mark = keyed(&rt, &super::caret_key(rt.state.active_document().id));
        assert!(!mark.is_empty(), "the caret character has a rect");
        assert!((c[0] - (mark[0][0] + mark[0][2] + super::CARET_GAP)).abs() < 0.5, "right after the word: {c:?} {mark:?}");
        assert!(c[0] < row[0] + row[2] * 0.4, "after «The dog», not at the end: {c:?}");
        let mark = keyed(&rt, "changed");
        assert_eq!(mark.len(), 1, "only one marked block");
        assert_eq!(mark[0][2], crate::ui::theme::CHANGED_MARK_W);
        assert!(mark[0][0] > row[0] && mark[0][0] + mark[0][2] < row[0] + crate::ui::theme::ROW_PAD_X, "in the left margin");
    }

    /// The caret character without a rect (record 2026-11-15-caret-at-the-edit-point, constraint 2):
    /// here the edit falls in a link URL, which is not visible. The caret goes to the end of the changed
    /// block, after the last word, instead of disappearing.
    #[test]
    fn without_the_character_rect_the_caret_goes_to_the_end_of_the_block() {
        let text = |u: &str| format!("# Title\n\nSee [the long page with many words that surely wraps onto two lines in the document column](http://example.com/{u}) and then text that goes on for a while until the end.\n\nEnd.\n");
        let mut rt = open(&text("a"));
        rt.update(|s, _| {
            let tab = s.tab_mut().unwrap();
            let old = tab.doc.blocks.clone();
            let change = tab.doc.update(&text("b"));
            let at = crate::live::caret::locate(&old, &tab.doc.blocks, &change);
            tab.caret_step(at.as_ref(), true, 0.0);
        });
        settle(&mut rt);
        let id = rt.state.active_document().id;
        assert!(keyed(&rt, &super::caret_key(id)).is_empty(), "the character in the URL has no rect");
        let caret = keyed(&rt, "caret");
        assert_eq!(caret.len(), 1, "the caret shows anyway");
        let end = keyed(&rt, &super::caret_end_key(id));
        assert_eq!(end.len(), 1, "the last word of the block has a rect");
        let (c, e) = (caret[0], end[0]);
        assert!((c[0] - (e[0] + e[2] + super::CARET_GAP)).abs() < 0.5, "right after «end.»: {c:?} {e:?}");
        let row = row_rects(&rt)[1].1;
        assert!(c[1] > row[1] + row[3] * 0.5, "on the last line of the block: {c:?} {row:?}");
    }

    /// YAML frontmatter while reading: a metadata block (key `frontmatter`) with the text as
    /// it is, in `fg2`; without a closing line it stays Markdown.
    #[test]
    fn frontmatter_reads_as_one_muted_metadata_block() {
        let rt = open("---\nname: RuneMD\ndescription: Reader\n---\n\n# Title\n\nText.\n");
        let els = rt.elements();
        let meta: Vec<_> = els.iter().filter(|e| e.key.as_deref() == Some("frontmatter") && !e.exiting).collect();
        assert_eq!(meta.len(), 1, "one metadata block");
        let theme = crate::ui::theme::Theme::for_mode(true);
        let text = els.iter().find(|e| e.text.as_deref() == Some("name: RuneMD\ndescription: Reader")).expect("the YAML as it is, without the ---");
        assert_eq!(text.text_color, Some(theme.fg2));
        let rt = open("---\nname: RuneMD\n\nText.\n");
        assert!(!rt.elements().iter().any(|e| e.key.as_deref() == Some("frontmatter")), "no closing line: Markdown");
    }

    #[test]
    fn fire_label() {
        assert_eq!(super::fire_label(1), "1 new paragraph below");
        assert_eq!(super::fire_label(4), "4 new paragraphs below");
    }

    // --- selection and copy ---

    use rune::{Key, KeyEvent, Mods};

    fn cmd(rt: &mut Runtime<Doc>, c: char) {
        rt.key(KeyEvent { key: Key::Command(c), mods: Mods { cmd: true, logo: true, ..Mods::default() } });
        rt.frame(1.0 / 60.0);
    }

    // --- tabs ---

    /// Two tabs on the same sample: B scrolls halfway, switch to A (Cmd+1) and back to B
    /// (Ctrl+Tab). A starts from the top, B comes back where it was: the block that was at the top is back at the
    /// top (`App::restore_view`), without the part cut by the edge.
    #[test]
    fn each_tab_keeps_its_place() {
        let mut rt = open(&join(&sample()));
        rt.update(|s, _| s.push_tab(Document::from_source(Path::new("b.md"), &join(&sample()))));
        settle(&mut rt);
        scroll_to(&mut rt, 6000.0);
        let b = anchor(&rt);
        cmd(&mut rt, '1');
        assert_eq!(rt.state.active, 0);
        assert_eq!(anchor(&rt).1.round(), 24.0, "A from the top, from the first frame");
        settle(&mut rt);
        rt.key(KeyEvent { key: Key::Tab, mods: Mods { ctrl: true, ..Mods::default() } });
        // The first frame after the switch is already right, and so are the next ones: no jump.
        for f in 0..10 {
            rt.frame(1.0 / 60.0);
            assert_eq!(rt.state.active, 1, "Ctrl+Tab");
            let back = anchor(&rt);
            if f == 0 {
                eprintln!("tab B: before {b:?}, first frame after {back:?}");
            }
            assert_eq!(back.0, b.0, "frame {f}: B restarts from its first visible block");
            assert!(back.1.abs() <= 1.0, "frame {f}: at the top of the view: {back:?}");
        }
    }

    /// The fire of a background tab rises again (420 ms) when the tab comes to the front:
    /// its clock starts again from the switch, not from the reload back then.
    #[test]
    fn a_background_fire_rises_again_when_its_tab_comes_back() {
        let mut rt = open(&join(&sample()));
        rt.update(|s, _| s.push_tab(Document::from_source(Path::new("b.md"), "# b\n")));
        settle(&mut rt);
        rt.update(|s, _| s.tabs[0].fire_since = Some(0.0));
        for _ in 0..120 {
            rt.frame(1.0 / 60.0);
        }
        cmd(&mut rt, '1');
        let since = rt.state.tab().unwrap().fire_since.expect("fire on");
        assert!(since > 1.5, "clock of the switch, not of the reload: {since}");
    }

    /// Tabs are as wide as their name, with `TAB_PAD_X` before it (mockup `scheda`): a
    /// long name is not cut while the bar has room.
    #[test]
    fn tabs_are_as_wide_as_their_names() {
        let mut rt = open("# a\n");
        rt.update(|s, _| s.push_tab(Document::from_source(Path::new("a-very-long-file-name-for-one-tab.md"), "# b\n")));
        settle(&mut rt);
        let els = rt.elements();
        let rect = |key: &str| els.iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key}")).rect;
        let ids: Vec<u64> = rt.state.tabs.iter().map(|t| t.doc.id).collect();
        let (a, b) = (rect(&format!("t-{:x}", ids[0])), rect(&format!("t-{:x}", ids[1])));
        let (la, lb) = (rect(&format!("t-{:x}-label", ids[0])), rect(&format!("t-{:x}-label", ids[1])));
        let pad = crate::ui::theme::TAB_PAD_X;
        for (tab, label) in [(a, la), (b, lb)] {
            assert!((label[0] - tab[0] - pad).abs() <= 0.5, "text at {pad} from the edge: {tab:?} {label:?}");
            // After the name, the × (always in its place): 6 + 16 + 8 (record 2026-11-18-vscode-tabs).
            let tail = crate::ui::tabs::CLOSE_GAP + crate::ui::tabs::CLOSE + crate::ui::tabs::CLOSE_PAD;
            assert!((tab[0] + tab[2] - label[0] - label[2] - tail).abs() <= 0.5, "and the × at the end: {tab:?} {label:?}");
        }
        assert!(b[2] > a[2] + 100.0, "as wide as the name: {a:?} {b:?}");
        assert_eq!(a[3], crate::ui::theme::TAB_H);
    }

    /// The × to close: always on the front tab, on the others only on hover; the place is
    /// reserved, so the name does not move. Label "Close <name>"; a click closes like Cmd+W.
    #[test]
    fn the_close_button_shows_on_the_front_and_hovered_tab_and_closes() {
        let mut rt = open("# a\n");
        rt.update(|s, _| s.push_tab(Document::from_source(Path::new("b.md"), "# b\n")));
        settle(&mut rt);
        let ids: Vec<u64> = rt.state.tabs.iter().map(|t| t.doc.id).collect();
        let key = |i: usize, part: &str| format!("t-{:x}{part}", ids[i]);
        let el = |rt: &Runtime<Doc>, k: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(k) && !e.exiting).unwrap_or_else(|| panic!("{k}"));
        // b.md is in front (the last opened), t.md behind.
        assert_eq!(rt.state.active, 1);
        let close = el(&rt, &key(1, "-close"));
        assert_eq!((close.role, close.label.as_deref()), ("button", Some("Close b.md")));
        assert!(close.rect[2] >= 16.0 && close.rect[3] >= 16.0, "16 pt area: {:?}", close.rect);
        assert!(el(&rt, &key(1, "-close")).opacity > 0.99, "front: visible");
        assert!(el(&rt, &key(0, "-close")).opacity < 0.01, "behind: hidden");
        let label = el(&rt, &key(0, "-label")).rect;
        let tab = el(&rt, &key(0, "")).rect;
        rt.pointer_move(tab[0] + 8.0, tab[1] + tab[3] / 2.0);
        settle(&mut rt);
        assert!(el(&rt, &key(0, "-close")).opacity > 0.99, "on hover: visible");
        assert_eq!(el(&rt, &key(0, "-label")).rect, label, "the name does not move");
        let c = el(&rt, &key(0, "-close")).rect;
        assert!(c[0] + c[2] <= tab[0] + tab[2] + 0.5 && c[0] > label[0] + label[2] - 0.5, "at the end of the tab");
        let (x, y) = (c[0] + c[2] / 2.0, c[1] + c[3] / 2.0);
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        settle(&mut rt);
        assert_eq!(rt.state.tabs.len(), 1, "closed");
        assert_eq!(rt.state.tab().unwrap().doc.id, ids[1], "b.md stays");
    }

    /// The bar shows one tab per file, the front one with `sel` fill, and "⌘E
    /// source"; Cmd+W closes the front tab, and with no tabs leaves the key to Rune.
    #[test]
    fn the_tab_bar_shows_the_files_and_cmd_w_closes() {
        let mut rt = open("# a\n");
        rt.update(|s, _| s.push_tab(Document::from_source(Path::new("b.md"), "# b\n")));
        settle(&mut rt);
        let ids: Vec<String> = rt.state.tabs.iter().map(|t| format!("t-{:x}", t.doc.id)).collect();
        let els = rt.elements();
        for id in &ids {
            assert!(els.iter().any(|e| e.key.as_deref() == Some(id.as_str())), "tab {id}");
        }
        assert!(els.iter().any(|e| e.key.as_deref() == Some("source-key")), "⌘E source");
        assert!(els.iter().any(|e| e.text.as_deref() == Some(crate::ui::tabs::SOURCE_KEY)));
        cmd(&mut rt, 'w');
        settle(&mut rt);
        assert_eq!(rt.state.tabs.len(), 1);
        cmd(&mut rt, 'w');
        settle(&mut rt);
        assert!(rt.state.tabs.is_empty(), "no tabs, no panic");
        let mut tx = rune::Tx::default();
        let key = KeyEvent { key: Key::Command('w'), mods: Mods { cmd: true, logo: true, ..Mods::default() } };
        assert!(!crate::on_key(&mut rt.state, &key, &mut tx), "Cmd+W with no tabs stays with Rune");
    }

    /// Cmd+A then Cmd+C on a runtime with a test clipboard; returns the copied text.
    fn copy_all(rt: &mut Runtime<Doc>) -> String {
        let clipboard = rune::clipboard::memory();
        rt.use_clipboard(&clipboard);
        // a click inside the document gives focus to the selection, as in the app
        let doc = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("doc")).unwrap();
        let (x, y) = (doc.rect[0] + doc.rect[2] / 2.0, doc.rect[1] + 12.0);
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        cmd(rt, 'a');
        cmd(rt, 'c');
        clipboard.text().map(|t| t.to_string()).unwrap_or_default()
    }

    #[test]
    fn copy_all_gives_the_markdown_source() {
        let src = join(&sample());
        let mut rt = open(&src);
        // the sample is many screens long: most rows are not in layout
        assert!(rt.elements().iter().filter(|e| e.path.contains("rows/")).count() < 120 * 4);
        let copied = copy_all(&mut rt);
        assert_eq!(copied.trim_end(), src.trim_end());
    }

    #[test]
    fn copy_all_after_scrolling_gives_the_markdown_source() {
        let src = join(&sample());
        let mut rt = open(&src);
        scroll_to(&mut rt, 6000.0);
        let copied = copy_all(&mut rt);
        assert_eq!(copied.trim_end(), src.trim_end());
    }

    #[test]
    fn copy_all_of_the_big_fixture_gives_the_file() {
        let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/big.md")).unwrap();
        let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/big.md"));
        let mut rt = open_doc(Document::load(path).unwrap());
        let blocks = rt.state.active_document().blocks.len();
        let copied = copy_all(&mut rt);
        eprintln!("big.md: {blocks} blocks, {} bytes copied of {}", copied.len(), src.len());
        assert_eq!(copied.trim_end(), src.trim_end());
    }

    #[test]
    fn copy_all_of_a_file_over_4_mib_gives_the_file() {
        let dir = std::env::temp_dir().join(format!("runemd-copy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("huge.md");
        let src = join(&sample()).repeat(5 * 1024 * 1024 / join(&sample()).len() + 1);
        assert!(src.len() > 4 * 1024 * 1024);
        std::fs::write(&path, &src).unwrap();
        let document = Document::load(&path).unwrap();
        let mut rt = open_doc(document);
        let copied = copy_all(&mut rt);
        std::fs::remove_dir_all(&dir).ok();
        eprintln!("huge: {} bytes copied of {}", copied.len(), src.len());
        assert_eq!(copied.trim_end(), src.trim_end());
    }

    /// Drags from the centre of row `from` to the centre of row `to` and copies with Cmd+C.
    fn drag_copy(rt: &mut Runtime<Doc>, from: usize, to: usize) -> String {
        let clipboard = rune::clipboard::memory();
        rt.use_clipboard(&clipboard);
        let doc = rt.state.active_document();
        let rect = |i: usize| {
            let key = row_key(i, doc);
            rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key.as_str())).unwrap().rect
        };
        let (a, c) = (rect(from), rect(to));
        let start = (a[0] + 60.0, a[1] + a[3] / 2.0);
        let end = (c[0] + 120.0, c[1] + c[3] / 2.0);
        rt.pointer_move(start.0, start.1);
        rt.pointer_down(start.0, start.1);
        rt.frame(1.0 / 60.0);
        for k in 1..=10 {
            let t = k as f32 / 10.0;
            rt.pointer_move(start.0 + (end.0 - start.0) * t, start.1 + (end.1 - start.1) * t);
            rt.frame(1.0 / 60.0);
        }
        rt.pointer_up(end.0, end.1);
        rt.frame(1.0 / 60.0);
        cmd(rt, 'c');
        clipboard.text().map(|t| t.to_string()).unwrap_or_default()
    }

    #[test]
    fn copy_of_a_drag_over_three_blocks_gives_those_three() {
        let mut rt = open(&join(&sample()));
        let blocks = source(&rt);
        assert_eq!(drag_copy(&mut rt, 2, 4), blocks[2..=4].join("\n\n"));
    }

    #[test]
    fn copy_of_a_drag_from_block_0_on_a_file_over_4_mib_gives_two_blocks() {
        let dir = std::env::temp_dir().join(format!("runemd-drag-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("huge.md");
        let one = join(&sample());
        std::fs::write(&path, one.repeat(5 * 1024 * 1024 / one.len() + 1)).unwrap();
        let mut rt = open_doc(Document::load(&path).unwrap());
        let blocks = source(&rt);
        let copied = drag_copy(&mut rt, 0, 1);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(copied, blocks[0..=1].join("\n\n"));
    }

    #[test]
    fn copy_all_of_a_cjk_file_over_4_mib_with_textless_blocks_gives_the_file() {
        let dir = std::env::temp_dir().join(format!("runemd-cjk-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cjk.md");
        let unit: String = (0..40).map(|n| format!("段落{n}は日本語のテキストです。読み取りとスクロールの確認に使います。\n\n---\n\n")).collect();
        let src = unit.repeat(5 * 1024 * 1024 / unit.len() + 1);
        std::fs::write(&path, &src).unwrap();
        let mut rt = open_doc(Document::load(&path).unwrap());
        let copied = copy_all(&mut rt);
        std::fs::remove_dir_all(&dir).ok();
        eprintln!("cjk: {} bytes copied of {}", copied.len(), src.len());
        assert_eq!(copied.trim_end(), src.trim_end());
    }

    /// Cmd+A, then an agent rewrites the file (reload), then Cmd+C: the new file is copied.
    #[test]
    fn copy_after_cmd_a_and_a_reload_gives_the_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("live.md");
        let before = join(&sample());
        std::fs::write(&path, &before).unwrap();
        let mut rt = open_doc(Document::load(&path).unwrap());
        let clipboard = rune::clipboard::memory();
        rt.use_clipboard(&clipboard);
        let doc = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("doc")).unwrap();
        let (x, y) = (doc.rect[0] + doc.rect[2] / 2.0, doc.rect[1] + 12.0);
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        cmd(&mut rt, 'a');
        let after = format!("New at the top: {}\n\n{before}New at the end: {}\n", paragraph(3), paragraph(4));
        std::fs::write(&path, &after).unwrap();
        rt.update(|s, _| {
            let own = crate::live::reload::OwnWrites::default();
            let doc = &mut s.tab_mut().unwrap().doc;
            let read = crate::live::reload::read(&path, &doc.snapshot()).unwrap();
            assert!(matches!(crate::live::reload::commit(doc, read, &own), crate::live::reload::Commit::Applied(_)));
        });
        settle(&mut rt);
        cmd(&mut rt, 'c');
        let copied = clipboard.text().map(|t| t.to_string()).unwrap_or_default();
        assert_eq!(copied.trim_end(), after.trim_end(), "the new file, not the old one nor the selection");
    }

    /// Cmd+A, scroll to the end, Shift+click in the second-to-last block (press and release in the
    /// same frame, like a fast click: the selection never gets to `dragging`), Cmd+C.
    fn cmd_a_then_shift_click_second_to_last(rt: &mut Runtime<Doc>) -> String {
        let n = rt.state.active_document().blocks.len();
        let clipboard = rune::clipboard::memory();
        rt.use_clipboard(&clipboard);
        let doc = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("doc")).unwrap();
        let (x, y) = (doc.rect[0] + doc.rect[2] / 2.0, doc.rect[1] + 12.0);
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        cmd(rt, 'a');
        scroll_to(rt, 1.0e9);
        let key = row_key(n - 2, rt.state.active_document());
        let r = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key.as_str())).expect("second-to-last block in view").rect;
        let (x, y) = (r[0] + 80.0, r[1] + r[3] / 2.0);
        rt.physical_input(rune::PhysicalInput::Modifiers(Mods { shift: true, ..Mods::default() }));
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        rt.physical_input(rune::PhysicalInput::Modifiers(Mods::default()));
        cmd(rt, 'c');
        clipboard.text().map(|t| t.to_string()).unwrap_or_default()
    }

    /// The cut removes less than 0.1 % of the file, but it is no longer "all": Cmd+C copies the blocks
    /// up to the second-to-last.
    #[test]
    fn a_shift_trim_after_cmd_a_copies_the_trimmed_blocks_not_the_file() {
        let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/big.md"));
        let src = std::fs::read_to_string(path).unwrap();
        let mut rt = open_doc(Document::load(path).unwrap());
        let blocks = source(&rt);
        let n = blocks.len();
        let trimmed = blocks[n - 2].len() + blocks[n - 1].len();
        assert!(trimmed * 1000 < src.len(), "the cut is within the old tolerance: {trimmed} of {}", src.len());
        let copied = cmd_a_then_shift_click_second_to_last(&mut rt);
        assert_ne!(copied.trim_end(), src.trim_end(), "copied the whole file after the cut");
        assert_eq!(copied, blocks[..=n - 2].join("\n\n"));
    }

    /// Leftover case, pinned as it is: past Rune's copy cap the last end is not
    /// compared, so a cut at the end after Cmd+A stays "all" and copies the whole file.
    #[test]
    fn a_shift_trim_after_cmd_a_over_the_copy_cap_still_copies_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("huge.md");
        let src = join(&sample()).repeat(5 * 1024 * 1024 / join(&sample()).len() + 1);
        std::fs::write(&path, &src).unwrap();
        let mut rt = open_doc(Document::load(&path).unwrap());
        let copied = cmd_a_then_shift_click_second_to_last(&mut rt);
        assert_eq!(copied.trim_end(), src.trim_end(), "known behaviour: the whole file");
    }

    #[test]
    fn a_drag_after_cmd_a_copies_the_dragged_blocks_not_the_file() {
        let mut rt = open(&join(&sample()));
        let blocks = source(&rt);
        cmd(&mut rt, 'a');
        assert_eq!(drag_copy(&mut rt, 2, 4), blocks[2..=4].join("\n\n"));
    }

    #[test]
    fn a_drag_after_a_cmd_a_that_selected_nothing_copies_the_dragged_blocks() {
        let mut rt = open(&join(&sample()));
        let blocks = source(&rt);
        // Cmd+Shift+A is an app key: Rune does not select, the view must not keep waiting.
        rt.key(KeyEvent { key: Key::Command('a'), mods: Mods { cmd: true, logo: true, shift: true, ..Mods::default() } });
        rt.frame(1.0 / 60.0);
        assert_eq!(drag_copy(&mut rt, 2, 3), blocks[2..=3].join("\n\n"));
    }

    /// The reader with the real app style (`markdown_style::style`), in the `dark` theme.
    fn open_themed(source: &str, dark: bool) -> Runtime<Doc> {
        open_sized(source, dark, VIEWPORT)
    }

    /// [`open_themed`] in a window of `size`.
    fn open_sized(source: &str, dark: bool, size: (f32, f32)) -> Runtime<Doc> {
        let mut state = Doc::bare(crate::index::Library::load(Vec::new(), None));
        state.set_style(crate::ui::markdown_style::style(&crate::ui::theme::Theme::for_mode(dark), crate::ui::theme::BODY_TEXT));
        state.push_tab(Document::from_source(Path::new("t.md"), source));
        let mut rt = Runtime::new(state, crate::view, size, Env::new(dark, 2.0));
        rt.frame(0.0);
        settle(&mut rt);
        rt
    }

    fn figures(rt: &Runtime<Doc>) -> Vec<String> {
        rt.elements().into_iter().filter(|e| e.role == "figure").filter_map(|e| e.label).collect()
    }

    fn has_source_block(rt: &Runtime<Doc>) -> bool {
        rt.elements().iter().any(|e| e.key.as_deref() == Some("source"))
    }

    /// The ```mermaid blocks of `fixtures/mermaid.md` are drawn (a flowchart and a
    /// sequence, in both themes); the invalid one stays a code block.
    #[test]
    fn mermaid_fences_draw_as_diagrams_and_an_unknown_one_stays_code() {
        let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/mermaid.md")).unwrap();
        for dark in [true, false] {
            let rt = open_themed(&src, dark);
            assert_eq!(figures(&rt), ["Flowchart", "Sequence diagram"], "dark={dark}");
            assert!(has_source_block(&rt), "the invalid block stays code, dark={dark}");
        }
    }

    /// Empty, broken or huge sources do not panic and do not block the view: they are either
    /// drawn, or stay the code block they were.
    #[test]
    fn broken_and_huge_mermaid_never_panic() {
        let huge_graph = format!("flowchart TD\n{}", (0..3000).map(|i| format!("  N{i} --> N{}\n", i + 1)).collect::<String>());
        let big_but_drawable = format!("flowchart TD\n{}", (0..70).map(|i| format!("  N{i} --> N{}\n  N{i} --> M{i}\n", i + 1)).collect::<String>());
        let sources = [
            String::new(),
            big_but_drawable,
            "graph".to_owned(),
            "flowchart TD\n  A -->".to_owned(),
            "sequenceDiagram\n  A->>".to_owned(),
            "pie\n  \"a\" : x".to_owned(),
            "flowchart LR\n  A[\n".to_owned(),
            "\u{0}\u{1f600}".to_owned(),
            huge_graph,
        ];
        for body in sources {
            let doc = format!("# t\n\n```mermaid\n{body}\n```\n\nfine\n");
            let t = std::time::Instant::now();
            let rt = open_themed(&doc, true);
            let drawn = !figures(&rt).is_empty();
            assert!(drawn || has_source_block(&rt), "neither drawing nor code for {:?}", body.chars().take(30).collect::<String>());
            println!("mermaid {:>6} byte: {} in {:?}", body.len(), if drawn { "disegnato" } else { "codice" }, t.elapsed());
        }
    }

    /// Two tabs with the same diagram do not step on each other: each has its own, and when switching tabs
    /// the diagram stays the one of the tab.
    #[test]
    fn the_same_diagram_in_two_tabs_draws_in_each() {
        let one = "```mermaid\nflowchart LR\n  A --> B\n```\n";
        let mut rt = open_themed(one, true);
        rt.update(|s, _| s.push_tab(Document::from_source(Path::new("b.md"), one)));
        settle(&mut rt);
        assert_eq!(figures(&rt), ["Flowchart"]);
        cmd(&mut rt, 'w');
        settle(&mut rt);
        assert_eq!(figures(&rt), ["Flowchart"]);
    }

    /// GitHub callouts (`> [!NOTE]` …) have the label in words and do not show the marker.
    #[test]
    fn callouts_show_a_label_not_the_marker() {
        let src = "> [!NOTE]\n> A note.\n\n> [!TIP]\n> A tip.\n\n> [!IMPORTANT]\n> Important.\n\n> [!warning]\n> A warning.\n\n> [!CAUTION]\n> A risk.\n";
        for dark in [true, false] {
            let rt = open_themed(src, dark);
            let texts: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).collect();
            assert!(!texts.iter().any(|t| t.contains("[!")), "the marker shows: {texts:?}");
            for label in ["Note", "Tip", "Important", "Warning", "Caution"] {
                assert!(texts.iter().any(|t| t == label), "missing {label}: {texts:?}");
            }
            assert!(texts.iter().any(|t| t.contains("A warning.")), "{texts:?}");
        }
    }

    /// Footnotes: the number raised in the text, the notes at the end of the file below a
    /// rule, not where they are written.
    #[test]
    fn footnotes_show_a_number_and_go_to_the_end() {
        let src = "A note[^1] here.\n\n[^1]: The note text.\n\nAfter the note.\n";
        let rt = open_themed(src, true);
        let els = rt.elements();
        let texts: Vec<String> = els.iter().filter_map(|e| e.text.as_deref()).map(|t| t.replace(['\u{2066}', '\u{2069}'], "")).collect();
        assert!(!texts.iter().any(|t| t.contains("[^")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("A note¹ here.")), "{texts:?}");
        let y = |needle: &str| els.iter().find(|e| e.text.as_deref().is_some_and(|t| t.replace(['\u{2066}', '\u{2069}'], "").contains(needle))).map(|e| e.rect[1]).unwrap_or_else(|| panic!("missing {needle}: {texts:?}"));
        assert!(y("The note text.") > y("After the note."), "the note goes to the end");
        assert!(els.iter().any(|e| e.key.as_deref() == Some("footnotes")));
    }

    /// Headings 3–6: 17 / 15.5 / 14 / 13, the color steps down (`body`, `body`, `fg2`, `fg3`); 4–6 are not
    /// the same as 3.
    #[test]
    fn headings_three_to_six_step_down() {
        let t = crate::ui::theme::Theme::for_mode(true);
        let rt = open_themed("### T3\n\n#### T4\n\n##### T5\n\n###### T6\n", true);
        let els = rt.elements();
        let h = |n: &str| els.iter().find(|e| e.text.as_deref() == Some(n)).map(|e| (e.rect[3], e.text_color)).unwrap_or_else(|| panic!("missing {n}"));
        let [h3, h4, h5, h6] = ["T3", "T4", "T5", "T6"].map(h);
        assert!(h3.0 > h4.0 && h4.0 > h5.0 && h5.0 > h6.0, "heights {h3:?} {h4:?} {h5:?} {h6:?}");
        assert_eq!([h3.1, h4.1, h5.1, h6.1], [Some(t.body), Some(t.body), Some(t.fg2), Some(t.fg3)]);
    }

    /// Paths (record 2026-11-16-paths-name-and-link): without backticks, in monospace; one that
    /// exists is a link that opens the file at the line; one that does not exist is not; in code, nothing.
    #[test]
    fn paths_show_as_links_that_open_the_file_at_the_line() {
        let t = std::env::temp_dir().join(format!("runemd-paths-view-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        std::fs::create_dir_all(t.join("src/live")).unwrap();
        std::fs::create_dir_all(t.join(".git")).unwrap();
        let target: String = (1..=40).map(|n| format!("Paragraph {n}.\n\n")).collect();
        std::fs::write(t.join("src/live/below.rs"), &target).unwrap();
        let src = "Vedi src/live/below.rs:31, `src/live/below.rs` e src/old/gone.rs.\n\n```\ncat src/live/below.rs\n```\n";
        let doc_path = t.join("doc.md");
        std::fs::write(&doc_path, src).unwrap();
        let mut state = Doc::bare(crate::index::Library::load(Vec::new(), None));
        state.set_style(crate::ui::markdown_style::style(&crate::ui::theme::Theme::for_mode(true), crate::ui::theme::BODY_TEXT));
        state.open_paths(vec![doc_path.clone()]);
        let mut rt = Runtime::new(state, crate::view, VIEWPORT, Env::new(true, 2.0));
        rt.frame(0.0);
        settle(&mut rt);
        let texts: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).map(|x| x.replace(['\u{200B}', '\u{2066}', '\u{2069}', '\u{202F}'], "")).collect();
        assert!(texts.iter().any(|x| x.contains("Vedi src/live/below.rs:31, src/live/below.rs e src/old/gone.rs.")), "{texts:?}");
        assert!(texts.iter().any(|x| x.contains("cat src/live/below.rs")), "the code stays: {texts:?}");
        let url = crate::doc::paths::open_target(&t.join("src/live/below.rs").canonicalize().unwrap(), Some(31));
        rt.update(|s, tx| s.open_link(&url, tx));
        settle(&mut rt);
        let tab = rt.state.tab().unwrap();
        assert!(tab.doc.path.ends_with("src/live/below.rs"), "{:?}", tab.doc.path);
        let texts: Vec<String> = rt.elements().into_iter().filter(|e| e.visible[3] > 0.0).filter_map(|e| e.text).collect();
        assert!(texts.iter().any(|x| x == "Paragraph 16."), "line 31 is in view: {texts:?}");
        let _ = std::fs::remove_dir_all(&t);
    }

    /// A path that does not exist is not a link (constraint 3 of the paths record): no hand,
    /// no click; it stays in monospace with the name highlighted.
    #[test]
    fn a_missing_path_is_not_a_link() {
        for src in ["See src/old/gone.rs:3 here.\n", "See src/old/gone.rs and `cargo test`.\n"] {
            let rt = open_themed(src, true);
            let els = rt.elements();
            let links: Vec<_> = els.iter().filter(|e| e.role == "link").map(|e| e.key.clone()).collect();
            assert!(links.is_empty(), "{src:?}: {links:?}");
            let texts: Vec<String> = els.iter().filter_map(|e| e.text.clone()).map(|x| x.replace(['\u{200B}', '\u{2066}', '\u{2069}', '\u{202F}'], "")).collect();
            assert!(texts.iter().any(|x| x.contains("See src/old/gone.rs")), "{texts:?}");
        }
    }

    /// The log (fixtures/registro.md, from the start of .superpowers/sdd/plan/progress.md) is
    /// read by lines: each line of the file is a row (constraint 2 of the line-breaks record).
    #[test]
    fn a_register_reads_as_lines() {
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/registro.md")).unwrap();
        let rt = open_sized(&source, true, NARROW);
        let texts: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).map(|x| x.replace('\u{200B}', "")).collect();
        let reg = texts.iter().find(|x| x.starts_with("Task 1: BASE root")).unwrap_or_else(|| panic!("{texts:?}"));
        assert!(reg.contains("Implementer a382e1858b685d390.\nRuling: keychain"), "{reg:?}");
        assert!(reg.contains("review clean)\nTask 2: BASE 0095282."), "{reg:?}");
        let preamble = texts.iter().find(|x| x.starts_with("Spec: docs/spec.md")).unwrap();
        assert!(preamble.contains("push).\nRuling: lavoro"), "two lines, both labelled: {preamble:?}");
    }

    /// A window where the text column is narrower than [`super::TWO_COLUMNS`].
    const NARROW: (f32, f32) = (640.0, 1000.0);

    /// With a wide column the log goes on two columns (constraint 4 of the logs record):
    /// labels without `:` on the left, all at the same x and at most 10 em wide, text on the
    /// right, aligned; a row without a label sits in the text column. Narrow, one column.
    #[test]
    fn a_wide_register_shows_two_columns() {
        let source = "Task 2: BASE 0095282. Implementer abd1.\nRuling: task 2 commit.\nmeasured again after the screen was unlocked: 118 fps.\nTask 2: minor (deferred): TreeRing.\nTask 2: complete (commits 0095282..f04c761)\n";
        let rt = open_themed(source, false);
        let els = rt.elements();
        let rows = els.iter().find(|e| e.key.as_deref() == Some("rows")).unwrap().rect;
        assert!(rows[2] - 2.0 * crate::ui::theme::ROW_PAD_X >= super::TWO_COLUMNS, "column {rows:?}");
        let at = |s: &str| els.iter().find(|e| e.text.as_deref().map(|t| t.replace(['\u{2066}', '\u{2069}'], "")).as_deref() == Some(s)).unwrap_or_else(|| panic!("missing {s:?}: {:?}", els.iter().filter_map(|e| e.text.clone()).collect::<Vec<_>>())).rect;
        let labels = ["Task 2", "Ruling", "Task 2: minor (deferred)"].map(at);
        let texts = ["BASE 0095282. Implementer abd1.", "task 2 commit.", "measured again after the screen was unlocked: 118 fps.", "TreeRing."].map(at);
        assert!(labels.iter().all(|l| l[0] == labels[0][0]), "labels at the same x: {labels:?}");
        assert!(texts.iter().all(|t| t[0] == texts[0][0] && t[0] > labels[0][0] + 40.0), "text aligned on the right: {texts:?}");
        let em = crate::ui::markdown_style::fonts(crate::fonts::Typeface::of("Inter", crate::ui::theme::BODY_TEXT)).body.size;
        assert!(texts[0][0] - labels[0][0] <= 10.0 * em + super::LABEL_GAP + 1.0, "at most 10 em");
        assert!((labels[0][1] - texts[0][1]).abs() < 1.0 && (labels[1][1] - texts[1][1]).abs() < 1.0, "rows aligned");
        let narrow = open_sized(source, false, NARROW);
        let one: Vec<String> = narrow.elements().into_iter().filter_map(|e| e.text).filter(|t| t.starts_with("Task 2: BASE")).collect();
        assert_eq!(one.len(), 1, "one column: {one:?}");
        assert!(one[0].contains("abd1.\nRuling: task 2 commit"));
    }

    /// DESIGN.md (paragraphs wrapped by hand, no log) comes out the same as it was without
    /// label blocks: no block is one, and the view has the same texts in the same places
    /// (constraint 5 of the line-breaks record).
    #[test]
    fn design_md_renders_as_before() {
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/DESIGN.md")).unwrap();
        let d = Document::from_source(Path::new("DESIGN.md"), &source);
        let labeled: Vec<&str> = d.blocks.iter().map(|b| &*b.source).filter(|b| crate::doc::labels::block(b).is_some() || crate::doc::labels::task_head(b).is_some()).collect();
        assert!(labeled.is_empty(), "{labeled:?}");
        let layout = |off: bool| {
            crate::doc::labels::OFF.with(|o| o.set(off));
            let mut rt = open_themed(&source, false);
            let mut seen = Vec::new();
            for step in 0..6 {
                rt.update(|_, tx| tx.scroll_to_instant("doc", 0.0, step as f32 * 2400.0));
                settle(&mut rt);
                seen.extend(rt.elements().into_iter().filter(|e| e.text.is_some()).map(|e| (e.text, e.rect.map(|v| (v * 2.0).round() as i32))));
            }
            crate::doc::labels::OFF.with(|o| o.set(false));
            seen
        };
        let before = layout(true);
        assert!(before.len() > 100, "texts: {}", before.len());
        assert_eq!(layout(false), before);
    }

    /// A `Task N` heading (constraint 3 of the logs record): the number in `fg3`, the heading without
    /// `Task N:`, and on the right the progress from the task checkboxes.
    #[test]
    fn task_headings_show_number_and_progress() {
        for dark in [true, false] {
            let t = crate::ui::theme::Theme::for_mode(dark);
            let rt = open_themed("### Task 12: Fire\n\n- [x] one\n- [x] two\n- [ ] three\n- [ ] four\n\n### Task 13: Dither\n\nText.\n", dark);
            let els = rt.elements();
            let find = |s: &str| els.iter().find(|e| e.text.as_deref() == Some(s)).unwrap_or_else(|| panic!("missing {s:?}")).clone();
            let (number, title, done) = (find("12"), find("Fire"), find("2 of 4"));
            assert_eq!(number.text_color, Some(t.fg3));
            assert_eq!(done.text_color, Some(t.fg3));
            assert!(number.rect[0] < title.rect[0] && title.rect[0] < done.rect[0], "{number:?} {title:?} {done:?}");
            assert!(find("Dither").rect[1] > done.rect[1]);
            assert!(!els.iter().any(|e| e.text.as_deref().is_some_and(|x| x.contains("Task 1") || x.contains("of 0"))), "no checkboxes, no progress");
        }
    }

    /// "2 of 4" sits on the heading baseline (mockup t-sezioni), not lower: with the
    /// row as tall as the heading row, the baseline is at mid-row plus `BASELINE_EM` per size.
    #[test]
    fn task_progress_sits_on_the_heading_baseline() {
        let rt = open_themed("### Task 12: Fire\n\n- [x] one\n- [ ] two\n", false);
        let els = rt.elements();
        let find = |s: &str| els.iter().find(|e| e.text.as_deref() == Some(s)).unwrap_or_else(|| panic!("missing {s:?}")).rect;
        let (title, done) = (find("Fire"), find("1 of 2"));
        let h3 = crate::ui::markdown_style::fonts(crate::fonts::Typeface { family: rune::Family::Sans, size: crate::ui::theme::BODY_TEXT }).headings[2];
        let small = crate::ui::sidebar::small_font();
        let title_base = title[1] + h3.line_height / 2.0 + super::BASELINE_EM * h3.size;
        let done_base = done[1] + done[3] - h3.line_height / 2.0 + super::BASELINE_EM * small.size;
        assert!((done_base - title_base).abs() < 0.5, "heading baseline {title_base}, \"1 of 2\" baseline {done_base} ({title:?} {done:?})");
    }

    /// The view link `key`: a click at its centre.
    fn click_link(rt: &mut Runtime<Doc>, key: &str) {
        let r = rt.elements().into_iter().find(|e| e.role == "link" && e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("missing link {key}")).rect;
        let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.pointer_move(x, y);
        rt.frame(1.0 / 120.0);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        settle(rt);
    }

    /// In a paragraph with a path, and outside it, the other links work: a URL opens in the
    /// browser, `#title` goes to the heading.
    #[test]
    fn links_beside_a_path_still_open() {
        let filler: String = (1..=40).map(|n| format!("Paragraph {n}.\n\n")).collect();
        for with_path in [true, false] {
            let path = if with_path { " e src/old/gone.rs" } else { "" };
            let src = format!("See [the site](https://example.com/a){path} or [the end](#the-end).\n\n{filler}## The end\n\nLast.\n");
            let mut rt = open_themed(&src, true);
            click_link(&mut rt, "https://example.com/a");
            assert_eq!(rt.take_url_requests(), ["https://example.com/a"], "with path: {with_path}");
            click_link(&mut rt, "#the-end");
            let shown: Vec<String> = rt.elements().into_iter().filter(|e| e.visible[3] > 0.0).filter_map(|e| e.text).collect();
            assert!(shown.iter().any(|x| x == "The end"), "with path: {with_path}: {shown:?}");
            assert!(!shown.iter().any(|x| x == "Paragraph 1."), "the heading at the top, with path: {with_path}");
        }
    }

    /// Task list checkboxes are the ones from Settings: off is empty (`fg3` border),
    /// lit, full in `fg2`.
    #[test]
    fn task_checkboxes_look_like_the_settings_one() {
        for dark in [true, false] {
            let t = crate::ui::theme::Theme::for_mode(dark);
            let rt = open_themed("- [ ] to do\n- [x] done\n", dark);
            let els = rt.elements();
            let control = |label: &str| {
                let row = els.iter().position(|e| e.role == "checkbox" && e.label.as_deref() == Some(label)).unwrap_or_else(|| panic!("missing {label}"));
                els.iter().find(|e| e.parent == Some(row) && e.key.as_deref() == Some("control")).unwrap().clone()
            };
            let (off, on) = (control("to do"), control("done"));
            assert_eq!(on.fill, t.fg2, "lit, dark={dark}");
            assert_eq!(off.fill.a, 0.0, "off is empty, dark={dark}");
            assert_eq!(on.rect[2], crate::ui::theme::ICON);
        }
    }

    /// Long words (fixtures/long-title.md: path-like heading, paragraph, hash without
    /// separators, list, cells) wrap: no line goes out of the column. Each text is
    /// inside the rows and its widest unbreakable part fits.
    #[test]
    fn long_words_never_overflow_the_column() {
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/long-title.md")).unwrap();
        let mut rt = open_themed(&source, true);
        let els = rt.elements();
        let rows = els.iter().find(|e| e.key.as_deref() == Some("rows")).unwrap().rect;
        let right = rows[0] + rows[2] - crate::ui::theme::ROW_PAD_X;
        let fonts = crate::ui::markdown_style::fonts(crate::fonts::Typeface::of("Inter", crate::ui::theme::BODY_TEXT));
        let mut seen = 0;
        for e in els.iter().filter(|e| e.text.as_deref().is_some_and(|t| !t.is_empty())) {
            let t = e.text.as_deref().unwrap();
            let mut up = e.parent;
            let mut inside = false;
            while let Some(i) = up {
                inside |= els[i].key.as_deref() == Some("rows");
                up = els[i].parent;
            }
            if !inside {
                continue;
            }
            seen += 1;
            assert!(e.rect[0] + e.rect[2] <= right + 0.5, "{t:?} overflows: {:?} past {right}", e.rect);
            let font = match (e.role, e.level) {
                ("heading", Some(n @ 1..=6)) => fonts.headings[n as usize - 1],
                _ => fonts.body,
            };
            let widest = rune::text::measure(t, &[], font, Some(0.0)).0;
            assert!(e.rect[0] + widest <= right + 0.5, "{t:?}: the widest part ({widest} pt) goes past {right}");
        }
        assert!(seen >= 6, "texts checked: {seen}");
        let _ = &mut rt;
    }
}
