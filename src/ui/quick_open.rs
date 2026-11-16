//! Quick open (Cmd+P): a 560-wide panel in the middle of the window, above the content
//! (docs/decisions/2026-11-15-compact-quick-open.md; mockup `2026-11-15-quick-open-raycast`,
//! variant "compatto"). The field at the top, below it the results on two lines like Recent (name;
//! repo or folder and time), the keys at the bottom. With an empty query it shows Recent. The sidebar's
//! "Open file ⌘P" field opens the panel. Single click: preview, the panel stays;
//! double click: pinned tab and the panel closes. Esc or a click outside close it.

use std::cmp::Reverse;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};

use rune::{Color, Key, KeyEvent, TextEdit, Tx, TextStyle, col, row, text, text_input, icon, El, ScrollSpec, el};
use rune_kit::glyphs;

use super::sidebar::{Hooks, Motion, pressable, row_meta, small_font, ui_font};
use crate::fx::{self, Mark};
use super::theme::{FIELD_H, FIELD_MARGIN_BOTTOM, FIELD_MARGIN_X, HAIRLINE, ICON, KBD_PAD_X, KBD_PAD_Y, KBD_TEXT, KBD_WEIGHT, R_FIELD, R_KBD, R_PANEL, R_ROW, ROW_GAP, ROW_PAD, SECTION_PAD_BOTTOM, SECTION_PAD_TOP, TAB_BAR_H, FRAME, Theme, UI_SMALL, UI_WEIGHT};

/// Maximum results (assumption: past this you do not scroll, you type more).
pub const MAX_RESULTS: usize = 50;

/// The panel: 560 wide (less with a narrow window, `QUICK_SIDE` from the edges), at the top,
/// `QUICK_TOP` from the window, below the tab bar (mockup: 72).
pub const QUICK_W: f32 = 560.0;
const QUICK_TOP: f32 = FRAME + TAB_BAR_H + 20.0;
const QUICK_SIDE: f32 = 16.0;
/// The field at the top (48, text 15), the rows (`QUICK_ROW`, margin 6, 2 between rows), the footer (36).
const QUICK_HEAD: f32 = 48.0;
const QUICK_TEXT: f32 = 15.0;
const QUICK_PAD: f32 = 6.0;
const QUICK_ROW_GAP: f32 = 2.0;
const QUICK_FOOT: f32 = 36.0;
/// A result row, on a single line (record 2026-11-15-quick-open-one-line; mockup
/// "compatto": `.qr` 40 tall, margins 10, 10 between icon, name, repo or folder and time).
pub const QUICK_ROW: f32 = 40.0;
const ROW_PX: f32 = 10.0;
const ROW_GAP_Q: f32 = 10.0;
/// The repo or folder does not shrink below this before the name is cut (mockup: `min-width: 40px`).
const PLACE_MIN: f32 = 40.0;
/// Rows in view before the list scrolls.
const QUICK_VISIBLE: f32 = 8.0;
/// The veil over the content: `panel` at 60 % (mockup).
const SCRIM: f32 = 0.6;


/// A matching name always beats a matching folder.
const BASENAME_BONUS: i32 = 10_000;
/// A folder whose name equals the query, and one that contains it as a subsequence.
const FOLDER_EXACT: i32 = 5_000;
const FOLDER_FUZZY: i32 = 1_000;
/// Each folder between the matching one and the file subtracts this: on a tie the nearest file wins.
const DEPTH_COST: i32 = 100;
/// Of the folders above the file, at most this many are looked at: the rest (`/Users/…`) says nothing about what to look for.
const FOLDER_PARTS: usize = 4;
/// A path with "/" in the query is compared with the last parts, as a single text.
const TAIL_PARTS: usize = 3;

/// Case-insensitive subsequence, like `score` in `rune-kit/src/combobox.rs` (private, so
/// copied): contiguous and word-start matches are worth more, each skipped byte subtracts
/// one. `None` if the text does not contain the whole query. Works on bytes (ASCII case): for
/// 7000 paths on every key the char version cost 4 ms (p95 5.1 ms).
fn score(needle: &[u8], hay: &[u8]) -> Option<i32> {
    if hay.len() < needle.len() {
        return None;
    }
    // Jumps from one query letter to the next: a path without the query ends early.
    let mut at = 0;
    let mut total = -((hay.len() - needle.len()) as i32);
    let mut last: Option<usize> = None;
    for &want in needle {
        let i = at + hay[at..].iter().position(|b| b.to_ascii_lowercase() == want)?;
        let boundary = i == 0 || !(hay[i - 1].is_ascii_alphanumeric() || hay[i - 1] >= 0x80);
        total += 10 + if last.is_some_and(|l| l + 1 == i) { 12 } else { 0 } + if boundary { 8 } else { 0 };
        last = Some(i);
        at = i + 1;
    }
    Some(total)
}

/// The score of one query word: the file name first; then the matching folder
/// (the whole name beats a subsequence, the one nearest the file beats the farthest).
/// No penalty for the length of the parts above the matching folder.
fn score_token(needle: &[u8], full: &[u8]) -> Option<i32> {
    let name_at = full.iter().rposition(|&b| b == b'/').map_or(0, |i| i + 1);
    if let Some(s) = score(needle, &full[name_at..]) {
        return Some(BASENAME_BONUS + s);
    }
    let mut best: Option<i32> = None;
    let mut end = name_at.saturating_sub(1);
    for depth in 0..FOLDER_PARTS {
        if end == 0 {
            break;
        }
        let start = full[..end].iter().rposition(|&b| b == b'/').map_or(0, |i| i + 1);
        let seg = &full[start..end];
        let found = if seg.eq_ignore_ascii_case(needle) { Some(FOLDER_EXACT) } else { score(needle, seg).map(|s| FOLDER_FUZZY + s) };
        if let Some(f) = found.map(|f| f - depth as i32 * DEPTH_COST) {
            best = Some(best.map_or(f, |b| b.max(f)));
        }
        end = start.saturating_sub(1);
    }
    if best.is_none() && needle.contains(&b'/') {
        let mut cut = name_at;
        for _ in 1..TAIL_PARTS {
            cut = full[..cut.saturating_sub(1)].iter().rposition(|&b| b == b'/').map_or(0, |i| i + 1);
            if cut == 0 {
                break;
            }
        }
        return score(needle, &full[cut..]);
    }
    best
}

