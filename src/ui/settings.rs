//! Il pannello delle impostazioni (spec, "Impostazioni"): Cmd+, lo apre e lo chiude, Esc lo
//! chiude. Sta nel pannello del documento, a destra, al posto dell'indice dei titoli: il documento
//! resta in vista e cambia subito con font, dimensione e tema.
//!
//! Nessun mockup: solo token di DESIGN.md. Colonna larga come la sidebar (`SIDEBAR`), separata dal
//! documento da una linea di 0,5 in `line`; etichette di sezione come quelle della sidebar (UI
//! piccola in `fg3`); righe da 28 (`ROW`, raggio 6, hover `sel`, scala 0,98 premute). La scelta
//! fatta ha riempimento `sel`, testo `fg` e il segno di spunta in `fg`; le altre testo `fg2`. La
//! casella del fuoco ha il bordo in `fg3` e, accesa, il riempimento `fg2` con la spunta in
//! `panel`: tutti ≥ 3:1 sul pannello nei due temi (`ui::contrast`). Nessun accento, nessuna pillola.

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::time::Instant;

use rune::{
    El, FileDialogOptions, GrowthSpec, ScrollSpec, TextStyle, Tx, col, el, icon, row, text,
};
use rune_kit::glyphs;

use super::sidebar::{self, Hooks, Motion};
use super::theme::{
    HAIRLINE, ICON, OUTLINE_PAD_X, R_KBD, ROW, ROW_GAP, ROW_PAD, SECTION_PAD_BOTTOM,
    SECTION_PAD_TOP, SIDEBAR, Theme, UI_SMALL, UI_SMALL_WEIGHT, UI_STRONG_WEIGHT, UI_TEXT,
    UI_WEIGHT,
};
use crate::app::App;
use crate::settings::ThemePref;

/// I testi del pannello (italiano, esatti).
pub const TITLE: &str = "Impostazioni";
pub const ROOTS: &str = "Cartelle osservate";
pub const NO_ROOTS: &str = "Nessuna cartella";
pub const ADD_ROOT: &str = "Aggiungi cartella…";
pub const REMOVE_ROOT: &str = "Togli";
pub const DIALOG_TITLE: &str = "Scegli una cartella da osservare";
pub const FONT: &str = "Font";
pub const SIZE: &str = "Dimensione";
pub const SMALLER: &str = "Più piccolo";
pub const BIGGER: &str = "Più grande";
pub const THEME: &str = "Tema";
pub const THEMES: [(ThemePref, &str); 3] = [
    (ThemePref::System, "Sistema"),
    (ThemePref::Light, "Chiaro"),
    (ThemePref::Dark, "Scuro"),
];
pub const FIRE: &str = "Fuoco in fondo al pannello";
pub const WIDTH: &str = "Larghezza del testo";
pub const FIRE_STRENGTH: &str = "Intensità";
pub const FIRE_HEIGHT: &str = "Altezza";
pub const FIRE_SPEED: &str = "Velocità";
pub const FIRE_COLOR: &str = "Colore";
/// L'opacità dei controlli del fuoco quando è spento.
const DIMMED: f32 = 0.4;
/// Il cursore della larghezza: binario di 2 in `fg3`, parte scelta e pomello (12) in `fg2`.
const TRACK: f32 = 2.0;
const THUMB: f32 = 12.0;
/// Lo spazio del valore a destra del cursore ("1240 pt", "Intera").
const WIDTH_VALUE: f32 = 64.0;
/// Il riscontro di "Aggiungi cartella…" quando la cartella scelta è già osservata (o sta sotto
/// una osservata): resta `NOTICE_FOR` o fino alla prossima azione.
pub const ALREADY: &str = "Già osservata";
pub const NOTICE_FOR: std::time::Duration = std::time::Duration::from_secs(3);
/// Righe della lista dei font visibili prima di scorrere: mezza riga tagliata dice che scorre.
const FONT_ROWS: f32 = 8.5;

/// Lo stato del pannello: aperto o no, le scelte di font (lette da CoreText alla prima apertura)
/// e fino a quando si vede "Già osservata".
#[derive(Default)]
pub struct Panel {
    pub open: bool,
    fonts: Option<Vec<String>>,
    notice: Option<Instant>,
}

