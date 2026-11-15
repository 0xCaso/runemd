//! The tab bar at the top of the panel (DESIGN.md, "Tab"; mockup `scheda`): the names of the
//! open files, each tab as wide as its name with `ROW_PAD` on the sides (at most 240, the name
//! cut in the middle), the front one with
//! `sel` fill, the "is writing" ring around the file's tab, "⌘E source" on the
//! right and, to its left, the text width ↔ button: it opens a popover with
//! the settings slider (same `settings.column`); ⌘⌥+ and ⌘⌥- take one step.
//!
//! Tabs are rows like the sidebar ones (`sidebar::pressable`: hover `sel`, scale 0.98
//! when pressed), not rune-kit's `TabStrip`: `TabStrip` gives every tab the same width
//! (28–224 pt) and keeps 16 pt for an avatar before the name, and neither can be changed
//! from outside. The mockup Matteo chose has tabs as wide as their name.
//!
//! The preview tab (single click in the sidebar, `App::open_preview`) has its name in italics;
//! a double click pins it.
//!
//! Too many tabs for the bar: the row scrolls sideways. A click brings a tab to the front; the × (on the
//! front tab, or on hover) and a middle click close it, Cmd+W closes the front one
//! (`main::tab_key`); with unsaved changes they ask first (`App::close`).

use rune::{Align, Color, El, Side, TextStyle, Tx, col, el, row, text};

use super::sidebar::{self, Hooks, Motion};
use super::theme::{HAIRLINE, KBD_PAD_X, KBD_PAD_Y, KBD_TEXT, KBD_WEIGHT, R_KBD, R_PANEL, R_ROW, ROW, ROW_GAP, ROW_PAD, SIDEBAR, TAB_BAR_H, TAB_BAR_PAD_X, TAB_GAP, TAB_MAX_W, Theme, UI_SMALL, UI_WEIGHT};
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

/// Opens or closes the width popover; opening it closes the shortcuts one.
pub fn toggle_width(s: &mut App) {
    s.width_popover = !s.width_popover;
    if s.width_popover {
        s.keys_popover = false;
    }
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

/// The bar: `TAB_BAR_H` tall, with a line at the bottom that separates it from the document. `hover`: the
/// pointer is a fine one and can hover.
pub fn view(s: &App, hooks: Hooks<App>, theme: &Theme, reduced: bool, hover: bool) -> El {
    let motion = Motion { reduced, hover };
    // `RING_W` around the row: the ring sits outside the tab and the scrolling row clips it.
    let list = row().key("tabs").role("tablist").label("Tabs").gap(TAB_GAP).center_y().padding(RING_W)
        .children(s.tabs.iter().enumerate().map(|(i, tab)| tab_row(s, hooks, i, tab, theme, motion)));
    let strip = row().key("tabs-scroll").grow(1.0).shrink().min_w(0.0).scroll_x().child(list);
    let key = el().key("source-key").border(HAIRLINE, theme.line).radius(R_KBD).px(KBD_PAD_X).py(KBD_PAD_Y)
        .child(text(SOURCE_KEY).font(TextStyle::new(KBD_TEXT, KBD_WEIGHT)).color(theme.fg3));
    let mut bar = row().key("tab-bar").w_full().h(TAB_BAR_H).px(TAB_BAR_PAD_X - RING_W).gap(TAB_GAP).center_y().child(strip);
    // Find (Cmd+F): the field sits in the bar, right of the tabs, so the document does not move.
    if s.find.open {
        bar = bar.child(super::find::field(s, theme).mr(ROW_GAP));
    }
    let bar = bar.child(super::shortcuts::button(s, theme, motion)).child(width_button(s, theme, motion)).child(key);
    rune::col().w_full().child(bar).child(el().key("tab-bar-line").w_full().h(HAIRLINE).fill(theme.line))
}

/// The × area (16 pt), its margin from the tab's right edge and from the name.
pub const CLOSE: f32 = 16.0;
const CLOSE_PAD: f32 = 6.0;
const CLOSE_GAP: f32 = 4.0;
const R_CLOSE: f32 = 4.0;

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
/// the pointer (or keyboard focus) on the tab (`hover_group`).
fn close_button(key: &str, id: u64, title: &str, front: bool, theme: &Theme, motion: Motion) -> El {
    let glyph = text("×").key(format!("{key}-close-glyph")).font(TextStyle::new(CLOSE_GLYPH, KBD_WEIGHT)).color(theme.fg2).a11y_hidden();
    let mut b = el()
        .key(format!("{key}-close"))
        .size(CLOSE, CLOSE)
        .radius(R_CLOSE)
        .center()
        .role("button")
        .label(format!("Close {title}"))
        .hover_alone()
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

/// A tab: `ROW` tall, radius `R_ROW`, `ROW_PAD` on the sides of the name. Front: `sel` and text
/// `fg`; the others: text `fg2`, hover `sel`.
fn tab_row(s: &App, hooks: Hooks<App>, i: usize, tab: &crate::app::Tab, theme: &Theme, motion: Motion) -> El {
    let front = i == s.active;
    let key = format!("t-{:x}", tab.doc.id);
    let title = tab.title();
    let id = tab.doc.id;
    // The preview tab has its name in italics (like VS Code).
    let font = if tab.preview { sidebar::ui_font().italic() } else { sidebar::ui_font() };
    let label = text(title.clone()).key(format!("{key}-label")).font(font).color(if front { theme.fg } else { theme.fg2 }).truncate_middle().shrink().min_w(0.0).a11y_hidden();
    let close = close_button(&key, id, &title, front, theme, motion);
    // At most `TAB_MAX_W`, the name cut in the middle: a long tab does not push the others
    // out.
    let mut e = row()
        .max_w(TAB_MAX_W)
        .h(ROW)
        .pl(ROW_PAD)
        .pr(CLOSE_PAD)
        .gap(CLOSE_GAP)
        .center_y()
        .role("tab")
        .label(title)
        .focus_selected(front)
        .hover_group()
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
    if front {
        e = e.fill(theme.sel);
    }
    let e = sidebar::pressable(e, key.clone(), &s.sidebar, hooks, theme, motion, !front).key(key);
    // The ring's host: always present, so the structure does not change when it lights up.
    let host = el().radius(R_ROW).child(e);
    match s.ring_of(tab.path()) {
        Some(since) => fx::ring::slot(host, Mark::TabRing(i as u32), since, motion.reduced, theme.dark),
        None => host,
    }
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
}
