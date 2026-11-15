//! La barra delle schede in cima al pannello (DESIGN.md, "Scheda"; mockup `scheda`): i nomi dei
//! file aperti, ogni scheda larga quanto il suo nome con `ROW_PAD` ai lati (al massimo 240, il nome
//! tagliato in mezzo), quella davanti con
//! riempimento `sel`, l'anello "sta scrivendo" attorno alla scheda del file, "⌘E sorgente" a
//! destra e, alla sua sinistra, il pulsante ↔ della larghezza del testo: apre un popover con
//! il cursore delle impostazioni (stessa `settings.column`); ⌘⌥+ e ⌘⌥- fanno un passo.
//!
//! Le schede sono righe come quelle della sidebar (`sidebar::pressable`: hover `sel`, scala 0,98
//! premuta), non il `TabStrip` di rune-kit: il `TabStrip` dà a tutte le schede la stessa larghezza
//! (28–224 pt) e tiene 16 pt per un avatar prima del nome, e non si può cambiare né l'una né
//! l'altro dall'esterno. Il mockup scelto da Matteo ha schede larghe quanto il nome.
//!
//! La scheda di anteprima (clic singolo nella sidebar, `App::open_preview`) ha il nome in corsivo;
//! un doppio clic la fissa.
//!
//! Troppe schede per la barra: la riga scorre di lato. Un clic porta davanti; la × (sulla scheda
//! davanti, o in hover) e il clic centrale chiudono, Cmd+W chiude quella davanti
//! (`main::tab_key`); con modifiche non salvate chiedono prima (`App::close`).

use rune::{Align, Color, El, Side, TextStyle, Tx, col, el, row, text};

use super::sidebar::{self, Hooks, Motion};
use super::theme::{HAIRLINE, KBD_PAD_X, KBD_PAD_Y, KBD_TEXT, KBD_WEIGHT, R_KBD, R_PANEL, R_ROW, ROW, ROW_GAP, ROW_PAD, SIDEBAR, TAB_BAR_H, TAB_BAR_PAD_X, TAB_GAP, TAB_MAX_W, Theme, UI_SMALL, UI_WEIGHT};
use crate::app::App;
use crate::fx::ring::RING_W;
use crate::fx::{self, Mark};

/// Il testo del tasto a destra (approvato, DESIGN.md "Layout").
pub const SOURCE_KEY: &str = "⌘E sorgente";

/// Il pulsante ↔ della larghezza del testo e il suo popover.
pub const WIDTH_BUTTON: &str = "width-button";
pub const WIDTH_POPOVER: &str = "width-popover";
/// Il glifo del pulsante, pt.
const WIDTH_GLYPH: f32 = 15.0;
/// Fra il pulsante e il popover, pt.
const POPOVER_GAP: f32 = 4.0;
/// Sotto il cursore nel popover: le scorciatoie.
pub const WIDTH_HINT: &str = "⌘⌥+ e ⌘⌥− un passo alla volta";

/// ⌘⌥+ (o ⌘⌥=) e ⌘⌥-: un passo della larghezza del testo, +1 o −1. `base` è il carattere del
/// tasto senza modificatori (`Tx::key_base`): con ⌥ macOS può dare "≠" o "–".
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

/// Il pulsante ↔: 28 × 28, raggio `R_ROW`, glifo in `fg2`, hover `sel`; col popover aperto
/// ha il riempimento `sel`. Apre e chiude il popover.
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
        .on_press(|s: &mut App| s.width_popover = !s.width_popover);
    if s.width_popover {
        b = b.fill(theme.sel);
    } else if motion.hover {
        b = b.hover(theme.sel);
    }
    b
}

/// Il popover della larghezza, da montare alla radice dopo la barra: un fondo trasparente che
/// chiude al clic fuori e la superficie sotto il pulsante, allineata al suo bordo destro.
/// Superficie `panel`, raggio `R_PANEL`, bordo e ombra del pannello; dentro il titolo in `fg2`,
/// il cursore delle impostazioni e le scorciatoie in `fg3`. Vuoto se è chiuso.
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

