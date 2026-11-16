//! Fire at the bottom of the panel (DESIGN.md, "Live signals"): always on and low (`REST_H`);
//! with new text below the view it rises to `FIRE_H` in `RISE` ease-out, and goes back down in `FALL`
//! when that text enters the view.
//!
//! `layer` is a pure function of the panel rect, the phase and time. In both themes the flames
//! cover the text below them (normal blending, same occlusion): Matteo's decision,
//! `docs/decisions/2026-11-14-light-theme-fire.md`. The light theme's Multiply (DESIGN.md and
//! spec) is dropped: it left the text above the flames and the label unreadable.

use std::time::Duration;

use rune::El;
use rune_fx::{FireParams, Fx, FxLayer, Rect, Rgb, Shape, rgb};
use serde::{Deserialize, Serialize};

use super::Mark;

/// Flame height with new text below the view, pt (DESIGN.md: 110).
pub const FIRE_H: f32 = 110.0;
/// Flame height at rest, pt: always lit at the bottom of the panel, low. It stays below the
/// space at the end of the document (`doc_view::BOTTOM`, 48): with the scroll at the end they cover no text.
pub const REST_H: f32 = 26.0;
/// Rise: from `REST_H` to `FIRE_H` in 420 ms, ease-out.
pub const RISE: f32 = 0.42;
/// Descent to rest when the new text enters the view: 300 ms, ease-out.
pub const FALL: f32 = 0.30;
/// At rest the flames do not hold the display link (at 120 Hz they cost ~11 % CPU): they
/// redraw on every `REST_FRAME` wake-up (12 frames per second).
pub const REST_FRAME: Duration = Duration::from_micros(83_333);
/// Thickness of the shape below the bottom of the panel the flames start from (out of view: the
/// slot is clipped by the panel). `Top` flames rise from the shape's top edge.
const BASE: f32 = 24.0;
/// How much the dense flame covers what is behind it (0..1), the same in both themes.
pub const OCCLUSION: f32 = 0.85;
/// Intensity in the light theme (1 in dark).
const LIGHT_INTENSITY: f32 = 1.0;

/// The intensities the "Intensity" slider chooses from (multiplies `FireParams::intensity`);
/// contains 1 (today).
pub const STRENGTHS: [f32; 7] = [0.4, 0.6, 0.8, 1.0, 1.25, 1.5, 1.75];
/// The "Height" heights, as a factor of `REST_H` (and of `FIRE_H` up to `SPAN_MAX`); contains 1.
/// The top steps (feedback group 3, item 11) raise the resting flames to about a quarter of an
/// 800 pt window, where Matteo drew the line; the end of the document gets room for them
/// (`doc_bottom`).
pub const HEIGHTS: [f32; 12] = [0.5, 0.65, 0.8, 1.0, 1.2, 1.4, 1.6, 2.2, 3.0, 4.0, 5.2, 6.5];
/// Past this factor the lit fire rises no further above the resting one: at 6.5 the lit flames would
/// otherwise fill the window.
const SPAN_MAX: f32 = 1.6;

/// The resting flame height at factor `k`, pt.
pub fn rest_height(k: f32) -> f32 {
    REST_H * k
}

/// The lit flame height at factor `k`, pt: `FIRE_H × k` up to `SPAN_MAX`, then the resting height plus
/// the rise it had there.
pub fn lit_height(k: f32) -> f32 {
    rest_height(k) + (FIRE_H - REST_H) * k.min(SPAN_MAX)
}

/// The space below the last row of the document: `doc_view::BOTTOM`, or more when the resting fire is
/// taller, so at the end of the document the flames cover no text.
pub fn doc_bottom(fire: bool, k: f32) -> f32 {
    let base = crate::ui::doc_view::BOTTOM;
    if fire { base.max(rest_height(k) + 6.0) } else { base }
}
/// The "Speed" speeds, as a factor of `SPEED`; contains 1. They do not change the wake-ups at
/// rest (`REST_FRAME`), so not the CPU at rest either.
pub const SPEEDS: [f32; 7] = [0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0];
/// Today's flame speed (`FireParams::default`).
const SPEED: f32 = 1.5;

