//! "Is writing" liquid metal ring (DESIGN.md, "Live signals"): a liquid metal border
//! of `RING_W` around a row, which enters in `ENTER` and turns once every `TURN`.
//!
//! `layer` is a pure function of the row rect and of time: the same for the tab, the row
//! in Recent and the row in the tree.
//!
//! metal-tab (docs/decisions/2026-11-16-metal-and-rune-effects.md): the active tab and the whole ⌘P
//! panel have a calm edge; while an agent writes the file (for the panel: any file in its list) the
//! same place gets the writing ring, which enters with a fade. Since the rune-fx decision
//! (docs/decisions/2026-11-18-rune-fx-markdown.md, constraint 1) both are the same moving liquid
//! chrome, [`RING_W`] wide, in the same colour; only the writing ring has an entrance. Reduce Motion
//! and an inactive window keep both still (constraint 2). [`edge`] picks one of the two.

use std::f32::consts::TAU;

use rune::El;
use rune_fx::{Fx, FxLayer, LiquidMetalParams, Rect, Shape};

use super::Mark;
use crate::lab::{self, RingTuning, bezier::ease};

/// Width of every metal edge, pt: calm tab, writing ring, ⌘P panel, Recent and tree rows
/// (docs/decisions/2026-11-18-rune-fx-markdown.md, constraint 1: 2 pt replaces 3.25 on the ring and
/// panel and 2.3 on the calm tab).
pub const RING_W: f32 = 2.0;

/// The metal around a place that shows a file (tab, ⌘P panel, Recent row, tree row).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edge {
    /// No metal.
    Off,
    /// The active tab or the open ⌘P panel, while nobody writes its file(s): the moving chrome,
    /// visible at once, turning on Rune's clock.
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

/// The calm edge around `rect` at Rune's clock `clock` (s): the writing ring's chrome, `RING_W`
/// outside the place, visible at once (no entrance), one turn every `TURN` counted from the clock,
/// so every calm edge turns in step. `still` (Reduce Motion, inactive window) holds the pose of a
/// still writing ring.
#[cfg(test)]
pub fn calm_layer(rect: Rect, radius: f32, clock: f64, still: bool, dark: bool) -> FxLayer {
    band(around(rect, radius), Pose::calm(clock, still), dark, true)
}

/// The calm edge's liquid time wraps every `FLOW_WRAP` seconds of clock (f32 precision).
const FLOW_WRAP: f64 = 3600.0;

/// At rest the metal does not hold the display link (at 120 Hz the moving edges cost ~14 % CPU):
/// it steps 8 times a second on a `wake_after(ZERO)` from `poll`, which draws a frame without
/// running the view. Rune follows each such frame with a second one (the zero wake is scheduled
/// again after the frame it asked for), so 8 steps are ~16 frames: ~2 % CPU at rest
/// (docs/perf-memory-2026-11-16.md, "Moving metal 2026-11-18").
pub const METAL_FRAME: std::time::Duration = std::time::Duration::from_millis(125);
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
#[cfg(test)]
pub fn layer_rounded(rect: Rect, radius: f32, t: f32, reduced: bool, dark: bool) -> FxLayer {
    band(around(rect, radius), Pose::writing(t, reduced), dark, true)
}

/// Where the metal is at a moment: the liquid's time, the studio's turn (0..1), still or moving, and
/// the opacity (the writing ring's entrance).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pose {
    pub t: f32,
    pub phase: f32,
    pub still: bool,
    pub opacity: f32,
}

impl Pose {
    /// A calm edge at Rune's clock `clock` (s): visible at once, one turn every `TURN` counted from the
    /// clock, so every calm edge turns in step. `still` (Reduce Motion, inactive window) holds the pose
    /// of a still writing ring.
    pub fn calm(clock: f64, still: bool) -> Pose {
        if still {
            return Pose { t: 0.0, phase: 0.0, still: true, opacity: 1.0 };
        }
        let tune = lab::tuning().ring;
        // In f64 first: after days of clock an f32 would step the turn. The liquid's own time wraps
        // once an hour, a step nobody sees on a 2 pt edge.
        let clock = clock.max(0.0);
        let phase = ease(tune.turn_ease, (clock / f64::from(tune.turn)).fract() as f32);
        Pose { t: clock.rem_euclid(FLOW_WRAP) as f32, phase, still: false, opacity: 1.0 }
    }

