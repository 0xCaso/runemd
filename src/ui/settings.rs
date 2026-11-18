//! The settings panel (spec, "Settings"): Cmd+, opens and closes it, Esc
//! closes it. It sits in the document panel, on the right, in place of the heading outline: the document
//! stays in view and changes at once with font, size and theme.
//!
//! No mockup: only DESIGN.md tokens. A column as wide as the sidebar (`SIDEBAR`), separated from the
//! document by a 0.5 line in `line`; section labels like the sidebar ones (small
//! UI in `fg3`); 28 rows (`ROW`, radius 6, hover `sel`, scale 0.98 when pressed). The chosen
//! option has the `sel` fill, `fg` text and the check mark in `fg`; the others `fg2` text. The
//! fire is a choice of three like the theme (Brief, Always, Off), with a note in `fg3` under Always:
//! all ≥ 3:1 on the panel in both themes (`ui::contrast`). No accent, no pill.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::time::Instant;

use rune::{
    El, FileDialogOptions, GrowthSpec, ScrollSpec, TextEdit, TextStyle, Tx, col, el, icon, row, text, text_input,
};
use rune_kit::glyphs;

use super::sidebar::{self, Hooks, Motion};
use super::theme::{
    HAIRLINE, ICON, OUTLINE_PAD_X, ROW, ROW_GAP, ROW_PAD, SECTION_PAD_BOTTOM,
    SECTION_PAD_TOP, SIDEBAR, Theme, UI_SMALL, UI_STRONG_WEIGHT, UI_TEXT,
    UI_WEIGHT,
};
use crate::app::App;
use crate::settings::{FireMode, ThemePref};

/// The panel texts (exact).
pub const TITLE: &str = "Settings";
pub const ROOTS: &str = "Watched folders";
pub const NO_ROOTS: &str = "No folders";
pub const ADD_ROOT: &str = "Add folder…";
pub const REMOVE_ROOT: &str = "Remove";
pub const DIALOG_TITLE: &str = "Choose a folder to watch";
pub const FONT: &str = "Font";
pub const SIZE: &str = "Size";
pub const SMALLER: &str = "Smaller";
pub const BIGGER: &str = "Larger";
pub const THEME: &str = "Theme";
pub const THEMES: [(ThemePref, &str); 3] = [
    (ThemePref::System, "System"),
    (ThemePref::Light, "Light"),
    (ThemePref::Dark, "Dark"),
];
pub const FIRE: &str = "Fire at the bottom of the panel";
/// Its three choices (decision `2026-11-17-doc-fire-at-rest`), Brief first (the default).
pub const FIRE_MODES: [(FireMode, &str); 3] = [
    (FireMode::Brief, "Brief"),
    (FireMode::Always, "Always"),
    (FireMode::Off, "Off"),
];
/// The note under Always (measured: 8.2–9.9 % CPU, `docs/perf-memory-2026-11-16.md`).
pub const FIRE_ALWAYS_NOTE: &str = "Uses about 8–10 % CPU while the window is active.";
/// The fire's own section (it used to sit under Theme without a title).
pub const FIRE_SECTION: &str = "Fire";
/// The font search field's placeholder and label.
pub const FONT_SEARCH: &str = "Search fonts";
pub const NO_FONTS: &str = "No matching fonts";
pub const WIDTH: &str = "Text width";
pub const FIRE_STRENGTH: &str = "Intensity";
pub const FIRE_HEIGHT: &str = "Height";
pub const FIRE_SPEED: &str = "Speed";
pub const FIRE_COLOR: &str = "Color";
/// The opacity of the fire controls when it is off.
const DIMMED: f32 = 0.4;
/// The width slider: a 2 track in `fg3`, chosen part and knob (12) in `fg2`.
const TRACK: f32 = 2.0;
const THUMB: f32 = 12.0;
/// The room for the value right of the slider ("1240 pt", "Full").
const WIDTH_VALUE: f32 = 64.0;
/// The feedback for "Add folder…" when the chosen folder is already watched (or sits under
/// a watched one): it stays for `NOTICE_FOR` or until the next action.
pub const ALREADY: &str = "Already watched";
pub const NOTICE_FOR: std::time::Duration = std::time::Duration::from_secs(3);
/// Rows of the font list visible before scrolling: half a row cut off says it scrolls.
const FONT_ROWS: f32 = 8.5;
/// A font row's name in its own face (feedback group 3, item 10), a point above the UI text.
const FONT_PREVIEW: f32 = UI_TEXT + 1.0;
/// Between sections (feedback group 3, item 9): a line, then the title.
const SECTION_GAP: f32 = 14.0;
/// Below the last setting, over the fire at the bottom of the panel (item 12): the lit fire at the
/// chosen height, so scrolled to the end the last row sits above the flames, also when they rise.
pub fn bottom_room(fire: bool, height: f32) -> f32 {
    if fire { crate::fx::fire::lit_height(height) + SECTION_GAP } else { SECTION_PAD_TOP }
}

/// A font row's name in its own face (item 10): the list says what each font looks like.
pub fn preview_face(name: &str) -> TextStyle {
    TextStyle::new(FONT_PREVIEW, UI_WEIGHT).family(crate::fonts::family(name))
}

/// The fonts whose name has every word of `query`, ignoring case; all of them for an empty query.
pub fn filter_fonts(fonts: &[String], query: &str) -> Vec<String> {
    let words: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    fonts.iter().filter(|f| {
        let name = f.to_lowercase();
        words.iter().all(|w| name.contains(w.as_str()))
    }).cloned().collect()
}

/// The panel state: open or not, the font choices (read from CoreText on first open)
/// and until when "Already watched" shows.
#[derive(Default)]
pub struct Panel {
    pub open: bool,
    fonts: Option<Vec<String>>,
    notice: Option<Instant>,
    /// The font search field; with text, `matches` holds the fonts it finds.
    pub font_query: TextEdit,
    matches: Option<Vec<String>>,
}

