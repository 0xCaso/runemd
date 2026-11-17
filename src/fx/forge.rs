//! forge-empty: the Rune fire inside the letters of the "RuneMD" wordmark on the screen with no file
//! open (docs/decisions/2026-11-16-wordmark-plate.md, 2026-11-16-metal-and-rune-effects.md).
//!
//! The slot is the letters' box from the cap height to the baseline. rune-fx shapes are rounded
//! rects only, so the fire is not clipped to the glyphs here: `ui::forge` draws the dark letters
//! under the slot and the chrome strip with the letters cut out over it, and the fire shows only
//! through the cut. Calm (no agent writing): flames at `CALM` of the cap height, moving gently with the
//! `fire::REST_FRAME` wake-ups, no display link (docs/decisions/2026-11-16-feedback-group-3.md, item 1).
//! While an agent writes: they move at full rate and rise to `WRITING` in `fire::RISE`. When it stops
//! they ease back down to `CALM` in the same time, then go on moving calm. With "reduce motion" or the
//! window inactive they stay still: at the writing height while it writes, calm (`Frozen`) after.
//!
//! While an agent writes the strip also melts: the ring's liquid chrome over it, `Overlay`-blended on a
//! backdrop copy ([`melt_layer`]); calm there is no melt slot at all.

use rune::El;
use rune_fx::{FireParams, Fx, FxBlend, FxLayer, LiquidMetalParams, Rect, Shape};

use super::{Mark, fire};

/// Flame height while calm, share of the cap height (decision: flames to about 60 %).
pub const CALM: f32 = 0.60;
/// Flame height while an agent writes, share of the cap height: the fire rises.
pub const WRITING: f32 = 0.82;
/// Where the flames start, share of the cap height above the baseline: the base sits inside the letters.
pub const BASE_IN: f32 = 0.05;
/// The frozen frame shown while calm (rune-fx time, s), chosen for even tongues across the six letters.
pub const CALM_T: f32 = 3.2;
/// Tongue size, share of the cap height (the panel fire uses 22 pt on 110 pt flames).
const SCALE: f32 = 0.13;
/// Brighter than the panel fire: the mockup's letters glow yellow-white at their feet.
const INTENSITY: f32 = 1.35;
/// Depth of the shape the flames rise from, below their base (hidden by the strip).
const DEPTH: f32 = 24.0;

/// How the fire behaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    /// No agent writes: low, moving gently at the `fire::REST_FRAME` pace.
    Calm,
    /// No agent writes, "reduce motion" on or the window inactive: low and still.
    Frozen,
    /// An agent writes: it rises and moves.
    Moving,
    /// An agent writes, but "reduce motion" is on or the window is inactive: risen, still.
    Still,
    /// The agent stopped: it eases back down to calm in `fire::RISE`, then moves on as calm.
    Falling,
}

/// The fire's motion: it always moves in an active window without "reduce motion", gently when calm
/// and at full rate while an agent writes. `stopped`: an agent wrote since the app started and stopped.
pub fn motion(writing: bool, stopped: bool, reduced: bool, active_window: bool) -> Motion {
    match (writing, stopped, reduced || !active_window) {
        (true, _, false) => Motion::Moving,
        (true, _, true) => Motion::Still,
        (false, true, false) => Motion::Falling,
        (false, false, false) => Motion::Calm,
        (false, _, true) => Motion::Frozen,
    }
}

/// Whether the fire redraws only with the `fire::REST_FRAME` wake-ups (calm, or a finished fall),
/// not with the display link. `t`: as in [`layer`].
pub fn paced(m: Motion, t: f32) -> bool {
    match m {
        Motion::Calm => true,
        Motion::Falling => t >= fire::RISE,
        Motion::Moving | Motion::Still | Motion::Frozen => false,
    }
}

