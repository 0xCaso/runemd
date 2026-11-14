//! Stile Markdown di RuneMD: la scala tipografica di `DESIGN.md` sui token del tema.

use rune::{Color, Family, TextStyle};
use rune_kit::MarkdownStyle;

use super::theme::{COLUMN, FIND_RING, Theme};

/// La famiglia viene da `Family::Sans`, cioè dallo slot sans di `AppSpec::families` ("Inter").
/// `MarkdownStyle` ha un solo colore di testo: corpo, titoli e grassetto usano `theme.body`.
pub fn style(theme: &Theme, body_size: f32) -> MarkdownStyle {
    let body = TextStyle::new(body_size, 400.0).lh(body_size * 1.62);
    let heading = |size: f32, weight: f32, lh: f32, tracking_em: f32| TextStyle::new(size, weight).lh(size * lh).tracking(size * tracking_em);
    MarkdownStyle::neutral(theme.dark)
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
        // `sel` e un anello interno in `line`, che li stacca dal codice e dalla selezione; quello
        // attivo ha l'anello in `fg3`.
        .highlight_bg(theme.sel)
        .highlight_fg(None)
        .highlight_ring(Some((theme.line, FIND_RING)))
        .highlight_current_bg(theme.sel)
        .highlight_current_fg(None)
        .highlight_current_ring(Some((theme.fg3, FIND_RING)))
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

    /// Cerca senza accento: i risultati in `sel` con un anello in `line` (si distinguono dal
    /// codice e dalla selezione), l'attivo in `sel` con l'anello in `fg3`.
    #[test]
    fn find_highlights_use_theme_tokens() {
        for dark in [true, false] {
            let t = Theme::for_mode(dark);
            let m = style(&t, 15.5);
            assert_eq!(h(&m), h(&m.clone().highlight_bg(t.sel).highlight_ring(Some((t.line, FIND_RING))).highlight_current_bg(t.sel).highlight_current_ring(Some((t.fg3, FIND_RING)))));
            assert_ne!(h(&m), h(&m.clone().highlight_current_ring(None)));
            assert_ne!(h(&m), h(&m.clone().highlight_ring(None)), "i risultati hanno un anello");
        }
    }
}