/// La barra: alta `TAB_BAR_H`, con una linea in fondo che la separa dal documento. `hover`: il
/// puntatore è fine e può passare sopra.
pub fn view(s: &App, hooks: Hooks<App>, theme: &Theme, reduced: bool, hover: bool) -> El {
    let motion = Motion { reduced, hover };
    // `RING_W` attorno alla riga: l'anello sta fuori dalla scheda e la riga che scorre lo taglia.
    let list = row().key("tabs").role("tablist").label("Schede").gap(TAB_GAP).center_y().padding(RING_W)
        .children(s.tabs.iter().enumerate().map(|(i, tab)| tab_row(s, hooks, i, tab, theme, motion)));
    let strip = row().key("tabs-scroll").grow(1.0).shrink().min_w(0.0).scroll_x().child(list);
    let key = el().key("source-key").border(HAIRLINE, theme.line).radius(R_KBD).px(KBD_PAD_X).py(KBD_PAD_Y)
        .child(text(SOURCE_KEY).font(TextStyle::new(KBD_TEXT, KBD_WEIGHT)).color(theme.fg3));
    let mut bar = row().key("tab-bar").w_full().h(TAB_BAR_H).px(TAB_BAR_PAD_X - RING_W).gap(TAB_GAP).center_y().child(strip);
    // Cerca (Cmd+F): il campo sta nella barra, a destra delle schede, così il documento non si sposta.
    if s.find.open {
        bar = bar.child(super::find::field(s, theme).mr(ROW_GAP));
    }
    let bar = bar.child(width_button(s, theme, motion)).child(key);
    rune::col().w_full().child(bar).child(el().key("tab-bar-line").w_full().h(HAIRLINE).fill(theme.line))
}

/// L'area della × (16 pt), il suo margine dal bordo destro della scheda e dal nome.
pub const CLOSE: f32 = 16.0;
const CLOSE_PAD: f32 = 6.0;
const CLOSE_GAP: f32 = 4.0;
const R_CLOSE: f32 = 4.0;

/// Chiude la scheda `id` (clic centrale, ×): con modifiche non salvate la porta davanti con la
/// domanda, come Cmd+W (`App::close`).
fn close_tab(s: &mut App, id: u64, tx: &mut Tx) {
    let Some(i) = s.tabs.iter().position(|t| t.doc.id == id) else { return };
    let before = s.tab().map(|t| t.doc.id);
    s.close(i);
    if s.tab().map(|t| t.doc.id) != before {
        s.restore_view(tx);
    }
}

/// La × in fondo alla scheda: area di `CLOSE` pt, glifo in `fg2`, hover `sel`. Il suo posto c'è
/// sempre, così il nome non si sposta; sulla scheda davanti si vede sempre, sulle altre solo con
/// il puntatore (o il fuoco della tastiera) sulla scheda (`hover_group`).
fn close_button(key: &str, id: u64, title: &str, front: bool, theme: &Theme, motion: Motion) -> El {
    let glyph = text("×").key(format!("{key}-close-glyph")).font(TextStyle::new(CLOSE_GLYPH, KBD_WEIGHT)).color(theme.fg2).a11y_hidden();
    let mut b = el()
        .key(format!("{key}-close"))
        .size(CLOSE, CLOSE)
        .radius(R_CLOSE)
        .center()
        .role("button")
        .label(format!("Chiudi {title}"))
        .hover_alone()
        .child(glyph)
        .on_press_tx(move |s: &mut App, tx: &mut Tx| close_tab(s, id, tx));
    if motion.hover {
        b = b.hover(theme.sel);
    }
    // Sul bottone, non sul glifo: il gruppo più vicino del glifo sarebbe il bottone (ha un hover).
    if !front {
        b = b.group_opacity(0.0, 1.0);
    }
    b
}

/// Il glifo della ×, pt.
const CLOSE_GLYPH: f32 = 14.0;

