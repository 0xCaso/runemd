//! Token di `DESIGN.md`: colori OKLCH per tema, misure e raggi.

use rune::{Color, oklcha};

pub const FRAME: f32 = 8.0;
pub const SIDEBAR: f32 = 248.0;
pub const COLUMN: f32 = 680.0;

/// La colonna del testo larga al massimo `column` (impostazione "Larghezza del testo",
/// `settings::COLUMNS`); con "Intera" segue il pannello, e i margini delle righe restano.
pub fn column(e: rune::El, column: f32) -> rune::El {
    match crate::settings::max_width(column) {
        Some(w) => e.max_w(w),
        None => e,
    }
}
pub const ROW: f32 = 28.0;
pub const TREE_ROW: f32 = 26.0;
pub const TREE_INDENT: f32 = 16.0;
/// Testo UI (DESIGN.md, `typography.ui`): 13 pt, peso 400.
pub const UI_TEXT: f32 = 13.0;
/// UI piccola (DESIGN.md, `typography.ui-small`): 12 pt, peso 500. Tempi, sezioni, branch.
pub const UI_SMALL: f32 = 12.0;
/// Pesi della UI (DESIGN.md, "Typography"): righe 400, nome del repo 600, UI piccola 500, tasto 400.
pub const UI_WEIGHT: f32 = 400.0;
pub const UI_STRONG_WEIGHT: f32 = 600.0;
pub const UI_SMALL_WEIGHT: f32 = 500.0;
pub const KBD_WEIGHT: f32 = 400.0;
/// Barra delle schede (DESIGN.md, `spacing.tab-bar`, `tab-bar-pad`, `tab-gap`): alta 44, margine
/// 10 ai lati, 2 fra una scheda e l'altra. Le schede sono alte `ROW`, raggio `R_ROW`, con
/// `ROW_PAD` ai lati del nome.
pub const TAB_BAR_H: f32 = 44.0;
pub const TAB_BAR_PAD_X: f32 = 10.0;
pub const TAB_GAP: f32 = 2.0;
/// Cerca nel file (DESIGN.md, `spacing.find-field`): il campo nella barra delle schede è largo 240.
pub const FIND_W: f32 = 240.0;
/// L'anello interno dei risultati di Cerca, 1 pt sopra il riempimento `sel`, in `mark`
/// (DESIGN.md, "Cerca nel file", `find-ring`).
pub const FIND_RING: f32 = 1.0;
/// L'anello del risultato attivo: in `fg2` e spesso il doppio (`find-ring-active`), così si
/// riconosce a colpo d'occhio in tutti e due i temi anche ora che `mark` è a 3:1.
pub const FIND_RING_ACTIVE: f32 = 2.0;
/// L'etichetta del fuoco sta a questa distanza dal fondo del pannello.
pub const FIRE_LABEL_BOTTOM: f32 = 10.0;
/// Corpo del documento (DESIGN.md, `typography.doc-body`): 15,5 pt, interlinea 1,62.
pub const BODY_TEXT: f32 = 15.5;
pub const BODY_LEADING: f32 = 1.62;
/// Codice e sorgente (DESIGN.md, `typography.doc-code`): mono 13,5 pt.
pub const CODE_TEXT: f32 = 13.5;
/// Margine orizzontale delle righe del documento: il testo parte da qui.
pub const ROW_PAD_X: f32 = 28.0;
/// Margine verticale delle righe del documento.
pub const ROW_PAD_Y: f32 = 7.0;
/// Segno "cosa è cambiato": largo 2 pt, in `mark`, nel margine sinistro della riga.
pub const CHANGED_MARK_W: f32 = 2.0;
/// Il segno sta a questa distanza dal bordo sinistro della riga (a metà del margine).
pub const CHANGED_MARK_X: f32 = (ROW_PAD_X - CHANGED_MARK_W) / 2.0;

