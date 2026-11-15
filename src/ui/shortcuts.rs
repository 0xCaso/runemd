//! Il pulsante ⌘ delle scorciatoie nella barra delle schede e il suo popover
//! (docs/decisions/2026-11-15-scorciatoie-popover.md; mockup `2026-11-15-scorciatoie`, variante
//! "popover").
//!
//! Il pulsante sta a sinistra di ↔ e ne copia l'aspetto (28 × 28, glifo in `fg2`, hover `sel`,
//! `sel` pieno col popover aperto). Il popover è quello della larghezza (`tabs::width_popover`):
//! superficie `panel`, raggio `R_PANEL`, bordo e ombra del pannello, montato alla radice sotto
//! il pulsante; un clic fuori o Esc lo chiudono. Aprirne uno chiude l'altro.

use rune::{Align, Color, El, Side, TextStyle, col, el, row, text};

use super::sidebar::{self, Motion};

use super::theme::{HAIRLINE, KBD_PAD_X, KBD_PAD_Y, R_KBD, R_PANEL, R_ROW, ROW, Theme, UI_SMALL, UI_SMALL_WEIGHT, UI_WEIGHT};
use crate::app::App;

pub const KEYS_BUTTON: &str = "keys-button";
pub const KEYS_POPOVER: &str = "keys-popover";
/// Titolo del popover, etichetta e tooltip del pulsante.
pub const TITLE: &str = "Scorciatoie";
/// Larghezza del popover (mockup: 300).
const POPOVER_W: f32 = 300.0;
/// Margini dentro il popover (mockup: 12 sopra e sotto, 14 ai lati).
const PAD_Y: f32 = 12.0;
const PAD_X: f32 = 14.0;
/// Altezza di una riga (mockup: 24).
const KEY_ROW: f32 = 24.0;
/// Testo dei tasti (mockup: 11,5).
const KEY_TEXT: f32 = 11.5;
const GLYPH: f32 = 15.0;
const POPOVER_GAP: f32 = 4.0;

/// Le scorciatoie dell'app, nei gruppi del mockup (design/mockups/2026-11-15-scorciatoie/scorciatoie.md).
pub const GROUPS: &[(&str, &[(&str, &str)])] = &[
    ("File", &[("Apri file", "⌘P"), ("Scegli e apri in Apri file", "↑↓  ↵"), ("Nuovo file", "⌘N"), ("Salva (nel sorgente)", "⌘S"), ("Chiudi la scheda", "⌘W")]),
    ("Schede", &[("Vai alla scheda", "⌘1 … ⌘8"), ("Ultima scheda", "⌘9"), ("Scheda dopo, prima", "⌃⇥  ⌃⇧⇥")]),
    (
        "Lettura",
        &[
            ("Sorgente e lettura", "⌘E"),
            ("Cerca nel file", "⌘F"),
            ("Risultato dopo, prima", "↵  ⇧↵"),
            ("Larghezza del testo", "⌘⌥+  ⌘⌥−"),
            ("Seleziona tutto", "⌘A"),
        ],
    ),
    ("App", &[("Impostazioni", "⌘,"), ("Chiudi popover e ricerca", "Esc")]),
];

/// Apre o chiude il popover delle scorciatoie; aprendolo chiude quello della larghezza.
pub fn toggle(s: &mut App) {
    s.keys_popover = !s.keys_popover;
    if s.keys_popover {
        s.width_popover = false;
    }
}

/// Il pulsante ⌘, a sinistra di ↔.
pub(super) fn button(s: &App, theme: &Theme, motion: Motion) -> El {
    let glyph = text("⌘").key("keys-button-glyph").font(TextStyle::new(GLYPH, UI_WEIGHT)).color(theme.fg2).a11y_hidden();
    let mut b = el()
        .key(KEYS_BUTTON)
        .size(ROW, ROW)
        .radius(R_ROW)
        .center()
        .role("button")
        .label(TITLE)
        .help(TITLE)
        .expanded(s.keys_popover)
        .child(glyph)
        .on_press(toggle);
    if s.keys_popover {
        b = b.fill(theme.sel);
    } else if motion.hover {
        b = b.hover(theme.sel);
    }
    b
}

