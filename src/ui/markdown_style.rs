//! RuneMD's Markdown style: the `DESIGN.md` type scale on the theme tokens.

use rune::{Color, Family, TextStyle};
use rune_kit::{Kit, MarkdownStyle};

use super::theme::{BODY_LEADING, BODY_TEXT, CODE_TEXT, COLUMN, FIND_RING, FIND_RING_ACTIVE, ICON, R_KBD, ROW_PAD_X, Theme, flatten};
use crate::fonts::Typeface;

/// Mermaid diagram colors (DESIGN.md, "Contrast"): all theme tokens. The diagram sits on the
/// panel without a card; nodes and notes have the `sel` fill already composited on the panel (opaque,
/// as rune-kit wants); borders, lines and arrows are `fg3`, text `fg`, muted text `fg2`.
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

/// The style in Inter (the sans slot of `AppSpec::families`) with the body at `body_size` (tests).
#[cfg(test)]
pub fn style(theme: &Theme, body_size: f32) -> MarkdownStyle {
    style_with(theme, Typeface { family: Family::Sans, size: body_size })
}

/// Document text fonts: body, headings 1–6 and cells.
#[derive(Clone, Copy, Debug)]
pub struct Fonts {
    pub body: TextStyle,
    pub headings: [TextStyle; 6],
    pub table: TextStyle,
}

/// Document fonts with the family and size of `face` (also used to measure long
/// words, `doc::breaks`).
pub fn fonts(face: Typeface) -> Fonts {
    let k = face.size / BODY_TEXT;
    let family = face.family;
    let heading = |size: f32, weight: f32, lh: f32, tracking_em: f32| {
        let size = size * k;
        TextStyle::new(size, weight).lh(size * lh).tracking(size * tracking_em).family(family)
    };
    Fonts {
        body: TextStyle::new(face.size, 400.0).lh(face.size * BODY_LEADING).family(family),
        headings: [
            heading(28.0, 680.0, 1.2, -0.015),
            heading(19.0, 640.0, 1.3, -0.005),
            heading(17.0, 640.0, 1.3, 0.0),
            heading(15.5, 600.0, 1.3, 0.0),
            heading(14.0, 600.0, 1.3, 0.0),
            heading(13.0, 500.0, 1.3, 0.0),
        ],
        table: TextStyle::new(14.0 * k, 400.0).family(family),
    }
}