impl Panel {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.notice = None;
        if self.open && self.fonts.is_none() {
            self.fonts = Some(crate::fonts::choices(&crate::fonts::installed()));
        }
    }

    pub fn close(&mut self) -> bool {
        self.notice = None;
        std::mem::take(&mut self.open)
    }

    /// "Già osservata" fino a `now + NOTICE_FOR`.
    pub fn notice_already(&mut self, now: Instant) {
        self.notice = Some(now + NOTICE_FOR);
    }

    /// Un'altra azione nel pannello: il riscontro sparisce.
    pub fn clear_notice(&mut self) {
        self.notice = None;
    }

    /// Se a `now` il riscontro sarebbe scaduto (i test).
    #[cfg(test)]
    fn clone_expire(&self, now: Instant) -> bool {
        self.notice.is_some_and(|t| now >= t)
    }

    /// "Già osservata" si vede.
    pub fn notice(&self) -> bool {
        self.notice.is_some()
    }

    /// Toglie il riscontro scaduto a `now`. Vero se la vista cambia.
    pub fn expire(&mut self, now: Instant) -> bool {
        if self.notice.is_some_and(|t| now >= t) {
            self.notice = None;
            return true;
        }
        false
    }

    /// Le scelte (prima di leggere CoreText: le prime, filtrate come `fonts::choices`).
    fn choices(&self) -> Cow<'_, [String]> {
        match &self.fonts {
            Some(f) => Cow::Borrowed(f),
            None => Cow::Owned(crate::fonts::choices(&[])),
        }
    }

    /// Dove va il font salvato se non è fra le scelte (non più installato): dopo l'ultima delle
    /// prime scelte presenti. `None` se c'è già.
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

    /// Le scelte di font; il font salvato c'è sempre, anche se non è più installato.
    #[cfg(test)]
    fn fonts(&self, current: &str) -> Vec<String> {
        let mut out = self.choices().into_owned();
        if let Some(at) = Self::missing_at(&out, current) {
            out.insert(at, current.to_owned());
        }
        out
    }

    /// Quante righe ha la lista dei font e dove sta il font salvato aggiunto (la versione della
    /// lista virtuale).
    fn font_shape(&self, current: &str) -> (usize, Option<usize>) {
        let choices = self.choices();
        let at = Self::missing_at(&choices, current);
        (choices.len() + usize::from(at.is_some()), at)
    }

    /// La riga `i` della lista dei font, senza copiare la lista.
    fn font_at(&self, current: &str, i: usize) -> Option<String> {
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

/// La dimensione come si scrive in italiano: "15,5 pt", "17 pt".
pub fn size_label(size: f32) -> String {
    let s = if size.fract() == 0.0 {
        format!("{size:.0}")
    } else {
        format!("{size:.1}").replace('.', ",")
    };
    format!("{s} pt")
}

/// La cartella come la si legge: la home diventa "~".
pub fn root_label(root: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home.as_deref().and_then(|h| root.strip_prefix(h).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => root.display().to_string(),
    }
}

/// La colonna del pannello. `hover`: il puntatore è fine e può passare sopra.
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
        .pb(SECTION_PAD_TOP)
        .child(title);

    body = body.child(section(ROOTS, theme));
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
                    Err(e) => eprintln!("runemd: cartella non scelta: {e}"),
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
    // Righe virtuali: la lista ha centinaia di famiglie, se ne costruiscono solo quelle vicine
    // alla vista. Ogni riga ritrova il suo font dallo stato (`Panel::font_at`).
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
                option(
                    s,
                    hooks,
                    format!("set-font-{name}"),
                    &name,
                    chosen,
                    &th,
                    motion,
                    move |s: &mut App| s.set_font(&pick),
                )
            },
        )
        .virtual_rows_version((count, at));
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
        .child(el().h(SECTION_PAD_TOP))
        .child(fire_toggle(s, hooks, theme, motion))
        .child(fire_look(s, hooks, theme, motion));

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

/// "Larghezza del testo": il cursore a passi di `settings::COLUMNS` (da 680 a "Intera").
pub(crate) fn width_slider(key: &'static str, column: f32, theme: &Theme) -> El {
    stepped_slider(
        key,
        WIDTH,
        crate::settings::column_t(column),
        crate::settings::column_label(column),
        true,
        theme,
        |s: &mut App, at, tx: &mut Tx| s.set_column(crate::settings::column_at(at), tx),
        |s: &mut App, step, tx: &mut Tx| s.set_column(s.settings.step_column(step), tx),
    )
}

