//! "Is writing" caret (record docs/decisions/2026-11-15-caret-at-the-edit-point.md): a
//! [`WIDTH`] pt bar in `fg` at the point of the last edit (`live::caret`), which pulses in
//! `STEPS` steps over `PERIOD` by changing opacity. It replaces the dithered line at the bottom.
//!
//! `layer` is a pure function of the caret rect and of time. The fill is a dither with the
//! two stops equal (a solid color: the same pipeline that was there for the dither); the beat is
//! the layer's opacity. Rune-fx does not count a fill that stays still within a step as "moving":
//! the compositor keeps the display link with [`SlotState::moving`] while it pulses.

use rune::El;
use rune_fx::{DitherParams, DitherPattern, FillInput, Fx, FxLayer, Rect, Rgb, Shape, Stops};

use super::Mark;
use crate::ui::theme::Theme;

/// Caret width, pt (mockup "caret": 2 px).
pub const WIDTH: f32 = 2.0;
/// One beat cycle, s (like the earlier dither: 4 steps in 1.6 s).
pub const PERIOD: f32 = 1.6;
pub const STEPS: usize = 4;
/// The opacity at each step: down and back up (record stronger-caret, constraint 2: from 100 to about
/// 50 %). The minimum is 62 %, not 50: at 50 % the caret drops to 3.32:1 in light and to 4.30:1 in
/// dark on `sel`, and 62 % is the first that keeps ≥ 4.5:1 in both themes (constraint 3, DESIGN.md). The lightest step keeps contrast ≥ 4.5:1
/// (test in `ui::contrast`).
pub const OPACITY: [f32; STEPS] = [1.0, 0.81, 0.62, 0.81];
/// The still step with "reduce motion": full.
const STILL_STEP: usize = 0;

/// The caret color in theme `t`: `fg`, the text color (record
/// docs/decisions/2026-11-15-stronger-caret.md, constraint 1).
pub fn color(t: &Theme) -> rune::Color {
    t.fg
}

/// The faintest caret of the beat, for the contrast test.
#[cfg(test)]
pub fn dimmest(t: &Theme) -> rune::Color {
    let c = color(t);
    let min = OPACITY.iter().copied().fold(1.0f32, f32::min);
    c.alpha(c.a * min)
}

/// The beat step at `t` seconds; still with `reduced`.
pub fn step(t: f32, reduced: bool) -> usize {
    if reduced {
        return STILL_STEP;
    }
    ((t.max(0.0) / (PERIOD / STEPS as f32)) as usize) % STEPS
}

/// The caret on `rect` (pt), `t` seconds after it lit up.
pub fn layer(rect: Rect, t: f32, dark: bool, reduced: bool) -> FxLayer {
    let c = color(&Theme::for_mode(dark));
    let rgb = Rgb([c.r, c.g, c.b]);
    let fill = DitherParams { pattern: DitherPattern::Bayer4, input: FillInput::Linear, colors: Stops::new(&[rgb, rgb]), contrast: 0.0, ..DitherParams::bayer() };
    let shape = Shape::rounded(rect.x, rect.y, rect.w, rect.h, rect.w * 0.5);
    let t = if reduced { 0.0 } else { t.max(0.0) };
    Fx::fill(fill).on(shape).at(t).reduced(reduced).opacity(OPACITY[step(t, reduced)])
}

/// Makes `el` the caret slot, lit at time `since` of Rune's clock.
pub fn slot(el: El, since: f64, dark: bool, reduced: bool) -> El {
    el.effect(Mark::Caret.id()).effect_data(slot_data(since, dark, reduced))
}

/// What the compositor reads from a [`slot`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotState {
    /// Seconds since the caret lit up.
    pub t: f32,
    pub dark: bool,
    pub reduced: bool,
}

impl SlotState {
    /// The caret pulses: the compositor must ask for frames. Still with "reduce motion".
    pub fn moving(&self) -> bool {
        !self.reduced
    }
}

const REDUCED: u32 = 1;
const LIGHT: u32 = 4;

/// `effect_data` of a [`slot`]: `since` in two f32 (like `fire::slot_data`) and the flags.
pub fn slot_data(since: f64, dark: bool, reduced: bool) -> [f32; 4] {
    let hi = since as f32;
    let flags = (u32::from(reduced) * REDUCED) | (u32::from(!dark) * LIGHT);
    [hi, flags as f32, (since - f64::from(hi)) as f32, 0.0]
}

/// From the `effect_data` of a [`slot`], at Rune's time `clock`. The subtraction is in f64.
pub fn slot_state(data: [f32; 4], clock: f64) -> SlotState {
    let since = f64::from(data[0]) + f64::from(data[2]);
    let flags = data[1] as u32;
    SlotState { t: (clock - since).max(0.0) as f32, dark: flags & LIGHT == 0, reduced: flags & REDUCED != 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caret() -> Rect {
        Rect::new(300.0, 400.0, WIDTH, 18.0)
    }

    #[test]
    fn a_2pt_bar_in_fg_on_the_slot_rect() {
        let l = layer(caret(), 0.0, true, false);
        let r = l.shape.rect;
        assert_eq!((r.x, r.y, r.w, r.h), (300.0, 400.0, 2.0, 18.0));
        assert_eq!(color(&Theme::for_mode(true)), Theme::for_mode(true).fg);
        assert_eq!(color(&Theme::for_mode(false)), Theme::for_mode(false).fg);
    }

    #[test]
    fn it_pulses_in_4_steps_over_1_6_s() {
        let steps: Vec<usize> = [0.0, 0.41, 0.81, 1.21, 1.61].iter().map(|t| step(*t, false)).collect();
        assert_eq!(steps, [0, 1, 2, 3, 0]);
        let o: Vec<f32> = (0..STEPS).map(|k| layer(caret(), k as f32 * 0.4 + 0.01, true, false).opacity).collect();
        assert_eq!(o, OPACITY);
    }

    #[test]
    fn reduced_motion_is_still_and_full() {
        let (a, b) = (layer(caret(), 0.0, true, true), layer(caret(), 0.9, true, true));
        assert_eq!((a.opacity, b.opacity), (1.0, 1.0));
        assert!(a.reduced_motion && !a.is_animating());
        assert!(!slot_state(slot_data(1.0, true, true), 2.0).moving());
        assert!(slot_state(slot_data(1.0, true, false), 2.0).moving());
    }

    #[test]
    fn slot_data_round_trips() {
        let since = 30.0 * 86_400.0 + 0.123_456;
        for (dark, reduced) in [(true, false), (false, true)] {
            let s = slot_state(slot_data(since, dark, reduced), since + 0.5);
            assert_eq!((s.dark, s.reduced), (dark, reduced));
            assert!((s.t - 0.5).abs() < 1e-3, "{}", s.t);
        }
    }
}