/// The style with the family and size from the settings: body, headings and tables in the
/// chosen family; code stays monospaced. Headings, tables and code scale with the body
/// (DESIGN.md, "Typography": headings 28, 19, 17, 15.5, 14, 13; tables 14, code 13.5 with the body at 15.5).
/// `MarkdownStyle` has a single text color: body, headings and bold use `theme.body`.
pub fn style_with(theme: &Theme, face: Typeface) -> MarkdownStyle {
    let k = face.size / BODY_TEXT;
    let Fonts { body, headings: [h1, h2, h3, h4, ..], table } = fonts(face);
    // Diagram text has no setter on `MarkdownStyle`: it comes from the kit. Start from the neutral
    // kit with the theme's text and muted text, and put back what depends on them and must not
    // change (inline code fill, diff text) to the neutral kit's values.
    let neutral = Kit::neutral(theme.dark);
    let d = diagram_tokens(theme);
    let mut kit = neutral.clone();
    kit.c.text_base = d.text;
    kit.c.text_muted = d.muted;
    MarkdownStyle::from_kit(&kit)
        .inline_code_bg(neutral.alpha(neutral.c.text_base, 0.08))
        // No border of the kit's own on code cards: the view frames them with the table's quiet
        // edge (`doc_view::framed`, 1 pt in `line`; review R2, record 2026-11-23-t3-paths-review).
        .code_block(neutral.c.layer_01, Color::TRANSPARENT)
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
        // A globe or GitHub's mark before web links, 1 em, like the path icons (mockup `colori`).
        .link_icons(body.size, body.size * super::doc_view::PATH_ICON_GAP)
        .text_width(COLUMN)
        .muted(theme.fg3)
        .body_font(body)
        .heading_font(1, h1)
        .heading_font(2, h2)
        // The kit folds headings from 4 down into level 4, in `muted`: 5 and 6 (and the color
        // of 4) are set by the view for their block (`doc_view::heading_style`).
        .heading_font(3, h3)
        .heading_font(4, h4)
        .code_block_font(TextStyle::new(CODE_TEXT * k, 400.0).family(Family::Mono))
        .inline_code_font(CODE_TEXT / BODY_TEXT, 400.0)
        .strong_weight(620.0)
        .table_font(table)
        .table_tabular_figures(true)
        .table_header_text(theme.fg2)
        .table_header_weight(560.0)
        .table_header_fill(Color::TRANSPARENT)
        .table_outline(None)
        // The table fills its box: columns share out the free width in proportion to their content
        // (review R1, record 2026-11-23-t3-paths-review), and shrink and wrap when it is too narrow.
        .table_full_width(true)
        // The box (`doc_view::framed`) closes the table: no rule of its own under the last row.
        .table_open_end(true)
        .table_rules(theme.line)
        // Items that wrap stand 12 apart, one-line items 4 (feedback 2026-11-24, G6). Measured
        // against the default text column inside the row padding, like `diagram_max_width`.
        .list(LIST_INDENT, LIST_GAP)
        .list_wrap(LIST_WRAP_GAP, COLUMN - 2.0 * ROW_PAD_X)
        .rule(theme.line)
        // Task list checkboxes like the Settings one (DESIGN.md, "Contrast"):
        // off is empty with a 1 border in `fg3`, on is filled `fg2` with the check in `panel`.
        .task_checkbox(ICON, R_KBD, TASK_LEFT, None, theme.fg2, theme.panel, Color::TRANSPARENT, theme.fg3)
        .selection(theme.sel)
        // Find (DESIGN.md, "Find in file"): no accent. Matches have the `sel` fill and
        // a 1 pt inner ring in `mark` (≥ 3:1), which sets them apart from code and the
        // selection; the active one has a 2 pt ring in `fg2`, darker and thicker.
        .highlight_bg(theme.sel)
        .highlight_fg(None)
        .highlight_ring(Some((theme.mark, FIND_RING)))
        .highlight_current_bg(theme.sel)
        .highlight_current_fg(None)
        .highlight_current_ring(Some((theme.fg2, FIND_RING_ACTIVE)))
}

/// Bullet indent and the gap between items: one-line items, and items that wrap (G6).
const LIST_INDENT: f32 = 32.0;
const LIST_GAP: f32 = 4.0;
const LIST_WRAP_GAP: f32 = 12.0;

