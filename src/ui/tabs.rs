//! La barra delle schede in cima al pannello (DESIGN.md, "Scheda"; mockup `scheda`): i nomi dei
//! file aperti, ogni scheda larga quanto il suo nome con `ROW_PAD` ai lati (al massimo 240, il nome
//! tagliato in mezzo), quella davanti con
//! riempimento `sel`, l'anello "sta scrivendo" attorno alla scheda del file, "⌘E sorgente" a
//! destra.
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

use rune::{El, TextStyle, Tx, el, row, text};

use super::sidebar::{self, Hooks, Motion};
use super::theme::{HAIRLINE, KBD_PAD_X, KBD_PAD_Y, KBD_TEXT, KBD_WEIGHT, R_KBD, R_ROW, ROW, ROW_GAP, ROW_PAD, TAB_BAR_H, TAB_BAR_PAD_X, TAB_GAP, TAB_MAX_W, Theme};
use crate::app::App;
use crate::fx::ring::RING_W;
use crate::fx::{self, Mark};

/// Il testo del tasto a destra (approvato, DESIGN.md "Layout").
pub const SOURCE_KEY: &str = "⌘E sorgente";

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
    let bar = bar.child(key);
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
