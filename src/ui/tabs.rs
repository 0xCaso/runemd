//! The tab bar at the top of the panel (DESIGN.md, "Tab"; record 2026-11-18-vscode-tabs): the names of
//! the open files in VS Code's shape, edge to edge, each tab as wide as its name with `TAB_PAD_X` before
//! it (at most 240, the name cut in the middle), the front one a `sel` pill inside the bar with a closed
//! metal ring round it (record 2026-11-18-tab-bar-style), the "is writing" ring around the file's tab, "⌘E source" on the right and, to its left,
//! the gear (settings, `ui::shortcuts`) and the text width ↔ button: it opens a popover with the
//! settings slider (same `settings.column`, continuous); ⌘⌥+ and ⌘⌥- take one step.
//!
//! Tabs press like the sidebar's rows (`sidebar::pressable`: hover `sel`, scale 0.98
//! when pressed), not rune-kit's `TabStrip`: `TabStrip` gives every tab the same width
//! (28–224 pt) and keeps 16 pt for an avatar before the name, and neither can be changed
//! from outside. The mockup Matteo chose has tabs as wide as their name.
//!
//! The preview tab (single click in the sidebar, `App::open_preview`) has its name in italics;
//! a double click pins it.
//!
//! Too many tabs for the bar: the strip scrolls sideways under the buttons, with its own scroll bar, and
//! the front tab scrolls into view when it changes. A click brings a tab to the front; the × (on the
//! front tab, or on hover) and a middle click close it, Cmd+W closes the front one
//! (`main::tab_key`); with unsaved changes they ask first (`App::close`). ⌘⇧] ⌘⇧[ and ⌃⇥ ⌃⇧⇥ go to
//! the next and previous tab, wrapping around (`step_key`, `App::cycle`).

use rune::{Align, Color, El, Side, TextStyle, Tx, col, el, row, text};

use super::sidebar::{self, Hooks, Motion};
use super::theme::{HAIRLINE, KBD_PAD_X, KBD_PAD_Y, KBD_TEXT, KBD_WEIGHT, R_KBD, R_PANEL, R_ROW, ROW, ROW_GAP, ROW_PAD, SIDEBAR, TAB_BAR_H, TAB_BAR_LEAD, TAB_BAR_PAD_X, TAB_DIVIDER, TAB_H, TAB_MAX_W, TAB_PAD_X, TAB_TOP, Theme, UI_SMALL, UI_WEIGHT};
use crate::app::App;
use crate::fx::ring::RING_W;
use crate::fx::{self, Mark};

/// The text of the key on the right (approved, DESIGN.md "Layout").
pub const SOURCE_KEY: &str = "⌘E source";

/// The text width ↔ button and its popover.
pub const WIDTH_BUTTON: &str = "width-button";
pub const WIDTH_POPOVER: &str = "width-popover";
/// The button glyph, pt.
const WIDTH_GLYPH: f32 = 15.0;
/// Between the button and the popover, pt.
const POPOVER_GAP: f32 = 4.0;
/// The tools on the right of the bar (feedback group 3, item 4): 4 pt between the gear and ↔ (their
/// glyphs then sit 17 apart), and `SOURCE_GAP` before "⌘E source", so the ↔ glyph and the key's border
/// are as far apart as the two glyphs; the key no longer looks stuck to ↔.
pub const TOOL_GAP: f32 = 4.0;
pub const SOURCE_GAP: f32 = 10.0;
/// Below the slider in the popover: the shortcuts.
pub const WIDTH_HINT: &str = "⌘⌥+ and ⌘⌥− one step at a time";

/// ⌘⌥+ (or ⌘⌥=) and ⌘⌥-: one text width step, +1 or −1. `base` is the character of the
/// key without modifiers (`Tx::key_base`): with ⌥ macOS can give "≠" or "–".
pub fn width_key(key: &rune::KeyEvent, base: Option<char>) -> Option<i32> {
    let m = key.mods;
    if !(m.cmd && m.alt) || m.ctrl {
        return None;
    }
    let rune::Key::Command(c) = key.key else { return None };
    match base.unwrap_or(c) {
        '=' | '+' => Some(1),
        '-' => Some(-1),
        _ => None,
    }
}

/// ⌘⇧] and ⌘⇧[: the next (+1) or previous (−1) tab, wrapping (record
/// 2026-11-18-release-small-choices, constraint 3). With ⇧ macOS gives "}" / "{" on a US layout, so
/// both the character and `base` (`Tx::key_base`, the key without modifiers) count.
pub fn step_key(key: &rune::KeyEvent, base: Option<char>) -> Option<isize> {
    let m = key.mods;
    if !(m.cmd && m.shift) || m.alt || m.ctrl {
        return None;
    }
    let rune::Key::Command(c) = key.key else { return None };
    [Some(c), base].into_iter().flatten().find_map(|c| match c {
        ']' | '}' => Some(1),
        '[' | '{' => Some(-1),
        _ => None,
    })
}

/// The ↔ button: 28 × 28, radius `R_ROW`, glyph in `fg2`, hover `sel`; with the popover open
/// it has the `sel` fill. Opens and closes the popover.
fn width_button(s: &App, theme: &Theme, motion: Motion) -> El {
    let glyph = text("↔").key("width-button-glyph").font(TextStyle::new(WIDTH_GLYPH, UI_WEIGHT)).color(theme.fg2).a11y_hidden();
    let mut b = el()
        .key(WIDTH_BUTTON)
        .size(ROW, ROW)
        .radius(R_ROW)
        .center()
        .role("button")
        .label(super::settings::WIDTH)
        .expanded(s.width_popover)
        .child(glyph)
        .help(super::settings::WIDTH)
        .on_press(toggle_width);
    if s.width_popover {
        b = b.fill(theme.sel);
    } else if motion.hover {
        b = b.hover(theme.sel);
    }
    b
}

/// Opens or closes the width popover.
pub fn toggle_width(s: &mut App) {
    s.width_popover = !s.width_popover;
}