impl Panel {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.notice = None;
        self.font_query = TextEdit::default();
        self.matches = None;
        if self.open && self.fonts.is_none() {
            self.fonts = Some(crate::fonts::choices(&crate::fonts::installed()));
        }
    }

    pub fn close(&mut self) -> bool {
        self.notice = None;
        self.font_query = TextEdit::default();
        self.matches = None;
        std::mem::take(&mut self.open)
    }

    /// The font search changed: the list shows only the fonts it finds.
    pub fn search_fonts(&mut self) {
        let q = self.font_query.text.trim().to_owned();
        self.matches = (!q.is_empty()).then(|| filter_fonts(&self.choices(), &q));
    }

    /// "Already watched" until `now + NOTICE_FOR`.
    pub fn notice_already(&mut self, now: Instant) {
        self.notice = Some(now + NOTICE_FOR);
    }

    /// Another action in the panel: the feedback disappears.
    pub fn clear_notice(&mut self) {
        self.notice = None;
    }

    /// Whether at `now` the feedback would have expired (tests).
    #[cfg(test)]
    fn clone_expire(&self, now: Instant) -> bool {
        self.notice.is_some_and(|t| now >= t)
    }

    /// "Already watched" is showing.
    pub fn notice(&self) -> bool {
        self.notice.is_some()
    }

    /// Removes the feedback that expired at `now`. True if the view changes.
    pub fn expire(&mut self, now: Instant) -> bool {
        if self.notice.is_some_and(|t| now >= t) {
            self.notice = None;
            return true;
        }
        false
    }

    /// The choices (before reading CoreText: the first ones, filtered like `fonts::choices`).
    fn choices(&self) -> Cow<'_, [String]> {
        match &self.fonts {
            Some(f) => Cow::Borrowed(f),
            None => Cow::Owned(crate::fonts::choices(&[])),
        }
    }

    /// Where the saved font goes if it is not among the choices (no longer installed): after the last of the
    /// first choices present. `None` if it is already there.
    fn missing_at(choices: &[String], current: &str) -> Option<usize> {
        if choices.iter().any(|f| f == current) {
            return None;
        }
        Some(
            choices
                .iter()
                .take_while(|f| crate::fonts::FIRST.contains(&f.as_str()))
                .count(),
        )
    }

    /// The font choices; the saved font is always there, even if it is no longer installed.
    #[cfg(test)]
    fn fonts(&self, current: &str) -> Vec<String> {
        let mut out = self.choices().into_owned();
        if let Some(at) = Self::missing_at(&out, current) {
            out.insert(at, current.to_owned());
        }
        out
    }

    /// How many rows the font list has and where the added saved font sits (the version of the
    /// virtual list).
    fn font_shape(&self, current: &str) -> (usize, Option<usize>) {
        if let Some(m) = &self.matches {
            return (m.len(), None);
        }
        let choices = self.choices();
        let at = Self::missing_at(&choices, current);
        (choices.len() + usize::from(at.is_some()), at)
    }

    /// Row `i` of the font list, without copying the list.
    fn font_at(&self, current: &str, i: usize) -> Option<String> {
        if let Some(m) = &self.matches {
            return m.get(i).cloned();
        }
        let choices = self.choices();
        match Self::missing_at(&choices, current) {
            Some(at) if i == at => Some(current.to_owned()),
            Some(at) if i > at => choices.get(i - 1).cloned(),
            _ => choices.get(i).cloned(),
        }
    }
}

fn ui_font() -> TextStyle {
    TextStyle::new(UI_TEXT, UI_WEIGHT)
}

/// The size as written: "15.5 pt", "17 pt".
pub fn size_label(size: f32) -> String {
    let s = if size.fract() == 0.0 {
        format!("{size:.0}")
    } else {
        format!("{size:.1}")
    };
    format!("{s} pt")
}

/// The folder as you read it: the home becomes "~".
pub fn root_label(root: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home.as_deref().and_then(|h| root.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => root.display().to_string(),
    }
}

/// The panel column. `hover`: the pointer is a fine one and can hover.
pub fn view(s: &App, hooks: Hooks<App>, theme: &Theme, reduced: bool, hover: bool) -> El {
    let motion = Motion { reduced, hover };
    let set = &s.settings;
    let title = text(TITLE)
        .key("settings-title")
        .font(TextStyle::new(UI_TEXT, UI_STRONG_WEIGHT))
        .color(theme.fg)
        .px(ROW_PAD)
        .pt(SECTION_PAD_TOP);
    let mut body = col()
        .key("settings-body")
        .w_full()
        .pb(bottom_room(set.fire.shows(), set.fire_height))
        .child(title);

    body = body.child(section_at(ROOTS, true, theme));
    let roots = s.library.roots();
    if roots.is_empty() {
        body = body.child(
            text(NO_ROOTS)
                .font(ui_font())
                .color(theme.fg3)
                .px(ROW_PAD)
                .h(ROW),
        );
    }
    for (i, root) in roots.iter().enumerate() {
        body = body.child(root_row(s, hooks, i, root, theme, motion));
    }
    let add = row()
        .h(ROW)
        .px(ROW_PAD)
        .gap(ROW_GAP)
        .center_y()
        .role("button")
        .label(ADD_ROOT)
        .child(
            text(ADD_ROOT)
                .font(ui_font())
                .color(theme.fg2)
                .a11y_hidden(),
        )
        .on_press_tx(|_: &mut App, tx: &mut Tx| {
            let directory = std::env::var_os("HOME").map(PathBuf::from);
            tx.open_folder(
                FileDialogOptions {
                    title: DIALOG_TITLE.into(),
                    directory,
                    ..Default::default()
                },
                |s: &mut App, result, tx: &mut Tx| match result {
                    Ok(Some(path)) => {
                        if !s.pick_root(&path, Instant::now()) {
                            tx.wake_after(NOTICE_FOR);
                        }
                    }
                    Ok(None) => {}
                    Err(e) => eprintln!("runemd: no folder chosen: {e}"),
                },
            );
        });
    body = body.child(press(add, "set-add-root", s, hooks, theme, motion));
    if s.settings_panel.notice() {
        body = body.child(
            text(ALREADY)
                .key("set-already")
                .role("status")
                .font(ui_font())
                .color(theme.fg2)
                .px(ROW_PAD)
                .h(ROW),
        );
    }

    body = body.child(section(FONT, theme));
    let search = text_input(&s.settings_panel.font_query, ui_font())
        .single_line()
        .key("set-font-search")
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .color(theme.fg)
        .placeholder(FONT_SEARCH, theme.fg3)
        .caret(theme.fg, theme.sel)
        .label(FONT_SEARCH)
        .on_edit(|s: &mut App| &mut s.settings_panel.font_query)
        .on_input_change(|s: &mut App, _: &mut Tx| s.settings_panel.search_fonts());
    body = body.child(
        row()
            .key("set-font-field")
            .mx(ROW_PAD - ROW_GAP)
            .mb(ROW_GAP)
            .h(ROW)
            .px(ROW_GAP + 2.0)
            .gap(ROW_GAP)
            .center_y()
            .radius(super::theme::R_FIELD)
            .fill(theme.sel)
            .child(icon(&glyphs::SEARCH, ICON).color(theme.fg3))
            .child(search),
    );
    // Virtual rows: the list has hundreds of families, only those near the view
    // are built. Each row finds its font again from the state (`Panel::font_at`).
    let (count, at) = s.settings_panel.font_shape(&set.font);
    let th = *theme;
    let list = col()
        .key("set-fonts")
        .w_full()
        .growth(GrowthSpec::default())
        .virtual_rows(
            count,
            ROW,
            |s: &App, i| {
                let name = s
                    .settings_panel
                    .font_at(&s.settings.font, i)
                    .unwrap_or_default();
                Cow::Owned(format!("set-font-{name}"))
            },
            move |s: &App, i| {
                let Some(name) = s.settings_panel.font_at(&s.settings.font, i) else {
                    return el();
                };
                let chosen = name == s.settings.font;
                let pick = name.clone();
                let face = preview_face(&name);
                option_in(
                    s,
                    hooks,
                    format!("set-font-{name}"),
                    &name,
                    face,
                    chosen,
                    &th,
                    motion,
                    move |s: &mut App| s.set_font(&pick),
                )
            },
        )
        .virtual_rows_version((count, at, s.settings_panel.font_query.text.as_str()));
    body = body.child(
        col()
            .key("set-fonts-scroll")
            .role("radiogroup")
            .label(FONT)
            .w_full()
            .max_h(ROW * FONT_ROWS)
            .scroll(ScrollSpec {
                stick_bottom: false,
            })
            .child(list),
    );
    if count == 0 {
        body = body.child(text(NO_FONTS).key("set-no-fonts").font(ui_font()).color(theme.fg3).px(ROW_PAD).h(ROW));
    }

    body = body.child(section(SIZE, theme));
    let step = |key: &'static str, glyph: &'static str, label: &'static str, by: i32| {
        let b = row()
            .w(ROW)
            .h(ROW)
            .center()
            .role("button")
            .label(label)
            .child(text(glyph).font(ui_font()).color(theme.fg2).a11y_hidden())
            .on_press(move |s: &mut App| s.step_size(by));
        press(b, key, s, hooks, theme, motion)
    };
    let value = text(size_label(set.size))
        .key("set-size-value")
        .font(ui_font().features("'tnum' 1"))
        .color(theme.fg)
        .px(ROW_GAP);
    body = body.child(
        row()
            .key("set-size")
            .px(ROW_PAD - ROW_GAP)
            .h(ROW)
            .center_y()
            .child(step("set-smaller", "−", SMALLER, -1))
            .child(value)
            .child(step("set-bigger", "+", BIGGER, 1)),
    );

    body = body.child(section(WIDTH, theme)).child(width_slider("set-width", set.column, theme));

    body = body.child(section(THEME, theme));
    let mut themes = col()
        .key("set-themes")
        .role("radiogroup")
        .label(THEME)
        .w_full();
    for (pref, label) in THEMES {
        themes = themes.child(option(
            s,
            hooks,
            format!("set-theme-{label}"),
            label,
            set.theme == pref,
            theme,
            motion,
            move |s: &mut App| s.set_theme(pref),
        ));
    }
    body = body.child(themes);

    body = body
        .child(section(FIRE_SECTION, theme))
        .child(sub_label(FIRE, theme))
        .child(fire_modes(s, hooks, theme, motion))
        .child(fire_look(s, hooks, theme, motion));

    body = body.child(section(super::shortcuts::TITLE, theme)).child(super::shortcuts::list(theme));

    let scroll = col()
        .key("settings-scroll")
        .w_full()
        .h_full()
        .scroll(ScrollSpec {
            stick_bottom: false,
        })
        .px(OUTLINE_PAD_X)
        .child(body);
    rune::row()
        .key("settings")
        .role("region")
        .label(TITLE)
        .w(SIDEBAR)
        .h_full()
        .child(
            el().key("settings-line")
                .w(HAIRLINE)
                .h_full()
                .fill(theme.line),
        )
        .child(scroll)
}

