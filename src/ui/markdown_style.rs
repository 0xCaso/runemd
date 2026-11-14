//! Stile Markdown di RuneMD: la scala tipografica di `DESIGN.md` sui token del tema.

use rune::{Color, Family, TextStyle};
use rune_kit::{Kit, MarkdownStyle};

use super::theme::{COLUMN, FIND_RING, FIND_RING_ACTIVE, ROW_PAD_X, Theme, flatten};

/// I colori dei diagrammi Mermaid (DESIGN.md, "Contrasto"): tutti token del tema. Il diagramma sta sul
/// pannello senza card; nodi e note hanno il riempimento `sel` già composto sul pannello (opaco,
/// come vuole rune-kit); bordi, linee e frecce sono `fg3`, il testo `fg`, quello smorzato `fg2`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiagramTokens {
    pub text: Color,
    pub muted: Color,
    pub surface: Color,
    pub node_fill: Color,
    pub node_border: Color,
    pub line: Color,
    pub cluster_fill: Color,
    pub note_fill: Color,
    pub note_border: Color,
}

pub fn diagram_tokens(theme: &Theme) -> DiagramTokens {
    let on_panel = flatten(theme.sel, theme.panel);
    DiagramTokens {
        text: theme.fg,
        muted: theme.fg2,
        surface: theme.panel,
        node_fill: on_panel,
        node_border: theme.fg3,
        line: theme.fg3,
        cluster_fill: theme.sel,
        note_fill: on_panel,
        note_border: theme.fg3,
    }
}