/// The flame color. `Fire` is today's (the colors of `FireParams::default`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FireColor {
    #[default]
    Fire,
    Blue,
    Violet,
    Green,
    /// The macOS accent color (System Settings › Appearance).
    Accent,
}

/// The colors in the panel's order, with their name.
pub const COLORS: [(FireColor, &str); 5] = [
    (FireColor::Fire, "Fire"),
    (FireColor::Blue, "Blue"),
    (FireColor::Violet, "Violet"),
    (FireColor::Green, "Green"),
    (FireColor::Accent, "System accent"),
];

/// The fire's look chosen in the settings. `accent`: the system accent color in
/// sRGB 0xRRGGBB (for `FireColor::Accent`). The default is today's fire.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub strength: f32,
    pub height: f32,
    pub speed: f32,
    pub color: FireColor,
    pub accent: u32,
}

impl Default for Look {
    fn default() -> Look {
        Look { strength: 1.0, height: 1.0, speed: 1.0, color: FireColor::Fire, accent: ACCENT_DEFAULT }
    }
}

/// The default macOS accent (blue), when no other is known.
pub const ACCENT_DEFAULT: u32 = 0x007aff;

/// A Rune color (linear) in sRGB 0xRRGGBB, for `Look::accent`.
pub fn accent_hex(c: rune::Color) -> u32 {
    let b = |v: f32| (rune::color::srgb_encode(v.clamp(0.0, 1.0)) * 255.0).round() as u32;
    (b(c.r) << 16) | (b(c.g) << 8) | b(c.b)
}

/// The three flame colors (low, mid, hot) of `color`. The hot one is always light:
/// the new-text label sits above it, in a dark grey.
pub fn palette(color: FireColor, accent: u32) -> [Rgb; 3] {
    match color {
        FireColor::Fire => {
            let d = FireParams::default();
            [d.low, d.mid, d.hot]
        }
        FireColor::Blue => [rgb(0x0b2a8c), rgb(0x2f7bff), rgb(0xb8e4ff)],
        FireColor::Violet => [rgb(0x4a0c8c), rgb(0xa44dff), rgb(0xead0ff)],
        FireColor::Green => [rgb(0x0b5a1c), rgb(0x2fc45e), rgb(0xd2ffbe)],
        FireColor::Accent => {
            let mid = rgb(accent & 0xff_ffff);
            let mix = |t: f32, to: f32| Rgb(mid.0.map(|c| c + (to - c) * t));
            [Rgb(mid.0.map(|c| c * 0.3)), mid, mix(0.72, 1.0)]
        }
    }
}

/// The position of `v` in `steps`, or that of 1 (the default) if it is not there.
fn step_index(steps: &[f32], v: f32) -> u32 {
    let at = steps.iter().position(|s| *s == v).or_else(|| steps.iter().position(|s| *s == 1.0));
    at.unwrap_or(0) as u32
}

/// Step `i` of `steps`; outside, 1.
fn step_at(steps: &[f32], i: u32) -> f32 {
    steps.get(i as usize).copied().unwrap_or(1.0)
}

/// Which phase the fire is in, with the seconds since it entered it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    /// No new text below the view: low, at `REST_H`.
    Rest,
    /// There is new text below: it rises to `FIRE_H` and stays there.
    Rising(f32),
    /// The new text entered the view: back to `REST_H`.
    Falling(f32),
}

/// The flame height in `phase`. `still` ("reduce motion", inactive window): straight
/// at the target height.
#[cfg(test)]
pub fn height(phase: Phase, still: bool) -> f32 {
    height_scaled(phase, still, 1.0)
}

/// [`height`] at the factor `k` ("Height"): from [`rest_height`] to [`lit_height`].
pub fn height_scaled(phase: Phase, still: bool, k: f32) -> f32 {
    let (fire, rest) = (lit_height(k), rest_height(k));
    let span = fire - rest;
    match phase {
        Phase::Rest => rest,
        Phase::Rising(_) if still => fire,
        Phase::Falling(_) if still => rest,
        Phase::Rising(t) => rest + span * ease(t, RISE),
        Phase::Falling(t) => fire - span * ease(t, FALL),
    }
}