/// "Text width": continuous per pt from 680 to 1240, then "Full" (`settings::column_at`). The same
/// slider in the ↔ popover and in the settings, on the same `settings.column`. Dragging changes the
/// width live and saves it when the drag ends; the arrows move it by `settings::COLUMN_NUDGE`.
pub(crate) fn width_slider(key: &'static str, column: f32, theme: &Theme) -> El {
    slider(
        key,
        WIDTH,
        crate::settings::column_t(column),
        crate::settings::column_label(column),
        true,
        theme,
        |s: &mut App, at, tx: &mut Tx| _ = s.set_column_live(crate::settings::column_at(at), tx),
        |s: &mut App, by, tx: &mut Tx| s.set_column(crate::settings::nudge_column(s.settings.column, by), tx),
        |s: &mut App| s.save_settings(),
    )
}

/// A stepped slider with the value `value` on the right (key `<key>-value`), at point `t`
/// (0..1). Dragging or clicking calls `at` with the point; the arrows call `step` with −1 or
/// +1. The track is in `fg3` and the chosen part with the knob in `fg2` (≥ 3:1). Off
/// (`enabled` false): no handlers, `disabled` for accessibility; the dimming is applied by
/// the container.
#[allow(clippy::too_many_arguments)]
pub(crate) fn stepped_slider(
    key: &'static str,
    label: &'static str,
    t: f32,
    value: String,
    enabled: bool,
    theme: &Theme,
    at: impl Fn(&mut App, f32, &mut Tx) + 'static,
    step: impl Fn(&mut App, i32, &mut Tx) + 'static,
) -> El {
    slider(key, label, t, value, enabled, theme, at, step, |_: &mut App| {})
}

/// [`stepped_slider`] with `end`, called when a drag ends.
#[allow(clippy::too_many_arguments)]
fn slider(
    key: &'static str,
    label: &'static str,
    t: f32,
    value: String,
    enabled: bool,
    theme: &Theme,
    at: impl Fn(&mut App, f32, &mut Tx) + 'static,
    step: impl Fn(&mut App, i32, &mut Tx) + 'static,
    end: impl Fn(&mut App) + 'static,
) -> El {
    let track = |grow: f32, color| el().grow(grow).h(TRACK).radius(TRACK / 2.0).fill(color);
    let thumb = el().size(THUMB, THUMB).radius(THUMB / 2.0).fill(theme.fg2);
    let mut slider = row()
        .key(key)
        .role("slider")
        .label(label)
        .a11y_value(value.clone())
        .grow(1.0)
        .min_w(0.0)
        .h(ROW)
        .center_y()
        .child(track(t, theme.fg2))
        .child(thumb)
        .child(track(1.0 - t, theme.fg3));
    slider = if enabled {
        slider
            .focusable(true)
            .on_drag(move |s: &mut App, d: &mut rune::Drag, tx: &mut Tx| {
                if d.phase == rune::Phase::Up {
                    end(s);
                    return;
                }
                at(s, ((d.x - THUMB / 2.0) / (d.w - THUMB).max(1.0)).clamp(0.0, 1.0), tx);
            })
            .on_key(move |s: &mut App, key: &rune::KeyEvent, tx: &mut Tx| {
                let by = match key.key {
                    rune::Key::Left | rune::Key::Down => -1,
                    rune::Key::Right | rune::Key::Up => 1,
                    _ => return false,
                };
                step(s, by, tx);
                true
            })
    } else {
        slider.disabled(true)
    };
    let value = text(value)
        .key(format!("{key}-value"))
        .font(ui_font().features("'tnum' 1"))
        .color(theme.fg)
        .w(WIDTH_VALUE)
        .pl(ROW_GAP);
    row().w_full().px(ROW_PAD).child(slider).child(value)
}