/// The width popover, to mount at the root after the bar: a transparent ground that
/// closes on a click outside, and the surface below the button, aligned to its right edge.
/// `panel` surface, radius `R_PANEL`, panel border and shadow; inside, the title in `fg2`,
/// the settings slider and the shortcuts in `fg3`. Empty if closed.
pub fn width_popover(s: &App, theme: &Theme) -> Vec<El> {
    if !s.width_popover {
        return Vec::new();
    }
    let scrim = el().key("width-scrim").role("none").absolute(0.0, 0.0).w_full().h_full().on_press(|s: &mut App| s.width_popover = false);
    let title = text(super::settings::WIDTH).key("width-popover-title").font(sidebar::ui_font()).color(theme.fg2).px(ROW_PAD);
    let hint = text(WIDTH_HINT).key("width-popover-hint").font(TextStyle::new(UI_SMALL, UI_WEIGHT)).color(theme.fg3).px(ROW_PAD);
    let surface = col()
        .key(WIDTH_POPOVER)
        .role("dialog")
        .label(super::settings::WIDTH)
        .focus_return(WIDTH_BUTTON)
        .w(SIDEBAR)
        .py(ROW_GAP + 2.0)
        .fill(theme.panel)
        .radius(R_PANEL)
        .border(HAIRLINE, theme.fg.alpha(0.10))
        .shadow(10.0, 30.0, Color::BLACK.alpha(0.35))
        .child(title)
        .child(super::settings::width_slider("width-slider", s.settings.column, theme))
        .child(hint)
        .anchored(WIDTH_BUTTON, Side::Bottom, Align::End, POPOVER_GAP);
    vec![scrim, surface]
}

/// The bar's key and the scrolling strip's (`App::restore_view` reveals the front tab in it).
pub const STRIP: &str = "tabs-scroll";

/// Where the strip passes under the bar's buttons (record 2026-11-18-vscode-tabs, "un leggero blur"):
/// `FADE_W` of the buttons' backdrop over the strip's end, blurred by `FADE_BLUR` and fading in from
/// the left; the strip's content ends `FADE_W` later, so the last tab can come out from under it, and a
/// tab brought to the front stops `FADE_W` clear of either edge.
pub const FADE_W: f32 = 24.0;
const FADE_BLUR: f32 = 6.0;
/// The room after the last tab and around a tab brought into view: the fade plus the front tab's
/// metal edge and a little air, so the edge never runs under the fade.
const END_ROOM: f32 = FADE_W + RING_W + 4.0;
/// How much of the bar's colour the buttons' backdrop keeps over the tabs passing under it.
const FADE_TINT: f32 = 0.78;

/// The scroll bar under the tabs (Matteo: "con uno slider anche visibile"): `SCROLL_BAR` tall, at the
/// strip's bottom, behind the tabs and below the front tab's ring, drawn only while the tabs overflow. Rune's own bars
/// follow the system setting and hide at rest, so the strip draws its own.
pub const SCROLL_BAR: f32 = 3.0;
const SCROLL_BAR_MIN: f32 = 24.0;

/// The thumb's rect in the track, `[x, 0, w, SCROLL_BAR]`, for a strip at `offset` of `max`,
/// `viewport` wide: as wide as the visible share of the tabs, at least `SCROLL_BAR_MIN`; zero wide when
/// nothing overflows.
pub fn thumb(offset: f32, max: f32, viewport: f32) -> [f32; 4] {
    if max <= 0.5 || viewport <= 0.0 {
        return [0.0, 0.0, 0.0, SCROLL_BAR];
    }
    let w = (viewport * viewport / (viewport + max)).max(SCROLL_BAR_MIN).min(viewport);
    let x = (offset / max).clamp(0.0, 1.0) * (viewport - w);
    [x, 0.0, w, SCROLL_BAR]
}

/// The bar: `TAB_BAR_H` tall on the `tab_bar` tint, with a 0.5 line at the bottom. The tabs scroll sideways under the buttons on the right. `hover`: the pointer is a fine one
/// and can hover.
pub fn view(s: &App, hooks: Hooks<App>, theme: &Theme, reduced: bool, hover: bool) -> El {
    let motion = Motion { reduced, hover };
    let n = s.tabs.len();
    let list = row().key("tabs").role("tablist").label("Tabs").h_full().items_start().pt(TAB_TOP).pl(TAB_BAR_LEAD).pr(END_ROOM)
        .children(s.tabs.iter().enumerate().map(|(i, tab)| tab_row(s, hooks, i, n, tab, theme, motion)))
        .on_layout_tx(|s: &mut App, r: [f32; 4], tx: &mut Tx| measure_tabs(s, 0, r[2], tx));
    // The thumb follows the scroll in the same frame, without running the view.
    let at = rune::Live::new(thumb(0.0, 0.0, 0.0));
    let set = at.clone();
    let strip = row().key(STRIP).w_full().h_full().scroll_x().scroll_margin(END_ROOM).scroll_edge_fade(FADE_W)
        .on_scroll_offset(move |p: rune::ScrollPos| { set.set(thumb(p.offset[0], p.max[0], p.viewport[0])); })
        .on_layout_tx(|s: &mut App, r: [f32; 4], tx: &mut Tx| measure_tabs(s, 1, r[2], tx))
        .child(list);
    let track = el().key("tabs-scroll-bar").pin(Some(0.0), Some(SCROLL_BAR_Y), Some(0.0), None).h(SCROLL_BAR).a11y_hidden()
        .child(el().key("tabs-scroll-thumb").pin(Some(0.0), Some(0.0), None, None).h(SCROLL_BAR).radius(SCROLL_BAR / 2.0).fill(theme.fg3.alpha(0.45)).live_rect(&at));
    // The track is drawn before the strip, under the tabs, in the room between the front tab's ring
    // and the bar's line (`_RING_FITS`).
    let strip_box = el().key("tabs-strip").grow(1.0).shrink().min_w(0.0).h_full().child(track).child(strip);
    let key = el().key("source-key").border(HAIRLINE, theme.line).radius(R_KBD).px(KBD_PAD_X).py(KBD_PAD_Y)
        .child(text(SOURCE_KEY).font(TextStyle::new(KBD_TEXT, KBD_WEIGHT)).color(theme.fg3));
    // Find (Cmd+F): the field sits in the bar, right of the tabs, so the document does not move.
    let find = s.find.open.then(|| super::find::field(s, theme).mr(ROW_GAP));
    let tools = row().key("bar-tools").gap(TOOL_GAP).center_y().child(super::shortcuts::button(s, theme, motion)).child(width_button(s, theme, motion)).child(key.ml(SOURCE_GAP - TOOL_GAP));
    // The buttons' backdrop: the bar's own colour, a little see-through over a blur, fading in over
    // `FADE_W` on the left, laid over the strip's end (`ml(-FADE_W)`). The blur only while the tabs
    // overflow: otherwise only the flat bar is under it, and it would cost every metal frame ~1 ms.
    let bar_color = super::theme::flatten(theme.tab_bar(), theme.panel);
    let end = row().key("bar-end").h(TAB_BAR_H).pt(TAB_TOP).pb(MID_PAD).ml(-FADE_W).pl(FADE_W + ROW_GAP).pr(TAB_BAR_PAD_X).center_y()
        .backdrop_blur(if s.tabs_overflow { FADE_BLUR } else { 0.0 }).fill(bar_color.alpha(FADE_TINT)).clip_fade_linear(FADE_W, 0.0, 0.0, 0.0)
        .children(find).child(tools);
    let line = el().key("tab-bar-line").pin(Some(0.0), None, Some(0.0), Some(0.0)).h(HAIRLINE).fill(theme.line);
    // The bar moves the window like a title bar (`main`: `Chrome::drag_regions`); its tabs and buttons
    // keep their presses, so a double click on a tab pins it and does not zoom the window.
    row().key("tab-bar").window_drag().w_full().h(TAB_BAR_H + HAIRLINE).fill(theme.tab_bar()).child(line).child(strip_box).child(end)
}

