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

use std::path::{Path, PathBuf};

use rune::{El, FileDialogOptions, ScrollSpec, TextStyle, Tx, col, el, icon, row, text};
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
pub const FIRE: &str = "Fuoco per il testo nuovo";
/// Righe della lista dei font visibili prima di scorrere.
const FONT_ROWS: f32 = 8.0;

/// Lo stato del pannello: aperto o no, e le scelte di font (lette da CoreText alla prima apertura).
#[derive(Default)]
pub struct Panel {
    pub open: bool,
    fonts: Option<Vec<String>>,
}

impl Panel {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        if self.open && self.fonts.is_none() {
            self.fonts = Some(crate::fonts::choices(&crate::fonts::installed()));
        }
    }

    pub fn close(&mut self) -> bool {
        std::mem::take(&mut self.open)
    }

    /// Le scelte di font; il font salvato c'è sempre, anche se non è più installato.
    fn fonts(&self, current: &str) -> Vec<String> {
        let mut out = self.fonts.clone().unwrap_or_else(|| {
            crate::fonts::FIRST
                .iter()
                .map(|s| (*s).to_owned())
                .collect()
        });
        if !out.iter().any(|f| f == current) {
            out.insert(crate::fonts::FIRST.len(), current.to_owned());
        }
        out
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
                |s: &mut App, result, _| match result {
                    Ok(Some(path)) => {
                        s.add_root(&path);
                    }
                    Ok(None) => {}
                    Err(e) => eprintln!("runemd: cartella non scelta: {e}"),
                },
            );
        });
    body = body.child(press(add, "set-add-root", s, hooks, theme, motion));

    body = body.child(section(FONT, theme));
    let fonts = s.settings_panel.fonts(&set.font);
    let list = col()
        .key("set-fonts")
        .w_full()
        .children(fonts.iter().map(|f| {
            let name = f.clone();
            option(
                s,
                hooks,
                format!("set-font-{f}"),
                f,
                *f == set.font,
                theme,
                motion,
                move |s: &mut App| s.set_font(&name),
            )
        }));
    body = body.child(
        col()
            .key("set-fonts-scroll")
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

    body = body.child(section(THEME, theme));
    for (pref, label) in THEMES {
        body = body.child(option(
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

    body = body
        .child(el().h(SECTION_PAD_TOP))
        .child(fire_toggle(s, hooks, theme, motion));

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
}