/// Indice dei titoli "In questo file" (DESIGN.md, `spacing.outline*`, mockup `scheda`): colonna
/// larga 196 compresa la linea, testo a 16 dalla linea (10 di margine più 6 dentro la riga, che
/// così ha spazio per l'hover), righe alte 25 (12,5 pt con interlinea 2), 10 di rientro per
/// livello, 4 fra l'etichetta e il primo titolo.
pub const OUTLINE_W: f32 = 196.0;
pub const OUTLINE_PAD_X: f32 = 10.0;
pub const OUTLINE_ROW_PAD: f32 = 6.0;
pub const OUTLINE_ROW: f32 = 25.0;
pub const OUTLINE_TEXT: f32 = 12.5;
pub const OUTLINE_INDENT: f32 = 10.0;
pub const OUTLINE_LABEL_GAP: f32 = 4.0;
/// Sotto questa larghezza della finestra l'indice si nasconde: sidebar + colonna + 200.
pub const OUTLINE_MIN_WINDOW: f32 = SIDEBAR + COLUMN + 200.0;

/// La soglia dell'indice con sidebar e indice larghi `sidebar` e `outline` (trascinati,
/// `panes`): si sposta di quanto sono più larghi o più stretti del default.
pub fn outline_min_window(sidebar: f32, outline: f32) -> f32 {
    OUTLINE_MIN_WINDOW + (sidebar - SIDEBAR) + (outline - OUTLINE_W)
}
/// Senza indice, la sidebar si allarga finché il pannello resta largo almeno questo.
pub const DOC_MIN: f32 = 400.0;
/// La fascia che si trascina sul bordo della sidebar e dell'indice, nel loro margine (8 e 10): cursore ↔.
pub const EDGE_W: f32 = 8.0;

/// La scheda più larga (`tab-max`): un nome più lungo si taglia in mezzo.
pub const TAB_MAX_W: f32 = 240.0;
/// Sidebar (DESIGN.md, `spacing`): spazio sopra il campo per i semafori, margini ai lati e in fondo.
pub const SIDEBAR_TOP: f32 = 44.0;
pub const SIDEBAR_PAD: f32 = 8.0;
pub const SIDEBAR_BOTTOM: f32 = 10.0;
/// Campo "Apri file" e riga del repo (`spacing.field`); margine del campo (`field-margin`).
pub const FIELD_H: f32 = 30.0;
pub const FIELD_MARGIN_X: f32 = 2.0;
pub const FIELD_MARGIN_BOTTOM: f32 = 6.0;
/// Margine orizzontale dentro le righe della sidebar (`row-pad`) e dell'albero (`tree-row-pad`).
pub const ROW_PAD: f32 = 10.0;
pub const TREE_ROW_PAD: f32 = 8.0;
/// Spazio fra icona, nome e tempo (`row-gap`), e fra nome e repo di un nome doppio (`qualifier-gap`).
pub const ROW_GAP: f32 = 8.0;
/// Icone di campo e repo (`icon`), freccia delle cartelle (`chevron`).
pub const ICON: f32 = 14.0;
pub const CHEVRON: f32 = 10.0;
/// Etichette di sezione: 14 sopra, 6 sotto, `ROW_PAD` ai lati (`section-pad`).
pub const SECTION_PAD_TOP: f32 = 14.0;
pub const SECTION_PAD_BOTTOM: f32 = 6.0;
/// Il tasto "⌘P": testo `typography.kbd` (11, 400) e margine interno `kbd-pad`.
pub const KBD_TEXT: f32 = 11.0;
pub const KBD_PAD_X: f32 = 5.0;
pub const KBD_PAD_Y: f32 = 1.0;
/// Le linee sono di 0,5 pt (`hairline`, DESIGN.md "Shapes").
pub const HAIRLINE: f32 = 0.5;
/// Riga premuta: scala 0,98 in 160 ms ease-out; hover che entra ed esce in 120 ms (`motion`).
pub const PRESS_SCALE: f32 = 0.98;
pub const PRESS_TIME: f32 = 0.16;
pub const HOVER_FADE: f32 = 0.12;
/// `motion.ease-out`: cubic-bezier(0.23, 1, 0.32, 1).
pub const EASE_OUT: rune::Easing = rune::Easing::cubic(0.23, 1.0, 0.32, 1.0);