/// The strip's tabs (`which` 0) or viewport (1) laid out `w` wide: the view runs again only when the
/// tabs start or stop running past the viewport (`App::tabs_overflow`).
fn measure_tabs(s: &mut App, which: usize, w: f32, tx: &mut Tx) {
    s.tab_widths[which] = w;
    let overflow = s.tab_widths[0] > s.tab_widths[1] + 0.5;
    if overflow == s.tabs_overflow {
        tx.live_only();
    }
    s.tabs_overflow = overflow;
}

/// The × area (16 pt), its margin from the tab's right edge (8, the sidebar's `row-gap`) and from the
/// name (6). Its place is always there, so hover does not change the tab's width.
pub const CLOSE: f32 = 16.0;
pub const CLOSE_PAD: f32 = ROW_GAP;
pub const CLOSE_GAP: f32 = 6.0;
const R_CLOSE: f32 = 4.0;

/// The front tab's pill (record 2026-11-18-tab-bar-style, constraint 2, as in the installed app): the
/// tab's full width, `PILL_H` tall, in `sel`. The metal ring runs closed `RING_W` outside it, inside
/// the bar: never on the page, with `RING_GAP` (8) above and below it, everything on whole device pixels.
pub const PILL_H: f32 = 24.0;
/// The pill's corner radius: half the dark page panel's (`R_PANEL / 2` = 5 pt), not half circles
/// (record 2026-11-18-tab-bar-style, constraint 5; Matteo, scrawl 1795009884949: "il border radius
/// dovrebbe essere la metà del border radius del container piu' scuro"). The ring stays concentric:
/// its outer radius is `PILL_R + RING_W` (7 pt, `fx::ring::chrome`).
pub const PILL_R: f32 = R_PANEL / 2.0;
const _PILL_R_IS_HALF_THE_PANEL: () = assert!(PILL_R == R_PANEL / 2.0 && PILL_R == 5.0 && PILL_R + RING_W == 7.0 && PILL_R < PILL_H / 2.0);
/// The room above the ring (to the bar's top) and below it (to the bar's line): the same.
pub const RING_GAP: f32 = (TAB_BAR_H - PILL_H - 2.0 * RING_W) / 2.0;
/// The pill's top and middle, from the bar's top.
const PILL_TOP: f32 = RING_GAP + RING_W;
const PILL_MID: f32 = PILL_TOP + PILL_H / 2.0;
/// The pill in the tab: from the tab's top, and the tab's bottom room that puts the name and × on the
/// pill's middle (the buttons on the right share that middle, `MID_PAD`).
const PILL_Y: f32 = PILL_TOP - TAB_TOP;
const TAB_PAD_B: f32 = TAB_H - 2.0 * PILL_Y - PILL_H;
const MID_PAD: f32 = TAB_BAR_H + TAB_TOP - 2.0 * PILL_MID;
/// The scroll bar's top from the bar's top: in the middle of the room under the ring, so it touches
/// neither the ring nor the bar's line.
pub const SCROLL_BAR_Y: f32 = PILL_TOP + PILL_H + RING_W + (RING_GAP - SCROLL_BAR) / 2.0;

/// The front tab's ring, `RING_W` outside the pill: inside the bar, the same room above and below,
/// clear of the first tab's lead, only over the neighbours' padding (never their names or ×), and
/// clear of the scroll bar, which ends above the bar's line.
const _RING_FITS: () = assert!(
    RING_GAP >= 2.0 * RING_W
        && TAB_TOP + PILL_Y - RING_W == TAB_BAR_H - (PILL_TOP + PILL_H + RING_W)
        && PILL_Y >= 0.0
        && TAB_PAD_B >= 0.0
        && TAB_BAR_LEAD > RING_W
        && TAB_PAD_X >= PILL_R
        && CLOSE_PAD > RING_W
        && PILL_TOP + PILL_H + RING_W < SCROLL_BAR_Y
        && SCROLL_BAR_Y + SCROLL_BAR < TAB_BAR_H
);

/// Closes tab `id` (middle click, ×): with unsaved changes it brings it to the front with the
/// question, like Cmd+W (`App::close`).
fn close_tab(s: &mut App, id: u64, tx: &mut Tx) {
    let Some(i) = s.tabs.iter().position(|t| t.doc.id == id) else { return };
    let before = s.tab().map(|t| t.doc.id);
    s.close(i);
    if s.tab().map(|t| t.doc.id) != before {
        s.restore_view(tx);
    }
}

/// The × at the end of the tab: an area of `CLOSE` pt, glyph in `fg2`, hover `sel`. Its place is
/// always there, so the name does not move; on the front tab it always shows, on the others only with
/// the pointer (or keyboard focus) on the tab (`hover_group`), the × itself included: no `hover_alone`,
/// which would rest the tab's group under the pointer and hide the × it is about to click.
fn close_button(key: &str, id: u64, title: &str, front: bool, theme: &Theme, motion: Motion) -> El {
    let glyph = text("×").key(format!("{key}-close-glyph")).font(TextStyle::new(CLOSE_GLYPH, KBD_WEIGHT)).color(theme.fg2).a11y_hidden();
    let mut b = el()
        .key(format!("{key}-close"))
        .size(CLOSE, CLOSE)
        .radius(R_CLOSE)
        .center()
        .role("button")
        .label(format!("Close {title}"))
        .child(glyph)
        .on_press_tx(move |s: &mut App, tx: &mut Tx| close_tab(s, id, tx));
    if motion.hover {
        b = b.hover(theme.sel);
    }
    // On the button, not on the glyph: the glyph's nearest group would be the button (it has a hover).
    if !front {
        b = b.group_opacity(0.0, 1.0);
    }
    b
}

