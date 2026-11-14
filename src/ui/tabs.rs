//! La barra delle schede in cima al pannello (DESIGN.md, "Scheda"): i nomi dei file aperti con il
//! `TabStrip` di rune-kit, la scheda davanti con riempimento `sel`, l'anello "sta scrivendo"
//! attorno alla scheda del file (`TabItem::decorate`), "⌘E sorgente" a destra.
//!
//! Nessuno stato di rune-kit (puntini, spinner): `status(None)`. Il `Kit` della barra prende i
//! colori dai token di `theme`, non quelli di rune-kit.

use rune::{Color, El, TextStyle, Tx, el, row, text};
use rune_kit::Kit;
use rune_kit::shell::{TabItem, TabOrientation, TabStrip};

use super::theme::{HAIRLINE, KBD_PAD_X, KBD_PAD_Y, KBD_TEXT, KBD_WEIGHT, R_KBD, R_ROW, ROW, TAB_BAR_H, TAB_BAR_PAD_X, TAB_GAP, Theme, UI_TEXT, UI_WEIGHT};
use crate::app::App;
use crate::fx::{self, Mark};

/// Il testo del tasto a destra (approvato, DESIGN.md "Layout").
pub const SOURCE_KEY: &str = "⌘E sorgente";

/// La chiave della barra: `viewport_width` la legge per dividere lo spazio fra le schede.
const STRIP: &str = "tabs";

/// Il `Kit` della barra: rune-kit dipinge la scheda davanti in `pressed`, il passaggio del
/// puntatore in `hover`, il titolo in `text_base` (davanti) e `text_faint` (le altre), il fondo in
/// `bg_deep`, il separatore fra schede in `layer_02`/`layer_03`. Qui diventano `sel`, `sel`, `fg`,
/// `fg2`, `panel` e nessun separatore (il mockup non ne ha); il testo è `typography.ui` (13/400).
pub fn kit(theme: &Theme) -> Kit {
    let mut k = Kit::neutral(theme.dark);
    k.c.pressed = theme.sel;
    k.c.hover = theme.sel;
    k.c.text_base = theme.fg;
    k.c.text_faint = theme.fg2;
    k.c.text_muted = theme.fg3;
    k.c.icon_muted = theme.fg3;
    k.c.bg_deep = theme.panel;
    k.c.layer_02 = Color::TRANSPARENT;
    k.c.layer_03 = Color::TRANSPARENT;
    k.ty.ui_compact = TextStyle { size: UI_TEXT, weight: UI_WEIGHT, ..k.ty.ui_compact };
    k.ty.ui_medium = TextStyle { weight: UI_WEIGHT, ..k.ty.ui_medium };
    k
}

/// La barra: alta `TAB_BAR_H`, con una linea in fondo che la separa dal documento.
pub fn view(s: &App, width: Option<f32>, theme: &Theme, reduced: bool) -> El {
    let ids: Vec<String> = s.tabs.iter().map(|t| format!("{:x}", t.doc.id)).collect();
    let titles: Vec<String> = s.tabs.iter().map(|t| t.title()).collect();
    let items = s.tabs.iter().enumerate().map(|(i, tab)| {
        let item = TabItem::new(ids[i].as_str(), titles[i].as_str()).status(None);
        match s.ring_of(tab.path()) {
            Some(since) => item.decorate(move |e, _| fx::ring::slot(e.radius(R_ROW), Mark::TabRing(i as u32), since, reduced)),
            None => item,
        }
    });
    let active = s.tab().map(|_| ids[s.active].as_str());
    let strip = TabStrip::new(STRIP, TabOrientation::Horizontal)
        .tabs(items)
        .active(active)
        .viewport_width(width)
        .row_height(ROW)
        .gap(TAB_GAP)
        .fill(theme.panel)
        .label("Schede")
        .on_select(|s: &mut App, id: &str, tx: &mut Tx| {
            if let Some(i) = index_of(s, id)
                && s.select(i)
            {
                s.restore_view(tx);
            }
        })
        .on_close(|s: &mut App, id: &str, tx: &mut Tx| {
            let Some(i) = index_of(s, id) else { return };
            let front = i == s.active;
            s.close(i);
            if front {
                s.restore_view(tx);
            }
        })
        .build(&s.kit);
    let key = el().key("source-key").border(HAIRLINE, theme.line).radius(R_KBD).px(KBD_PAD_X).py(KBD_PAD_Y)
        .child(text(SOURCE_KEY).font(TextStyle::new(KBD_TEXT, KBD_WEIGHT)).color(theme.fg3));
    let bar = row().key("tab-bar").w_full().h(TAB_BAR_H).px(TAB_BAR_PAD_X).gap(TAB_GAP).center_y().child(strip).child(key);
    rune::col().w_full().child(bar).child(el().key("tab-bar-line").w_full().h(HAIRLINE).fill(theme.line))
}

/// La larghezza che la barra dà alle schede (`TabStrip::viewport_width`).
pub fn strip_width(cx: &rune::Cx<App>) -> Option<f32> {
    cx.layout_size(STRIP).map(|s| s[0])
}

fn index_of(s: &App, id: &str) -> Option<usize> {
    let id = u64::from_str_radix(id, 16).ok()?;
    s.tabs.iter().position(|t| t.doc.id == id)
}
