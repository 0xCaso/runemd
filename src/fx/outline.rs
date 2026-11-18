//! The front tab and the page in one metal outline (record 2026-11-18-tab-outline, Matteo: "se ad arco
//! vuol dire che il bordo dovrebbe anche propagarsi" round the whole page, "rotondo qui" where the tab
//! meets it): one `RING_W` band of the moving chrome that runs up and round the front tab, turns into
//! the page's top edge with a concave fillet of `FILLET` on each side, and goes round the page.
//!
//! rune-fx draws a liquid metal border only on a rounded rect, so the shape is cut from three of them,
//! each shown through a clip whose cut lines fall on whole device pixels (a pixel is then drawn by
//! exactly one piece, with no double-drawn seam):
//! - the tab's ring (outside the tab), down to the fillets' top;
//! - two quarter rings of radius `FILLET` (a circle's border, which bends the other way), one each
//!   side, from the tab's side into the page's top edge;
//! - the page's ring (inside the page's edge), with its top edge cut out between the fillets.
//!
//! A border's tube shading depends only on the depth across the band and the edge's direction, so the
//! pieces meet without a step; the dome (a slow tilt across each shape) would differ from piece to
//! piece and is off on the outline, and the liquid is pinned to the window, so it flows across the
//! joints.

use rune_fx::{Rect, Shape};

use crate::ui::theme::R_ROW;

/// The concave fillet where the tab's side meets the page's top edge: the tab's corner radius.
pub const FILLET: f32 = R_ROW;

/// A piece of the outline: a shape whose border the compositor paints, and the device-pixel clip
/// `[x0, y0, x1, y1]` it shows through (to be intersected with the slot's own clip).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cut {
    pub shape: Shape,
    pub clip: [f32; 4],
}

const OPEN: f32 = f32::INFINITY;

/// A cut line at `pt`, on a whole device pixel at `scale`.
fn px(pt: f32, scale: f32) -> f32 {
    (pt * scale).round()
}

/// The tab's pieces: the tab's ring and the two fillets. `tab` is the tab's rect (pt) with top corner
/// radius `radius`; it may run below the page's top. `page_top` is the page outline's outer top edge
/// (pt): the band runs from there to `page_top + width`, the fillets end on it.
pub fn tab_cuts(tab: Rect, radius: f32, page_top: f32, width: f32, scale: f32) -> [Cut; 3] {
    let inner = page_top + width;
    let cy = inner - FILLET;
    let (x0, x1) = (tab.x, tab.x + tab.w);
    let r = tab.inflate(width);
    let ring = Cut { shape: Shape::rounded(r.x, r.y, r.w, r.h, radius + width), clip: [-OPEN, -OPEN, OPEN, px(cy, scale)] };
    // Each fillet is the lower quarter of a ring centred off the tab's corner, on the bar's side: its
    // band runs from the tab's side band (at its top) to the page's top band (at its far end).
    let left = Cut { shape: Shape::circle(x0 - FILLET, cy, FILLET), clip: [px(x0 - FILLET, scale), px(cy, scale), px(x0 + 1.0, scale), px(inner + 1.0, scale)] };
    let right = Cut { shape: Shape::circle(x1 + FILLET, cy, FILLET), clip: [px(x1 - 1.0, scale), px(cy, scale), px(x1 + FILLET, scale), px(inner + 1.0, scale)] };
    [ring, left, right]
}

/// The page's pieces: its ring, `page` being the outer edge of the band (pt) with corner radius
/// `radius`, the band `width` inside it. `hole` is the stretch of the top edge the front tab opens
/// (pt, from the left fillet's start to the right one's end), `None` without a front tab on screen:
/// then one piece, the whole ring.
pub fn page_cuts(page: Rect, radius: f32, width: f32, hole: Option<(f32, f32)>, scale: f32) -> Vec<Cut> {
    let shape = Shape::rounded(page.x, page.y, page.w, page.h, radius);
    let Some((a, b)) = hole.filter(|(a, b)| b > a) else {
        return vec![Cut { shape, clip: [-OPEN, -OPEN, OPEN, OPEN] }];
    };
    let (a, b) = (px(a, scale), px(b, scale));
    vec![
        Cut { shape, clip: [-OPEN, -OPEN, a, OPEN] },
        Cut { shape, clip: [a, px(page.y + width, scale), b, OPEN] },
        Cut { shape, clip: [b, -OPEN, OPEN, OPEN] },
    ]
}

/// The stretch of the page's top edge the tab at `x0..x1` (pt) opens: from the left fillet's start to
/// the right one's end.
pub fn hole(x0: f32, x1: f32) -> (f32, f32) {
    (x0 - FILLET, x1 + FILLET)
}