/// The × glyph, pt.
const CLOSE_GLYPH: f32 = 14.0;

/// A tab (record 2026-11-18-vscode-tabs): `TAB_H` tall, edge to edge with the others, `TAB_PAD_X` before
/// the name and the × after it. Front: a `sel` pill (`PILL_H`, corners `PILL_R`) with the metal
/// ring round it, text `fg`, no divider. The others: no fill (the bar's tint), text `fg2`, hover `sel`, a `TAB_DIVIDER` line in
/// `line` on the right unless the next tab is the front one.
fn tab_row(s: &App, hooks: Hooks<App>, i: usize, n: usize, tab: &crate::app::Tab, theme: &Theme, motion: Motion) -> El {
    let front = i == s.active;
    let key = tab_key(tab.doc.id);
    let title = tab.title();
    let id = tab.doc.id;
    // The preview tab has its name in italics (like VS Code).
    let font = if tab.preview { sidebar::ui_font().italic() } else { sidebar::ui_font() };
    let label = text(title.clone()).key(format!("{key}-label")).font(font).color(if front { theme.fg } else { theme.fg2 }).truncate_middle().shrink().min_w(0.0).a11y_hidden();
    let close = close_button(&key, id, &title, front, theme, motion);
    // At most `TAB_MAX_W`, the name cut in the middle: a long tab does not push the others
    // out.
    // The pill behind the name, and the ring's host: always present, so the structure does not change
    // when the tab comes to the front or the ring lights up.
    let mut pill = el().key(format!("{key}-pill")).pin(Some(0.0), Some(PILL_Y), Some(0.0), None).h(PILL_H).radius(PILL_R).a11y_hidden();
    if front {
        pill = pill.fill(theme.sel);
    }
    let pill = fx::ring::slot(pill, Mark::TabRing(i as u32), fx::ring::edge(s.ring_of(tab.path()), front), motion.reduced, theme.dark);
    let e = row()
        .max_w(TAB_MAX_W)
        .h(TAB_H)
        .pb(TAB_PAD_B)
        .pl(TAB_PAD_X)
        .pr(CLOSE_PAD)
        .gap(CLOSE_GAP)
        .center_y()
        .role("tab")
        .label(title)
        .focus_selected(front)
        .hover_group()
        .child(pill)
        .child(label)
        .child(close)
        .on_press_tx(move |s: &mut App, tx: &mut Tx| {
            if let Some(i) = s.tabs.iter().position(|t| t.doc.id == id)
                && s.select(i)
            {
                s.restore_view(tx);
            }
        })
        .on_double_press(move |s: &mut App| s.pin(id))
        .on_middle_press(move |s: &mut App, tx: &mut Tx| close_tab(s, id, tx));
    let divider = !front && i + 1 != s.active && i + 1 < n;
    let e = sidebar::pressable(e, key.clone(), &s.sidebar, hooks, theme, motion, !front).corners([R_ROW, R_ROW, 0.0, 0.0]).key(key.clone());
    // The front tab comes out from under the bar's buttons and the strip's ends, however it got to the
    // front (click, keys, a file opened, launch): a marker keyed by the document is new each time the
    // front changes, so its first layout scrolls the strip once; later layouts (the strip scrolling by
    // hand) leave it.
    let e = if front {
        e.child(el().key(format!("{key}-reveal")).pin(Some(0.0), Some(0.0), None, None).size(0.0, 0.0).a11y_hidden().on_layout_tx(move |s: &mut App, _: [f32; 4], tx: &mut Tx| {
            if s.revealed_tab != Some(id) {
                s.revealed_tab = Some(id);
                tx.scroll_into_view_in(STRIP, tab_key(id));
            }
            tx.live_only();
        }))
    } else {
        e
    };
    if divider {
        e.child(el().key("tab-divider").pin(None, Some(TAB_DIVIDER_INSET), Some(0.0), Some(TAB_DIVIDER_INSET)).w(TAB_DIVIDER).fill(theme.line).a11y_hidden())
    } else {
        e
    }
}

/// The divider between back tabs stops this far from the tab's top and bottom, so it reads as a light
/// stroke rather than a wall (Matteo: "un leggero bordino").
const TAB_DIVIDER_INSET: f32 = 9.0;

