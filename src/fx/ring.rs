//! "Is writing" liquid metal ring (DESIGN.md, "Live signals"): a liquid metal border
//! of `RING_W` around a row, which enters in `ENTER` and turns once every `TURN`.
//!
//! `layer` is a pure function of the row rect and of time: the same for the tab, the row
//! in Recent and the row in the tree.
//!
//! metal-tab (docs/decisions/2026-11-16-metal-and-rune-effects.md): when calm, the active tab and
//! the whole ⌘P panel have a frozen gunmetal edge ([`CALM_W`] on the tab, [`RING_W`] on the panel); while an agent writes the file
//! (for the panel: any file in its list) the same place gets the moving ring
//! (docs/decisions/2026-11-16-metal-tab-quick-open-panel.md). [`edge`] picks one of the two.

use std::f32::consts::TAU;

use rune::El;
use rune_fx::{Fx, FxLayer, LiquidMetalParams, Rect, Shape};

use super::Mark;
use crate::lab::{self, RingTuning, bezier::ease};

/// Ring width, pt (DESIGN.md: 2.5; docs/decisions/2026-11-15-thick-ring.md).
pub const RING_W: f32 = 2.5;
/// Width of the calm edge, pt (metal-tab, constraint 3: about 1.5 to 2).
pub const CALM_W: f32 = 1.75;

/// The metal around a place that shows a file (tab, ⌘P panel, Recent row, tree row).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edge {
    /// No metal.
    Off,
    /// Frozen gunmetal: the active tab or the open ⌘P panel, while nobody writes its file(s).
    Calm,
    /// Moving liquid chrome: an agent writes the file; lit at this Rune clock.
    Writing(f64),
}

/// The edge of a place: `writing` is the clock at which the file's ring lit (if it is writing),
/// `chosen` says the place is the active tab or the ⌘P panel. Writing wins over calm.
pub fn edge(writing: Option<f64>, chosen: bool) -> Edge {
    match (writing, chosen) {
        (Some(since), _) => Edge::Writing(since),
        (None, true) => Edge::Calm,
        (None, false) => Edge::Off,
    }
}

/// Which place a calm edge is on: it sets the width and, in light, the metal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Calm {
    /// The active tab: [`CALM_W`], the light mid grey.
    Tab,
    /// The open ⌘P panel: always [`RING_W`] (docs/decisions/2026-11-16-metal-tab-quick-open-panel.md,
    /// follow-ups) and, in light, a lighter grey, so the long thick edge reads as light as the tab's.
    Panel,
}

impl Calm {
    pub fn width(self) -> f32 {
        match self {
            Calm::Tab => CALM_W,
            Calm::Panel => RING_W,
        }
    }
}

/// The calm edge around `rect`: frozen gunmetal, `look.width()` outside the place, visible at once,
/// never animating (it holds no display link).
pub fn calm_layer(rect: Rect, radius: f32, dark: bool, look: Calm) -> FxLayer {
    let width = look.width();
    let r = rect.inflate(width);
    let shape = Shape::rounded(r.x, r.y, r.w, r.h, radius + width);
    Fx::fill(calm_metal(dark, look).border(width)).on(shape).at(0.0).reduced(true).opacity(1.0)
}

/// The calm edge's metal: Rune's gunmetal, still (no flow). In light the gunmetal's deep floor
/// reads as a black outline, as heavy as the writing ring: there it is a softer mid grey (lower
/// contrast, lighter studio), still darker than the dark-theme tint so it reads on the light bar.
/// On the panel the edge is longer and thicker, so the same grey reads darker: there it is lighter
/// still. Dark is the same gunmetal everywhere.
pub fn calm_metal(dark: bool, look: Calm) -> LiquidMetalParams {
    let gun = LiquidMetalParams { flow: 0.0, ..LiquidMetalParams::gunmetal() };
    if dark {
        return gun;
    }
    let (tint, studio, contrast) = match look {
        Calm::Tab => (CALM_LIGHT_TINT, CALM_LIGHT_STUDIO, 0.85),
        Calm::Panel => (CALM_PANEL_LIGHT_TINT, CALM_PANEL_LIGHT_STUDIO, CALM_PANEL_LIGHT_CONTRAST),
    };
    LiquidMetalParams { tint: rune_fx::rgb(tint), light: rune_fx::rgb(studio), contrast, ..gun }
}

const CALM_LIGHT_TINT: u32 = 0x7f868f;
const CALM_LIGHT_STUDIO: u32 = 0xdde2e8;
const CALM_PANEL_LIGHT_TINT: u32 = 0xdadee3;
const CALM_PANEL_LIGHT_STUDIO: u32 = 0xf6f7f9;
const CALM_PANEL_LIGHT_CONTRAST: f32 = 0.3;
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
    let tune = lab::tuning().ring;
    let base = metal(dark);
    let metal = LiquidMetalParams { angle: base.angle + TAU * ease(tune.turn_ease, (t / tune.turn).fract()), ..base };
    let r = rect.inflate(tune.width);
    let shape = Shape::rounded(r.x, r.y, r.w, r.h, radius + tune.width);
    Fx::fill(metal.border(tune.width)).on(shape).at(t).reduced(reduced).opacity(if reduced { 1.0 } else { enter(t, tune) })
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

/// Makes `el` the slot of `edge` with id `mark`. A writing edge is lit at time `since` of Rune's
/// clock (`poll` receives it, the compositor reads `Runtime::clock`). The compositor paints it with
/// [`layer_rounded`] or [`calm_layer`] on the rect and radius of `el`. `Off` leaves `el` as it is.
pub fn slot(el: El, mark: Mark, edge: Edge, reduced: bool, dark: bool) -> El {
    slot_as(el, mark, edge, reduced, dark, Calm::Tab)
}

