//! "Is writing" liquid metal ring (DESIGN.md, "Live signals"): a liquid metal border
//! of `RING_W` around a row, which enters in `ENTER` and turns once every `TURN`.
//!
//! `layer` is a pure function of the row rect and of time: the same for the tab, the row
//! in Recent and the row in the tree.

use std::f32::consts::TAU;

use rune::El;
use rune_fx::{Fx, FxLayer, LiquidMetalParams, Rect, Shape};

use super::Mark;

/// Ring width, pt (DESIGN.md: 2.5; docs/decisions/2026-11-15-thick-ring.md).
pub const RING_W: f32 = 2.5;
/// Entrance: opacity from 0 to 1 in 220 ms, ease-out.
pub const ENTER: f32 = 0.22;
/// One turn of the reflected studio every 3 s.
pub const TURN: f32 = 3.0;
/// Row radius (`theme::R_ROW`): the ring widens it by `RING_W`.
#[cfg(test)]
pub const ROW_RADIUS: f32 = crate::ui::theme::R_ROW;

/// The ring around `rect` (the row, in pt), `t` seconds after it lit up.
/// `.border` paints inside the shape (rune-fx README, "Fills"): the shape is the rect widened by
/// `RING_W` per side, so the band sits entirely outside the row. With `reduced` it is still and
/// visible at once.
#[cfg(test)]
pub fn layer(rect: Rect, t: f32, reduced: bool) -> FxLayer {
    layer_rounded(rect, ROW_RADIUS, t, reduced, true)
}

/// Like [`layer`], for a slot with radius `radius` (the sidebar rows and the tab),
/// in the `dark` theme or in light ([`metal`]).
pub fn layer_rounded(rect: Rect, radius: f32, t: f32, reduced: bool, dark: bool) -> FxLayer {
    let t = if reduced { 0.0 } else { t.max(0.0) };
    let base = metal(dark);
    let metal = LiquidMetalParams { angle: base.angle + TAU * (t / TURN).fract(), ..base };
    let r = rect.inflate(RING_W);
    let shape = Shape::rounded(r.x, r.y, r.w, r.h, radius + RING_W);
    Fx::fill(metal.border(RING_W)).on(shape).at(t).reduced(reduced).opacity(if reduced { 1.0 } else { enter(t) })
}

/// The ring's metal: Rune's chrome in dark. In light the brightest reflections of the
/// chrome are almost white and on the panel and on the tab's `sel` they drop below 3:1: the
/// metal is darker (studio tint and lights), so it keeps ≥ 3:1 along the whole perimeter
/// (measured in DESIGN.md, "Contrast").
pub fn metal(dark: bool) -> LiquidMetalParams {
    let chrome = LiquidMetalParams::chrome();
    if dark {
        return chrome;
    }
    // Tint and lights of the metal in the light theme (the chrome has `e9ecf0` and `ffffff`).
    LiquidMetalParams { tint: rune_fx::rgb(LIGHT_TINT), light: rune_fx::rgb(LIGHT_STUDIO), ..chrome }
}

const LIGHT_TINT: u32 = 0xd8dbe0;
const LIGHT_STUDIO: u32 = 0xe6e7ea;

/// Makes `el` the slot of ring `mark`, lit at time `since` of Rune's clock
/// (`poll` receives it, the compositor reads `Runtime::clock`). The compositor paints it with
/// [`layer_rounded`] on the rect and radius of `el`.
pub fn slot(el: El, mark: Mark, since: f64, reduced: bool, dark: bool) -> El {
    el.effect(mark.id()).effect_bleed(RING_W).effect_data(slot_data(since, reduced, dark))
}

/// `effect_data` of a [`slot`]: `since` in two f32 (high part and remainder), so after days of
/// clock the difference stays precise to the µs; then "reduce motion" and the light theme.
pub fn slot_data(since: f64, reduced: bool, dark: bool) -> [f32; 4] {
    let hi = since as f32;
    [hi, if reduced { 1.0 } else { 0.0 }, (since - f64::from(hi)) as f32, if dark { 0.0 } else { 1.0 }]
}

/// From the `effect_data` of a [`slot`]: the dark theme.
pub fn slot_dark(data: [f32; 4]) -> bool {
    data[3] < 0.5
}

/// From the `effect_data` of a [`slot`]: seconds since the ring lit up and "reduce motion".
/// The subtraction is in f64; it goes to f32 only afterwards.
pub fn slot_time(data: [f32; 4], clock: f64) -> (f32, bool) {
    let since = f64::from(data[0]) + f64::from(data[2]);
    ((clock - since).max(0.0) as f32, data[1] > 0.5)
}

/// Ease-out cubica da 0 a 1 in `ENTER`.
fn enter(t: f32) -> f32 {
    let x = (t / ENTER).clamp(0.0, 1.0);
    1.0 - (1.0 - x).powi(3)
}