/// The key of the tab of document `id`.
pub fn tab_key(id: u64) -> String {
    format!("t-{id:x}")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use rune::{Env, Key, KeyEvent, Mods, Runtime};

    use super::*;
    use crate::doc::document::Document;
    use crate::settings::COLUMN_FULL;

    fn cmd_alt(c: char) -> KeyEvent {
        KeyEvent { key: Key::Command(c), mods: Mods { cmd: true, alt: true, ..Mods::default() } }
    }

    #[test]
    fn cmd_alt_plus_and_minus_step_the_width() {
        assert_eq!(width_key(&cmd_alt('='), None), Some(1));
        assert_eq!(width_key(&cmd_alt('+'), None), Some(1));
        assert_eq!(width_key(&cmd_alt('-'), None), Some(-1));
        // With ⌥ macOS can give another character ("≠", "–"): count the key without modifiers.
        assert_eq!(width_key(&cmd_alt('≠'), Some('=')), Some(1));
        assert_eq!(width_key(&cmd_alt('–'), Some('-')), Some(-1));
        // ⌘+ and ⌘- without ⌥, or with Ctrl, are not ours.
        let plain = KeyEvent { key: Key::Command('='), mods: Mods { cmd: true, ..Mods::default() } };
        assert_eq!(width_key(&plain, None), None);
        let ctrl = KeyEvent { key: Key::Command('='), mods: Mods { cmd: true, alt: true, ctrl: true, ..Mods::default() } };
        assert_eq!(width_key(&ctrl, None), None);
        assert_eq!(width_key(&cmd_alt('e'), None), None);
    }

    fn open() -> Runtime<App> {
        let mut state = App::bare(crate::index::Library::load(Vec::new(), None));
        state.push_tab(Document::from_source(Path::new("t.md"), "# Title\n\nText.\n"));
        let mut rt = Runtime::new(state, crate::view, (1200.0, 900.0), Env::new(true, 2.0));
        rt.on_key = Some(crate::on_key);
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
        panic!("the view does not settle");
    }

    fn find(rt: &Runtime<App>, key: &str) -> Option<rune::ElementInfo> {
        rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key))
    }

    fn tap(rt: &mut Runtime<App>, x: f32, y: f32) {
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        settle(rt);
    }

    fn value(rt: &Runtime<App>) -> String {
        find(rt, "width-slider-value").and_then(|e| e.text).expect("value")
    }

    /// Feedback group 3, item 6: a double click on the preview tab pins it (italic → normal) and asks
    /// nothing of the window; the bar around the tabs is a drag region, so a double click there is the
    /// title bar's (zoom). With `Chrome::drag_regions` AppKit no longer zooms on its own in the band.
    #[test]
    fn a_double_click_on_a_tab_pins_it_and_does_not_zoom() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("preview.md");
        std::fs::write(&p, "# Preview\n").unwrap();
        let mut rt = open();
        rt.update(|s: &mut App, _| _ = s.open_preview(p.clone()));
        settle(&mut rt);
        let tab = rt.state.tabs.iter().find(|t| t.preview).map(|t| t.doc.id).expect("a preview tab");
        let r = find(&rt, &format!("t-{tab:x}-label")).expect("tab label").rect;
        let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.set_native_titlebar_band(28.0);
        for _ in 0..2 {
            rt.pointer_move(x, y);
            rt.pointer_down(x, y);
            assert_eq!(rt.take_titlebar_action(), None, "a press on a tab never moves or zooms the window");
            rt.pointer_up(x, y);
        }
        settle(&mut rt);
        assert!(!rt.state.tabs.iter().any(|t| t.preview), "pinned");
        let bar = find(&rt, "tab-bar").unwrap().rect;
        let tools = find(&rt, "bar-tools").unwrap().rect;
        let (gx, gy) = ((r[0] + r[2] + tools[0]) / 2.0 + 40.0, bar[1] + bar[3] / 2.0);
        assert!(gx < tools[0], "an empty spot of the bar");
        rt.pointer_move(gx, gy);
        rt.pointer_down(gx, gy);
        assert_eq!(rt.take_titlebar_action(), Some(rune::TitlebarAction::Drag), "the bar drags the window");
        rt.pointer_up(gx, gy);
        rt.pointer_down(gx, gy);
        assert_eq!(rt.take_titlebar_action(), Some(rune::TitlebarAction::DoubleClick), "and zooms it on a double click");
        rt.pointer_up(gx, gy);
        assert!(crate::window_chrome().drag_regions, "only drag regions move the window");
    }

    /// Feedback group 3, item 4: gear, ↔ and "⌘E source" with even gaps: the gear and ↔ 4 apart, and the
    /// key `SOURCE_GAP` from ↔, so the glyph-to-border gap matches the glyph-to-glyph one (± 2 pt).
    #[test]
    fn the_bar_tools_have_even_gaps() {
        let rt = open();
        let gear = find(&rt, crate::ui::shortcuts::SETTINGS_BUTTON).unwrap().rect;
        let w = find(&rt, WIDTH_BUTTON).unwrap().rect;
        let key = find(&rt, "source-key").unwrap().rect;
        let gg = find(&rt, "settings-button-glyph").unwrap().rect;
        let wg = find(&rt, "width-button-glyph").unwrap().rect;
        assert!((w[0] - (gear[0] + gear[2]) - TOOL_GAP).abs() < 0.5, "gear | ↔: {gear:?} {w:?}");
        assert!((key[0] - (w[0] + w[2]) - SOURCE_GAP).abs() < 0.5, "↔ | ⌘E: {w:?} {key:?}");
        let glyphs = wg[0] - (gg[0] + gg[2]);
        let to_key = key[0] - (wg[0] + wg[2]);
        assert!((glyphs - to_key).abs() <= 3.0, "glyph to glyph {glyphs}, glyph to key {to_key}");
        let bar = find(&rt, "tab-bar").unwrap().rect;
        assert!(key[0] + key[2] <= bar[0] + bar[2] + 1.0, "inside the bar");
    }

    /// The ↔ button sits left of "⌘E source" and opens the popover with the width
    /// slider; Esc or a click outside close it. ⌘⌥+ and ⌘⌥- change the same
    /// `settings.column` as the settings slider, even with the popover closed.
    #[test]
    fn the_width_button_opens_a_popover_with_the_width_slider() {
        let mut rt = open();
        let button = find(&rt, WIDTH_BUTTON).expect("button");
        let source = find(&rt, "source-key").unwrap();
        assert_eq!(button.role, "button");
        assert!(button.rect[0] + button.rect[2] <= source.rect[0], "left of ⌘E: {:?} {:?}", button.rect, source.rect);
        assert!(find(&rt, WIDTH_POPOVER).is_none());

        tap(&mut rt, button.rect[0] + button.rect[2] / 2.0, button.rect[1] + button.rect[3] / 2.0);
        let pop = find(&rt, WIDTH_POPOVER).expect("popover open");
        assert!(pop.rect[1] >= button.rect[1] + button.rect[3], "below the button");
        let slider = find(&rt, "width-slider").expect("slider");
        assert_eq!(slider.role, "slider");
        assert_eq!(value(&rt), "680 pt");

        // ⌘⌥+ twice, ⌘⌥- once: 760 pt, and the popover shows it.
        rt.key(cmd_alt('='));
        rt.key(cmd_alt('='));
        rt.key(cmd_alt('-'));
        settle(&mut rt);
        assert_eq!(rt.state.settings.column, 760.0);
        assert_eq!(value(&rt), "760 pt");
        rt.key(cmd_alt('-'));
        settle(&mut rt);

        // A drag lands between the old steps, per pt, and both sliders show it; the drag end saves it.
        let slider = find(&rt, "width-slider").unwrap();
        let y = slider.rect[1] + slider.rect[3] / 2.0;
        let x0 = slider.rect[0] + 6.0;
        rt.pointer_move(x0, y);
        rt.pointer_down(x0, y);
        let x1 = slider.rect[0] + 6.0 + (slider.rect[2] - 12.0) * crate::settings::column_t(803.0);
        rt.pointer_move(x1, y);
        rt.frame(1.0 / 60.0);
        let mid = rt.state.settings.column;
        assert!((mid - 803.0).abs() <= 1.0 && mid.fract() == 0.0, "continuous, whole pt: {mid}");
        rt.pointer_up(x1, y);
        settle(&mut rt);
        assert_eq!(value(&rt), format!("{mid:.0} pt"));
        assert!(rt.state.width_popover, "the drag keeps the popover open");
        rt.update(|s: &mut App, _| {
            s.width_popover = false;
            s.toggle_settings();
        });
        settle(&mut rt);
        assert_eq!(find(&rt, "set-width-value").and_then(|e| e.text), Some(format!("{mid:.0} pt")), "the settings slider shows the same value");
        rt.update(|s: &mut App, _| {
            s.toggle_settings();
            s.width_popover = true;
        });
        settle(&mut rt);
        // ⌘⌥+ from between the steps: the next step.
        rt.key(cmd_alt('='));
        settle(&mut rt);
        assert_eq!(rt.state.settings.column, 840.0, "the shortcut goes to the next step");
        rt.key(cmd_alt('-'));
        rt.key(cmd_alt('-'));
        settle(&mut rt);
        assert_eq!(rt.state.settings.column, 680.0);

        // A click at the end of the slider: "Full", like the settings one.
        let slider = find(&rt, "width-slider").unwrap();
        tap(&mut rt, slider.rect[0] + slider.rect[2] - 1.0, slider.rect[1] + slider.rect[3] / 2.0);
        assert_eq!(rt.state.settings.column, COLUMN_FULL);
        assert_eq!(value(&rt), "Full");

        rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
        settle(&mut rt);
        assert!(find(&rt, WIDTH_POPOVER).is_none(), "Esc closes");

        // With the popover closed the shortcuts work all the same.
        rt.key(cmd_alt('-'));
        settle(&mut rt);
        assert_eq!(rt.state.settings.column, 1240.0);

        // Reopened, a click on the document closes it without changing the width.
        let button = find(&rt, WIDTH_BUTTON).unwrap();
        tap(&mut rt, button.rect[0] + button.rect[2] / 2.0, button.rect[1] + button.rect[3] / 2.0);
        assert!(find(&rt, WIDTH_POPOVER).is_some());
        tap(&mut rt, 600.0, 700.0);
        assert!(find(&rt, WIDTH_POPOVER).is_none(), "a click outside closes");
        assert_eq!(rt.state.settings.column, 1240.0);

        // The button again: opens and closes.
        tap(&mut rt, button.rect[0] + button.rect[2] / 2.0, button.rect[1] + button.rect[3] / 2.0);
        tap(&mut rt, button.rect[0] + button.rect[2] / 2.0, button.rect[1] + button.rect[3] / 2.0);
        assert!(find(&rt, WIDTH_POPOVER).is_none());
    }

    fn tabs(names: &[&str]) -> Runtime<App> {
        let mut state = App::bare(crate::index::Library::load(Vec::new(), None));
        for n in names {
            state.push_tab(Document::from_source(Path::new(n), &format!("# {n}\n")));
        }
        let mut rt = Runtime::new(state, crate::view, (1200.0, 900.0), Env::new(true, 2.0));
        rt.on_key = Some(crate::on_key);
        rt.frame(0.0);
        settle(&mut rt);
        rt
    }

    /// Record 2026-11-18-tab-bar-style, constraint 2 (the installed app): the front tab is a pill inside
    /// the bar with a closed metal ring round it. The ring's shape is `RING_W` outside the pill, all of it
    /// inside the slot's clip (no side cut), inside the bar and above the page's top edge, and above the
    /// scroll bar, which shows whole below it when the tabs overflow.
    #[test]
    fn the_front_tab_is_a_pill_with_a_closed_ring_inside_the_bar() {
        for n in [3, 16] {
            let names: Vec<String> = (0..n).map(|i| format!("a-long-file-name-{i:02}.md")).collect();
            let mut rt = tabs(&names.iter().map(String::as_str).collect::<Vec<_>>());
            rt.track_marks = true;
            rt.frame(1.0 / 60.0);
            let k = 2.0;
            let slots = crate::fx::tab_ring_slots(&rt);
            assert_eq!(slots.len(), 1, "only the front tab has metal: {slots:?}");
            let (_, clip, layer) = slots[0];
            let front = tab_key(rt.state.tabs[rt.state.active].doc.id);
            let pill = find(&rt, &format!("{front}-pill")).expect("pill").rect;
            let tab = find(&rt, &front).expect("front tab").rect;
            assert!((pill[3] - PILL_H).abs() < 0.01 && (pill[0] - tab[0]).abs() < 0.01 && (pill[2] - tab[2]).abs() < 0.01, "the pill spans the tab, {PILL_H} tall: {pill:?} {tab:?}");
            assert_eq!(layer.shape.radius, R_PANEL / 2.0 + RING_W, "the ring concentric with the pill (whose radius the compositor reads): outer radius R_PANEL / 2 + RING_W");
            let label = find(&rt, &format!("{front}-label")).expect("label").rect;
            let mid = pill[1] + pill[3] / 2.0;
            assert!((label[1] + label[3] / 2.0 - mid).abs() < 0.51, "the name on the pill's middle: {label:?} {pill:?}");
            let r = layer.shape.rect;
            let (x0, y0, x1, y1) = (r.x, r.y, r.x + r.w, r.y + r.h);
            assert!((x0 - (pill[0] - RING_W)).abs() < 0.01 && (y0 - (pill[1] - RING_W)).abs() < 0.01 && (x1 - (pill[0] + pill[2] + RING_W)).abs() < 0.01 && (y1 - (pill[1] + pill[3] + RING_W)).abs() < 0.01, "RING_W outside the pill: {r:?} {pill:?}");
            assert!(clip[0] <= x0 * k && clip[1] <= y0 * k && x1 * k <= clip[2] && y1 * k <= clip[3], "{n} tabs: closed on all four sides, nothing cut: {clip:?} vs {r:?}");
            let bar = find(&rt, "tab-bar").expect("bar").rect;
            let page = bar[1] + bar[3];
            assert!(y0 > bar[1] && y1 < page, "{n} tabs: the ring is inside the bar, its bottom {y1} above the page's top {page}");
            // Matteo 2026-11-18: the same room above the ring (to the bar's top) and below it (to the
            // bar's line), ±0.5 pt.
            let line = find(&rt, "tab-bar-line").expect("line").rect;
            let (above, below) = (y0 - bar[1], line[1] - y1);
            assert!((above - below).abs() <= 0.5 && below >= 2.0 * RING_W, "{n} tabs: {above} above the ring, {below} below");
            let track = find(&rt, "tabs-scroll-bar").expect("scroll bar").rect;
            assert!(y1 < track[1] && track[1] + track[3] < line[1], "{n} tabs: the scroll bar {track:?} between the ring's bottom {y1} and the line {line:?}, touching neither");
            if n == 16 {
                assert!(find(&rt, "tabs-scroll-thumb").expect("thumb").rect[2] >= SCROLL_BAR_MIN, "the thumb shows");
            }
        }
    }

    fn close_of(rt: &Runtime<App>, i: usize) -> rune::ElementInfo {
        find(rt, &format!("t-{:x}-close", rt.state.tabs[i].doc.id)).expect("×")
    }

    /// Record 2026-11-18-release-small-choices, constraint 2: a back tab shows its × with the pointer on
    /// the tab and keeps it with the pointer on the × itself; a click there closes that tab only, and the
    /// front tab stays in front with its ×. The × has its place at rest too, so the name does not move.
    #[test]
    fn a_back_tab_shows_its_close_on_hover_and_closes_alone() {
        let mut rt = tabs(&["a.md", "b.md", "c.md"]);
        assert_eq!(rt.state.active, 2);
        assert_eq!((close_of(&rt, 0).opacity, close_of(&rt, 1).opacity, close_of(&rt, 2).opacity), (0.0, 0.0, 1.0), "at rest only the front ×");
        let label = |rt: &Runtime<App>, i: usize| find(rt, &format!("t-{:x}-label", rt.state.tabs[i].doc.id)).expect("label").rect;
        let before = label(&rt, 0);
        rt.pointer_move(before[0] + 4.0, before[1] + before[3] / 2.0);
        settle(&mut rt);
        assert_eq!(close_of(&rt, 0).opacity, 1.0, "pointer on the name: the × shows");
        assert_eq!(label(&rt, 0), before, "the name does not move or change width");
        let x = close_of(&rt, 0).rect;
        rt.pointer_move(x[0] + x[2] / 2.0, x[1] + x[3] / 2.0);
        settle(&mut rt);
        assert_eq!(close_of(&rt, 0).opacity, 1.0, "pointer on the ×: it stays");
        let (a, c) = (rt.state.tabs[0].doc.id, rt.state.tabs[2].doc.id);
        tap(&mut rt, x[0] + x[2] / 2.0, x[1] + x[3] / 2.0);
        let left: Vec<u64> = rt.state.tabs.iter().map(|t| t.doc.id).collect();
        assert_eq!(left.len(), 2, "one tab closed");
        assert!(!left.contains(&a), "the hovered one");
        assert_eq!(rt.state.tab().map(|t| t.doc.id), Some(c), "the front tab stays in front");
        assert_eq!(close_of(&rt, rt.state.active).opacity, 1.0, "with its ×");
    }

    /// Record 2026-11-18-vscode-tabs: tabs sit edge to edge from `TAB_TOP` to the bar's bottom, `TAB_PAD_X`
    /// before the name, 6 between name and ×, 8 after it; a divider between back tabs only.
    #[test]
    fn the_tabs_sit_edge_to_edge_like_vs_code() {
        let rt = tabs(&["alpha.md", "beta.md", "gamma.md", "delta.md"]);
        let tab = |i: usize| find(&rt, &tab_key(rt.state.tabs[i].doc.id)).expect("tab").rect;
        let label = |i: usize| find(&rt, &format!("{}-label", tab_key(rt.state.tabs[i].doc.id))).expect("label").rect;
        let bar = find(&rt, "tab-bar").expect("bar").rect;
        assert_eq!(bar[3], TAB_BAR_H + HAIRLINE, "the bar stays 44 tall, with its line");
        assert!((tab(0)[0] - bar[0] - TAB_BAR_LEAD).abs() < 0.01, "{TAB_BAR_LEAD} before the first tab");
        for i in 0..4 {
            let (t, l, x) = (tab(i), label(i), close_of(&rt, i).rect);
            assert!((t[1] - bar[1] - TAB_TOP).abs() < 0.01 && (t[1] + t[3] - bar[1] - bar[3]).abs() < 0.01, "tab {i} from TAB_TOP to the bar's bottom: {t:?} {bar:?}");
            assert!((l[0] - t[0] - TAB_PAD_X).abs() < 0.01, "tab {i}: name {TAB_PAD_X} from the left edge");
            assert!((x[0] - (l[0] + l[2]) - CLOSE_GAP).abs() < 0.01, "tab {i}: {CLOSE_GAP} between name and ×");
            assert!(((t[0] + t[2]) - (x[0] + x[2]) - CLOSE_PAD).abs() < 0.01, "tab {i}: {CLOSE_PAD} after the ×");
        }
        for i in 0..3 {
            assert!((tab(i + 1)[0] - (tab(i)[0] + tab(i)[2])).abs() < 0.01, "no gap between tab {i} and the next");
        }
        // Front is the last (3): dividers after 0 and 1, none after 2 (next to the front) or on the front.
        let dividers = rt.elements().iter().filter(|e| e.key.as_deref() == Some("tab-divider")).count();
        assert_eq!(dividers, 2);
        assert!(find(&rt, "tab-bar-line").is_some(), "the bar's line runs under every tab");
    }

    /// Hovering a back tab shows its × in the slot it always had: no tab moves or changes width.
    #[test]
    fn hover_does_not_change_any_tab_width() {
        let mut rt = tabs(&["alpha.md", "beta.md", "gamma.md"]);
        let rects = |rt: &Runtime<App>| (0..3).map(|i| find(rt, &tab_key(rt.state.tabs[i].doc.id)).expect("tab").rect).collect::<Vec<_>>();
        let before = rects(&rt);
        for i in 0..3 {
            let t = before[i];
            rt.pointer_move(t[0] + t[2] / 2.0, t[1] + t[3] / 2.0);
            settle(&mut rt);
            assert_eq!(close_of(&rt, i).opacity, 1.0, "tab {i} hovered: its × shows");
            assert_eq!(rects(&rt), before, "hover on tab {i} moves nothing");
        }
    }

    fn visible(rt: &Runtime<App>, i: usize) -> bool {
        let strip = find(rt, STRIP).expect("strip").rect;
        let t = find(rt, &tab_key(rt.state.tabs[i].doc.id)).expect("tab").rect;
        // Clear of the buttons' fade at the right end.
        t[0] >= strip[0] - 0.5 && t[0] + t[2] <= strip[0] + strip[2] - FADE_W + 0.5
    }

    /// With more tabs than the bar holds, the strip scrolls: the front tab is in view at launch and
    /// after a switch by keys (⌘1, ⌘9), clear of the buttons; the scroll bar's thumb shows.
    #[test]
    fn the_front_tab_scrolls_into_view_when_the_tabs_overflow() {
        let names: Vec<String> = (0..16).map(|i| format!("a-long-file-name-{i:02}.md")).collect();
        let mut rt = tabs(&names.iter().map(String::as_str).collect::<Vec<_>>());
        assert_eq!(rt.state.active, 15);
        assert!(visible(&rt, 15), "the last-opened (front) tab is in view");
        assert!(!visible(&rt, 0), "the first is scrolled away");
        assert!(find(&rt, "tabs-scroll-thumb").expect("thumb").rect[2] >= SCROLL_BAR_MIN, "the thumb shows");
        rt.key(key('1', true, false, false, false));
        settle(&mut rt);
        assert_eq!(rt.state.active, 0);
        assert!(visible(&rt, 0), "⌘1 brings the first tab into view");
        rt.key(key('9', true, false, false, false));
        settle(&mut rt);
        assert!(visible(&rt, 15), "⌘9 brings the last back");
    }

    /// The buttons' blurred backdrop costs ~1 ms on every frame of the moving metal (idle 2.6 % with it,
    /// 2.1 % without, docs/perf-memory-2026-11-16.md "Release integration 2026-11-18"); it only shows
    /// something when tabs pass under the buttons, so it is on only while the strip overflows.
    #[test]
    fn the_buttons_blur_only_while_the_tabs_overflow() {
        let few = tabs(&["notes.md", "readme.md", "release.md"]);
        assert!(!few.state.tabs_overflow, "three tabs fit: no blur");
        let names: Vec<String> = (0..16).map(|i| format!("a-long-file-name-{i:02}.md")).collect();
        let many = tabs(&names.iter().map(String::as_str).collect::<Vec<_>>());
        assert!(many.state.tabs_overflow, "sixteen tabs scroll under the buttons: blur");
    }

    #[test]
    fn the_thumb_follows_the_scroll() {
        assert_eq!(thumb(0.0, 0.0, 500.0)[2], 0.0, "nothing overflows: no thumb");
        let [x0, _, w, _] = thumb(0.0, 500.0, 500.0);
        assert_eq!((x0, w), (0.0, 250.0), "half the tabs visible: half the track");
        let [x1, _, _, _] = thumb(500.0, 500.0, 500.0);
        assert_eq!(x1, 250.0, "at the end: the thumb at the end");
        assert_eq!(thumb(0.0, 100_000.0, 500.0)[2], SCROLL_BAR_MIN);
    }

    fn key(c: char, cmd: bool, shift: bool, alt: bool, ctrl: bool) -> KeyEvent {
        KeyEvent { key: Key::Command(c), mods: Mods { cmd, shift, alt, ctrl, ..Mods::default() } }
    }

    /// ⌘⇧] next, ⌘⇧[ previous, as the character macOS gives with ⇧ ("}" "{") or the key's own
    /// (`key_base`); other modifiers, or no ⇧, are not ours (⌘[ ⌘] stay free).
    #[test]
    fn cmd_shift_brackets_step_the_tabs() {
        assert_eq!(step_key(&key(']', true, true, false, false), None), Some(1));
        assert_eq!(step_key(&key('}', true, true, false, false), None), Some(1));
        assert_eq!(step_key(&key('[', true, true, false, false), None), Some(-1));
        assert_eq!(step_key(&key('{', true, true, false, false), None), Some(-1));
        assert_eq!(step_key(&key('x', true, true, false, false), Some(']')), Some(1), "a layout that gives another character");
        assert_eq!(step_key(&key(']', true, false, false, false), None), None, "no ⇧");
        assert_eq!(step_key(&key(']', true, true, true, false), None), None, "with ⌥");
        assert_eq!(step_key(&key(']', true, true, false, true), None), None, "with ⌃");
        assert_eq!(step_key(&key(']', false, true, false, false), None), None, "no ⌘");
        assert_eq!(step_key(&KeyEvent { key: Key::Tab, mods: Mods { cmd: true, shift: true, ..Mods::default() } }, None), None);
    }

    /// Next and previous wrap around both ends; with no tab or one tab there is nowhere to go.
    #[test]
    fn next_and_previous_wrap_and_need_two_tabs() {
        use crate::app::cycle_index;
        assert_eq!(cycle_index(0, 3, 1), Some(1));
        assert_eq!(cycle_index(2, 3, 1), Some(0), "next from the last: the first");
        assert_eq!(cycle_index(0, 3, -1), Some(2), "previous from the first: the last");
        assert_eq!(cycle_index(1, 3, -1), Some(0));
        assert_eq!(cycle_index(1, 2, 1), Some(0));
        assert_eq!(cycle_index(0, 1, 1), None, "one tab");
        assert_eq!(cycle_index(0, 1, -1), None);
        assert_eq!(cycle_index(0, 0, 1), None, "no tab");
        assert_eq!(cycle_index(0, 0, -1), None);
    }

    /// The keys through the app's key handler: ⌘⇧] and ⌃⇥ forward, ⌘⇧[ and ⌃⇧⇥ back, wrapping; with
    /// one tab nothing changes and the key is still taken.
    #[test]
    fn the_tab_keys_go_round_the_tabs() {
        let mut rt = tabs(&["a.md", "b.md", "c.md"]);
        let ctrl_tab = |shift| KeyEvent { key: Key::Tab, mods: Mods { ctrl: true, shift, ..Mods::default() } };
        assert_eq!(rt.state.active, 2);
        rt.key(key('}', true, true, false, false));
        settle(&mut rt);
        assert_eq!(rt.state.active, 0, "⌘⇧] from the last wraps to the first");
        rt.key(key('{', true, true, false, false));
        settle(&mut rt);
        assert_eq!(rt.state.active, 2, "⌘⇧[ from the first wraps to the last");
        rt.key(key('[', true, true, false, false));
        settle(&mut rt);
        assert_eq!(rt.state.active, 1);
        rt.key(ctrl_tab(false));
        settle(&mut rt);
        assert_eq!(rt.state.active, 2, "⌃⇥");
        rt.key(ctrl_tab(false));
        settle(&mut rt);
        assert_eq!(rt.state.active, 0, "⌃⇥ wraps");
        rt.key(ctrl_tab(true));
        settle(&mut rt);
        assert_eq!(rt.state.active, 2, "⌃⇧⇥ wraps back");

        let mut one = tabs(&["a.md"]);
        one.key(key('}', true, true, false, false));
        one.key(ctrl_tab(true));
        settle(&mut one);
        assert_eq!((one.state.active, one.state.tabs.len()), (0, 1), "one tab: nothing moves");
    }
}