/// Like [`slot`], with the calm edge of `look`: the open ⌘P panel passes [`Calm::Panel`].
pub fn slot_as(el: El, mark: Mark, edge: Edge, reduced: bool, dark: bool, look: Calm) -> El {
    let data = match edge {
        Edge::Off => return el,
        Edge::Calm => calm_data(dark, look),
        Edge::Writing(since) => slot_data(since, reduced, dark),
    };
    el.effect(mark.id()).effect_bleed(RING_W.max(lab::tuning().ring.width)).effect_data(data)
}

/// `effect_data[1]` of a calm slot (0 and 1 are the writing ring, moving or reduced).
const CALM_MODE: f32 = 2.0;

/// `effect_data` of a calm [`slot`]: the calm mode, the place (0 tab, 1 panel), the theme.
pub fn calm_data(dark: bool, look: Calm) -> [f32; 4] {
    [0.0, CALM_MODE, if look == Calm::Panel { 1.0 } else { 0.0 }, if dark { 0.0 } else { 1.0 }]
}

/// From the `effect_data` of a calm [`slot`]: the place.
pub fn slot_calm_look(data: [f32; 4]) -> Calm {
    if data[2] > 0.5 { Calm::Panel } else { Calm::Tab }
}

/// From the `effect_data` of a [`slot`]: the calm edge.
pub fn slot_calm(data: [f32; 4]) -> bool {
    data[1] > 1.5
}

/// `effect_data` of a writing [`slot`]: `since` in two f32 (high part and remainder), so after days of
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

/// Opacity from 0 to 1 in `tune.enter` on `tune.enter_ease` (by default `ENTER`, ease-out cubic).
fn enter(t: f32, tune: RingTuning) -> f32 {
    ease(tune.enter_ease, (t / tune.enter).clamp(0.0, 1.0))
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

    /// docs/decisions/2026-11-15-thick-ring.md, constraint 1: 2.5 px, not 1.5.
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

    /// metal-tab: writing wins over calm; calm only on the chosen place.
    #[test]
    fn edge_picks_writing_then_calm() {
        assert_eq!(edge(Some(4.0), true), Edge::Writing(4.0));
        assert_eq!(edge(Some(4.0), false), Edge::Writing(4.0));
        assert_eq!(edge(None, true), Edge::Calm);
        assert_eq!(edge(None, false), Edge::Off);
    }

    /// metal-tab, constraint 3: a thin frozen gunmetal edge, 1.5 to 2 pt, outside the row and
    /// rounded like it; it never animates.
    #[test]
    fn calm_edge_is_thin_frozen_gunmetal() {
        assert!((1.5..=2.0).contains(&CALM_W));
        let l = calm_layer(row(), 6.0, true, Calm::Tab);
        assert_eq!(fill(&l).border, CALM_W);
        let m = metal_of(&l);
        assert_eq!(m.tint, LiquidMetalParams::gunmetal().tint);
        assert_eq!(m.flow, 0.0);
        let r = l.shape.rect;
        assert_eq!((r.x, r.w), (10.0 - CALM_W, 200.0 + 2.0 * CALM_W));
        assert!(!l.is_animating());
        assert_eq!(l.opacity, 1.0);
    }

    /// In light the calm edge is darker than in dark, so it reads on the light bar.
    #[test]
    fn the_light_calm_edge_is_darker() {
        let lum = |c: rune_fx::Rgb| 0.2126 * c.0[0] + 0.7152 * c.0[1] + 0.0722 * c.0[2];
        assert!(lum(calm_metal(false, Calm::Tab).tint) < lum(calm_metal(true, Calm::Tab).tint));
        assert_eq!(metal_of(&calm_layer(row(), 6.0, false, Calm::Tab)).tint, calm_metal(false, Calm::Tab).tint);
    }

    /// Follow-up 2026-11-16: in light the panel's calm metal is lighter than the tab's (and still
    /// darker than the bar, so it reads); in dark both are the same gunmetal.
    #[test]
    fn the_light_panel_calm_edge_is_lighter_than_the_tab() {
        let lum = |c: rune_fx::Rgb| 0.2126 * c.0[0] + 0.7152 * c.0[1] + 0.0722 * c.0[2];
        let (panel, tab) = (calm_metal(false, Calm::Panel), calm_metal(false, Calm::Tab));
        assert!(lum(panel.tint) > lum(tab.tint) && lum(panel.light) >= lum(tab.light));
        assert!(panel.contrast < tab.contrast);
        assert_eq!(panel.flow, 0.0);
        assert_eq!(calm_metal(true, Calm::Panel), calm_metal(true, Calm::Tab));
    }

    #[test]
    fn calm_data_is_told_apart_from_writing() {
        assert!(slot_calm(calm_data(true, Calm::Tab)) && slot_calm(calm_data(false, Calm::Panel)));
        assert!(!slot_calm(slot_data(3.0, true, true)) && !slot_calm(slot_data(3.0, false, false)));
        assert!(slot_dark(calm_data(true, Calm::Tab)) && !slot_dark(calm_data(false, Calm::Tab)));
        assert_eq!(slot_calm_look(calm_data(false, Calm::Panel)), Calm::Panel);
        assert_eq!(slot_calm_look(calm_data(false, Calm::Tab)), Calm::Tab);
    }

    /// The thick calm edge (open ⌘P panel) is as wide as the writing ring and still frozen gunmetal.
    #[test]
    fn the_thick_calm_edge_is_ring_wide_and_frozen() {
        let l = calm_layer(row(), 6.0, true, Calm::Panel);
        assert_eq!(fill(&l).border, RING_W);
        assert_eq!(metal_of(&l).flow, 0.0);
        assert!(!l.is_animating());
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