/// La famiglia viene da `Family::Sans`, cioè dallo slot sans di `AppSpec::families` ("Inter").
/// `MarkdownStyle` ha un solo colore di testo: corpo, titoli e grassetto usano `theme.body`.
pub fn style(theme: &Theme, body_size: f32) -> MarkdownStyle {
    let body = TextStyle::new(body_size, 400.0).lh(body_size * 1.62);
    let heading = |size: f32, weight: f32, lh: f32, tracking_em: f32| TextStyle::new(size, weight).lh(size * lh).tracking(size * tracking_em);
    // Il testo dei diagrammi non ha un setter su `MarkdownStyle`: viene dal kit. Si parte dal kit
    // neutro con testo e testo smorzato del tema, e si rimette ciò che ne dipende e che non deve
    // cambiare (riempimento del codice in linea, testo dei diff) ai valori del kit neutro.
    let neutral = Kit::neutral(theme.dark);
    let d = diagram_tokens(theme);
    let mut kit = neutral.clone();
    kit.c.text_base = d.text;
    kit.c.text_muted = d.muted;
    MarkdownStyle::from_kit(&kit)
        .inline_code_bg(neutral.alpha(neutral.c.text_base, 0.08))
        .diff_text(neutral.c.text_base, neutral.c.text_base)
        .diff_hunk(neutral.c.text_muted)
        .diagram(d.node_fill, d.node_border, d.line)
        .diagram_surface(d.surface, Color::TRANSPARENT, d.cluster_fill)
        .diagram_note(d.note_fill, d.note_border)
        .diagram_hover(theme.fg, false)
        .diagram_max_width(COLUMN - 2.0 * ROW_PAD_X)
        .text(theme.body)
        .link(theme.fg)
        .link_underline(theme.fg3)
        .text_width(COLUMN)
        .muted(theme.fg3)
        .body_font(body)
        .heading_font(1, heading(28.0, 680.0, 1.2, -0.015))
        .heading_font(2, heading(19.0, 640.0, 1.3, -0.005))
        .heading_font(3, heading(15.5, 640.0, 1.3, 0.0))
        .code_block_font(TextStyle::new(13.5, 400.0).family(Family::Mono))
        .inline_code_font(13.5 / body_size, 400.0)
        .strong_weight(620.0)
        .table_font(TextStyle::new(14.0, 400.0))
        .table_tabular_figures(true)
        .table_header_text(theme.fg2)
        .table_header_weight(560.0)
        .table_header_fill(Color::TRANSPARENT)
        .table_outline(None)
        .table_rules(theme.line)
        .rule(theme.line)
        .selection(theme.sel)
        // Cerca (DESIGN.md, "Cerca nel file"): nessun accento. I risultati hanno il riempimento
        // `sel` e un anello interno di 1 pt in `mark` (≥ 3:1), che li stacca dal codice e dalla
        // selezione; quello attivo ha l'anello di 2 pt in `fg2`, più scuro e più spesso.
        .highlight_bg(theme.sel)
        .highlight_fg(None)
        .highlight_ring(Some((theme.mark, FIND_RING)))
        .highlight_current_bg(theme.sel)
        .highlight_current_fg(None)
        .highlight_current_ring(Some((theme.fg2, FIND_RING_ACTIVE)))
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::hash::{DefaultHasher, Hash, Hasher};

    fn h(m: &MarkdownStyle) -> u64 {
        let mut s = DefaultHasher::new();
        m.hash(&mut s);
        s.finish()
    }

    #[test]
    fn style_builds_for_both_modes() {
        assert_ne!(h(&style(&Theme::for_mode(true), 15.5)), h(&style(&Theme::for_mode(false), 15.5)));
    }

    /// I campi di `MarkdownStyle` sono privati: si prova che riapplicare lo stesso valore non cambia
    /// l'hash (il campo ha già quel valore) e che applicarne un altro lo cambia.
    #[test]
    fn text_is_body_and_links_have_no_accent() {
        for dark in [true, false] {
            let t = Theme::for_mode(dark);
            let m = style(&t, 15.5);
            assert_eq!(h(&m), h(&m.clone().text(t.body)), "testo = body");
            assert_ne!(h(&m), h(&m.clone().text(t.fg)));
            assert_eq!(h(&m), h(&m.clone().link(t.fg).link_underline(t.fg3)), "link fg, sottolineatura fg3");
            assert_ne!(h(&m), h(&m.clone().link_underline(t.fg)));
        }
    }

    /// Cerca senza accento: i risultati in `sel` con un anello in `mark` (si distinguono dal
    /// codice e dalla selezione), l'attivo in `sel` con l'anello in `fg2`, spesso il doppio.
    #[test]
    fn find_highlights_use_theme_tokens() {
        for dark in [true, false] {
            let t = Theme::for_mode(dark);
            let m = style(&t, 15.5);
            assert_eq!(h(&m), h(&m.clone().highlight_bg(t.sel).highlight_ring(Some((t.mark, FIND_RING))).highlight_current_bg(t.sel).highlight_current_ring(Some((t.fg2, FIND_RING_ACTIVE)))));
            assert_ne!(h(&m), h(&m.clone().highlight_current_ring(Some((t.fg2, FIND_RING)))), "l'attivo è più spesso");
            assert_ne!(h(&m), h(&m.clone().highlight_current_ring(None)));
            assert_ne!(h(&m), h(&m.clone().highlight_ring(None)), "i risultati hanno un anello");
        }
    }

    /// I colori dei diagrammi sono i token del tema, non quelli del kit: testo ≥ 4,5:1 sul
    /// fondo e sul riempimento dei nodi, segni (bordi, linee, frecce) ≥ 3:1 sul fondo.
    #[test]
    fn diagram_tokens_meet_the_contrast_floors() {
        use super::super::contrast::{over, ratio};
        for dark in [true, false] {
            let t = Theme::for_mode(dark);
            let d = diagram_tokens(&t);
            let name = if dark { "scuro" } else { "chiaro" };
            for (what, fg, bg, min) in [
                ("testo su fondo", d.text, d.surface, 4.5),
                ("testo su nodo", d.text, d.node_fill, 4.5),
                ("testo smorzato su fondo", d.muted, d.surface, 4.5),
                ("testo su nota", d.text, d.note_fill, 4.5),
                ("testo su sottogruppo", d.text, over(d.cluster_fill, d.surface), 4.5),
                ("bordo del nodo", d.node_border, d.surface, 3.0),
                ("bordo del nodo sul riempimento", d.node_border, d.node_fill, 3.0),
                ("linee e frecce", d.line, d.surface, 3.0),
                ("bordo della nota", d.note_border, d.surface, 3.0),
            ] {
                let c = ratio(fg, bg);
                assert!(c >= min, "{what} ({name}): {c:.2} < {min}");
            }
            assert_eq!(d.surface, t.panel, "il diagramma sta sul pannello, senza card");
            assert_eq!(d.surface.a, 1.0, "fondo opaco");
            assert_eq!(d.node_fill.a, 1.0, "riempimento opaco");
        }
    }

    /// Il diagramma prende il testo dal tema: lo stile cambia se cambia il testo del diagramma.
    #[test]
    fn diagram_style_follows_the_theme() {
        let dark = Theme::for_mode(true);
        let a = style(&dark, 15.5);
        let mut other = dark;
        other.fg = Theme::for_mode(false).fg;
        assert_ne!(h(&a), h(&style(&other, 15.5)), "il testo del diagramma viene da fg");
    }
}