/// Cubic ease-out from 0 to 1 over `fire::RISE`.
fn ease(t: f32) -> f32 {
    let x = (t.max(0.0) / fire::RISE).clamp(0.0, 1.0);
    1.0 - (1.0 - x).powi(3)
}

/// Flame height as a share of the cap height, `t` seconds after writing started (or stopped, falling).
pub fn height_share(m: Motion, t: f32) -> f32 {
    match m {
        Motion::Calm | Motion::Frozen => CALM,
        Motion::Still => WRITING,
        Motion::Moving => CALM + (WRITING - CALM) * ease(t),
        Motion::Falling => WRITING - (WRITING - CALM) * ease(t),
    }
}

/// The fire for a slot whose rect `letters` (pt) spans the letters from the cap height (top) to the
/// baseline (bottom). `t`: seconds since writing started or stopped; `clock`: Rune's clock, which
/// moves the flames. Calm and falling both run on `clock`, so the flames do not jump when a fall ends;
/// only `Frozen` and `Still` hold one frame (`CALM_T`).
pub fn layer(letters: Rect, m: Motion, t: f32, clock: f32) -> FxLayer {
    let cap = letters.h;
    let p = FireParams {
        height: cap * height_share(m, t),
        scale: (cap * SCALE).max(4.0),
        embers: 0,
        intensity: INTENSITY,
        base_glow: 0.9,
        occlusion: fire::OCCLUSION,
        ..FireParams::default()
    };
    let top = letters.y + cap * (1.0 - BASE_IN);
    let shape = Shape::rounded(letters.x, top, letters.w, DEPTH, 0.0);
    let (time, moving) = match m {
        Motion::Moving | Motion::Falling | Motion::Calm => (clock.max(0.0), true),
        Motion::Frozen | Motion::Still => (CALM_T, false),
    };
    Fx::fire(p).on(shape).at(time).seed(12).reduced(!moving)
}

/// What the compositor reads from a [`slot`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotState {
    pub motion: Motion,
    /// Seconds since writing started, or stopped when falling (0 when calm).
    pub t: f32,
}

const MOVING: u32 = 1;
const STILL: u32 = 2;
const FALLING: u32 = 3;
const FROZEN: u32 = 4;

/// `effect_data` of a [`slot`]: since when an agent writes (or stopped, falling) in two f32 (high part
/// and remainder, like `fire::slot_data`) and the motion.
pub fn slot_data(m: Motion, since: f64) -> [f32; 4] {
    let hi = since as f32;
    let flags = match m {
        Motion::Calm => 0,
        Motion::Moving => MOVING,
        Motion::Still => STILL,
        Motion::Falling => FALLING,
        Motion::Frozen => FROZEN,
    };
    [hi, flags as f32, (since - f64::from(hi)) as f32, 0.0]
}

/// From the `effect_data` of a [`slot`], at Rune's time `clock`; the subtraction is in f64.
pub fn slot_state(data: [f32; 4], clock: f64) -> SlotState {
    let motion = match data[1] as u32 {
        MOVING => Motion::Moving,
        STILL => Motion::Still,
        FALLING => Motion::Falling,
        FROZEN => Motion::Frozen,
        _ => Motion::Calm,
    };
    let since = f64::from(data[0]) + f64::from(data[2]);
    let t = if matches!(motion, Motion::Calm | Motion::Frozen) { 0.0 } else { (clock - since).max(0.0) as f32 };
    SlotState { motion, t }
}

/// Makes `el` the forge fire slot. `since`: the Rune clock at which an agent started writing, or
/// stopped for `Falling`.
pub fn slot(el: El, m: Motion, since: f64) -> El {
    el.effect(Mark::Forge.id()).effect_data(slot_data(m, since))
}

/// How strongly the melt shows at its peak: the liquid chrome over the strip, `Overlay`-blended.
pub const MELT: f32 = 0.85;
/// The frame shown while melted and still (reduce motion, inactive window), rune-fx time.
pub const MELT_T: f32 = 1.7;
/// The melt's blend: the moving chrome lights and darkens the strip under it, so the letters' cut
/// and the fire stay readable (rune-fx shapes are rounded rects: the layer covers the whole strip).
pub const MELT_BLEND: FxBlend = FxBlend::Overlay;