/// Il popover, da montare alla radice dopo la barra. Vuoto se è chiuso. Titolo in `fg2`,
/// gruppi in `fg3` 500, righe in `fg2` con i tasti in `fg2` dentro un bordo `line`.
pub fn popover(s: &App, theme: &Theme) -> Vec<El> {
    if !s.keys_popover {
        return Vec::new();
    }
    let scrim = el().key("keys-scrim").role("none").absolute(0.0, 0.0).w_full().h_full().on_press(|s: &mut App| s.keys_popover = false);
    let mut list = col().key("keys-list").w_full().child(text(TITLE).key("keys-popover-title").font(sidebar::ui_font()).color(theme.fg2).mb(6.0));
    for (gi, (group, keys)) in GROUPS.iter().enumerate() {
        list = list.child(
            text(*group)
                .key(format!("keys-group-{gi}"))
                .font(TextStyle::new(UI_SMALL, UI_SMALL_WEIGHT))
                .color(theme.fg3)
                .mt(if gi == 0 { 4.0 } else { 10.0 })
                .mb(3.0),
        );
        for (ki, (what, keys)) in keys.iter().enumerate() {
            let cap = el()
                .border(HAIRLINE, theme.line)
                .radius(R_KBD)
                .px(KBD_PAD_X)
                .py(KBD_PAD_Y)
                .child(text(*keys).key(format!("keys-cap-{gi}-{ki}")).font(TextStyle::new(KEY_TEXT, UI_WEIGHT)).color(theme.fg2));
            let label = text(*what).font(sidebar::ui_font()).color(theme.fg2).grow(1.0).shrink().min_w(0.0);
            list = list.child(row().key(format!("keys-row-{gi}-{ki}")).w_full().h(KEY_ROW).gap(12.0).center_y().child(label).child(cap));
        }
    }
    let surface = col()
        .key(KEYS_POPOVER)
        .role("dialog")
        .label(TITLE)
        .focus_return(KEYS_BUTTON)
        .w(POPOVER_W)
        .px(PAD_X)
        .py(PAD_Y)
        .fill(theme.panel)
        .radius(R_PANEL)
        .border(HAIRLINE, theme.fg.alpha(0.10))
        .shadow(10.0, 30.0, Color::BLACK.alpha(0.35))
        .child(list)
        .anchored(KEYS_BUTTON, Side::Bottom, Align::End, POPOVER_GAP);
    vec![scrim, surface]
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use rune::{Env, Key, KeyEvent, Mods, Runtime};

    use super::*;
    use crate::doc::document::Document;
    use crate::ui::tabs::{WIDTH_BUTTON, WIDTH_POPOVER};

    fn open() -> Runtime<App> {
        let mut state = App::bare(crate::index::Library::load(Vec::new(), None));
        state.push_tab(Document::from_source(Path::new("t.md"), "# Titolo\n\nTesto.\n"));
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
        panic!("la vista non si ferma");
    }

    fn find(rt: &Runtime<App>, key: &str) -> Option<rune::ElementInfo> {
        rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key))
    }

    fn tap_el(rt: &mut Runtime<App>, key: &str) {
        let r = find(rt, key).expect(key).rect;
        tap(rt, r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
    }

    fn tap(rt: &mut Runtime<App>, x: f32, y: f32) {
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        settle(rt);
    }

    /// Ogni riga di scorciatoie.md (tranne Esc, che il mockup scrive come "Chiudi popover e
    /// ricerca") ha i suoi tasti nel popover.
    #[test]
    fn the_list_is_the_apps_list() {
        let md = include_str!("../../design/mockups/2026-11-15-scorciatoie/scorciatoie.md");
        let caps: Vec<&str> = GROUPS.iter().flat_map(|(_, k)| k.iter().map(|(_, c)| *c)).collect();
        for want in ["⌘P", "⌘N", "⌘S", "⌘W", "⌘1 … ⌘8", "⌘9", "⌘E", "⌘F", "⌘A", "⌘,", "Esc", "↑↓  ↵", "⌃⇥  ⌃⇧⇥", "↵  ⇧↵", "⌘⌥+  ⌘⌥−"] {
            assert!(caps.contains(&want), "manca {want}");
        }
        for k in ["⌘P", "⌘N", "⌘S", "⌘W", "⌘9", "⌘E", "⌘F", "⌘A", "⌘,"] {
            assert!(md.contains(k), "{k} non è in scorciatoie.md");
        }
        let groups: Vec<&str> = GROUPS.iter().map(|(g, _)| *g).collect();
        assert_eq!(groups, ["File", "Schede", "Lettura", "App"], "gruppi del mockup");
    }

    /// Vincoli 1, 2, 4: ⌘ accanto a ↔ (a sinistra), con tooltip; il clic apre il popover sotto
    /// il pulsante; Esc, un clic fuori e il pulsante lo chiudono; aprire ↔ chiude ⌘ e viceversa.
    #[test]
    fn the_keys_button_opens_a_popover_like_the_width_one() {
        let mut rt = open();
        let b = find(&rt, KEYS_BUTTON).expect("pulsante ⌘");
        let w = find(&rt, WIDTH_BUTTON).unwrap();
        assert_eq!(b.role, "button");
        assert_eq!(b.help.as_deref(), Some(TITLE), "tooltip");
        assert!(b.rect[0] + b.rect[2] <= w.rect[0] && w.rect[0] - (b.rect[0] + b.rect[2]) < 8.0, "subito a sinistra di ↔: {:?} {:?}", b.rect, w.rect);
        assert_eq!((b.rect[2], b.rect[3]), (w.rect[2], w.rect[3]), "stessa misura di ↔");
        assert!(find(&rt, KEYS_POPOVER).is_none());

        tap_el(&mut rt, KEYS_BUTTON);
        let pop = find(&rt, KEYS_POPOVER).expect("aperto");
        assert!(pop.rect[1] >= b.rect[1] + b.rect[3], "sotto il pulsante");
        assert!(find(&rt, "keys-cap-0-0").and_then(|e| e.text).as_deref() == Some("⌘P"));

        rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
        settle(&mut rt);
        assert!(find(&rt, KEYS_POPOVER).is_none(), "Esc chiude");

        tap_el(&mut rt, KEYS_BUTTON);
        tap(&mut rt, 600.0, 700.0);
        assert!(find(&rt, KEYS_POPOVER).is_none(), "un clic fuori chiude");

        tap_el(&mut rt, KEYS_BUTTON);
        tap_el(&mut rt, KEYS_BUTTON);
        assert!(find(&rt, KEYS_POPOVER).is_none(), "il pulsante chiude");

        // Uno alla volta: ↔ chiude ⌘, ⌘ chiude ↔.
        // Il primo clic fuori dal popover aperto lo chiude (il fondo trasparente), il secondo apre l'altro.
        tap_el(&mut rt, KEYS_BUTTON);
        tap_el(&mut rt, WIDTH_BUTTON);
        tap_el(&mut rt, WIDTH_BUTTON);
        assert!(find(&rt, KEYS_POPOVER).is_none() && find(&rt, WIDTH_POPOVER).is_some());
        toggle(&mut rt.state);
        assert!(rt.state.keys_popover && !rt.state.width_popover);
        crate::ui::tabs::toggle_width(&mut rt.state);
        assert!(!rt.state.keys_popover && rt.state.width_popover);
    }
}