    /// A writing ring `t` seconds after it lit: entering, then turning; `reduced` still and visible.
    pub fn writing(t: f32, reduced: bool) -> Pose {
        let t = if reduced { 0.0 } else { t.max(0.0) };
        let (opacity, phase) = lab::ring_at(f64::from(t) * 1000.0, &lab::tuning().ring);
        Pose { t, phase, still: reduced, opacity: if reduced { 1.0 } else { opacity } }
    }
}

/// The shape of the band `width` outside `rect` with corner radius `radius`: `.border` paints inside
/// the shape, so the shape is the rect widened by the band.
pub fn around(rect: Rect, radius: f32) -> Shape {
    let width = lab::tuning().ring.width;
    let r = rect.inflate(width);
    Shape::rounded(r.x, r.y, r.w, r.h, radius + width)
}

/// The moving chrome band, `width` wide inside `shape`, at `pose`. `dome` off keeps the metal's slow
/// tilt across the shape out, so pieces of one outline cut from different shapes meet without a step
/// (`fx::outline`).
pub fn band(shape: Shape, pose: Pose, dark: bool, dome: bool) -> FxLayer {
    let width = lab::tuning().ring.width;
    let base = metal(dark);
    let metal = LiquidMetalParams { angle: base.angle + TAU * pose.phase, dome: if dome { base.dome } else { 0.0 }, ..base };
    Fx::fill(metal.border(width)).on(shape).at(pose.t).reduced(pose.still).opacity(pose.opacity)
}

/// Whether the ring `t` seconds after it lit is past its entrance: from there it only turns, and the
/// compositor paces it like a calm edge.
pub fn entered(t: f32) -> bool {
    t >= lab::tuning().ring.enter
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
    let data = match edge {
        Edge::Off => return el,
        Edge::Calm => calm_data(reduced, dark),
        Edge::Writing(since) => slot_data(since, reduced, dark),
    };
    el.effect(mark.id()).effect_bleed(RING_W.max(lab::tuning().ring.width)).effect_data(data)
}

/// `effect_data[1]` of a calm slot (0 and 1 are the writing ring, moving or reduced).
const CALM_MODE: f32 = 2.0;

/// `effect_data` of a calm [`slot`]: the calm mode, "reduce motion", the theme.
pub fn calm_data(reduced: bool, dark: bool) -> [f32; 4] {
    [0.0, CALM_MODE, if reduced { 1.0 } else { 0.0 }, if dark { 0.0 } else { 1.0 }]
}