/// The score of a path: every query word (separated by spaces) must match,
/// in any order; the score is the sum.
fn score_path(tokens: &[Vec<u8>], path: &Path) -> Option<i32> {
    let full = path.as_os_str().as_bytes();
    let mut total = 0;
    for t in tokens {
        total += score_token(t, full)?;
    }
    Some(total)
}

/// The `limit` paths that best match `query`, best first; on a tie the shortest and then
/// alphabetical order. An empty query gives nothing. Runs on the UI thread on every key:
/// no copies for paths that do not match.
pub fn rank<'a>(query: &str, paths: impl Iterator<Item = &'a Path>, limit: usize) -> Vec<PathBuf> {
    let lower = query.to_lowercase();
    let tokens: Vec<Vec<u8>> = lower.split_whitespace().map(|t| t.as_bytes().to_vec()).collect();
    if tokens.is_empty() || limit == 0 {
        return Vec::new();
    }
    let mut hits: Vec<(i32, &Path)> = paths.filter_map(|p| score_path(&tokens, p).map(|s| (s, p))).collect();
    let order = |a: &(i32, &Path), b: &(i32, &Path)| (Reverse(a.0), a.1.as_os_str().len(), a.1).cmp(&(Reverse(b.0), b.1.as_os_str().len(), b.1));
    if hits.len() > limit {
        hits.select_nth_unstable_by(limit, order);
        hits.truncate(limit);
    }
    hits.sort_by(order);
    hits.into_iter().map(|(_, p)| p.to_path_buf()).collect()
}

/// The Cmd+P state: the open panel, the field, the results and the row chosen with the arrows.
#[derive(Default)]
pub struct QuickOpen {
    pub open: bool,
    pub edit: TextEdit,
    pub hits: Vec<PathBuf>,
    pub selected: usize,
}

impl QuickOpen {
    /// The panel is visible.
    pub fn active(&self) -> bool {
        self.open
    }

    /// The query is empty: the panel shows Recent.
    pub fn empty_query(&self) -> bool {
        self.edit.text.trim().is_empty()
    }

    /// Cmd+P or the sidebar field: the panel opens with focus in the field.
    pub fn show(&mut self) {
        self.open = true;
        self.edit.focused = true;
    }

    /// Esc, a click outside or a chosen file: the panel closes, empty query.
    pub fn close(&mut self) {
        self.open = false;
        self.edit = TextEdit::default();
        self.hits.clear();
        self.selected = 0;
    }

    /// After an index change with an empty query: the results are `paths` (Recent), the
    /// chosen row stays by path if `keep`.
    pub fn set_hits(&mut self, paths: Vec<PathBuf>, keep: bool) {
        let before = self.chosen().map(Path::to_path_buf);
        self.hits = paths;
        self.selected = before.filter(|_| keep).and_then(|b| self.hits.iter().position(|h| *h == b)).unwrap_or(0);
    }

    /// The query changed: new results from the index names, choice on the first row.
    pub fn requery<'a>(&mut self, names: impl Iterator<Item = &'a Path>) {
        self.hits = rank(&self.edit.text, names, MAX_RESULTS);
        self.selected = 0;
    }

    /// The index changed, not the query: the chosen row stays the same, by path (a
    /// write elsewhere must not move what Return opens). If it is gone, back to the first.
    pub fn refresh<'a>(&mut self, names: impl Iterator<Item = &'a Path>) {
        let before = self.chosen().map(Path::to_path_buf);
        self.hits = rank(&self.edit.text, names, MAX_RESULTS);
        self.selected = before.and_then(|b| self.hits.iter().position(|h| *h == b)).unwrap_or(0);
    }

    /// Moves the choice by `delta` rows, wrapping around.
    pub fn step(&mut self, delta: isize) {
        if !self.hits.is_empty() {
            self.selected = (self.selected as isize + delta).rem_euclid(self.hits.len() as isize) as usize;
        }
    }

    pub fn chosen(&self) -> Option<&Path> {
        self.hits.get(self.selected).map(PathBuf::as_path)
    }
}

fn row_key(path: &Path) -> String {
    format!("quick-{}", path.display())
}

/// The sidebar's "Open file ⌘P" field: a button that looks like the field and opens the panel.
pub(super) fn launcher<S: 'static>(hooks: Hooks<S>, theme: &Theme) -> El {
    let kbd = kbd("⌘P", theme);
    let hint = text("Open file").key("quick-launch").font(ui_font()).color(theme.fg2).grow(1.0).shrink().min_w(0.0).truncate();
    field_row(icon(&glyphs::SEARCH, ICON).color(theme.fg2), hint, kbd, theme).role("button").label("Open file").on_press(move |s: &mut S| {
        (hooks.get_mut)(s).quick.show();
        (hooks.query)(s);
    })
}