/// A task list checkbox starts 8 pt inside the gutter column (like the kit).
const TASK_LEFT: f32 = 8.0;

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

    /// `MarkdownStyle` fields are private: this checks that reapplying the same value does not change
    /// the hash (the field already has that value) and that applying another one changes it.
    #[test]
    fn text_is_body_and_links_have_no_accent() {
        for dark in [true, false] {
            let t = Theme::for_mode(dark);
            let m = style(&t, 15.5);
            assert_eq!(h(&m), h(&m.clone().text(t.body)), "text = body");
            assert_ne!(h(&m), h(&m.clone().text(t.fg)));
            assert_eq!(h(&m), h(&m.clone().link(t.fg).link_underline(t.fg3)), "link fg, underline fg3");
            assert_ne!(h(&m), h(&m.clone().link_underline(t.fg)));
        }
    }

    /// Review R2: code cards draw no border of their own (the kit's `border_muted`), so the only
    /// edge is the quiet one the view gives tables too.
    #[test]
    fn code_cards_have_no_border_of_their_own() {
        for dark in [true, false] {
            let m = style(&Theme::for_mode(dark), 15.5);
            let fill = Kit::neutral(dark).c.layer_01;
            assert_eq!(h(&m), h(&m.clone().code_block(fill, Color::TRANSPARENT)), "dark {dark}");
            assert_ne!(h(&m), h(&m.clone().code_block(fill, Kit::neutral(dark).c.border_muted)));
        }
    }

    /// Review R1: tables fill their box and leave its bottom edge to the box.
    #[test]
    fn tables_fill_their_box_and_end_on_it() {
        let m = style(&Theme::for_mode(false), 15.5);
        assert_eq!(h(&m), h(&m.clone().table_full_width(true).table_open_end(true)));
        assert_ne!(h(&m), h(&m.clone().table_full_width(false)));
        assert_ne!(h(&m), h(&m.clone().table_open_end(false)));
    }

    /// Find without accent: matches in `sel` with a ring in `mark` (they stand apart from
    /// code and the selection), the active one in `sel` with the ring in `fg2`, twice as thick.
    #[test]
    fn find_highlights_use_theme_tokens() {
        for dark in [true, false] {
            let t = Theme::for_mode(dark);
            let m = style(&t, 15.5);
            assert_eq!(h(&m), h(&m.clone().highlight_bg(t.sel).highlight_ring(Some((t.mark, FIND_RING))).highlight_current_bg(t.sel).highlight_current_ring(Some((t.fg2, FIND_RING_ACTIVE)))));
            assert_ne!(h(&m), h(&m.clone().highlight_current_ring(Some((t.fg2, FIND_RING)))), "the active one is thicker");
            assert_ne!(h(&m), h(&m.clone().highlight_current_ring(None)));
            assert_ne!(h(&m), h(&m.clone().highlight_ring(None)), "matches have a ring");
        }
    }

    /// Diagram colors are the theme tokens, not the kit's: text ≥ 4.5:1 on the
    /// ground and on the node fill, marks (borders, lines, arrows) ≥ 3:1 on the ground.
    #[test]
    fn diagram_tokens_meet_the_contrast_floors() {
        use super::super::contrast::{over, ratio};
        for dark in [true, false] {
            let t = Theme::for_mode(dark);
            let d = diagram_tokens(&t);
            let name = if dark { "dark" } else { "light" };
            for (what, fg, bg, min) in [
                ("text on ground", d.text, d.surface, 4.5),
                ("text on node", d.text, d.node_fill, 4.5),
                ("muted text on ground", d.muted, d.surface, 4.5),
                ("text on note", d.text, d.note_fill, 4.5),
                ("text on subgraph", d.text, over(d.cluster_fill, d.surface), 4.5),
                ("node border", d.node_border, d.surface, 3.0),
                ("node border on fill", d.node_border, d.node_fill, 3.0),
                ("lines and arrows", d.line, d.surface, 3.0),
                ("note border", d.note_border, d.surface, 3.0),
            ] {
                let c = ratio(fg, bg);
                assert!(c >= min, "{what} ({name}): {c:.2} < {min}");
            }
            assert_eq!(d.surface, t.panel, "the diagram sits on the panel, without a card");
            assert_eq!(d.surface.a, 1.0, "opaque ground");
            assert_eq!(d.node_fill.a, 1.0, "opaque fill");
        }
    }

    /// The font and size from the settings change the style; Inter at 15.5 is the
    /// usual style.
    #[test]
    fn font_and_size_change_the_style() {
        let t = Theme::for_mode(true);
        let base = h(&style(&t, BODY_TEXT));
        assert_eq!(base, h(&style_with(&t, Typeface::of("Inter", 15.5))));
        assert_ne!(base, h(&style_with(&t, Typeface::of("New York", 15.5))));
        assert_ne!(base, h(&style_with(&t, Typeface::of("Inter", 17.0))));
    }

    /// The diagram takes its text from the theme: the style changes if the diagram text changes.
    #[test]
    fn diagram_style_follows_the_theme() {
        let dark = Theme::for_mode(true);
        let a = style(&dark, 15.5);
        let mut other = dark;
        other.fg = Theme::for_mode(false).fg;
        assert_ne!(h(&a), h(&style(&other, 15.5)), "diagram text comes from fg");
    }
}