/// Una scheda: alta `ROW`, raggio `R_ROW`, `ROW_PAD` ai lati del nome. Davanti: `sel` e testo
/// `fg`; le altre: testo `fg2`, hover `sel`.
fn tab_row(s: &App, hooks: Hooks<App>, i: usize, tab: &crate::app::Tab, theme: &Theme, motion: Motion) -> El {
    let front = i == s.active;
    let key = format!("t-{:x}", tab.doc.id);
    let title = tab.title();
    let id = tab.doc.id;
    // La scheda di anteprima ha il nome in corsivo (come VS Code).
    let font = if tab.preview { sidebar::ui_font().italic() } else { sidebar::ui_font() };
    let label = text(title.clone()).key(format!("{key}-label")).font(font).color(if front { theme.fg } else { theme.fg2 }).truncate_middle().shrink().min_w(0.0).a11y_hidden();
    let close = close_button(&key, id, &title, front, theme, motion);
    // Al massimo `TAB_MAX_W`, il nome tagliato in mezzo: una scheda lunga non spinge fuori
    // le altre.
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
    // L'ospite dell'anello: sempre presente, così la struttura non cambia quando si accende.
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
        // Con ⌥ macOS può dare un altro carattere ("≠", "–"): conta il tasto senza modificatori.
        assert_eq!(width_key(&cmd_alt('≠'), Some('=')), Some(1));
        assert_eq!(width_key(&cmd_alt('–'), Some('-')), Some(-1));
        // ⌘+ e ⌘- senza ⌥, o con Ctrl, non sono nostri.
        let plain = KeyEvent { key: Key::Command('='), mods: Mods { cmd: true, ..Mods::default() } };
        assert_eq!(width_key(&plain, None), None);
        let ctrl = KeyEvent { key: Key::Command('='), mods: Mods { cmd: true, alt: true, ctrl: true, ..Mods::default() } };
        assert_eq!(width_key(&ctrl, None), None);
        assert_eq!(width_key(&cmd_alt('e'), None), None);
    }

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

    fn tap(rt: &mut Runtime<App>, x: f32, y: f32) {
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        settle(rt);
    }

    fn value(rt: &Runtime<App>) -> String {
        find(rt, "width-slider-value").and_then(|e| e.text).expect("valore")
    }

    /// Il pulsante ↔ sta a sinistra di "⌘E sorgente" e apre il popover con il cursore della
    /// larghezza; Esc o un clic fuori lo chiudono. ⌘⌥+ e ⌘⌥- cambiano la stessa
    /// `settings.column` del cursore delle impostazioni, anche col popover chiuso.
    #[test]
    fn the_width_button_opens_a_popover_with_the_width_slider() {
        let mut rt = open();
        let button = find(&rt, WIDTH_BUTTON).expect("pulsante");
        let source = find(&rt, "source-key").unwrap();
        assert_eq!(button.role, "button");
        assert!(button.rect[0] + button.rect[2] <= source.rect[0], "a sinistra di ⌘E: {:?} {:?}", button.rect, source.rect);
        assert!(find(&rt, WIDTH_POPOVER).is_none());

        tap(&mut rt, button.rect[0] + button.rect[2] / 2.0, button.rect[1] + button.rect[3] / 2.0);
        let pop = find(&rt, WIDTH_POPOVER).expect("popover aperto");
        assert!(pop.rect[1] >= button.rect[1] + button.rect[3], "sotto il pulsante");
        let slider = find(&rt, "width-slider").expect("cursore");
        assert_eq!(slider.role, "slider");
        assert_eq!(value(&rt), "680 pt");

        // ⌘⌥+ due volte, ⌘⌥- una: 760 pt, e il popover lo mostra.
        rt.key(cmd_alt('='));
        rt.key(cmd_alt('='));
        rt.key(cmd_alt('-'));
        settle(&mut rt);
        assert_eq!(rt.state.settings.column, 760.0);
        assert_eq!(value(&rt), "760 pt");

        // Un clic in fondo al cursore: "Intera", come quello delle impostazioni.
        let slider = find(&rt, "width-slider").unwrap();
        tap(&mut rt, slider.rect[0] + slider.rect[2] - 1.0, slider.rect[1] + slider.rect[3] / 2.0);
        assert_eq!(rt.state.settings.column, COLUMN_FULL);
        assert_eq!(value(&rt), "Intera");

        rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
        settle(&mut rt);
        assert!(find(&rt, WIDTH_POPOVER).is_none(), "Esc chiude");

        // Col popover chiuso le scorciatoie funzionano lo stesso.
        rt.key(cmd_alt('-'));
        settle(&mut rt);
        assert_eq!(rt.state.settings.column, 1240.0);

        // Riaperto, un clic sul documento lo chiude senza cambiare la larghezza.
        let button = find(&rt, WIDTH_BUTTON).unwrap();
        tap(&mut rt, button.rect[0] + button.rect[2] / 2.0, button.rect[1] + button.rect[3] / 2.0);
        assert!(find(&rt, WIDTH_POPOVER).is_some());
        tap(&mut rt, 600.0, 700.0);
        assert!(find(&rt, WIDTH_POPOVER).is_none(), "un clic fuori chiude");
        assert_eq!(rt.state.settings.column, 1240.0);

        // Il pulsante di nuovo: apre e chiude.
        tap(&mut rt, button.rect[0] + button.rect[2] / 2.0, button.rect[1] + button.rect[3] / 2.0);
        tap(&mut rt, button.rect[0] + button.rect[2] / 2.0, button.rect[1] + button.rect[3] / 2.0);
        assert!(find(&rt, WIDTH_POPOVER).is_none());
    }
}