/// Whether in `phase` the fire redraws only with the `REST_FRAME` wake-ups (rest, or descent
/// done), not with the display link.
pub fn paced(phase: Phase) -> bool {
    match phase {
        Phase::Rest => true,
        Phase::Falling(t) => t >= FALL,
        Phase::Rising(_) => false,
    }
}

/// The fire at the bottom of `panel` (pt) in `phase`; `clock` is Rune's clock, which moves the
/// flames (at rest too). With `reduced` it is still at the target height; with the window not
/// active (`!active_window`) the same. Still means `!is_animating()`: the compositor
/// does not hold the display link. `look`: intensity, height, speed and color from the settings.
pub fn layer(panel: Rect, phase: Phase, clock: f32, dark: bool, reduced: bool, active_window: bool, look: Look) -> FxLayer {
    let still = reduced || !active_window;
    let t = if still { 0.0 } else { clock.max(0.0) };
    let [low, mid, hot] = palette(look.color, look.accent);
    let fire = FireParams {
        height: height_scaled(phase, still, look.height),
        scale: 22.0,
        embers: 18,
        ember_rise: 64.0,
        speed: SPEED * look.speed,
        low,
        mid,
        hot,
        intensity: look.strength * if dark { 1.0 } else { LIGHT_INTENSITY },
        // The dense flame covers the text behind, in both themes: the label above stays readable.
        occlusion: OCCLUSION,
        ..FireParams::default()
    };
    let shape = Shape::rounded(panel.x, panel.y + panel.h, panel.w, BASE, 0.0);
    Fx::fire(fire).on(shape).at(t).seed(12).reduced(still)
}

/// Makes `el` the fire slot: lit (new text below) since time `lit` of Rune's
/// clock, at rest with `None`. The compositor paints it with [`layer`] on the rect of `el`, which must
/// touch the bottom of the panel and have its width.
pub fn slot(el: El, lit: Option<f64>, dark: bool, reduced: bool, active_window: bool, look: Look) -> El {
    el.effect(Mark::Fire.id()).effect_data(slot_data(lit, dark, reduced, active_window, look))
}

/// What the compositor reads from a [`slot`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotState {
    /// There is new text below the view.
    pub lit: bool,
    /// Seconds since the fire lit up (0 at rest).
    pub t: f32,
    pub dark: bool,
    pub reduced: bool,
    pub active_window: bool,
    pub look: Look,
}

const REDUCED: u32 = 1;
const INACTIVE: u32 = 2;
const LIGHT: u32 = 4;
const LIT: u32 = 8;

/// Where the look's steps sit in the flags (4 bits each, above the 4 flag bits): an integer
/// below 2^24, exact in an f32.
const STRENGTH_AT: u32 = 4;
const HEIGHT_AT: u32 = 8;
const SPEED_AT: u32 = 12;
const COLOR_AT: u32 = 16;

/// `effect_data` of a [`slot`]: `lit` in two f32 (high part and remainder, like `ring::slot_data`),
/// the flags with the look's steps, and the accent (0xRRGGBB, exact in an f32).
pub fn slot_data(lit: Option<f64>, dark: bool, reduced: bool, active_window: bool, look: Look) -> [f32; 4] {
    let since = lit.unwrap_or(0.0);
    let hi = since as f32;
    let color = COLORS.iter().position(|(c, _)| *c == look.color).unwrap_or(0) as u32;
    let flags = (u32::from(reduced) * REDUCED)
        | (u32::from(!active_window) * INACTIVE)
        | (u32::from(!dark) * LIGHT)
        | (u32::from(lit.is_some()) * LIT)
        | (step_index(&STRENGTHS, look.strength) << STRENGTH_AT)
        | (step_index(&HEIGHTS, look.height) << HEIGHT_AT)
        | (step_index(&SPEEDS, look.speed) << SPEED_AT)
        | (color << COLOR_AT);
    [hi, flags as f32, (since - f64::from(hi)) as f32, (look.accent & 0xff_ffff) as f32]
}