#[cfg(test)]
mod tests {
    use rune_fx::{FillParams, FillStyle, Fx, LiquidMetalParams};

    use super::*;

    fn row() -> Rect {
        Rect::new(10.0, 20.0, 200.0, 28.0)
    }

    fn fill(l: &FxLayer) -> FillParams {
        match l.fx {
            Fx::Fill(p) => p,
            other => panic!("not a fill: {other:?}"),
        }
    }

    fn metal_of(l: &FxLayer) -> LiquidMetalParams {
        match fill(l).style {
            FillStyle::LiquidMetal(m) => m,
            other => panic!("not liquid metal: {other:?}"),
        }
    }

    /// docs/decisions/2026-11-15-thick-ring.md, vincolo 1: 2,5 px, non 1,5.
    #[test]
    fn ring_is_2_5_px_wide() {
        assert_eq!(RING_W, 2.5);
        assert_eq!(fill(&layer(row(), 1.0, false)).border, 2.5);
    }

    #[test]
    fn ring_is_a_liquid_metal_border_outside_the_row() {
        let l = layer(row(), 1.0, false);
        metal_of(&l);
        assert_eq!(fill(&l).border, RING_W);
        let r = l.shape.rect;
        assert_eq!((r.x, r.y, r.w, r.h), (10.0 - RING_W, 20.0 - RING_W, 200.0 + 2.0 * RING_W, 28.0 + 2.0 * RING_W));
        assert!(l.is_animating());
    }

    #[test]
    fn ring_fades_in_over_220_ms_ease_out() {
        assert_eq!(layer(row(), 0.0, false).opacity, 0.0);
        let mid = layer(row(), ENTER / 2.0, false).opacity;
        assert!(mid > 0.5 && mid < 1.0, "ease-out: past half at half time ({mid})");
        assert_eq!(layer(row(), ENTER, false).opacity, 1.0);
        assert_eq!(layer(row(), 5.0, false).opacity, 1.0);
    }

    #[test]
    fn ring_turns_once_every_3_s() {
        let tau = std::f32::consts::TAU;
        let angle = |t| metal_of(&layer(row(), t, false)).angle.rem_euclid(tau);
        assert!((angle(1.0) - angle(1.0 + TURN / 2.0)).abs() > 3.0, "mezzo giro in TURN / 2");
        assert!((angle(1.0) - angle(1.0 + TURN)).abs() < 1e-3, "one turn in TURN");
    }

    #[test]
    fn slot_data_round_trips() {
        assert_eq!(slot_time(slot_data(12.5, true, true), 13.0), (0.5, true));
        assert_eq!(slot_time(slot_data(12.5, false, true), 12.0), (0.0, false), "never negative");
        assert!(slot_dark(slot_data(1.0, false, true)) && !slot_dark(slot_data(1.0, false, false)));
    }

    /// After 30 days of clock (2.6 million s, beyond the precision of an f32) the 220 ms
    /// entrance still rises in frame steps, not in 0.25 s jumps.
    #[test]
    fn fade_still_ramps_after_30_days() {
        let since = 30.0 * 86_400.0 + 0.123_456;
        let data = slot_data(since, false, true);
        let opacity = |dt: f64| layer(row(), slot_time(data, since + dt).0, false).opacity;
        let ramp: Vec<f32> = [0.0, 0.033, 0.066, 0.1, 0.15, 0.22].iter().map(|dt| opacity(*dt)).collect();
        assert!(ramp[0] < 0.01, "{ramp:?}");
        assert!(ramp.windows(2).all(|w| w[1] > w[0]), "rises on every frame: {ramp:?}");
        assert!(ramp[5] > 0.999, "{ramp:?}");
        assert!((slot_time(data, since + 1.5).0 - 1.5).abs() < 1e-3);
    }

    /// In the light theme the metal is darker than the chrome (tint and lights), in dark it is the chrome.
    #[test]
    fn the_light_ring_is_darker_metal() {
        let lum = |c: rune_fx::Rgb| 0.2126 * c.0[0] + 0.7152 * c.0[1] + 0.0722 * c.0[2];
        let (dark, light) = (metal(true), metal(false));
        assert_eq!(dark, LiquidMetalParams::chrome());
        assert!(lum(light.tint) < lum(dark.tint) && lum(light.light) < lum(dark.light));
        let l = layer_rounded(row(), 6.0, 1.0, false, false);
        assert_eq!(metal_of(&l).tint, light.tint);
    }

    #[test]
    fn reduced_motion_is_still_and_visible() {
        let a = layer(row(), 0.0, true);
        let b = layer(row(), 7.0, true);
        assert_eq!(a.opacity, 1.0, "visible at once, without entrance");
        assert!(a.reduced_motion);
        assert!(!a.is_animating());
        assert_eq!((a.time, metal_of(&a)), (b.time, metal_of(&b)), "still");
    }
}