/// How melted the wordmark is, 0 (frozen chrome) to 1 (liquid): it eases in over `fire::RISE` when an
/// agent starts writing and out in the same time when it stops, like the flames' height. Still with
/// reduce motion: melted at once while writing, frozen at once after.
pub fn melt(m: Motion, t: f32) -> f32 {
    match m {
        Motion::Calm | Motion::Frozen => 0.0,
        Motion::Still => 1.0,
        Motion::Moving => ease(t),
        Motion::Falling => 1.0 - ease(t),
    }
}

/// Whether the wordmark has a melt slot: only while an agent writes or the melt eases out. Calm, the
/// slot is not in the tree, so no backdrop copy and no layer (`ui::forge::view`).
pub fn melts(m: Motion) -> bool {
    matches!(m, Motion::Moving | Motion::Still | Motion::Falling)
}

/// The liquid chrome of the ring metal, faster and rippling more: molten.
pub fn melt_metal() -> LiquidMetalParams {
    let chrome = LiquidMetalParams::chrome();
    LiquidMetalParams { flow: 1.0, speed: 0.6, scale: 60.0, ..chrome }
}

/// The melt over the strip `plate` (pt) with corner radius `radius`; `None` when nothing shows (the
/// fall ended). Moving on Rune's `clock`, still at `MELT_T` with `Still`.
pub fn melt_layer(plate: Rect, radius: f32, m: Motion, t: f32, clock: f32) -> Option<FxLayer> {
    let k = melt(m, t);
    if k <= 0.0 {
        return None;
    }
    let shape = Shape::rounded(plate.x, plate.y, plate.w, plate.h, radius);
    let (time, moving) = match m {
        Motion::Still => (MELT_T, false),
        _ => (clock.max(0.0), true),
    };
    Some(Fx::fill(melt_metal()).on(shape).at(time).seed(12).reduced(!moving).opacity(MELT * k))
}

