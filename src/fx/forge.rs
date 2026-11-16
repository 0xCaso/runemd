//! forge-empty: the Rune fire inside the letters of the "RuneMD" wordmark on the screen with no file
//! open (docs/decisions/2026-11-16-wordmark-plate.md, 2026-11-16-metal-and-rune-effects.md).
//!
//! The slot is the letters' box from the cap height to the baseline. rune-fx shapes are rounded
//! rects only, so the fire is not clipped to the glyphs here: `ui::forge` draws the dark letters
//! under the slot and the chrome strip with the letters cut out over it, and the fire shows only
//! through the cut. Calm (no agent writing): flames at `CALM` of the cap height, frozen on one frame.
//! While an agent writes: they move and rise to `WRITING` in `fire::RISE`. With "reduce motion" or
//! the window inactive they stay still at the writing height.

use rune::El;
use rune_fx::{FireParams, Fx, FxLayer, Rect, Shape};

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
    /// No agent writes: low and frozen.
    Calm,
    /// An agent writes: it rises and moves.
    Moving,
    /// An agent writes, but "reduce motion" is on or the window is inactive: risen, still.
    Still,
}

/// The fire's motion: it moves only while an agent writes, in an active window, without "reduce motion".
pub fn motion(writing: bool, reduced: bool, active_window: bool) -> Motion {
    match (writing, reduced || !active_window) {
        (false, _) => Motion::Calm,
        (true, false) => Motion::Moving,
        (true, true) => Motion::Still,
    }
}

/// Flame height as a share of the cap height, `t` seconds after writing started.
pub fn height_share(m: Motion, t: f32) -> f32 {
    match m {
        Motion::Calm => CALM,
        Motion::Still => WRITING,
        Motion::Moving => {
            let x = (t.max(0.0) / fire::RISE).clamp(0.0, 1.0);
            CALM + (WRITING - CALM) * (1.0 - (1.0 - x).powi(3))
        }
    }
}

/// The fire for a slot whose rect `letters` (pt) spans the letters from the cap height (top) to the
/// baseline (bottom). `t`: seconds since writing started; `clock`: Rune's clock, which moves the flames.
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
    let moving = m == Motion::Moving;
    Fx::fire(p).on(shape).at(if moving { clock.max(0.0) } else { CALM_T }).seed(12).reduced(!moving)
}

/// What the compositor reads from a [`slot`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotState {
    pub motion: Motion,
    /// Seconds since writing started (0 when calm).
    pub t: f32,
}

const MOVING: u32 = 1;
const STILL: u32 = 2;

/// `effect_data` of a [`slot`]: since when an agent writes in two f32 (high part and remainder, like
/// `fire::slot_data`) and the motion.
pub fn slot_data(m: Motion, since: f64) -> [f32; 4] {
    let hi = since as f32;
    let flags = match m {
        Motion::Calm => 0,
        Motion::Moving => MOVING,
        Motion::Still => STILL,
    };
    [hi, flags as f32, (since - f64::from(hi)) as f32, 0.0]
}

/// From the `effect_data` of a [`slot`], at Rune's time `clock`; the subtraction is in f64.
pub fn slot_state(data: [f32; 4], clock: f64) -> SlotState {
    let motion = match data[1] as u32 {
        MOVING => Motion::Moving,
        STILL => Motion::Still,
        _ => Motion::Calm,
    };
    let since = f64::from(data[0]) + f64::from(data[2]);
    let t = if motion == Motion::Calm { 0.0 } else { (clock - since).max(0.0) as f32 };
    SlotState { motion, t }
}

/// Makes `el` the forge fire slot. `since`: the Rune clock at which an agent started writing.
pub fn slot(el: El, m: Motion, since: f64) -> El {
    el.effect(Mark::Forge.id()).effect_data(slot_data(m, since))
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
    fn the_fire_moves_only_while_an_agent_writes() {
        assert_eq!(motion(false, false, true), Motion::Calm);
        assert_eq!(motion(false, true, false), Motion::Calm);
        assert_eq!(motion(true, false, true), Motion::Moving);
        assert_eq!(motion(true, true, true), Motion::Still, "reduce motion");
        assert_eq!(motion(true, false, false), Motion::Still, "inactive window");
    }

    #[test]
    fn calm_is_frozen_and_low() {
        let a = layer(letters(), Motion::Calm, 0.0, 1.0);
        let b = layer(letters(), Motion::Calm, 0.0, 9.0);
        assert!(!a.is_animating(), "calm holds no display link");
        assert_eq!((a.time, a.shape.rect.y), (b.time, b.shape.rect.y), "same frame at any clock");
        assert!((params(&a).height - 0.6 * 70.0).abs() < 1e-3, "flames to 60 % of the cap height");
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

    #[test]
    fn slot_data_round_trips() {
        let since = 40.0 * 86_400.0 + 0.25;
        for m in [Motion::Calm, Motion::Moving, Motion::Still] {
            let s = slot_state(slot_data(m, since), since + 0.5);
            assert_eq!(s.motion, m);
            let want = if m == Motion::Calm { 0.0 } else { 0.5 };
            assert!((s.t - want).abs() < 1e-3, "{m:?}: {}", s.t);
        }
        assert_eq!(slot_state(slot_data(Motion::Moving, 10.0), 9.0).t, 0.0, "never negative");
    }
}