/// Un cursore a passi con il valore `value` a destra (chiave `<key>-value`), al punto `t`
/// (0..1). Trascinare o cliccare chiama `at` con il punto; le frecce chiamano `step` con −1 o
/// +1. Il binario è in `fg3` e la parte scelta col pomello in `fg2` (≥ 3:1). Spento
/// (`enabled` falso): niente gestori, `disabled` per l'accessibilità; l'attenuazione la mette
/// chi lo contiene.
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

/// Le impostazioni del fuoco sotto l'interruttore: Intensità, Altezza, Velocità (cursori a
/// passi) e Colore (scelte). Con il fuoco spento sono attenuate (`DIMMED`) e disabilitate.
fn fire_look(s: &App, hooks: Hooks<App>, theme: &Theme, motion: Motion) -> El {
    use crate::settings::FireKnob;
    let on = s.settings.fire;
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

/// L'etichetta di un controllo dentro una sezione: 13 in `fg2`, senza lo spazio di sezione.
fn sub_label(label: &'static str, theme: &Theme) -> El {
    text(label)
        .key(label)
        .font(ui_font())
        .color(theme.fg2)
        .pt(ROW_GAP)
        .px(ROW_PAD)
}

/// Un'etichetta di sezione: 12/500 in `fg3` (`section-pad`), come nella sidebar.
fn section(label: &'static str, theme: &Theme) -> El {
    text(label)
        .key(label)
        .font(TextStyle::new(UI_SMALL, UI_SMALL_WEIGHT))
        .color(theme.fg3)
        .pt(SECTION_PAD_TOP)
        .px(ROW_PAD)
        .pb(SECTION_PAD_BOTTOM)
}

/// `e` premibile con la chiave `key` (hover `sel`, scala 0,98).
fn press(e: El, key: &str, s: &App, hooks: Hooks<App>, theme: &Theme, motion: Motion) -> El {
    sidebar::pressable(e, key.to_owned(), &s.sidebar, hooks, theme, motion, true)
        .key(key.to_owned())
}

/// Una cartella osservata: icona, percorso in `fg2` (troncato) e "Togli" a destra.
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

/// Una scelta (font, tema): la spunta e il testo in `fg` con il riempimento `sel` se è quella
/// fatta, altrimenti solo il testo in `fg2`.
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
                .font(ui_font())
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

/// Una scelta che non si può fare (fuoco spento): come `option`, senza pressione né hover.
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

/// L'interruttore del fuoco: una casella con il bordo in `fg3`; accesa, piena in `fg2` con la
/// spunta in `panel`.
fn fire_toggle(s: &App, hooks: Hooks<App>, theme: &Theme, motion: Motion) -> El {
    let on = s.settings.fire;
    let mut boxed = el()
        .key("set-fire-box")
        .w(ICON)
        .h(ICON)
        .radius(R_KBD)
        .center();
    boxed = if on {
        boxed
            .fill(theme.fg2)
            .child(icon(&glyphs::CHECK_SMALL, ICON).color(theme.panel))
    } else {
        boxed.border(1.0, theme.fg3)
    };
    let e = row()
        .w_full()
        .h(ROW)
        .px(ROW_PAD)
        .gap(ROW_GAP)
        .center_y()
        .role("checkbox")
        .label(FIRE)
        .checked(on)
        .child(el().w(ICON).h(ICON).center().child(boxed))
        .child(
            text(FIRE)
                .font(ui_font())
                .color(theme.fg2)
                .truncate()
                .shrink()
                .min_w(0.0)
                .a11y_hidden(),
        )
        .on_press(|s: &mut App| s.toggle_fire());
    press(e, "set-fire", s, hooks, theme, motion)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use rune::{Env, Key, KeyEvent, Mods, Runtime};

    use super::*;
    use crate::doc::document::Document;

    #[test]
    fn sizes_read_in_italian() {
        assert_eq!(size_label(15.5), "15,5 pt");
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
        panic!("la vista non si ferma");
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

    /// Cmd+, apre il pannello (al posto dell'indice), Cmd+, ed Esc lo chiudono.
    #[test]
    fn cmd_comma_opens_and_esc_or_cmd_comma_closes() {
        let mut rt = open("# Titolo\n\nTesto.\n", true);
        assert!(!has(&rt, "settings"));
        cmd(&mut rt, ',');
        assert!(has(&rt, "settings"));
        assert!(
            !has(&rt, "outline"),
            "il pannello prende il posto dell'indice"
        );
        for label in [
            ROOTS, FONT, SIZE, THEME, "Sistema", "Chiaro", "Scuro", FIRE, ADD_ROOT, "Inter",
            "SF Pro", "15,5 pt",
        ] {
            assert!(
                texts(&rt).iter().any(|t| t == label),
                "manca {label}: {:?}",
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

    /// Tema, font e dimensione cambiano il documento subito, senza riavvio.
    #[test]
    fn theme_font_and_size_apply_live() {
        let mut rt = open("# Titolo\n\nTesto.\n", true);
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
        assert!(!rt.state.dark(true), "Chiaro vince sul sistema scuro");
    }

    /// "Larghezza del testo": il cursore cambia la colonna dal vivo; "Intera" segue il pannello
    /// meno i margini; il blocco in cima alla vista resta in cima.
    #[test]
    fn text_width_changes_the_column_and_keeps_the_reading_place() {
        let body: String = (0..80).map(|n| format!("## Sezione {n}\n\nUn paragrafo abbastanza lungo da andare a capo più volte quando la colonna è stretta, numero {n}.\n\n")).collect();
        let mut rt = open(&body, true);
        cmd(&mut rt, ',');
        assert!(texts(&rt).iter().any(|t| t == WIDTH), "la sezione");
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
        assert!(first != rt.state.tab().unwrap().doc.blocks[0].id, "scorso");
        let slider = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("set-width")).expect("cursore");
        assert_eq!(slider.role, "slider");
        // Un clic in fondo al cursore: "Intera".
        let (sx, sy) = (slider.rect[0] + slider.rect[2] - 1.0, slider.rect[1] + slider.rect[3] / 2.0);
        rt.pointer_move(sx, sy);
        rt.pointer_down(sx, sy);
        rt.pointer_up(sx, sy);
        settle(&mut rt);
        assert_eq!(rt.state.settings.column, crate::settings::COLUMN_FULL);
        assert!(texts(&rt).iter().any(|t| t == "Intera"));
        assert!((rows(&rt)[2] - doc(&rt)[2]).abs() < 1.0, "segue il pannello: {:?} {:?}", rows(&rt), doc(&rt));
        assert_eq!(rt.state.tab().unwrap().visible_first, first, "stesso blocco in cima");
        // Le frecce sul cursore: un passo indietro.
        rt.key(KeyEvent { key: Key::Left, mods: Mods::default() });
        settle(&mut rt);
        assert_eq!(rt.state.settings.column, 1240.0);
    }

    /// Con il fuoco spento (impostazioni) le fiamme non si disegnano: niente slot `fire` e il
    /// compositor resta a riposo. L'etichetta "N paragrafi nuovi più in basso" resta, in `fg2`
    /// sul pannello. Acceso, ci sono tutti e due.
    #[test]
    fn fire_off_keeps_the_label_and_draws_no_flames() {
        let mut rt = open("# Titolo\n\nTesto.\n", true);
        rt.update(|s, _| s.tabs[0].fire_since = Some(0.0));
        settle_some(&mut rt);
        assert!(
            has(&rt, "fire") && has(&rt, "fire-label"),
            "acceso: fiamme ed etichetta"
        );
        assert!(rt.state.effects_lit());
        rt.update(|s, _| s.toggle_fire());
        settle(&mut rt);
        assert!(!rt.state.settings.fire);
        assert!(!has(&rt, "fire"), "spento: nessuno slot del fuoco");
        assert!(
            has(&rt, "fire-label")
                && texts(&rt)
                    .iter()
                    .any(|t| t == "0 paragrafi nuovi più in basso"),
            "l'etichetta resta"
        );
        assert!(
            !rt.state.effects_lit(),
            "il compositor non scorre le istanze per il fuoco"
        );
        let theme = crate::ui::theme::Theme::for_mode(true);
        assert!(crate::ui::contrast::ratio(theme.fg2, theme.panel) >= 4.5);
        // Dal pannello: un clic sulla casella lo riaccende.
        cmd(&mut rt, ',');
        let r = rt
            .elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("set-fire"))
            .expect("interruttore")
            .rect;
        let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        settle_some(&mut rt);
        assert!(rt.state.settings.fire);
        assert!(has(&rt, "fire"));
    }

    /// Con il fuoco acceso la vista anima: qualche frame basta.
    fn settle_some(rt: &mut Runtime<App>) {
        for _ in 0..30 {
            rt.frame(1.0 / 60.0);
        }
    }

    /// Un font salvato che non è più installato va dopo le prime scelte presenti (New York può
    /// mancare); prima di leggere CoreText le scelte sono quelle filtrate.
    #[test]
    fn a_missing_saved_font_goes_after_the_first_choices() {
        let fallback = Panel::default();
        assert_eq!(
            fallback.fonts("Inter"),
            ["Inter", "SF Pro"],
            "New York non c'è senza il pacchetto"
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

    /// La lista dei font è virtuale e alta 8 righe e mezza: la nona si vede tagliata.
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
                "righe virtuali: {built} costruite su {n}"
            );
        }
        let groups: Vec<_> = els
            .iter()
            .filter(|e| e.role == "radiogroup")
            .filter_map(|e| e.label.clone())
            .collect();
        assert_eq!(groups, [FONT, THEME, FIRE_COLOR]);
    }

    /// Una cartella già osservata (o sotto una osservata) non si aggiunge e il pannello lo dice
    /// per 3 s, o fino alla prossima azione.
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
        assert!(!inside, "sotto una osservata: non si aggiunge");
        settle(&mut rt);
        assert!(texts(&rt).iter().any(|t| t == ALREADY));
        assert!(!rt.state.settings_panel.clone_expire(now + NOTICE_FOR / 2));
        assert!(rt.state.settings_panel.clone_expire(now + NOTICE_FOR));
        rt.update(|s, _| s.step_size(1));
        settle(&mut rt);
        assert!(
            !texts(&rt).iter().any(|t| t == ALREADY),
            "la prossima azione lo toglie"
        );
    }

    /// Le impostazioni del fuoco: tre cursori e i cinque colori sotto l'interruttore. Cambiano
    /// le impostazioni; con il fuoco spento sono attenuati, disabilitati e non rispondono.
    #[test]
    fn fire_look_controls_change_settings_and_go_dim_when_off() {
        let mut rt = open("# Titolo\n\nTesto.\n", true);
        cmd(&mut rt, ',');
        for label in [FIRE_STRENGTH, FIRE_HEIGHT, FIRE_SPEED, FIRE_COLOR, "Fuoco", "Blu", "Viola", "Verde", "Accento di sistema", "100 %", "110 pt", "1×"] {
            assert!(texts(&rt).iter().any(|t| t == label), "manca {label}: {:?}", texts(&rt));
        }
        let find = |rt: &Runtime<App>, k: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(k)).unwrap_or_else(|| panic!("manca {k}"));
        let click = |rt: &mut Runtime<App>, k: &str, right: bool| {
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
        assert_eq!(rt.state.settings.fire_strength, 1.5, "la freccia va di un passo");
        click(&mut rt, "set-fire-height", true);
        assert_eq!(rt.state.settings.fire_height, 1.6);
        click(&mut rt, "set-fire-speed", true);
        assert_eq!(rt.state.settings.fire_speed, 3.0);
        // I colori sono in fondo al pannello: lo si scorre fin lì.
        let r = find(&rt, "settings-scroll").rect;
        for _ in 0..20 {
            rt.wheel(r[0] + 40.0, r[1] + 100.0, -400.0);
            rt.frame(1.0 / 120.0);
        }
        settle(&mut rt);
        let v = find(&rt, "set-fire-color-Viola").rect;
        assert!(v[1] + v[3] <= r[1] + r[3], "Viola si vede: {v:?} in {r:?}");
        click(&mut rt, "set-fire-color-Viola", false);
        assert_eq!(rt.state.settings.fire_color, crate::fx::fire::FireColor::Violet);
        assert!(find(&rt, "set-fire-look").opacity > 0.99, "acceso: pieno");

        rt.update(|s, _| s.toggle_fire());
        settle(&mut rt);
        assert!(find(&rt, "set-fire-look").opacity < 0.5, "spento: attenuato");
        assert!(!find(&rt, "set-fire-strength").focusable, "spento: fuori dal giro del Tab");
        let k = find(&rt, "set-fire-strength").rect;
        assert!(k[1] >= r[1] && k[1] + k[3] <= r[1] + r[3], "il cursore si vede: {k:?}");
        click(&mut rt, "set-fire-strength", false);
        click(&mut rt, "set-fire-color-Blu", false);
        assert_eq!(rt.state.settings.fire_strength, 1.5, "spento: il cursore non risponde");
        assert_eq!(rt.state.settings.fire_color, crate::fx::fire::FireColor::Violet, "spento: i colori non rispondono");
    }
}
