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
//! Troppe schede per la barra: la riga scorre di lato. Un clic porta davanti, il clic centrale
//! chiude, Cmd+W chiude quella davanti (`main::tab_key`); con modifiche non salvate tutti e due
//! chiedono prima (`App::close`).

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

/// Una scheda: alta `ROW`, raggio `R_ROW`, `ROW_PAD` ai lati del nome. Davanti: `sel` e testo
/// `fg`; le altre: testo `fg2`, hover `sel`.
fn tab_row(s: &App, hooks: Hooks<App>, i: usize, tab: &crate::app::Tab, theme: &Theme, motion: Motion) -> El {
    let front = i == s.active;
    let key = format!("t-{:x}", tab.doc.id);
    let title = tab.title();
    let id = tab.doc.id;
    let label = text(title.clone()).key(format!("{key}-label")).font(sidebar::ui_font()).color(if front { theme.fg } else { theme.fg2 }).truncate_middle().shrink().min_w(0.0).a11y_hidden();
    // Al massimo `TAB_MAX_W`, il nome tagliato in mezzo (D5): una scheda lunga non spinge fuori
    // le altre.
    let mut e = row()
        .max_w(TAB_MAX_W)
        .h(ROW)
        .px(ROW_PAD)
        .center_y()
        .role("tab")
        .label(title)
        .focus_selected(front)
        .child(label)
        .on_press_tx(move |s: &mut App, tx: &mut Tx| {
            if let Some(i) = s.tabs.iter().position(|t| t.doc.id == id)
                && s.select(i)
            {
                s.restore_view(tx);
            }
        })
        .on_middle_press(move |s: &mut App, tx: &mut Tx| {
            let Some(i) = s.tabs.iter().position(|t| t.doc.id == id) else { return };
            // Chiude, o con modifiche non salvate la porta davanti con la domanda (`App::close`).
            let before = s.tab().map(|t| t.doc.id);
            s.close(i);
            if s.tab().map(|t| t.doc.id) != before {
                s.restore_view(tx);
            }
        });
    if front {
        e = e.fill(theme.sel);
    }
    let e = sidebar::pressable(e, key.clone(), &s.sidebar, hooks, theme, motion, !front).key(key);
    // L'ospite dell'anello: sempre presente, così la struttura non cambia quando si accende.
    let host = el().radius(R_ROW).child(e);
    match s.ring_of(tab.path()) {
        Some(since) => fx::ring::slot(host, Mark::TabRing(i as u32), since, motion.reduced),
        None => host,
    }
}