/// Lettura ↔ sorgente (Cmd+E): dissolvenza incrociata con molle criticamente smorzate (Rune anima
/// entrate e uscite solo con molle). Entra in 0,22 s di durata percepita (95 % a ~166 ms; la
/// velocità ha il picco a 35 ms e poi cala, una ease-out forte); esce in 0,17 s (sotto il 5 % a
/// ~128 ms). Prima era la dissolvenza di default di Rune, 0,30 / 0,24 s (95 % a ~227 ms).
pub const MODE_ENTER: f32 = 0.22;
pub const MODE_EXIT: f32 = 0.17;

/// La transizione della vista di lettura e di quella del sorgente; con "riduci movimento" è un
/// taglio netto.
pub fn mode_transition(reduced: bool) -> rune::Transition {
    let springs = if reduced { rune::SpringPair::NONE } else { rune::SpringPair::new(rune::Spring::new(MODE_ENTER, 0.0), rune::Spring::new(MODE_EXIT, 0.0)) };
    rune::Transition::opacity().springs(springs)
}

pub const R_KBD: f32 = 4.0;
pub const R_ROW: f32 = 6.0;
pub const R_FIELD: f32 = 7.0;
pub const R_PANEL: f32 = 10.0;

/// `fg` composto su `bg` (opaco) in sRGB codificato, come lo compone la finestra (`srgb_blend`).
/// Serve dove rune-kit vuole un colore opaco (riempimenti dei diagrammi) e per misurare il contrasto.
pub fn flatten(fg: Color, bg: Color) -> Color {
    fn encode(c: f32) -> f32 {
        if c <= 0.003_130_8 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
    }
    fn decode(c: f32) -> f32 {
        if c <= 0.040_45 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    }
    let mix = |f: f32, b: f32| decode(encode(f) * fg.a + encode(b) * (1.0 - fg.a));
    Color { r: mix(fg.r, bg.r), g: mix(fg.g, bg.g), b: mix(fg.b, bg.b), a: 1.0 }
}

/// Il colore del fuoco della tastiera (`Cx::focus_ring`), dal colore del sistema `system`
/// (`Cx::focus_color`, che AppKit dà al 50 %), pieno in tutti e due i temi: al 50 % sul vetro
/// resta sotto 3:1 (DESIGN.md, "Contrasto"). Nel tema chiaro è anche scurito del `FOCUS_DARKEN`
/// verso il nero (in luce lineare): sul vetro vero al 60 % (scrivania nera, fondo `999a9c`) il blu
/// scurito del 20 % dava 1,70:1. Nel tema scuro è
/// schiarito del `FOCUS_LIGHTEN` verso il bianco: sul vetro vero al 60 % (scrivania bianca) il blu
/// pieno dava 2,94:1.
pub fn focus_ring(system: Color, dark: bool) -> Color {
    let solid = system.alpha(1.0);
    if dark { solid.mix(Color::WHITE, FOCUS_LIGHTEN) } else { solid.mix(Color::BLACK, FOCUS_DARKEN) }
}

/// Quanto si schiarisce il fuoco della tastiera nel tema scuro.
pub const FOCUS_LIGHTEN: f32 = 0.1;

/// Quanto si scurisce il fuoco della tastiera nel tema chiaro.
pub const FOCUS_DARKEN: f32 = 0.68;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub dark: bool,
    pub panel: Color,
    pub fg: Color,
    pub body: Color,
    pub fg2: Color,
    pub fg3: Color,
    pub glass: Color,
    pub line: Color,
    pub sel: Color,
    pub field: Color,
    pub mark: Color,
}

impl Theme {
    /// Il blocco del frontmatter YAML: testo `fg2` su `sel` a metà (più quieto del codice).
    pub fn meta(&self) -> crate::ui::doc_view::Meta {
        crate::ui::doc_view::Meta { text: self.fg2, fill: self.sel.alpha(self.sel.a * 0.5) }
    }

