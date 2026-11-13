//! Stile Markdown di RuneMD: la scala tipografica di `DESIGN.md` sui token del tema.

use rune::{Family, TextStyle};
use rune_kit::MarkdownStyle;

use super::theme::Theme;

/// Il corpo usa lo slot sans di `AppSpec::families` ("Inter"); `body_family` resta per chi cambia
/// famiglia in futuro, oggi non serve a scegliere un font diverso.
pub fn style(theme: &Theme, _body_family: &str, body_size: f32) -> MarkdownStyle {
    let body = TextStyle::new(body_size, 400.0).lh(body_size * 1.62);
    let heading = |size: f32, weight: f32, lh: f32, tracking_em: f32| TextStyle::new(size, weight).lh(size * lh).tracking(size * tracking_em);
    MarkdownStyle::neutral(theme.dark)
        .text(theme.fg)
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
        .table_rules(theme.line)
        .rule(theme.line)
        .selection(theme.sel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_builds_for_both_modes() {
        use std::hash::{DefaultHasher, Hash, Hasher};
        let h = |dark: bool| {
            let mut s = DefaultHasher::new();
            style(&Theme::for_mode(dark), "Inter", 15.5).hash(&mut s);
            s.finish()
        };
        assert_ne!(h(true), h(false));
    }
}