/// The sidebar field row (Cmd+P and Cmd+N): icon, text field and the key on the right.
pub(super) fn field_row(lead: El, input: El, kbd: El, theme: &Theme) -> El {
    row()
        .key("open-file")
        .h(FIELD_H)
        .mx(FIELD_MARGIN_X)
        .mb(FIELD_MARGIN_BOTTOM)
        .px(ROW_PAD)
        .gap(ROW_GAP)
        .center_y()
        .radius(R_FIELD)
        .fill(theme.field)
        .border(HAIRLINE, theme.line)
        .child(lead)
        .child(input)
        .child(kbd)
}

/// The shortcut key right of the field ("⌘P").
pub(super) fn kbd(label: &'static str, theme: &Theme) -> El {
    el().key("kbd").border(HAIRLINE, theme.line).radius(R_KBD).px(KBD_PAD_X).py(KBD_PAD_Y).child(text(label).font(TextStyle::new(KBD_TEXT, KBD_WEIGHT)).color(theme.fg2))
}

/// The field's keys, before the editor. True if it takes them.
fn on_key<S: 'static>(s: &mut S, hooks: Hooks<S>, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if (hooks.get)(s).quick.edit.preedit().is_some() {
        return false;
    }
    let repeat = tx.key_repeat();
    match &ev.key {
        Key::Escape => {
            if !repeat {
                (hooks.get_mut)(s).quick.close();
            }
            true
        }
        Key::Enter if !ev.mods.alt && !ev.mods.shift => {
            if !repeat {
                let chosen = (hooks.get)(s).quick.chosen().map(Path::to_path_buf);
                if let Some(path) = chosen {
                    (hooks.get_mut)(s).quick.close();
                    (hooks.open)(s, path, tx);
                }
            }
            true
        }
        Key::Down | Key::Up if !ev.mods.cmd && !ev.mods.ctrl && !ev.mods.alt => {
            let quick = &mut (hooks.get_mut)(s).quick;
            quick.step(if matches!(ev.key, Key::Down) { 1 } else { -1 });
            if let Some(p) = quick.chosen() {
                tx.scroll_into_view(row_key(p));
            }
            true
        }
        _ => false,
    }
}

/// A key in the panel footer and what it does: keys in `fg2` inside a `line` border, word in `fg3`.
fn hint(keys: &[&'static str], what: &'static str, theme: &Theme) -> El {
    let mut r = row().gap(4.0).center_y();
    for k in keys {
        r = r.child(el().border(HAIRLINE, theme.line).radius(R_KBD).px(KBD_PAD_X).py(KBD_PAD_Y).child(text(*k).font(TextStyle::new(KBD_TEXT, KBD_WEIGHT)).color(theme.fg2)));
    }
    r.child(text(what).font(TextStyle::new(UI_SMALL, UI_WEIGHT)).color(theme.fg3).ml(2.0))
}

/// The fixed space of a row `row_w` wide with the time `time_w` wide: margins, icon, the space
/// after the icon and the one before the time.
fn row_fixed(time_w: f32) -> f32 {
    2.0 * ROW_PX + ICON + 2.0 * ROW_GAP_Q + time_w
}

/// How wide the name is (`name_w` when whole) in a row `row_w` wide with the time
/// `time_w` wide: whole as long as the repo or folder has at least `PLACE_MIN`; then the name is cut
/// (constraint 2 of record quick-open-one-line: first the repo or folder, then the name).
fn name_width(row_w: f32, name_w: f32, time_w: f32) -> f32 {
    name_w.min(row_w - row_fixed(time_w) - ROW_GAP_Q - PLACE_MIN).max(0.0)
}

/// A result row, on a single line (record 2026-11-15-quick-open-one-line): file
/// icon, the name (`fg`, cut in the middle), repo or folder (cut at the end) and at the far right
/// the time (in [`row_meta`] also on hover and when chosen). `row_w` is the row width. Click:
/// preview and the row becomes the choice, the panel stays; double click: pinned tab and the
/// panel closes.
#[allow(clippy::too_many_arguments)]
fn result_row<S: 'static>(sb: &super::sidebar::Sidebar, hooks: Hooks<S>, i: usize, r: &super::sidebar::RecentRow, label: &str, row_w: f32, theme: &Theme, motion: Motion) -> El {
    let selected = i == sb.quick.selected;
    let (name_font, place_font, time_font) = (ui_font(), TextStyle::new(UI_SMALL, UI_WEIGHT), small_font());
    // The name is measured as the view measures it; the extra half point avoids a cut from rounding.
    let name_w = name_width(row_w, (rune::text::advance(&r.name, name_font) + 0.5).ceil(), rune::text::advance(label, time_font).ceil());
    let name = text(r.name.clone()).font(name_font).color(theme.fg).truncate_middle().w(name_w);
    let place = text(r.qualifier.clone().unwrap_or_default()).font(place_font).color(row_meta(theme)).truncate().grow(1.0).shrink().min_w(0.0);
    let time = text(label.to_owned()).font(time_font).color(row_meta(theme));
    let lines = row().grow(1.0).shrink().min_w(0.0).gap(ROW_GAP_Q).center_y().child(name).child(place).child(time);
    let glyph = icon(&glyphs::FILE, ICON).color(if selected { theme.fg } else { theme.fg2 });
    let (open, pin) = (r.path.clone(), r.path.clone());
    let mut e = row()
        .w_full()
        .h(QUICK_ROW)
        .px(ROW_PX)
        .gap(ROW_GAP_Q)
        .center_y()
        .child(glyph)
        .child(lines)
        .on_double_press_tx(move |s: &mut S, tx: &mut Tx| {
            (hooks.get_mut)(s).quick.close();
            (hooks.pin)(s, pin.clone(), tx);
        })
        .on_press_tx(move |s: &mut S, tx: &mut Tx| {
            (hooks.get_mut)(s).quick.selected = i;
            (hooks.open)(s, open.clone(), tx);
        });
    if selected {
        e = e.fill(theme.sel);
    }
    let key = row_key(&r.path);
    // No metal on the row: the edge goes around the whole panel (docs/decisions/2026-11-16-metal-tab-quick-open-panel.md).
    pressable(e, key.clone(), sb, hooks, theme, motion, !selected).radius(R_ROW).key(key)
}