/// The fire settings below the switch: Intensity, Height, Speed (stepped
/// sliders) and Color (choices). With the fire off they are dimmed (`DIMMED`) and disabled.
fn fire_look(s: &App, hooks: Hooks<App>, theme: &Theme, motion: Motion) -> El {
    use crate::settings::FireKnob;
    let on = s.settings.fire.shows();
    let mut look = col().key("set-fire-look").w_full().disabled(!on);
    if !on {
        look = look.opacity(DIMMED);
    }
    for (knob, key, label) in [
        (FireKnob::Strength, "set-fire-strength", FIRE_STRENGTH),
        (FireKnob::Height, "set-fire-height", FIRE_HEIGHT),
        (FireKnob::Speed, "set-fire-speed", FIRE_SPEED),
    ] {
        let v = s.settings.fire_knob(knob);
        look = look.child(sub_label(label, theme)).child(stepped_slider(
            key,
            label,
            knob.t(v),
            knob.label(v),
            on,
            theme,
            move |s: &mut App, at, _: &mut Tx| s.set_fire_knob(knob, knob.at(at)),
            move |s: &mut App, by, _: &mut Tx| s.set_fire_knob(knob, knob.step(s.settings.fire_knob(knob), by)),
        ));
    }
    look = look.child(sub_label(FIRE_COLOR, theme));
    let mut colors = col()
        .key("set-fire-colors")
        .role("radiogroup")
        .label(FIRE_COLOR)
        .w_full();
    for (color, label) in crate::fx::fire::COLORS {
        let key = format!("set-fire-color-{label}");
        let chosen = s.settings.fire_color == color;
        colors = colors.child(if on {
            option(s, hooks, key, label, chosen, theme, motion, move |s: &mut App| s.set_fire_color(color))
        } else {
            still_option(key, label, chosen, theme)
        });
    }
    look.child(colors)
}

/// The label of a control inside a section: 13 in `fg2`, without the section spacing.
fn sub_label(label: &'static str, theme: &Theme) -> El {
    text(label)
        .key(label)
        .font(ui_font())
        .color(theme.fg2)
        .pt(ROW_GAP)
        .px(ROW_PAD)
}

/// A section title (feedback group 3, item 9): a `line` rule across the column `SECTION_GAP` below the
/// last section, then the label in 12/600 `fg2`, so each section starts visibly. The first one has no rule.
fn section(label: &'static str, theme: &Theme) -> El {
    section_at(label, false, theme)
}

fn section_at(label: &'static str, first: bool, theme: &Theme) -> El {
    let title = text(label)
        .key(label)
        .font(TextStyle::new(UI_SMALL, UI_STRONG_WEIGHT))
        .color(theme.fg2)
        .pt(if first { SECTION_PAD_TOP } else { SECTION_GAP })
        .px(ROW_PAD)
        .pb(SECTION_PAD_BOTTOM);
    let mut e = col().key(format!("section-{label}")).w_full();
    if !first {
        e = e.mt(SECTION_GAP - SECTION_PAD_TOP).child(el().key(format!("rule-{label}")).w_full().h(HAIRLINE).fill(theme.line));
    }
    e.child(title)
}

/// `e` pressable with key `key` (hover `sel`, scale 0.98).
fn press(e: El, key: &str, s: &App, hooks: Hooks<App>, theme: &Theme, motion: Motion) -> El {
    sidebar::pressable(e, key.to_owned(), &s.sidebar, hooks, theme, motion, true)
        .key(key.to_owned())
}

/// A watched folder: icon, path in `fg2` (truncated) and "Remove" on the right.
fn root_row(
    s: &App,
    hooks: Hooks<App>,
    i: usize,
    root: &Path,
    theme: &Theme,
    motion: Motion,
) -> El {
    let label = root_label(root);
    let path = root.to_owned();
    let remove = row()
        .h(ROW)
        .px(ROW_PAD)
        .center_y()
        .role("button")
        .label(format!("{REMOVE_ROOT} {label}"))
        .child(
            text(REMOVE_ROOT)
                .font(ui_font())
                .color(theme.fg2)
                .a11y_hidden(),
        )
        .on_press(move |s: &mut App| {
            s.remove_root(&path);
        });
    let remove = press(
        remove,
        &format!("set-root-{i}-remove"),
        s,
        hooks,
        theme,
        motion,
    );
    row()
        .key(format!("set-root-{i}"))
        .w_full()
        .h(ROW)
        .pl(ROW_PAD)
        .gap(ROW_GAP)
        .center_y()
        .child(icon(&glyphs::FOLDER, ICON).color(theme.fg3))
        .child(
            text(label)
                .font(ui_font())
                .color(theme.fg2)
                .truncate()
                .grow(1.0)
                .shrink()
                .min_w(0.0),
        )
        .child(remove)
}

/// A choice (font, theme): the check and the text in `fg` with the `sel` fill if it is the chosen
/// one, otherwise only the text in `fg2`.
#[allow(clippy::too_many_arguments)]
fn option(
    s: &App,
    hooks: Hooks<App>,
    key: String,
    label: &str,
    chosen: bool,
    theme: &Theme,
    motion: Motion,
    pick: impl Fn(&mut App) + 'static,
) -> El {
    option_in(s, hooks, key, label, ui_font(), chosen, theme, motion, pick)
}

/// [`option`] with the label in `font`.
#[allow(clippy::too_many_arguments)]
fn option_in(
    s: &App,
    hooks: Hooks<App>,
    key: String,
    label: &str,
    font: TextStyle,
    chosen: bool,
    theme: &Theme,
    motion: Motion,
    pick: impl Fn(&mut App) + 'static,
) -> El {
    let mark = el().w(ICON).h(ICON).center();
    let mark = if chosen {
        mark.child(icon(&glyphs::CHECK, ICON).color(theme.fg))
    } else {
        mark
    };
    let mut e = row()
        .w_full()
        .h(ROW)
        .px(ROW_PAD)
        .gap(ROW_GAP)
        .center_y()
        .role("radio")
        .label(label.to_owned())
        .checked(chosen)
        .child(mark)
        .child(
            text(label.to_owned())
                .font(font)
                .color(if chosen { theme.fg } else { theme.fg2 })
                .truncate()
                .shrink()
                .min_w(0.0)
                .a11y_hidden(),
        )
        .on_press(pick);
    if chosen {
        e = e.fill(theme.sel);
    }
    sidebar::pressable(e, key.clone(), &s.sidebar, hooks, theme, motion, !chosen).key(key)
}

