//! Per-character colours after an inline icon (vendor/rune/VENDOR.md). Rune colours a glyph by
//! the byte its quad carries (text highlights, link hover, Find); an inline icon is a parley
//! inline box with no text, so the glyph -> byte table must count its width or every glyph
//! after it takes a byte further right (`below.rs L24` drew `below` bright, `.rs L2` grey).

use std::sync::Arc;

use parley::fontique::Blob;
use parley::{FontContext, FontFamily, InlineBox, InlineBoxKind, LayoutContext, StyleProperty, VerticalAlign};
use rune::text::{GlyphQuad, Ink, TextSystem};

const INTER: &[u8] = include_bytes!("../vendor/rune/assets/Inter-Regular.ttf");

/// The bytes the glyph quads of `text` carry, with an inline box `width` px wide before byte `at`.
fn quad_bytes(text: &str, icon: Option<(usize, f32)>) -> Vec<u32> {
    let mut fcx = FontContext::new();
    fcx.collection.register_fonts(Blob::new(Arc::new(INTER)), None);
    let mut lcx: LayoutContext<Ink> = LayoutContext::new();
    let mut b = lcx.ranged_builder(&mut fcx, text, 1.0, true);
    b.push_default(StyleProperty::FontFamily(FontFamily::from("Inter")));
    b.push_default(StyleProperty::FontSize(16.0));
    b.push_default(StyleProperty::Brush(Ink(0)));
    if let Some((index, width)) = icon {
        b.push_inline_box(InlineBox {
            id: 1,
            kind: InlineBoxKind::InFlow,
            index,
            width,
            height: 0.0,
            baseline: None,
            vertical_align: VerticalAlign::BASELINE,
        });
    }
    let mut layout = b.build(text);
    layout.break_all_lines(None);
    let mut ts = TextSystem::new(&[INTER], ("Inter", "Inter"));
    let mut out: Vec<GlyphQuad> = Vec::new();
    ts.glyphs_for_layout(&layout, 1.0, [0.0, 0.0], true, &mut out);
    out.iter().map(|q| q.byte).collect()
}

/// The characters the quads' bytes point at: one per drawn glyph, spaces draw none.
fn chars_at(text: &str, bytes: &[u32]) -> String {
    bytes.iter().map(|&b| text[b as usize..].chars().next().unwrap_or('?')).collect()
}

#[test]
fn glyphs_after_an_inline_icon_carry_their_own_bytes() {
    let text = "see below.rs L24 and more";
    let drawn: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    assert_eq!(chars_at(text, &quad_bytes(text, None)), drawn, "no icon");
    // The path chip: an 18 px icon (14 + 4 gap) before `below.rs`.
    assert_eq!(chars_at(text, &quad_bytes(text, Some((4, 18.0)))), drawn, "icon before `below`");
    // A wider icon later on the line.
    assert_eq!(chars_at(text, &quad_bytes(text, Some((13, 40.0)))), drawn, "wide icon before `L24`");
}

#[test]
fn a_colour_range_after_an_icon_lands_on_its_glyphs() {
    // RuneMD's dim suffix: `L24` (bytes 13..16) grey, `below.rs` (4..12) bright.
    let text = "see below.rs L24";
    let bytes = quad_bytes(text, Some((4, 18.0)));
    let dim: String = bytes.iter().filter(|&&b| (13..16).contains(&b)).map(|&b| text.as_bytes()[b as usize] as char).collect();
    assert_eq!(dim, "L24");
    let bright: String = bytes.iter().filter(|&&b| (4..12).contains(&b)).map(|&b| text.as_bytes()[b as usize] as char).collect();
    assert_eq!(bright, "below.rs");
}