/// From the `effect_data` of a [`slot`], at Rune's time `clock`. The subtraction is in f64.
pub fn slot_state(data: [f32; 4], clock: f64) -> SlotState {
    let since = f64::from(data[0]) + f64::from(data[2]);
    let flags = data[1] as u32;
    let lit = flags & LIT != 0;
    SlotState {
        lit,
        t: if lit { (clock - since).max(0.0) as f32 } else { 0.0 },
        dark: flags & LIGHT == 0,
        reduced: flags & REDUCED != 0,
        active_window: flags & INACTIVE == 0,
        look: Look {
            strength: step_at(&STRENGTHS, (flags >> STRENGTH_AT) & 0xf),
            height: step_at(&HEIGHTS, (flags >> HEIGHT_AT) & 0xf),
            speed: step_at(&SPEEDS, (flags >> SPEED_AT) & 0xf),
            color: COLORS.get(((flags >> COLOR_AT) & 0xf) as usize).map_or(FireColor::Fire, |(c, _)| *c),
            accent: data[3] as u32,
        },
    }
}

/// Ease-out cubica da 0 a 1 in `span` secondi.
fn ease(t: f32, span: f32) -> f32 {
    let x = (t.max(0.0) / span).clamp(0.0, 1.0);
    1.0 - (1.0 - x).powi(3)
}

#[cfg(test)]
mod tests {
    use rune_fx::{FireStyle, Fx};

    use super::*;

    fn panel() -> Rect {
        Rect::new(8.0, 8.0, 884.0, 984.0)
    }

    fn fire(l: &FxLayer) -> FireParams {
        match l.fx {
            Fx::Fire(p) => p,
            other => panic!("not fire: {other:?}"),
        }
    }

    #[test]
    fn fire_rises_from_the_panel_bottom_across_its_width() {
        let l = layer(panel(), Phase::Rising(1.0), 1.0, true, false, true, Look::default());
        assert_eq!(fire(&l).style, FireStyle::Top, "flames from the shape's top edge");
        let r = l.shape.rect;
        assert_eq!((r.x, r.y, r.w), (8.0, 8.0 + 984.0, 884.0), "the shape starts at the bottom of the panel");
        assert_eq!(fire(&l).height, FIRE_H);
        assert!(l.is_animating());
    }

    #[test]
    fn fire_rises_over_420_ms_ease_out() {
        let h = |t| fire(&layer(panel(), Phase::Rising(t), t, true, false, true, Look::default())).height;
        assert_eq!(h(0.0), REST_H);
        let mid = h(RISE / 2.0);
        assert!(mid > FIRE_H / 2.0 && mid < FIRE_H, "ease-out: past half at half time ({mid})");
        assert_eq!(h(RISE), FIRE_H);
        assert_eq!(h(9.0), FIRE_H);
    }

    #[test]
    fn reduced_motion_shows_it_without_rising_and_still() {
        let a = layer(panel(), Phase::Rising(0.0), 0.0, true, true, true, Look::default());
        let b = layer(panel(), Phase::Rising(5.0), 5.0, true, true, true, Look::default());
        assert_eq!(fire(&a).height, FIRE_H, "at full height at once");
        assert!(a.reduced_motion && !a.is_animating());
        assert_eq!(a.time, b.time, "still");
    }

    #[test]
    fn an_inactive_window_keeps_it_still_and_visible() {
        let l = layer(panel(), Phase::Rising(0.1), 0.1, true, false, false, Look::default());
        assert_eq!(fire(&l).height, FIRE_H);
        assert!(l.opacity > 0.99 && !l.is_animating());
    }

    /// In the light theme the flames cover the text as in dark (decision 2026-11-14).
    #[test]
    fn light_theme_covers_the_text_like_dark() {
        let (d, l) = (fire(&layer(panel(), Phase::Rising(1.0), 1.0, true, false, true, Look::default())), fire(&layer(panel(), Phase::Rising(1.0), 1.0, false, false, true, Look::default())));
        assert_eq!(l.occlusion, d.occlusion);
        assert_eq!(l.occlusion, OCCLUSION);
    }