/// A choice that cannot be made (fire off): like `option`, without press or hover.
fn still_option(key: String, label: &str, chosen: bool, theme: &Theme) -> El {
    let mark = el().w(ICON).h(ICON).center();
    let mark = if chosen { mark.child(icon(&glyphs::CHECK, ICON).color(theme.fg)) } else { mark };
    row()
        .key(key)
        .w_full()
        .h(ROW)
        .px(ROW_PAD)
        .gap(ROW_GAP)
        .center_y()
        .role("radio")
        .label(label.to_owned())
        .checked(chosen)
        .disabled(true)
        .child(mark)
        .child(
            text(label.to_owned())
                .font(ui_font())
                .color(if chosen { theme.fg } else { theme.fg2 })
                .truncate()
                .shrink()
                .min_w(0.0)
                .a11y_hidden(),
        )
}

/// The fire choices (Brief, Always, Off) like the theme ones, with the CPU note in `fg3` right under
/// Always, aligned with the option text.
fn fire_modes(s: &App, hooks: Hooks<App>, theme: &Theme, motion: Motion) -> El {
    let mut modes = col().key("set-fire-modes").role("radiogroup").label(FIRE).w_full();
    for (mode, label) in FIRE_MODES {
        let chosen = s.settings.fire == mode;
        modes = modes.child(option(s, hooks, format!("set-fire-{label}"), label, chosen, theme, motion, move |s: &mut App| s.set_fire_mode(mode)));
        if mode == FireMode::Always {
            modes = modes.child(
                text(FIRE_ALWAYS_NOTE)
                    .key("set-fire-always-note")
                    .font(TextStyle::new(UI_SMALL, UI_WEIGHT))
                    .color(theme.fg3)
                    .wrap()
                    .w_full()
                    .pl(ROW_PAD + ICON + ROW_GAP)
                    .pr(ROW_PAD)
                    .pb(ROW_GAP),
            );
        }
    }
    modes
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use rune::{Env, Key, KeyEvent, Mods, Runtime};

    use super::*;
    use crate::doc::document::Document;
    use crate::settings::FireMode;

    #[test]
    fn sizes_read_in_italian() {
        assert_eq!(size_label(15.5), "15.5 pt");
        assert_eq!(size_label(17.0), "17 pt");
    }

    #[test]
    fn home_becomes_a_tilde() {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        assert_eq!(root_label(&home.join("Desktop/GitHub")), "~/Desktop/GitHub");
        assert_eq!(root_label(Path::new("/tmp/x")), "/tmp/x");
    }

    fn open(source: &str, dark: bool) -> Runtime<App> {
        let mut state = App::bare(crate::index::Library::load(Vec::new(), None));
        state.push_tab(Document::from_source(Path::new("t.md"), source));
        let mut rt = Runtime::new(state, crate::view, (1200.0, 900.0), Env::new(dark, 2.0));
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

    fn cmd(rt: &mut Runtime<App>, c: char) {
        rt.key(KeyEvent {
            key: Key::Command(c),
            mods: Mods {
                cmd: true,
                ..Mods::default()
            },
        });
        settle(rt);
    }

    fn has(rt: &Runtime<App>, key: &str) -> bool {
        rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
    }

    fn texts(rt: &Runtime<App>) -> Vec<String> {
        rt.elements().into_iter().filter_map(|e| e.text).collect()
    }

    /// Feedback group 3, item 10: the search keeps the fonts whose name has every word, ignoring case.
    #[test]
    fn the_font_search_filters_by_words() {
        let fonts: Vec<String> = ["Inter", "SF Pro", "American Typewriter", "Avenir Next", "Times New Roman", "New York"].map(String::from).to_vec();
        assert_eq!(filter_fonts(&fonts, ""), fonts, "empty: all");
        assert_eq!(filter_fonts(&fonts, "new"), ["Times New Roman", "New York"]);
        assert_eq!(filter_fonts(&fonts, "  NEW  york "), ["New York"], "words, any case");
        assert_eq!(filter_fonts(&fonts, "type"), ["American Typewriter"]);
        assert!(filter_fonts(&fonts, "zzz").is_empty());
        assert_eq!(preview_face("American Typewriter"), TextStyle::new(FONT_PREVIEW, UI_WEIGHT).family(rune::Family::Named("American Typewriter")), "each row in its own face");
        assert_eq!(preview_face("Inter").family, rune::Family::Sans);
    }

    /// In the panel: typing in "Search fonts" leaves only the matching rows, and "No matching fonts"
    /// when none; closing the panel clears the search.
    #[test]
    fn the_font_list_follows_the_search() {
        let mut rt = open("# Title\n\nText.\n", true);
        cmd(&mut rt, ',');
        assert!(has(&rt, "set-font-search"));
        assert!(has(&rt, "set-font-Inter"));
        rt.update(|s: &mut App, _| {
            s.settings_panel.font_query.replace_text("sf pr");
            s.settings_panel.search_fonts();
        });
        settle(&mut rt);
        let rows: Vec<String> = rt.elements().into_iter().filter_map(|e| e.key).filter(|k| k.starts_with("set-font-") && !k.starts_with("set-font-s") && !k.starts_with("set-font-f")).collect();
        assert!(!rows.is_empty() && rows.iter().all(|k| k.starts_with("set-font-SF Pro")), "only the matches: {rows:?}");
        assert!(!has(&rt, "set-font-Inter"));
        rt.update(|s: &mut App, _| {
            s.settings_panel.font_query.replace_text("no such font here");
            s.settings_panel.search_fonts();
        });
        settle(&mut rt);
        assert!(texts(&rt).iter().any(|t| t == NO_FONTS));
        cmd(&mut rt, ',');
        cmd(&mut rt, ',');
        assert!(has(&rt, "set-font-Inter"), "reopened: the full list");
    }

    /// Feedback group 3, item 9: every section after the first starts with a rule and a title in `fg2`.
    #[test]
    fn sections_are_set_apart() {
        let mut rt = open("# Title\n\nText.\n", true);
        cmd(&mut rt, ',');
        assert!(!has(&rt, &format!("rule-{ROOTS}")), "no rule above the first section");
        for label in [FONT, SIZE, WIDTH, THEME, FIRE_SECTION, super::super::shortcuts::TITLE] {
            assert!(has(&rt, &format!("rule-{label}")), "a rule above {label}");
            assert!(has(&rt, label), "the title {label}");
        }
    }

    /// Feedback group 3, item 12: with the fire on, the panel ends with room as tall as the lit fire, so
    /// scrolled to the end its last row sits above the flames; with the fire off, the usual margin.
    #[test]
    fn the_last_settings_stay_above_the_fire() {
        let mut rt = open("# Title\n\nText.\n", true);
        rt.update(|s: &mut App, _| s.settings.fire = crate::settings::FireMode::Always);
        cmd(&mut rt, ',');
        let r = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("settings-scroll")).unwrap().rect;
        for _ in 0..40 {
            rt.wheel(r[0] + 40.0, r[1] + 100.0, -400.0);
            rt.frame(1.0 / 120.0);
        }
        settle(&mut rt);
        let last = rt.elements().into_iter().filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with("keys-row-"))).map(|e| e.rect).fold([0.0f32; 4], |a, r| if r[1] > a[1] { r } else { a });
        let fire = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("fire")).expect("the fire slot").rect;
        let lit_top = fire[1] + fire[3] - crate::fx::fire::lit_height(rt.state.settings.fire_height);
        assert!(last[1] + last[3] <= lit_top + 0.5, "the last row {last:?} above the lit fire (top {lit_top})");
        assert_eq!(bottom_room(false, 5.5), SECTION_PAD_TOP);
        assert!(bottom_room(true, 5.5) > bottom_room(true, 1.0));
    }

    /// Decision 2026-11-17-doc-fire-at-rest: the fire is a choice of three (Brief, Always, Off), Brief
    /// chosen by default; the note on the CPU sits right under Always; a click picks and saves; the
    /// room above the fire stays for Brief and Always, not for Off (constraints 1, 3, 7).
    #[test]
    fn the_fire_is_a_choice_of_three_with_a_note_under_always() {
        let mut rt = open("# Title\n\nText.\n", true);
        cmd(&mut rt, ',');
        let find = |rt: &Runtime<App>, k: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(k)).unwrap_or_else(|| panic!("missing {k}"));
        let theme = crate::ui::theme::Theme::for_mode(true);
        assert_eq!(find(&rt, "set-fire-modes").role, "radiogroup");
        for (mode, label) in FIRE_MODES {
            let e = find(&rt, &format!("set-fire-{label}"));
            assert_eq!(e.role, "radio");
            assert_eq!(e.fill == theme.sel, mode == FireMode::Brief, "{label}: only Brief chosen");
        }
        let (always, note, off) = (find(&rt, "set-fire-Always").rect, find(&rt, "set-fire-always-note").rect, find(&rt, "set-fire-Off").rect);
        assert!(note[1] >= always[1] + always[3] - 0.5 && note[1] + note[3] <= off[1] + 0.5, "the note between Always {always:?} and Off {off:?}: {note:?}");
        assert!(texts(&rt).iter().any(|t| t == FIRE_ALWAYS_NOTE));
        let column = find(&rt, "settings-scroll").rect;
        assert!(note[0] + note[2] <= column[0] + column[2] + 0.5, "the note wraps inside the panel: {note:?} in {column:?}");
        assert!(note[3] - ROW_GAP > UI_SMALL * 2.0, "two lines in the narrow column: {note:?}");
        assert!(crate::ui::contrast::ratio(theme.fg3, theme.panel) >= 4.5, "the note reads on the panel");
        for (mode, label) in [(FireMode::Always, "Always"), (FireMode::Off, "Off"), (FireMode::Brief, "Brief")] {
            let key = format!("set-fire-{label}");
            rt.update(move |_, tx| tx.scroll_into_view_in("settings-scroll", key.clone()));
            settle(&mut rt);
            let r = find(&rt, &format!("set-fire-{label}")).rect;
            let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
            rt.pointer_move(x, y);
            rt.pointer_down(x, y);
            rt.pointer_up(x, y);
            settle(&mut rt);
            assert_eq!(rt.state.settings.fire, mode, "{label}");
            assert_eq!(find(&rt, &format!("set-fire-{label}")).fill, theme.sel, "{label}: chosen");
            let room = bottom_room(mode.shows(), rt.state.settings.fire_height);
            assert_eq!(room > SECTION_PAD_TOP, mode != FireMode::Off, "{label}: room above the fire");
        }
    }

    /// Cmd+, opens the panel (in place of the outline), Cmd+, and Esc close it.
    #[test]
    fn cmd_comma_opens_and_esc_or_cmd_comma_closes() {
        let mut rt = open("# Title\n\nText.\n", true);
        assert!(!has(&rt, "settings"));
        cmd(&mut rt, ',');
        assert!(has(&rt, "settings"));
        assert!(
            !has(&rt, "outline"),
            "the panel takes the outline's place"
        );
        for label in [
            ROOTS, FONT, SIZE, THEME, "System", "Light", "Dark", FIRE, "Brief", "Always", "Off", FIRE_ALWAYS_NOTE, ADD_ROOT, "Inter",
            "SF Pro", "15.5 pt",
        ] {
            assert!(
                texts(&rt).iter().any(|t| t == label),
                "missing {label}: {:?}",
                texts(&rt)
            );
        }
        cmd(&mut rt, ',');
        assert!(!has(&rt, "settings"));
        cmd(&mut rt, ',');
        rt.key(KeyEvent {
            key: Key::Escape,
            mods: Mods::default(),
        });
        settle(&mut rt);
        assert!(!has(&rt, "settings"));
    }

    fn h(m: &rune_kit::MarkdownStyle) -> u64 {
        use std::hash::{DefaultHasher, Hash, Hasher};
        let mut s = DefaultHasher::new();
        m.hash(&mut s);
        s.finish()
    }

    /// Theme, font and size change the document at once, without a restart.
    #[test]
    fn theme_font_and_size_apply_live() {
        let mut rt = open("# Title\n\nText.\n", true);
        let before = h(rt.state.style(true));
        rt.update(|s, _| s.set_font("New York"));
        settle(&mut rt);
        assert_ne!(h(rt.state.style(true)), before);
        let font = h(rt.state.style(true));
        rt.update(|s, _| s.step_size(1));
        settle(&mut rt);
        assert_eq!(rt.state.settings.size, 17.0);
        assert_ne!(h(rt.state.style(true)), font);
        assert!(rt.state.dark(true));
        rt.update(|s, _| s.set_theme(ThemePref::Light));
        settle(&mut rt);
        assert!(!rt.state.dark(true), "Light wins over the dark system");
    }

    /// "Text width": the slider changes the column live; "Full" follows the panel
    /// minus the margins; the block at the top of the view stays at the top.
    #[test]
    fn text_width_changes_the_column_and_keeps_the_reading_place() {
        let body: String = (0..80).map(|n| format!("## Section {n}\n\nA paragraph that is long enough to wrap onto several lines when the column is narrow, number {n}.\n\n")).collect();
        let mut rt = open(&body, true);
        cmd(&mut rt, ',');
        assert!(texts(&rt).iter().any(|t| t == WIDTH), "the section");
        assert!(texts(&rt).iter().any(|t| t == "680 pt"));
        let rows = |rt: &Runtime<App>| rt.elements().into_iter().find(|e| e.key.as_deref() == Some("rows")).unwrap().rect;
        let doc = |rt: &Runtime<App>| rt.elements().into_iter().find(|e| e.key.as_deref() == Some("doc")).unwrap().rect;
        assert_eq!(rows(&rt)[2], 680.0);
        let (x, y) = { let d = doc(&rt); (d[0] + 100.0, d[1] + 100.0) };
        for _ in 0..30 {
            rt.wheel(x, y, -400.0);
            rt.frame(1.0 / 120.0);
        }
        settle(&mut rt);
        let first = rt.state.tab().unwrap().visible_first;
        assert!(first != rt.state.tab().unwrap().doc.blocks[0].id, "scrolled");
        let slider = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("set-width")).expect("slider");
        assert_eq!(slider.role, "slider");
        // A click at the end of the slider: "Full".
        let (sx, sy) = (slider.rect[0] + slider.rect[2] - 1.0, slider.rect[1] + slider.rect[3] / 2.0);
        rt.pointer_move(sx, sy);
        rt.pointer_down(sx, sy);
        rt.pointer_up(sx, sy);
        settle(&mut rt);
        assert_eq!(rt.state.settings.column, crate::settings::COLUMN_FULL);
        assert!(texts(&rt).iter().any(|t| t == "Full"));
        assert!((rows(&rt)[2] - doc(&rt)[2]).abs() < 1.0, "follows the panel: {:?} {:?}", rows(&rt), doc(&rt));
        assert_eq!(rt.state.tab().unwrap().visible_first, first, "same block at the top");
        // Arrows on the slider: one step back.
        rt.key(KeyEvent { key: Key::Left, mods: Mods::default() });
        settle(&mut rt);
        assert_eq!(rt.state.settings.column, 1240.0);
    }

    /// The row under the top edge of the view and the part of it above that edge (0..1 of its height).
    fn reading_place(rt: &Runtime<App>) -> (usize, f32) {
        let els = rt.elements();
        let top = els.iter().find(|e| e.key.as_deref() == Some("doc")).unwrap().rect[1];
        let doc = &rt.state.tab().unwrap().doc;
        (0..doc.blocks.len())
            .find_map(|i| {
                let k = crate::ui::doc_view::row_key(i, doc);
                let r = els.iter().find(|e| e.key.as_deref() == Some(k.as_str()))?.rect;
                (r[1] <= top + 0.5 && r[1] + r[3] > top + 0.5).then_some((i, (top - r[1]) / r[3]))
            })
            .expect("a row under the top edge")
    }

    /// The text width changes while the reader is halfway down a long file: the line at the top
    /// edge of the view stays there. Rows rewrap (one line at "Full", two at 680): the reader's
    /// row stays under the edge with the same part of it above the edge. Rune alone holds the
    /// first row it reports as visible, often the one above the edge, and the reader's row moved
    /// out of view.
    #[test]
    fn the_line_at_the_top_stays_when_the_width_changes() {
        let body: String = (0..80).map(|n| format!("## Section {n}\n\nA paragraph that is long enough to wrap onto several lines when the column is narrow, number {n}.\n\n")).collect();
        let mut rt = open(&body, true);
        rt.set_viewport((1900.0, 900.0), 2.0);
        settle(&mut rt);
        let (x, y) = (900.0, 300.0);
        for _ in 0..8 {
            rt.wheel(x, y, -400.0);
            rt.frame(1.0 / 120.0);
        }
        settle(&mut rt);
        let place = reading_place(&rt);
        assert!(place.0 > 20 && place.1 > 0.1, "halfway, in the middle of a row: {place:?}");
        for column in [crate::settings::COLUMN_FULL, 680.0, 1000.0, crate::settings::COLUMN_FULL, 680.0] {
            rt.update(|s, tx| s.set_column(column, tx));
            settle(&mut rt);
            let now = reading_place(&rt);
            assert_eq!(now.0, place.0, "same row at the top, width {column}: {now:?} was {place:?}");
            assert!((now.1 - place.1).abs() < 0.03, "same part above the edge, width {column}: {now:?} was {place:?}");
        }
        // A drag on the slider: one change per pt.
        for column in 681..=740 {
            rt.update(|s, tx| {
                s.set_column_live(column as f32, tx);
            });
            rt.frame(1.0 / 120.0);
        }
        settle(&mut rt);
        let now = reading_place(&rt);
        assert!(now.0 == place.0 && (now.1 - place.1).abs() < 0.03, "after a drag: {now:?} was {place:?}");
    }

    /// With the fire off (settings) the flames are not drawn: no `fire` slot and the
    /// compositor stays at rest. The "N new paragraphs below" label stays, in `fg2`
    /// on the panel. On, both are there.
    #[test]
    fn fire_off_keeps_the_label_and_draws_no_flames() {
        let mut rt = open("# Title\n\nText.\n", true);
        rt.update(|s, _| s.tabs[0].fire_since = Some(0.0));
        settle_some(&mut rt);
        assert!(
            has(&rt, "fire") && has(&rt, "fire-label"),
            "on: flames and label"
        );
        assert!(rt.state.effects_lit());
        rt.update(|s, _| s.set_fire_mode(FireMode::Off));
        settle(&mut rt);
        assert_eq!(rt.state.settings.fire, FireMode::Off);
        assert!(!has(&rt, "fire"), "off: no fire slot");
        assert!(
            has(&rt, "fire-label")
                && texts(&rt)
                    .iter()
                    .any(|t| t == "0 new paragraphs below"),
            "the label stays"
        );
        // The compositor still walks the instances, for the calm edge of the tab (metal-tab), not for the fire.
        let theme = crate::ui::theme::Theme::for_mode(true);
        assert!(crate::ui::contrast::ratio(theme.fg2, theme.panel) >= 4.5);
        // From the panel: a click on Brief turns it back on.
        cmd(&mut rt, ',');
        let r = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("set-fire-Brief"))
            .expect("Brief")
            .rect;
        let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        settle_some(&mut rt);
        assert_eq!(rt.state.settings.fire, FireMode::Brief);
        assert!(has(&rt, "fire"));
    }

    /// With the fire on the view animates: a few frames are enough.
    fn settle_some(rt: &mut Runtime<App>) {
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
    }

    /// A saved font that is no longer installed goes after the first choices present (New York can
    /// be missing); before reading CoreText the choices are the filtered ones.
    #[test]
    fn a_missing_saved_font_goes_after_the_first_choices() {
        let fallback = Panel::default();
        assert_eq!(
            fallback.fonts("Inter"),
            ["Inter", "SF Pro"],
            "New York is not there without the package"
        );
        assert_eq!(fallback.fonts("Gone"), ["Inter", "SF Pro", "Gone"]);
        let read = Panel {
            fonts: Some(crate::fonts::choices(&["Arial".into()])),
            ..Panel::default()
        };
        assert_eq!(read.fonts("Gone"), ["Inter", "SF Pro", "Gone", "Arial"]);
        assert_eq!(
            (0..4)
                .map(|i| read.font_at("Gone", i).unwrap())
                .collect::<Vec<_>>(),
            read.fonts("Gone")
        );
        assert_eq!(read.font_shape("Gone"), (4, Some(2)));
        assert_eq!(read.font_shape("Arial"), (3, None));
    }

    /// The font list is virtual and 8 and a half rows tall: the ninth shows cut off.
    #[test]
    fn the_font_list_shows_it_scrolls() {
        let mut rt = open("# T\n", true);
        cmd(&mut rt, ',');
        let els = rt.elements();
        let r = els
            .iter()
            .find(|e| e.key.as_deref() == Some("set-fonts-scroll"))
            .unwrap()
            .rect;
        let n = rt.state.settings_panel.font_shape("Inter").0 as f32;
        if n > FONT_ROWS {
            assert!((r[3] - ROW * FONT_ROWS).abs() < 0.5, "{r:?}");
            let built = els
                .iter()
                .filter(|e| e.key.as_deref().is_some_and(|k| k.starts_with("set-font-")))
                .count() as f32;
            assert!(
                built < n || n < 40.0,
                "virtual rows: {built} built out of {n}"
            );
        }
        let groups: Vec<_> = els
            .iter()
            .filter(|e| e.role == "radiogroup")
            .filter_map(|e| e.label.clone())
            .collect();
        assert_eq!(groups, [FONT, THEME, FIRE, FIRE_COLOR]);
    }

    /// A folder already watched (or under a watched one) is not added and the panel says so
    /// for 3 s, or until the next action.
    #[test]
    fn an_already_watched_folder_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        let mut rt = open("# T\n", true);
        cmd(&mut rt, ',');
        let now = Instant::now();
        let (mut first, mut inside) = (false, true);
        rt.update(|s, _| first = s.pick_root(&root, now));
        assert!(first);
        settle(&mut rt);
        assert!(!texts(&rt).iter().any(|t| t == ALREADY));
        rt.update(|s, _| inside = s.pick_root(&root.join("sub"), now));
        assert!(!inside, "under a watched one: not added");
        settle(&mut rt);
        assert!(texts(&rt).iter().any(|t| t == ALREADY));
        assert!(!rt.state.settings_panel.clone_expire(now + NOTICE_FOR / 2));
        assert!(rt.state.settings_panel.clone_expire(now + NOTICE_FOR));
        rt.update(|s, _| s.step_size(1));
        settle(&mut rt);
        assert!(
            !texts(&rt).iter().any(|t| t == ALREADY),
            "the next action removes it"
        );
    }

    /// The fire settings: three sliders and the five colors below the switch. They change
    /// the settings; with the fire off they are dimmed, disabled and do not respond.
    #[test]
    fn fire_look_controls_change_settings_and_go_dim_when_off() {
        let mut rt = open("# Title\n\nText.\n", true);
        cmd(&mut rt, ',');
        for label in [FIRE_STRENGTH, FIRE_HEIGHT, FIRE_SPEED, FIRE_COLOR, "Fire", "Blue", "Violet", "Green", "System accent", "100 %", "110 pt", "1×"] {
            assert!(texts(&rt).iter().any(|t| t == label), "missing {label}: {:?}", texts(&rt));
        }
        let find = |rt: &Runtime<App>, k: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(k)).unwrap_or_else(|| panic!("missing {k}"));
        let click = |rt: &mut Runtime<App>, k: &str, right: bool| {
            let key = k.to_owned();
            rt.update(move |_, tx| tx.scroll_into_view_in("settings-scroll", key.clone()));
            settle(rt);
            let r = find(rt, k).rect;
            let x = if right { r[0] + r[2] - 1.0 } else { r[0] + r[2] / 2.0 };
            let y = r[1] + r[3] / 2.0;
            rt.pointer_move(x, y);
            rt.pointer_down(x, y);
            rt.pointer_up(x, y);
            settle(rt);
        };
        assert_eq!(find(&rt, "set-fire-strength").role, "slider");
        click(&mut rt, "set-fire-strength", true);
        assert_eq!(rt.state.settings.fire_strength, 1.75);
        assert!(texts(&rt).iter().any(|t| t == "175 %"));
        rt.key(KeyEvent { key: Key::Left, mods: Mods::default() });
        settle(&mut rt);
        assert_eq!(rt.state.settings.fire_strength, 1.5, "the arrow moves one step");
        click(&mut rt, "set-fire-height", true);
        assert_eq!(rt.state.settings.fire_height, *crate::fx::fire::HEIGHTS.last().unwrap());
        click(&mut rt, "set-fire-speed", true);
        assert_eq!(rt.state.settings.fire_speed, 3.0);
        // The colors are at the bottom of the panel: scroll it down there (the wheel low in the
        // panel, off the font list, which scrolls on its own).
        let r = find(&rt, "settings-scroll").rect;
        for _ in 0..20 {
            rt.wheel(r[0] + 40.0, r[1] + r[3] - 60.0, -400.0);
            rt.frame(1.0 / 120.0);
        }
        settle(&mut rt);
        let v = find(&rt, "set-fire-color-Violet").rect;
        assert!(v[1] + v[3] <= r[1] + r[3], "Violet shows: {v:?} in {r:?}");
        click(&mut rt, "set-fire-color-Violet", false);
        assert_eq!(rt.state.settings.fire_color, crate::fx::fire::FireColor::Violet);
        assert!(find(&rt, "set-fire-look").opacity > 0.99, "on: solid");

        rt.update(|s, _| s.set_fire_mode(FireMode::Off));
        settle(&mut rt);
        assert!(find(&rt, "set-fire-look").opacity < 0.5, "off: dimmed");
        assert!(!find(&rt, "set-fire-strength").focusable, "off: out of the Tab order");
        // With the fire off the room at the bottom shrinks and the panel moves: bring the slider into view.
        for _ in 0..20 {
            let k = find(&rt, "set-fire-strength").rect;
            if k[1] >= r[1] {
                break;
            }
            rt.wheel(r[0] + 40.0, r[1] + r[3] - 60.0, 40.0);
            settle(&mut rt);
        }
        let k = find(&rt, "set-fire-strength").rect;
        assert!(k[1] >= r[1] && k[1] + k[3] <= r[1] + r[3], "the slider shows: {k:?}");
        click(&mut rt, "set-fire-strength", false);
        click(&mut rt, "set-fire-color-Blue", false);
        assert_eq!(rt.state.settings.fire_strength, 1.5, "off: the slider does not respond");
        assert_eq!(rt.state.settings.fire_color, crate::fx::fire::FireColor::Violet, "off: the colors do not respond");
    }
}