/// `clip` (device px) narrowed to `cut`, with `fade` the slot's packed soft-edge and rounded-clip words
/// (`rune::render` `CLIP_WGSL`): an edge the cut moves loses its soft edge (its ramp belongs to the old
/// edge), and a narrowed clip drops the rounded clip, which is measured from the clip's own rect (the
/// pieces already follow the panel's radius). `None` when nothing is left.
pub fn narrow(clip: [f32; 4], fade: [u32; 4], cut: [f32; 4]) -> Option<([f32; 4], [u32; 4])> {
    let out = [clip[0].max(cut[0]), clip[1].max(cut[1]), clip[2].min(cut[2]), clip[3].min(cut[3])];
    if out[2] <= out[0] || out[3] <= out[1] {
        return None;
    }
    let mut fade = fade;
    let mut moved = false;
    for edge in 0..4 {
        if out[edge] != clip[edge] {
            moved = true;
            // Soft widths: two f16 per word (left, top | right, bottom), the first in the low half.
            let (word, shift) = (edge / 2, (edge % 2) * 16);
            fade[word] &= !(0xffff << shift);
            // Offsets: one byte per edge in the third word.
            fade[2] &= !(0xff << (edge * 8));
        }
    }
    if moved {
        fade[3] = 0;
    }
    Some((out, fade))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::ring::RING_W;

    const S: f32 = 2.0;

    fn tab() -> Rect {
        Rect::new(300.0, 8.0, 90.0, 46.75)
    }

    /// The pieces meet on whole device pixels: the tab's ring stops where the fillets start, the page's
    /// top edge stops where they end, and the middle of the page starts below its top band.
    #[test]
    fn the_pieces_meet_on_whole_pixels() {
        let [ring, left, right] = tab_cuts(tab(), R_ROW, 44.0, RING_W, S);
        assert_eq!(ring.clip[3], left.clip[1]);
        assert_eq!(ring.clip[3], right.clip[1]);
        assert_eq!(ring.clip[3], ((44.0 + RING_W - FILLET) * S).round());
        let (a, b) = hole(300.0, 390.0);
        let page = page_cuts(Rect::new(0.0, 44.0, 800.0, 600.0), 10.0, RING_W, Some((a, b)), S);
        assert_eq!(page.len(), 3);
        assert_eq!(page[0].clip[2], left.clip[0], "the page's top edge ends where the left fillet starts");
        assert_eq!(page[2].clip[0], right.clip[2], "and starts again where the right one ends");
        assert_eq!(page[1].clip[1], ((44.0 + RING_W) * S).round(), "under the tab only below the top band");
        for c in [ring.clip, left.clip, right.clip].iter().chain(page.iter().map(|c| &c.clip)) {
            assert!(c.iter().all(|v| v.is_infinite() || v.fract() == 0.0), "{c:?}");
        }
    }

    /// Each fillet is a ring of radius `FILLET` whose band starts in line with the tab's side band and
    /// ends in line with the page's top band, turning the other way from a corner (concave).
    #[test]
    fn the_fillets_join_the_tab_side_to_the_page_top() {
        let [ring, left, right] = tab_cuts(tab(), R_ROW, 44.0, RING_W, S);
        assert_eq!(FILLET, R_ROW);
        let inner = 44.0 + RING_W;
        // Left: the circle's right end is the tab's left edge, its bottom the page's inner top edge.
        let l = left.shape;
        assert_eq!((l.rect.x + l.rect.w, l.rect.y + l.rect.h, l.radius), (300.0, inner, FILLET));
        // The tab's ring band runs from the tab's edge outwards, the same place as the circle's band.
        assert_eq!(ring.shape.rect.x, 300.0 - RING_W);
        let r = right.shape;
        assert_eq!((r.rect.x, r.rect.y + r.rect.h), (390.0, inner));
        // The quarter shown: the one between the tab and the page, below the circle's centre.
        assert_eq!(left.clip[1], ((inner - FILLET) * S).round());
        assert_eq!(left.clip[0], ((300.0 - FILLET) * S).round());
        assert_eq!(right.clip[2], ((390.0 + FILLET) * S).round());
    }

    #[test]
    fn without_a_tab_the_page_is_one_ring() {
        let page = page_cuts(Rect::new(0.0, 44.0, 800.0, 600.0), 10.0, RING_W, None, S);
        assert_eq!(page.len(), 1);
        assert!(page[0].clip.iter().all(|v| v.is_infinite()));
        assert_eq!(page_cuts(Rect::new(0.0, 44.0, 800.0, 600.0), 10.0, RING_W, Some((5.0, 5.0)), S).len(), 1);
    }

    /// A moved edge drops its soft edge and offset; the rounded clip goes once any edge moves; an edge
    /// the cut leaves keeps its fade.
    #[test]
    fn narrowing_drops_the_soft_edges_it_moves() {
        let fade = [0x1111_2222, 0x3333_4444, 0x0a0b_0c0d, 0x6000_0001];
        let (clip, f) = narrow([0.0, 0.0, 100.0, 100.0], fade, [-OPEN, -OPEN, OPEN, 50.0]).expect("left");
        assert_eq!(clip, [0.0, 0.0, 100.0, 50.0]);
        assert_eq!(f, [0x1111_2222, 0x0000_4444, 0x000b_0c0d, 0]);
        let (_, same) = narrow([0.0, 0.0, 100.0, 100.0], fade, [-OPEN, -OPEN, OPEN, OPEN]).expect("all");
        assert_eq!(same, fade, "nothing moved");
        assert!(narrow([0.0, 0.0, 100.0, 100.0], fade, [120.0, -OPEN, OPEN, OPEN]).is_none());
    }
}