/// The Cmd+P panel, to mount at the root: a `panel` veil at 60 % over the whole window and
/// above it, centered, the panel (`panel`, radius `R_PANEL`, panel border and shadow). The
/// panel is modal: a click outside or Esc close it. Empty if closed.
pub fn panel<S: 'static>(s: &S, hooks: Hooks<S>, list: Option<[f32; 4]>, theme: &Theme, reduced: bool, hover: bool) -> Vec<El> {
    let sb = (hooks.get)(s);
    if !sb.quick.active() {
        return Vec::new();
    }
    let motion = Motion { reduced, hover };
    let input = text_input(&sb.quick.edit, TextStyle::new(QUICK_TEXT, UI_WEIGHT))
        .single_line()
        .key("quick-input")
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .color(theme.fg)
        .placeholder("Open file", theme.fg2)
        .caret(theme.fg, theme.sel)
        .label("Open file")
        .on_edit(move |s: &mut S| &mut (hooks.get_mut)(s).quick.edit)
        .on_input_change(move |s: &mut S, _: &mut Tx| (hooks.query)(s))
        .on_input_key(move |s: &mut S, ev: &KeyEvent, tx: &mut Tx| on_key(s, hooks, ev, tx));
    let head = row().key("quick-head").w_full().h(QUICK_HEAD).px(14.0).gap(10.0).center_y().child(icon(&glyphs::SEARCH, ICON).color(theme.fg2)).child(input);
    let line = || el().w_full().h(HAIRLINE).fill(theme.line);
    // The row is as wide as the list in the last layout minus the margins; before the first, the full panel.
    let row_w = list.map_or(QUICK_W, |r| r[2]) - 2.0 * QUICK_PAD;
    let mut rows = col().key("quick-rows").w_full().padding(QUICK_PAD).gap(QUICK_ROW_GAP);
    if sb.quick_rows.is_empty() {
        let none = if sb.quick.empty_query() { "No recent files" } else { "No matching files" };
        rows = rows.child(text(none).key("quick-none").font(small_font()).color(theme.fg3).px(ROW_PAD).pt(SECTION_PAD_TOP).pb(SECTION_PAD_BOTTOM));
    }
    // metal-tab: the panel edge moves while an agent writes any listed file, lit at the earliest one.
    let mut writing: Option<f64> = None;
    for (i, r) in sb.quick_rows.iter().enumerate() {
        let label = sb.quick_labels.get(i).map(String::as_str).unwrap_or("");
        if let Some(since) = (hooks.ring)(s, &r.path) {
            writing = Some(writing.map_or(since, |w| w.min(since)));
        }
        rows = rows.child(result_row(sb, hooks, i, r, label, row_w, theme, motion));
    }
    let list = rows;
    let list_max = QUICK_PAD * 2.0 + QUICK_VISIBLE * QUICK_ROW + (QUICK_VISIBLE - 1.0) * QUICK_ROW_GAP;
    let scroll = col().key("quick").w_full().max_h(list_max).shrink().min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(list);
    let n = sb.quick_rows.len();
    let count = text(if n == 1 { "1 file".to_owned() } else { format!("{n} files") }).key("quick-count").font(small_font()).color(theme.fg3);
    let foot = row()
        .key("quick-foot")
        .w_full()
        .h(QUICK_FOOT)
        .px(14.0)
        .gap(16.0)
        .center_y()
        .child(hint(&["↑", "↓"], "select", theme))
        .child(hint(&["↵"], "open", theme))
        .child(hint(&["Esc"], "close", theme))
        .child(el().grow(1.0))
        .child(count);
    let surface = col()
        .key("quick-panel")
        .role("dialog")
        .label("Open file")
        .w_full()
        .max_w(QUICK_W)
        .fill(theme.panel)
        .radius(R_PANEL)
        .clip_rounded(R_PANEL)
        .border(HAIRLINE, theme.fg.alpha(0.10))
        .shadow(10.0, 30.0, Color::BLACK.alpha(0.35))
        .child(head)
        .child(line())
        .child(scroll)
        .child(line())
        .child(foot)
        .modal(true, move |s: &mut S| (hooks.get_mut)(s).quick.close());
    // metal-tab: the metal edge around the whole panel, calm (frozen gunmetal) or moving, always the
    // thick width (follow-up 2026-11-16). The host sits outside the panel's rounded clip, so the edge
    // drawn outside its rect is not cut.
    let host = el().key("quick-edge").w_full().max_w(QUICK_W).radius(R_PANEL).child(surface);
    let host = fx::ring::slot_as(host, Mark::QuickPanel, fx::ring::edge(writing, true), reduced, theme.dark, fx::ring::Calm::Panel);
    let veil = col().key("quick-scrim").absolute(0.0, 0.0).w_full().h_full().fill(theme.panel.alpha(SCRIM)).items_center().pt(QUICK_TOP).px(QUICK_SIDE).child(host);
    vec![veil]
}

#[cfg(test)]
mod tests {
    use super::*;

    mod panel {
        use rune::{Env, Key, KeyEvent, Mods, Runtime};

        use crate::app::App;
        use crate::doc::document::Document;
        use crate::fx::Mark;
        use crate::index::{Entry, Index, Library};