/// From the `effect_data` of a calm [`slot`]: "reduce motion".
pub fn slot_calm_reduced(data: [f32; 4]) -> bool {
    data[2] > 0.5
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

/// Opacity from 0 to 1 in `tune.enter` on `tune.enter_ease` (by default `ENTER`, ease-out cubic);
/// a tuned curve that overshoots stays within 0..=1.
pub(crate) fn enter(t: f32, tune: RingTuning) -> f32 {
    ease(tune.enter_ease, (t / tune.enter).clamp(0.0, 1.0)).clamp(0.0, 1.0)
}

/// The turn's phase (0..1) `t` seconds after the ring lit: one turn every `tune.turn` on `tune.turn_ease`.
pub(crate) fn phase(t: f32, tune: RingTuning) -> f32 {
    ease(tune.turn_ease, (t / tune.turn).fract())
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

    /// docs/decisions/2026-11-18-rune-fx-markdown.md, constraint 1: one width, 2 pt, for the
    /// writing ring and the calm edge (3.25 and 2.3 before).
    #[test]
    fn every_metal_edge_is_2_pt_wide() {
        assert_eq!(RING_W, 2.0);
        assert_eq!(lab::LabTuning::DEFAULT.ring.width, RING_W);
        assert_eq!(fill(&layer(row(), 1.0, false)).border, 2.0);
        assert_eq!(fill(&calm_layer(row(), 6.0, 1.0, false, true)).border, 2.0);
        assert_eq!(fill(&calm_layer(row(), 6.0, 1.0, true, false)).border, 2.0);
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

    /// A tuned entrance curve that overshoots keeps the opacity within 0..=1.
    #[test]
    fn an_overshooting_entrance_stays_a_valid_opacity() {
        let tune = RingTuning { enter_ease: [0.3, 1.8, 0.5, 1.5], ..lab::LabTuning::DEFAULT.ring };
        assert!(crate::lab::bezier::cubic_bezier(tune.enter_ease, 0.6) > 1.0, "the curve overshoots");
        for i in 0..=20 {
            let o = enter(tune.enter * i as f32 / 20.0, tune);
            assert!((0.0..=1.0).contains(&o), "{o}");
        }
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

    /// rune-fx, constraint 1: the calm edge is the writing ring's chrome, same colour in both
    /// themes, outside the row, visible at once and moving.
    #[test]
    fn the_calm_edge_is_the_moving_writing_chrome() {
        for dark in [true, false] {
            let calm = calm_layer(row(), 6.0, 4.0, false, dark);
            let ring = layer_rounded(row(), 6.0, 4.0, false, dark);
            let (c, r) = (metal_of(&calm), metal_of(&ring));
            assert_eq!((c.tint, c.light, c.contrast, c.flow), (r.tint, r.light, r.contrast, r.flow), "dark {dark}");
            assert_eq!(c, LiquidMetalParams { angle: c.angle, ..metal(dark) });
            assert!(calm.is_animating(), "moves");
            assert_eq!(calm.opacity, 1.0, "no entrance");
            assert_eq!(calm.shape.rect.x, 10.0 - RING_W);
        }
    }

    /// The calm edge turns once every `TURN` on Rune's clock, so every calm edge turns in step.
    #[test]
    fn the_calm_edge_turns_on_the_clock() {
        let tau = std::f32::consts::TAU;
        let angle = |rect: Rect, clock| metal_of(&calm_layer(rect, 6.0, clock, false, true)).angle.rem_euclid(tau);
        assert!((angle(row(), 1.0) - angle(row(), 1.0 + f64::from(TURN) / 2.0)).abs() > 3.0, "half a turn in TURN / 2");
        assert!((angle(row(), 1.0) - angle(row(), 1.0 + f64::from(TURN))).abs() < 1e-3);
        assert_eq!(angle(row(), 2.2), angle(Rect::new(400.0, 0.0, 600.0, 400.0), 2.2), "in step");
    }

    /// rune-fx, constraint 2: still (Reduce Motion, inactive window), the calm edge holds the pose of
    /// a still writing ring, whatever the clock.
    #[test]
    fn a_still_calm_edge_is_a_still_ring() {
        let (a, b) = (calm_layer(row(), 6.0, 1.0, true, true), calm_layer(row(), 6.0, 9.5, true, true));
        assert!(!a.is_animating() && a.reduced_motion);
        assert_eq!((a.time, metal_of(&a)), (b.time, metal_of(&b)));
        let ring = layer_rounded(row(), 6.0, 7.0, true, true);
        assert_eq!((a.time, metal_of(&a), a.opacity), (ring.time, metal_of(&ring), ring.opacity));
    }

    #[test]
    fn calm_data_is_told_apart_from_writing() {
        assert!(slot_calm(calm_data(false, true)) && slot_calm(calm_data(true, false)));
        assert!(!slot_calm(slot_data(3.0, true, true)) && !slot_calm(slot_data(3.0, false, false)));
        assert!(slot_dark(calm_data(false, true)) && !slot_dark(calm_data(false, false)));
        assert!(slot_calm_reduced(calm_data(true, true)) && !slot_calm_reduced(calm_data(false, true)));
    }

    /// The ring is paced like a calm edge only once its entrance is over.
    #[test]
    fn the_ring_has_entered_after_its_fade() {
        assert!(!entered(0.0) && !entered(ENTER / 2.0));
        assert!(entered(ENTER) && entered(30.0));
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