    #[test]
    fn slot_data_round_trips() {
        let since = 30.0 * 86_400.0 + 0.123_456;
        for (dark, reduced, active) in [(true, false, true), (false, true, false), (false, false, true)] {
            let s = slot_state(slot_data(Some(since), dark, reduced, active, Look::default()), since + 0.2);
            assert_eq!((s.dark, s.reduced, s.active_window), (dark, reduced, active));
            assert!((s.t - 0.2).abs() < 1e-3, "{}", s.t);
        }
        assert_eq!(slot_state(slot_data(Some(10.0), true, false, true, Look::default()), 9.0).t, 0.0, "never negative");
    }

    // --- fire always on (v1.1) ---

    #[test]
    fn at_rest_it_stays_low_at_the_panel_bottom() {
        assert_eq!(height(Phase::Rest, false), REST_H);
        const { assert!(REST_H > 0.0 && REST_H < 40.0, "low: below the margin at the end of the document") };
        let l = layer(panel(), Phase::Rest, 3.0, true, false, true, Look::default());
        assert_eq!(fire(&l).height, REST_H);
        assert!(l.is_animating(), "at rest it still moves (at a few frames per second)");
    }

    #[test]
    fn new_text_raises_it_from_rest_to_full_height() {
        assert_eq!(height(Phase::Rising(0.0), false), REST_H);
        let mid = height(Phase::Rising(RISE / 2.0), false);
        assert!(mid > (REST_H + FIRE_H) / 2.0 && mid < FIRE_H, "ease-out: {mid}");
        assert_eq!(height(Phase::Rising(RISE), false), FIRE_H);
        assert_eq!(height(Phase::Rising(9.0), false), FIRE_H);
    }

    #[test]
    fn when_the_text_is_read_it_falls_back_to_rest() {
        assert_eq!(height(Phase::Falling(0.0), false), FIRE_H);
        let mid = height(Phase::Falling(FALL / 2.0), false);
        assert!(mid < (REST_H + FIRE_H) / 2.0 && mid > REST_H, "ease-out: {mid}");
        assert_eq!(height(Phase::Falling(FALL), false), REST_H);
    }

    #[test]
    fn still_fire_jumps_to_its_height_without_moving() {
        assert_eq!(height(Phase::Rising(0.0), true), FIRE_H);
        assert_eq!(height(Phase::Falling(0.0), true), REST_H);
        for (reduced, active) in [(true, true), (false, false)] {
            let a = layer(panel(), Phase::Rest, 1.0, false, reduced, active, Look::default());
            let b = layer(panel(), Phase::Rest, 7.0, false, reduced, active, Look::default());
            assert!(!a.is_animating() && a.time == b.time, "still");
        }
    }

    /// At rest the fire does not hold the display link: the `REST_FRAME` wake-ups redraw it.
    #[test]
    fn only_the_rest_is_paced_by_wakes() {
        assert!(paced(Phase::Rest));
        assert!(!paced(Phase::Rising(0.1)) && !paced(Phase::Falling(0.1)));
        assert!(paced(Phase::Falling(FALL + 0.01)), "descent done = rest");
        assert!(REST_FRAME >= Duration::from_secs(1) / 15, "at most 15 frames per second at rest");
    }

    #[test]
    fn slot_data_says_whether_it_is_lit() {
        for lit in [true, false] {
            let s = slot_state(slot_data(lit.then_some(4.0), true, false, true, Look::default()), 5.0);
            assert_eq!(s.lit, lit);
        }
    }

    // --- fire settings ---

    /// The default look is today's fire, parameter by parameter.
    #[test]
    fn the_default_look_is_todays_fire() {
        let p = fire(&layer(panel(), Phase::Rising(9.0), 9.0, true, false, true, Look::default()));
        let d = FireParams::default();
        assert_eq!((p.low, p.mid, p.hot), (d.low, d.mid, d.hot));
        assert_eq!((p.height, p.speed, p.intensity), (FIRE_H, 1.5, 1.0));
        assert_eq!((p.scale, p.embers, p.ember_rise, p.occlusion), (22.0, 18, 64.0, OCCLUSION));
        for steps in [&STRENGTHS[..], &HEIGHTS, &SPEEDS] {
            assert!(steps.contains(&1.0), "the default is a step");
            assert!(steps.windows(2).all(|w| w[0] < w[1]), "in order");
            assert!(steps.len() <= 16, "fits in 4 bits");
        }
    }