        fn find(rt: &Runtime<App>, key: &str) -> Option<rune::ElementInfo> {
            rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key))
        }

        fn settle(rt: &mut Runtime<App>) {
            for _ in 0..2000 {
                if !rt.frame(1.0 / 120.0) {
                    return;
                }
            }
            panic!("the view does not settle");
        }

        fn click(rt: &mut Runtime<App>, x: f32, y: f32) {
            rt.pointer_move(x, y);
            rt.pointer_down(x, y);
            rt.pointer_up(x, y);
            rt.frame(1.0 / 60.0);
        }

        fn centre(rt: &Runtime<App>, key: &str) -> (f32, f32) {
            let r = find(rt, key).unwrap_or_else(|| panic!("{key}")).rect;
            (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0)
        }

        fn cmd_p() -> KeyEvent {
            KeyEvent { key: Key::Command('p'), mods: Mods { cmd: true, ..Mods::default() } }
        }

        /// A 1200 × 900 app with alfa.md open and beta.md, gamma.md in the index.
        fn open() -> (tempfile::TempDir, Runtime<App>) {
            let dir = tempfile::tempdir().unwrap();
            let root = dir.path().canonicalize().unwrap();
            let mut entries = Vec::new();
            for n in ["alfa", "beta", "gamma"] {
                let p = root.join(format!("{n}.md"));
                std::fs::write(&p, format!("# {n}\n")).unwrap();
                entries.push(Entry { path: p, mtime: std::time::SystemTime::now() });
            }
            let mut state = App::bare(Library::load(Vec::new(), None));
            state.library.index = Index::from_entries(entries);
            state.push_tab(Document::from_source(&root.join("alfa.md"), "# alfa\n"));
            let mut rt = Runtime::new(state, crate::view, (1200.0, 900.0), Env::new(true, 2.0));
            rt.on_key = Some(crate::on_key);
            rt.frame(0.0);
            settle(&mut rt);
            (dir, rt)
        }

        /// metal-tab: the slot ids with an edge, split into calm and writing.
        fn edges(rt: &mut Runtime<App>) -> (Vec<u32>, Vec<u32>) {
            rt.track_marks = true;
            rt.frame(1.0 / 60.0);
            let all = crate::fx::edges(rt);
            let pick = |calm: bool| {
                let mut v: Vec<u32> = all.iter().filter(|e| e.1 == calm).map(|e| e.0).collect();
                v.sort_unstable();
                v
            };
            (pick(true), pick(false))
        }

        /// The ⌘P edges must be one slot around the whole panel, never on a row: the slot with
        /// the panel mark wraps "quick-panel" (its rect, widened by the bleed).
        fn panel_edge_wraps_the_panel(rt: &mut Runtime<App>) {
            rt.track_marks = true;
            rt.frame(1.0 / 60.0);
            let panel = find(rt, "quick-panel").expect("panel").rect;
            let slots: Vec<[f32; 4]> = rt
                .instances
                .iter()
                .enumerate()
                .filter(|(n, i)| i.params[1] == rune::render::KIND_EFFECT && rt.marks.get(*n).copied() == Some(Mark::QuickPanel.id()))
                .map(|(_, i)| i.rect.map(|v| v / i.params[3].max(f32::EPSILON)))
                .collect();
            assert_eq!(slots.len(), 1, "one edge for the whole panel, none on rows: {slots:?}");
            let r = slots[0];
            assert!(r[2] >= panel[2] && r[3] >= panel[3], "the edge wraps the panel {panel:?}, not a row: {r:?}");
        }

        fn quick_index(rt: &Runtime<App>, name: &str) -> u32 {
            rt.state.sidebar.quick_rows.iter().position(|r| r.name == name).unwrap_or_else(|| panic!("{name} not in ⌘P")) as u32
        }

        fn recent_index(rt: &Runtime<App>, name: &str) -> Option<u32> {
            rt.state.sidebar.recent.iter().position(|r| r.name == name).map(|i| i as u32)
        }

        /// ⌘P open on "a": alfa, beta and gamma are rows.
        fn quick_on_a(rt: &mut Runtime<App>) {
            rt.key(cmd_p());
            settle(rt);
            rt.update(|s: &mut App, _| {
                s.sidebar.quick.edit.replace_text("a");
                s.requery_sidebar();
            });
            settle(rt);
        }

        fn write(rt: &mut Runtime<App>, path: std::path::PathBuf) {
            rt.update(move |s: &mut App, _| {
                s.ring_step(&[(path.clone(), crate::live::activity::WRITING_WINDOW)], 1.0, std::time::Instant::now());
            });
            rt.frame(1.0 / 60.0);
        }

        /// metal-tab, calm (docs/decisions/2026-11-16-metal-tab-quick-open-panel.md): a frozen edge
        /// on the active tab and, with ⌘P open, on the whole panel; no ⌘P row has an edge and
        /// nothing moves.
        #[test]
        fn calm_edge_on_the_active_tab_and_the_quick_panel() {
            let (_d, mut rt) = open();
            assert_eq!(edges(&mut rt), (vec![Mark::TabRing(0).id()], vec![]));
            quick_on_a(&mut rt);
            assert_eq!(edges(&mut rt), (vec![Mark::TabRing(0).id(), Mark::QuickPanel.id()], vec![]));
            panel_edge_wraps_the_panel(&mut rt);
            assert!(rt.state.effects_lit());
        }

        /// The fill the compositor paints for slot `mark` in the last frame.
        fn fill(rt: &mut Runtime<App>, mark: Mark) -> rune_fx::FillParams {
            rt.track_marks = true;
            rt.frame(1.0 / 60.0);
            crate::fx::edge_fills(rt).into_iter().find(|e| e.0 == mark.id()).unwrap_or_else(|| panic!("no edge on {mark:?}")).1
        }

        /// The width the compositor paints for slot `mark` in the last frame.
        fn width(rt: &mut Runtime<App>, mark: Mark) -> f32 {
            fill(rt, mark).border
        }

        fn metal(rt: &mut Runtime<App>, mark: Mark) -> rune_fx::LiquidMetalParams {
            match fill(rt, mark).style {
                rune_fx::FillStyle::LiquidMetal(m) => m,
                other => panic!("not liquid metal: {other:?}"),
            }
        }

        fn lum(c: rune_fx::Rgb) -> f32 {
            0.2126 * c.0[0] + 0.7152 * c.0[1] + 0.0722 * c.0[2]
        }

        /// Follow-up 2026-11-16 (case c_bf56ebdc): in light, the panel's calm edge is lighter metal
        /// than the tab's, so the long thick edge reads as light as the tab's; in dark both are the
        /// same gunmetal.
        #[test]
        fn in_light_the_calm_panel_edge_is_lighter_than_the_tab() {
            let (_d, mut rt) = open();
            rt.update(|s: &mut App, _| s.set_theme(crate::settings::ThemePref::Light));
            quick_on_a(&mut rt);
            let (panel, tab) = (metal(&mut rt, Mark::QuickPanel), metal(&mut rt, Mark::TabRing(0)));
            assert_eq!(panel.flow, 0.0, "still frozen");
            assert!(lum(panel.tint) > lum(tab.tint), "panel {:?} vs tab {:?}", panel.tint, tab.tint);
            assert_eq!(tab, crate::fx::ring::calm_metal(false, crate::fx::ring::Calm::Tab), "the tab is unchanged");

            rt.update(|s: &mut App, _| s.set_theme(crate::settings::ThemePref::Dark));
            settle(&mut rt);
            assert_eq!(metal(&mut rt, Mark::QuickPanel), metal(&mut rt, Mark::TabRing(0)), "dark: same gunmetal");
        }

        /// Follow-up 2026-11-16 (case c_bf56ebdc): the open panel always has the thick edge,
        /// calm and writing; the active tab keeps the two widths.
        #[test]
        fn the_panel_edge_is_always_thick_and_the_tab_keeps_two_widths() {
            use crate::fx::ring::{CALM_W, RING_W};
            let (d, mut rt) = open();
            quick_on_a(&mut rt);
            assert_eq!(width(&mut rt, Mark::QuickPanel), RING_W, "calm panel");
            assert_eq!(width(&mut rt, Mark::TabRing(0)), CALM_W, "calm tab");
            write(&mut rt, d.path().canonicalize().unwrap().join("alfa.md"));
            assert_eq!(width(&mut rt, Mark::QuickPanel), RING_W, "writing panel");
            assert_eq!(width(&mut rt, Mark::TabRing(0)), RING_W, "writing tab");
        }

        /// metal-tab, writing a listed file: the ⌘P panel edge moves (no row edge, the chosen row
        /// included); the file's Recent row moves too; the active tab of another file stays calm.
        #[test]
        fn writing_a_listed_file_moves_the_panel_edge() {
            let (d, mut rt) = open();
            let root = d.path().canonicalize().unwrap();
            quick_on_a(&mut rt);
            quick_index(&rt, "beta.md");
            write(&mut rt, root.join("beta.md"));
            let (calm, writing) = edges(&mut rt);
            assert_eq!(calm, vec![Mark::TabRing(0).id()], "only the active tab is calm");
            assert!(writing.contains(&Mark::QuickPanel.id()), "{writing:?}");
            panel_edge_wraps_the_panel(&mut rt);
            if let Some(r) = recent_index(&rt, "beta.md") {
                assert!(writing.contains(&Mark::RecentRing(r).id()), "{writing:?}");
            }
            assert!(!writing.contains(&Mark::TabRing(0).id()));
        }

        /// metal-tab: a file that is written but not in the ⌘P list leaves the panel edge calm.
        #[test]
        fn writing_an_unlisted_file_keeps_the_panel_calm() {
            let (d, mut rt) = open();
            let root = d.path().canonicalize().unwrap();
            rt.key(cmd_p());
            settle(&mut rt);
            rt.update(|s: &mut App, _| {
                s.sidebar.quick.edit.replace_text("beta");
                s.requery_sidebar();
            });
            settle(&mut rt);
            assert!(rt.state.sidebar.quick_rows.iter().all(|r| r.name != "gamma.md"), "gamma is not listed");
            write(&mut rt, root.join("gamma.md"));
            let (calm, writing) = edges(&mut rt);
            assert!(calm.contains(&Mark::QuickPanel.id()), "{calm:?}");
            assert!(!writing.contains(&Mark::QuickPanel.id()), "{writing:?}");
            panel_edge_wraps_the_panel(&mut rt);
        }

        /// metal-tab: when the active tab's file is written, its calm edge becomes the moving one.
        #[test]
        fn writing_the_open_file_turns_its_calm_edge_into_moving_metal() {
            let (d, mut rt) = open();
            write(&mut rt, d.path().canonicalize().unwrap().join("alfa.md"));
            let (calm, writing) = edges(&mut rt);
            assert!(!calm.contains(&Mark::TabRing(0).id()) && writing.contains(&Mark::TabRing(0).id()), "{calm:?} {writing:?}");
        }

        fn tabs(rt: &Runtime<App>) -> Vec<(String, bool)> {
            rt.state.tabs.iter().map(|t| (t.title(), t.preview)).collect()
        }

        /// Constraints 1, 3, 6: ⌘P opens a 560-wide panel in the middle of the window, above the
        /// content (the document does not move); the keys at the bottom; Esc and a click outside close it.
        #[test]
        fn cmd_p_opens_a_560_panel_in_the_centre() {
            let (_d, mut rt) = open();
            let doc_before = find(&rt, "main").unwrap().rect;
            rt.key(cmd_p());
            settle(&mut rt);
            let p = find(&rt, "quick-panel").expect("panel").rect;
            assert_eq!(p[2], super::super::QUICK_W);
            assert!((p[0] + p[2] / 2.0 - 600.0).abs() < 1.0, "centered: {p:?}");
            assert!(p[1] > 44.0, "below the tab bar: {p:?}");
            assert_eq!(find(&rt, "main").unwrap().rect, doc_before, "the document does not move");
            let foot: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).collect();
            for k in ["↑", "↓", "↵", "Esc", "select", "open", "close"] {
                assert!(foot.iter().any(|t| t == k), "missing {k} in the footer");
            }
            assert!(rt.state.sidebar.quick.edit.focused, "focus in the field");

            rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
            settle(&mut rt);
            assert!(find(&rt, "quick-panel").is_none(), "Esc closes");

            rt.key(cmd_p());
            settle(&mut rt);
            click(&mut rt, 600.0, 860.0);
            settle(&mut rt);
            assert!(find(&rt, "quick-panel").is_none(), "a click outside closes");

            // The sidebar field opens the same panel.
            let (x, y) = centre(&rt, "open-file");
            click(&mut rt, x, y);
            settle(&mut rt);
            assert!(find(&rt, "quick-panel").is_some());
        }

        /// Record 2026-11-15-quick-open-one-line, constraint 1: each row is on a single line,
        /// `QUICK_ROW` tall: name on the left, repo or folder right after, the time at the far right,
        /// all at the same height.
        #[test]
        fn rows_are_on_one_line() {
            let (_d, mut rt) = open();
            rt.key(cmd_p());
            settle(&mut rt);
            rt.update(|s: &mut App, _| {
                s.sidebar.quick.edit.replace_text("beta");
                s.requery_sidebar();
            });
            settle(&mut rt);
            let key = rt.elements().into_iter().filter_map(|e| e.key).find(|k| k.starts_with("quick-/")).expect("row");
            let row = find(&rt, &key).unwrap().rect;
            assert_eq!(row[3], super::super::QUICK_ROW, "a single line");
            let (place, time) = (rt.state.sidebar.quick_rows[0].qualifier.clone().unwrap(), rt.state.sidebar.quick_labels[0].clone());
            let at = |t: &str| rt.elements().into_iter().find(|e| e.text.as_deref() == Some(t) && e.rect[1] >= row[1] && e.rect[1] < row[1] + row[3]).unwrap_or_else(|| panic!("missing {t}")).rect;
            let (n, p, t) = (at("beta.md"), at(&place), at(&time));
            let mid = |r: [f32; 4]| r[1] + r[3] / 2.0;
            assert!((mid(n) - mid(p)).abs() < 1.5 && (mid(n) - mid(t)).abs() < 1.5, "same line: {n:?} {p:?} {t:?}");
            assert!(n[0] + n[2] < p[0] && p[0] + p[2] <= t[0], "name, then repo or folder, then the time: {n:?} {p:?} {t:?}");
            assert!((t[0] + t[2] - (row[0] + row[2] - 10.0)).abs() < 1.0, "the time at the far right: {t:?} {row:?}");
        }

        /// Constraints 2 and 4 of the compact record: a single click opens a preview and the panel stays,
        /// a double click pins the tab and closes.
        #[test]
        fn a_double_click_pins_the_tab() {
            let (_d, mut rt) = open();
            rt.key(cmd_p());
            settle(&mut rt);
            rt.update(|s: &mut App, _| {
                s.sidebar.quick.edit.replace_text("beta");
                s.requery_sidebar();
            });
            settle(&mut rt);
            let rows: Vec<String> = rt.state.sidebar.quick_rows.iter().map(|r| r.name.clone()).collect();
            assert_eq!(rows, ["beta.md"]);
            assert!(rt.state.sidebar.quick_rows[0].qualifier.is_some(), "repo or folder");
            assert!(!rt.state.sidebar.quick_labels[0].is_empty(), "the time");
            let key = rt.elements().into_iter().filter_map(|e| e.key).find(|k| k.starts_with("quick-/")).expect("row");

            let (x, y) = centre(&rt, &key);
            click(&mut rt, x, y);
            settle(&mut rt);
            assert_eq!(tabs(&rt), [("alfa.md".to_owned(), false), ("beta.md".to_owned(), true)], "single click: preview");
            assert!(find(&rt, "quick-panel").is_some(), "the panel stays");

            click(&mut rt, x, y);
            click(&mut rt, x, y);
            settle(&mut rt);
            assert_eq!(tabs(&rt), [("alfa.md".to_owned(), false), ("beta.md".to_owned(), false)], "double click: pinned");
            assert!(find(&rt, "quick-panel").is_none(), "and closes");
        }
    }

    /// Constraint 2 of record quick-open-one-line: if there is not enough room, first the repo or
    /// folder shrinks (down to `PLACE_MIN`), then the name.
    #[test]
    fn the_place_gives_way_before_the_name() {
        // Everything fits: the whole name.
        assert_eq!(name_width(500.0, 200.0, 30.0), 200.0);
        // The whole name, the repo or folder takes what is left, at least PLACE_MIN.
        let room = 500.0 - row_fixed(30.0);
        assert_eq!(name_width(500.0, room - PLACE_MIN - ROW_GAP_Q, 30.0), room - PLACE_MIN - ROW_GAP_Q);
        // A longer name: it gets cut, the repo or folder stays at PLACE_MIN.
        assert_eq!(name_width(500.0, 1000.0, 30.0), room - PLACE_MIN - ROW_GAP_Q);
        // A very narrow row: no negative values.
        assert_eq!(name_width(40.0, 1000.0, 30.0), 0.0);
    }

    #[test]
    fn ranks_basename_first() {
        let paths = [Path::new("a/spec.md"), Path::new("spec/x.md")];
        let hits = rank("spec", paths.iter().copied(), 10);
        assert_eq!(hits.first().map(PathBuf::as_path), Some(Path::new("a/spec.md")));
        let swapped = [Path::new("spec/x.md"), Path::new("a/spec.md")];
        assert_eq!(rank("spec", swapped.iter().copied(), 10)[0], Path::new("a/spec.md"));
    }

    #[test]
    fn needs_every_letter_in_order_and_ignores_case() {
        let paths = [Path::new("r/Notes.md"), Path::new("r/nos.md"), Path::new("r/readme.md")];
        let hits = rank("NTS", paths.iter().copied(), 10);
        assert_eq!(hits, [Path::new("r/Notes.md")]);
        assert!(rank("  ", paths.iter().copied(), 10).is_empty());
    }

    #[test]
    fn caps_the_results_and_keeps_the_best() {
        let owned: Vec<PathBuf> = (0..200).map(|i| PathBuf::from(format!("r/x{i}/file{i}.md"))).chain([PathBuf::from("r/zzz/f.md")]).collect();
        let hits = rank("f", owned.iter().map(PathBuf::as_path), 5);
        assert_eq!(hits.len(), 5);
        assert_eq!(hits[0], Path::new("r/zzz/f.md"), "the shortest and nearest name wins");
    }

    #[test]
    fn folder_match_still_finds_files_without_a_name_match() {
        let paths = [Path::new("/Users/me/runemd/README.md"), Path::new("/Users/me/other/notes.md")];
        assert_eq!(rank("runemd", paths.iter().copied(), 10), [Path::new("/Users/me/runemd/README.md")]);
    }

    #[test]
    fn arrows_wrap_and_enter_picks() {
        let mut q = QuickOpen::default();
        q.edit.replace_text("a");
        q.requery([Path::new("/r/a.md"), Path::new("/r/ab.md"), Path::new("/r/b.md")].into_iter());
        assert_eq!(q.hits.len(), 2);
        assert_eq!(q.chosen(), Some(Path::new("/r/a.md")));
        q.step(-1);
        assert_eq!(q.chosen(), Some(Path::new("/r/ab.md")));
        q.step(1);
        assert_eq!(q.chosen(), Some(Path::new("/r/a.md")));
        q.close();
        assert!(!q.active() && q.hits.is_empty() && !q.edit.focused);
    }

    #[test]
    fn exact_folder_beats_scattered_and_shallow_beats_deep() {
        let paths = [
            Path::new("/Users/me/GitHub/runemd/docs/spec.md"),
            Path::new("/Users/me/GitHub/rune-md-spike/README.md"),
            Path::new("/Users/me/GitHub/runemd/README.md"),
            Path::new("/Users/me/GitHub/runemd/docs/deep/more.md"),
        ];
        let hits = rank("runemd", paths.iter().copied(), 10);
        assert_eq!(hits[0], Path::new("/Users/me/GitHub/runemd/README.md"));
        assert_eq!(hits[1], Path::new("/Users/me/GitHub/runemd/docs/spec.md"));
        assert_eq!(hits[2], Path::new("/Users/me/GitHub/runemd/docs/deep/more.md"));
        assert_eq!(hits[3], Path::new("/Users/me/GitHub/rune-md-spike/README.md"));
        let named = [Path::new("/x/runemd/a.md"), Path::new("/y/runemd.md")];
        assert_eq!(rank("runemd", named.iter().copied(), 10)[0], Path::new("/y/runemd.md"), "the name beats the folder");
    }

    #[test]
    fn every_space_separated_word_must_match_in_any_order() {
        let paths = [Path::new("/r/docs/plan.md"), Path::new("/r/docs/notes.md"), Path::new("/r/plan/x.txt")];
        assert_eq!(rank("plan md", paths.iter().copied(), 10), [Path::new("/r/docs/plan.md")]);
        assert_eq!(rank("md plan", paths.iter().copied(), 10), [Path::new("/r/docs/plan.md")]);
        let tasks = [Path::new("/r/task/18/a.md"), Path::new("/r/task/19/a.md")];
        assert_eq!(rank("task 18", tasks.iter().copied(), 10), [Path::new("/r/task/18/a.md")]);
    }

    #[test]
    fn refresh_keeps_the_chosen_file_by_path() {
        let mut q = QuickOpen::default();
        q.edit.replace_text("md");
        let names = ["/r/a.md", "/r/b.md", "/r/c.md", "/r/d.md"].map(Path::new);
        q.requery(names.iter().copied());
        q.step(2);
        let chosen = q.chosen().unwrap().to_path_buf();
        // A new file comes in front: the chosen row stays the same file.
        let more = ["/r/a.md", "/r/0.md", "/r/b.md", "/r/c.md", "/r/d.md"].map(Path::new);
        q.refresh(more.iter().copied());
        assert_eq!(q.chosen(), Some(chosen.as_path()));
        // It disappears: back to the first. With a changed query it starts again from the first.
        let fewer: Vec<&Path> = more.iter().copied().filter(|p| **p != *chosen).collect();
        q.refresh(fewer.iter().copied());
        assert_eq!(q.selected, 0);
        q.step(1);
        q.requery(more.iter().copied());
        assert_eq!(q.selected, 0);
    }
}
