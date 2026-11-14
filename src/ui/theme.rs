//! Token di `DESIGN.md`: colori OKLCH per tema, misure e raggi.
#![allow(dead_code)] // i token li usano i task 9 e seguenti

use rune::{Color, oklcha};

pub const FRAME: f32 = 8.0;
pub const SIDEBAR: f32 = 248.0;
pub const COLUMN: f32 = 680.0;
pub const ROW: f32 = 28.0;
pub const TREE_ROW: f32 = 26.0;
pub const TREE_INDENT: f32 = 16.0;
/// Testo UI (DESIGN.md, `typography.ui`): 13 pt, peso 400.
pub const UI_TEXT: f32 = 13.0;
/// UI piccola (DESIGN.md, `typography.ui-small`): 12 pt, peso 500. Tempi, sezioni, branch.
pub const UI_SMALL: f32 = 12.0;
/// L'etichetta del fuoco sta a questa distanza dal fondo del pannello.
pub const FIRE_LABEL_BOTTOM: f32 = 10.0;
/// Corpo del documento (DESIGN.md, `typography.doc-body`): 15,5 pt, interlinea 1,62.
pub const BODY_TEXT: f32 = 15.5;
pub const BODY_LEADING: f32 = 1.62;
/// Una riga di testo del corpo, pt: l'altezza del dither "sta arrivando".
pub const BODY_LINE: f32 = BODY_TEXT * BODY_LEADING;
/// Margine orizzontale delle righe del documento: il testo parte da qui.
pub const ROW_PAD_X: f32 = 28.0;
/// Margine verticale delle righe del documento.
pub const ROW_PAD_Y: f32 = 7.0;
/// Segno "cosa è cambiato": largo 2 pt, in `line`, nel margine sinistro della riga.
pub const CHANGED_MARK_W: f32 = 2.0;
/// Il segno sta a questa distanza dal bordo sinistro della riga (a metà del margine).
pub const CHANGED_MARK_X: f32 = (ROW_PAD_X - CHANGED_MARK_W) / 2.0;

pub const R_KBD: f32 = 4.0;
pub const R_ROW: f32 = 6.0;
pub const R_FIELD: f32 = 7.0;
pub const R_PANEL: f32 = 10.0;

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
}

impl Theme {
    pub fn for_mode(dark: bool) -> Theme {
        if dark {
            Theme {
                dark,
                panel: oklcha(0.155, 0.004, 270.0, 1.0),
                fg: oklcha(0.95, 0.003, 270.0, 1.0),
                body: oklcha(0.90, 0.004, 270.0, 1.0),
                fg2: oklcha(0.80, 0.006, 270.0, 1.0),
                fg3: oklcha(0.69, 0.008, 270.0, 1.0),
                glass: oklcha(0.18, 0.01, 260.0, 0.84),
                line: oklcha(1.0, 0.0, 0.0, 0.08),
                sel: oklcha(1.0, 0.0, 0.0, 0.09),
                field: oklcha(0.0, 0.0, 0.0, 0.22),
            }
        } else {
            Theme {
                dark,
                panel: oklcha(0.995, 0.001, 270.0, 1.0),
                fg: oklcha(0.22, 0.006, 270.0, 1.0),
                body: oklcha(0.28, 0.006, 270.0, 1.0),
                fg2: oklcha(0.36, 0.008, 270.0, 1.0),
                fg3: oklcha(0.45, 0.008, 270.0, 1.0),
                glass: oklcha(0.965, 0.004, 260.0, 0.86),
                line: oklcha(0.0, 0.0, 0.0, 0.09),
                sel: oklcha(0.0, 0.0, 0.0, 0.06),
                field: oklcha(1.0, 0.0, 0.0, 0.55),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_match_design_md() {
        assert_eq!(Theme::for_mode(true).fg3, oklcha(0.69, 0.008, 270.0, 1.0));
        assert_eq!(Theme::for_mode(false).glass, oklcha(0.965, 0.004, 260.0, 0.86));
    }
}