    /// I colori del testo sul vetro (sidebar): `fg2` e `fg3` più forti che sul pannello, perché
    /// con la tinta al 60 % il vetro vero lascia passare la scrivania. Valgono ≥ 4,5:1 sui fondi
    /// misurati (`contrast::GLASS_GROUNDS`), anche sotto `sel`; `fg` > `fg2` > `fg3` resta. Il
    /// pannello del documento tiene i suoi.
    pub fn on_glass(&self) -> Theme {
        if self.dark {
            Theme { fg2: oklcha(0.90, 0.006, 270.0, 1.0), fg3: oklcha(0.86, 0.008, 270.0, 1.0), ..*self }
        } else {
            Theme { fg2: oklcha(0.25, 0.008, 270.0, 1.0), fg3: oklcha(0.28, 0.008, 270.0, 1.0), ..*self }
        }
    }

    /// Il riempimento della finestra quando il vetro di sistema non c'è (headless, materiale
    /// spento): la tinta del vetro piena, così non si vede la scrivania senza sfocatura.
    pub fn glass_fallback(&self) -> Color {
        self.glass.alpha(1.0)
    }

    pub fn for_mode(dark: bool) -> Theme {
        if dark {
            Theme {
                dark,
                panel: oklcha(0.155, 0.004, 270.0, 1.0),
                fg: oklcha(0.95, 0.003, 270.0, 1.0),
                body: oklcha(0.90, 0.004, 270.0, 1.0),
                fg2: oklcha(0.82, 0.006, 270.0, 1.0),
                fg3: oklcha(0.72, 0.008, 270.0, 1.0),
                glass: oklcha(0.18, 0.01, 260.0, 0.60),
                line: oklcha(1.0, 0.0, 0.0, 0.14),
                sel: oklcha(1.0, 0.0, 0.0, 0.12),
                field: oklcha(0.0, 0.0, 0.0, 0.22),
                mark: oklcha(0.72, 0.008, 270.0, 0.65),
            }
        } else {
            Theme {
                dark,
                panel: oklcha(0.995, 0.001, 270.0, 1.0),
                fg: oklcha(0.22, 0.006, 270.0, 1.0),
                body: oklcha(0.28, 0.006, 270.0, 1.0),
                fg2: oklcha(0.36, 0.008, 270.0, 1.0),
                fg3: oklcha(0.45, 0.008, 270.0, 1.0),
                glass: oklcha(0.965, 0.004, 260.0, 0.60),
                line: oklcha(0.0, 0.0, 0.0, 0.09),
                sel: oklcha(0.0, 0.0, 0.0, 0.06),
                field: oklcha(1.0, 0.0, 0.0, 0.55),
                mark: oklcha(0.45, 0.008, 270.0, 0.70),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_match_design_md() {
        assert_eq!((FIND_RING, FIND_RING_ACTIVE), (1.0, 2.0), "DESIGN.md, spacing.find-ring e find-ring-active");
        assert_eq!(TAB_MAX_W, 240.0, "DESIGN.md, spacing.tab-max");
        assert_eq!((crate::ui::sidebar::RECENT_GAP, crate::ui::sidebar::RECENT_SHARE), (3.0, 0.4), "DESIGN.md, spacing.recent-gap e recent-share");
        assert_eq!(crate::ui::sidebar::RECENT_ROW, 44.0, "DESIGN.md, spacing.recent-row");
        let dark = Theme::for_mode(true);
        assert_eq!(dark.fg2, oklcha(0.82, 0.006, 270.0, 1.0));
        assert_eq!(dark.fg3, oklcha(0.72, 0.008, 270.0, 1.0));
        assert_eq!(dark.line, oklcha(1.0, 0.0, 0.0, 0.14));
        assert_eq!(dark.sel, oklcha(1.0, 0.0, 0.0, 0.12));
        assert_eq!(dark.mark, oklcha(0.72, 0.008, 270.0, 0.65));
        assert_eq!(Theme::for_mode(false).mark, oklcha(0.45, 0.008, 270.0, 0.70));
        assert_eq!(Theme::for_mode(false).glass, oklcha(0.965, 0.004, 260.0, 0.60));
        assert_eq!(dark.glass, oklcha(0.18, 0.01, 260.0, 0.60));
    }
}