/// Makes `el` the melt slot over the strip, radius `radius` (pt): data as [`slot_data`], the radius in
/// the last float; the slot reads the backdrop for [`MELT_BLEND`].
pub fn melt_slot(el: El, m: Motion, since: f64, radius: f32) -> El {
    let mut d = slot_data(m, since);
    d[3] = radius;
    el.effect(super::Mark::Melt.id()).effect_data(d).effect_backdrop(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn letters() -> Rect {
        Rect::new(100.0, 50.0, 380.0, 70.0)
    }

    fn params(l: &FxLayer) -> FireParams {
        match l.fx {
            Fx::Fire(p) => p,
            other => panic!("not fire: {other:?}"),
        }
    }

    #[test]
    fn the_fire_always_moves_unless_still_is_asked() {
        assert_eq!(motion(false, false, false, true), Motion::Calm);
        assert_eq!(motion(false, false, true, false), Motion::Frozen);
        assert_eq!(motion(false, false, true, true), Motion::Frozen, "reduce motion: still when calm");
        assert_eq!(motion(false, false, false, false), Motion::Frozen, "inactive window: still when calm");
        assert_eq!(motion(true, false, false, true), Motion::Moving);
        assert_eq!(motion(true, true, false, true), Motion::Moving, "writing again after a stop");
        assert_eq!(motion(true, false, true, true), Motion::Still, "reduce motion");
        assert_eq!(motion(true, false, false, false), Motion::Still, "inactive window");
        assert_eq!(motion(false, true, false, true), Motion::Falling, "the agent stopped");
        assert_eq!(motion(false, true, true, true), Motion::Frozen, "reduce motion: straight to calm, still");
        assert_eq!(motion(false, true, false, false), Motion::Frozen, "inactive window: straight to calm, still");
    }

    /// Calm moves gently: the flames follow the clock, but on the `REST_FRAME` wake-ups, not the
    /// display link. Frozen (reduce motion, inactive window) holds one frame at the same height.
    #[test]
    fn calm_moves_low_and_paced() {
        let a = layer(letters(), Motion::Calm, 0.0, 1.0);
        let b = layer(letters(), Motion::Calm, 0.0, 9.0);
        assert!(a.is_animating() && paced(Motion::Calm, 0.0), "calm moves, at the paced rate");
        assert_ne!(a.time, b.time, "it moves with the clock");
        assert_eq!(a.shape.rect.y, b.shape.rect.y);
        assert!((params(&a).height - 0.6 * 70.0).abs() < 1e-3, "flames to 60 % of the cap height");
        let f = layer(letters(), Motion::Frozen, 0.0, 1.0);
        let g = layer(letters(), Motion::Frozen, 0.0, 9.0);
        assert!(!f.is_animating() && f.reduced_motion && !paced(Motion::Frozen, 0.0));
        assert_eq!(f.time, g.time, "frozen: same frame at any clock");
        assert_eq!(params(&f).height, params(&a).height);
        assert!(!paced(Motion::Moving, 1.0), "writing: the display link drives it");
    }

    #[test]
    fn writing_moves_and_raises_the_fire() {
        let a = layer(letters(), Motion::Moving, 0.0, 1.0);
        assert!(a.is_animating());
        assert!((params(&a).height - CALM * 70.0).abs() < 1e-3, "it starts from the calm height");
        let mid = height_share(Motion::Moving, fire::RISE / 2.0);
        assert!(mid > (CALM + WRITING) / 2.0 && mid < WRITING, "ease-out: {mid}");
        assert_eq!(height_share(Motion::Moving, 9.0), WRITING);
        const { assert!(WRITING < 1.0 - BASE_IN, "the flames stay inside the letters' height") };
    }

    #[test]
    fn reduced_motion_shows_the_risen_fire_still() {
        let a = layer(letters(), Motion::Still, 0.0, 1.0);
        let b = layer(letters(), Motion::Still, 3.0, 8.0);
        assert!(!a.is_animating() && a.reduced_motion);
        assert_eq!(a.time, b.time);
        assert!((params(&a).height - WRITING * 70.0).abs() < 1e-3);
    }

    #[test]
    fn the_base_sits_inside_the_letters_across_their_width() {
        let l = layer(letters(), Motion::Calm, 0.0, 0.0);
        let r = l.shape.rect;
        assert_eq!((r.x, r.w), (100.0, 380.0));
        let baseline = 50.0 + 70.0;
        assert!(r.y < baseline && r.y > baseline - 0.1 * 70.0, "flames start just above the baseline: {}", r.y);
        assert_eq!(params(&l).embers, 0, "no embers outside the letters' cut");
    }

    /// When the agent stops the fire eases back down in the rise's time with the display link, then
    /// moves on calm at the paced rate, on the same clock (no jump).
    #[test]
    fn stopping_eases_the_fire_down_then_freezes_it() {
        assert_eq!(height_share(Motion::Falling, 0.0), WRITING, "it starts from the writing height");
        let mid = height_share(Motion::Falling, fire::RISE / 2.0);
        assert!(mid < (CALM + WRITING) / 2.0 && mid > CALM, "ease-out: {mid}");
        assert_eq!(height_share(Motion::Falling, fire::RISE), CALM);
        let falling = layer(letters(), Motion::Falling, 0.2, 10.0);
        assert!(falling.is_animating(), "it moves while it falls");
        assert_eq!(falling.time, 10.0);
        assert!(!paced(Motion::Falling, 0.2), "the fall holds the display link");
        // Stopped at clock 10: seen 1 s after the fall ended.
        let done = layer(letters(), Motion::Falling, fire::RISE + 1.0, 10.0 + fire::RISE + 1.0);
        assert!(done.is_animating() && paced(Motion::Falling, fire::RISE + 1.0), "calm again, paced");
        assert!((done.time - (10.0 + fire::RISE + 1.0)).abs() < 1e-4, "on the clock: {}", done.time);
        assert!((params(&done).height - CALM * 70.0).abs() < 1e-3);
    }


    fn plate() -> Rect {
        Rect::new(0.0, 0.0, 440.0, 101.0)
    }

    /// Calm the wordmark is frozen chrome: no melt slot, no layer. Writing melts it in the rise's
    /// time, moving; stopping freezes it back in the same time.
    #[test]
    fn the_wordmark_melts_while_an_agent_writes() {
        assert!(!melts(Motion::Calm) && !melts(Motion::Frozen), "calm: no slot, no backdrop copy");
        assert!(melts(Motion::Moving) && melts(Motion::Still) && melts(Motion::Falling));
        assert_eq!(melt(Motion::Calm, 5.0), 0.0);
        assert_eq!(melt(Motion::Moving, 0.0), 0.0, "it starts frozen: no jump");
        let mid = melt(Motion::Moving, fire::RISE / 2.0);
        assert!(mid > 0.5 && mid < 1.0, "ease-out: {mid}");
        assert_eq!(melt(Motion::Moving, fire::RISE), 1.0);
        assert_eq!(melt(Motion::Falling, 0.0), 1.0, "the fall starts melted");
        assert_eq!(melt(Motion::Falling, fire::RISE), 0.0);
        let a = melt_layer(plate(), 14.0, Motion::Moving, 9.0, 3.0).unwrap();
        let b = melt_layer(plate(), 14.0, Motion::Moving, 9.0, 4.0).unwrap();
        assert!(a.is_animating() && a.time != b.time, "liquid: it moves with the clock");
        assert!((a.opacity - MELT).abs() < 1e-4);
        assert_eq!((a.shape.rect.w, a.shape.rect.h), (440.0, 101.0), "over the whole strip");
        match a.fx {
            Fx::Fill(f) => assert!(matches!(f.style, rune_fx::FillStyle::LiquidMetal(p) if p.flow > LiquidMetalParams::chrome().flow)),
            other => panic!("not a fill: {other:?}"),
        }
        let half = melt_layer(plate(), 14.0, Motion::Falling, fire::RISE / 2.0, 3.0).unwrap();
        assert!(half.opacity < MELT && half.opacity > 0.0, "it eases out");
        assert!(melt_layer(plate(), 14.0, Motion::Falling, fire::RISE, 3.0).is_none(), "frozen again: no layer");
        assert!(melt_layer(plate(), 14.0, Motion::Calm, 0.0, 3.0).is_none());
    }

    #[test]
    fn reduced_motion_shows_the_melt_still() {
        let a = melt_layer(plate(), 14.0, Motion::Still, 0.0, 1.0).unwrap();
        let b = melt_layer(plate(), 14.0, Motion::Still, 4.0, 9.0).unwrap();
        assert!(!a.is_animating() && a.reduced_motion);
        assert_eq!(a.time, b.time, "one frame");
        assert!((a.opacity - MELT).abs() < 1e-4, "melted at once");
    }

    #[test]
    fn slot_data_round_trips() {
        let since = 40.0 * 86_400.0 + 0.25;
        for m in [Motion::Calm, Motion::Frozen, Motion::Moving, Motion::Still, Motion::Falling] {
            let s = slot_state(slot_data(m, since), since + 0.5);
            assert_eq!(s.motion, m);
            let want = if matches!(m, Motion::Calm | Motion::Frozen) { 0.0 } else { 0.5 };
            assert!((s.t - want).abs() < 1e-3, "{m:?}: {}", s.t);
        }
        assert_eq!(slot_state(slot_data(Motion::Moving, 10.0), 9.0).t, 0.0, "never negative");
    }
}