    #[test]
    fn strength_height_and_speed_scale_the_flames() {
        let look = Look { strength: 1.5, height: 1.4, speed: 2.0, ..Look::default() };
        let p = fire(&layer(panel(), Phase::Rising(9.0), 9.0, true, false, true, look));
        assert_eq!(p.intensity, 1.5);
        assert!((p.height - FIRE_H * 1.4).abs() < 1e-3);
        assert_eq!(p.speed, 3.0);
        let rest = fire(&layer(panel(), Phase::Rest, 1.0, true, false, true, look)).height;
        assert!((rest - REST_H * 1.4).abs() < 1e-3, "the rest too");
    }

    /// Every resting height stays below the space at the end of the document (which grows with it): with
    /// the scroll at the end the flames cover no text. Up to 1.6 that space is the usual 48.
    #[test]
    fn the_rest_stays_below_the_document_bottom() {
        for k in HEIGHTS {
            assert!(height_scaled(Phase::Rest, false, k) < doc_bottom(true, k), "{k}");
        }
        assert_eq!(doc_bottom(true, 1.6), crate::ui::doc_view::BOTTOM);
        assert_eq!(doc_bottom(false, 6.5), crate::ui::doc_view::BOTTOM, "no fire, no extra room");
    }

    /// Feedback group 3, item 11: the top step raises the resting flames to about a quarter of an 800 pt
    /// window (Matteo's line, 22–25 % above the bottom edge); the lit fire rises above it by at most the
    /// old top rise, and the steps up to 1.6 are as before.
    #[test]
    fn the_top_height_reaches_a_quarter_of_the_window() {
        let top = *HEIGHTS.last().unwrap();
        let rest = rest_height(top);
        assert!((150.0..=200.0).contains(&rest), "rest at the top step: {rest}");
        assert!(lit_height(top) - rest <= (FIRE_H - REST_H) * 1.6 + 1e-3);
        assert!(lit_height(top) < 340.0, "the lit fire does not fill the window: {}", lit_height(top));
        for k in [0.5, 1.0, 1.6] {
            assert!((lit_height(k) - FIRE_H * k).abs() < 1e-3, "{k}: as before");
        }
        assert!(HEIGHTS.windows(2).all(|w| w[0] < w[1]));
        assert!(HEIGHTS.len() <= 16, "the step index fits the slot's 4 bits");
    }

    #[test]
    fn the_look_round_trips_through_the_slot() {
        for (i, (color, _)) in COLORS.iter().enumerate() {
            let look = Look { strength: STRENGTHS[i % STRENGTHS.len()], height: HEIGHTS[(i + 2) % HEIGHTS.len()], speed: SPEEDS[(i + 4) % SPEEDS.len()], color: *color, accent: 0xff2d55 };
            let s = slot_state(slot_data(Some(3.0), false, true, false, look), 4.0);
            assert_eq!(s.look, look);
            assert_eq!((s.dark, s.reduced, s.active_window, s.lit), (false, true, false, true));
        }
        // A value that is not a step arrives as the default.
        let odd = Look { strength: 0.33, height: 9.0, speed: -1.0, ..Look::default() };
        assert_eq!(slot_state(slot_data(None, true, false, true, odd), 0.0).look, Look::default());
    }

    /// In every color the hot part is light: the dark label above stays readable (≥ 4.5:1
    /// with the lighter grey of the two inks, dark `panel` and light `fg`).
    #[test]
    fn every_color_keeps_a_light_hot_base_for_the_label() {
        let ink = [crate::ui::theme::Theme::for_mode(true).panel, crate::ui::theme::Theme::for_mode(false).fg];
        for accent in [ACCENT_DEFAULT, 0xff2d55, 0x8e8e93, 0xffcc00] {
            for (color, name) in COLORS {
                let [_, _, hot] = palette(color, accent);
                let hot = rune::Color { r: hot.0[0], g: hot.0[1], b: hot.0[2], a: 1.0 };
                for ink in ink {
                    let r = crate::ui::contrast::ratio(ink, hot);
                    assert!(r >= 4.5, "{name} (accent {accent:06x}): {r:.2}");
                }
            }
        }
    }
}
