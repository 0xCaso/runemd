//! Caret "sta scrivendo" (record docs/decisions/2026-11-15-caret-al-punto-modificato.md): una
//! barra di [`WIDTH`] pt in `fg` nel punto dell'ultima modifica (`live::caret`), che pulsa a
//! `STEPS` passi in `PERIOD` cambiando opacità. Prende il posto della riga a dither in fondo.
//!
//! `layer` è una funzione pura del rect del caret e del tempo. Il riempimento è un dither con i
//! due stop uguali (un colore pieno: lo stesso pipeline che c'era per il dither); il battito è
//! l'opacità del layer. Rune-fx non conta "in movimento" un riempimento fermo dentro un passo:
//! il compositor tiene il display link con [`SlotState::moving`] finché pulsa.

use rune::El;
use rune_fx::{DitherParams, DitherPattern, FillInput, Fx, FxLayer, Rect, Rgb, Shape, Stops};

use super::Mark;
use crate::ui::theme::Theme;

/// Larghezza del caret, pt (mockup "caret": 2 px).
pub const WIDTH: f32 = 2.0;
/// Un ciclo del battito, s (come il dither di prima: 4 passi in 1,6 s).
pub const PERIOD: f32 = 1.6;
pub const STEPS: usize = 4;
/// L'opacità a ogni passo: scende e risale (record caret-piu-forte, vincolo 2: da 100 a circa
/// 50 %). Il minimo è 62 %, non 50: al 50 % il caret scende a 3,32:1 nel chiaro e a 4,30:1 nello
/// scuro su `sel`, e il 62 % è il primo che tiene ≥ 4,5:1 in tutti e due i temi (vincolo 3, DESIGN.md). Il passo più chiaro tiene il contrasto ≥ 4,5:1
/// (test in `ui::contrast`).
pub const OPACITY: [f32; STEPS] = [1.0, 0.81, 0.62, 0.81];
/// Il passo fermo con "riduci movimento": pieno.
const STILL_STEP: usize = 0;

/// Il colore del caret nel tema `t`: `fg`, il colore del testo (record
/// docs/decisions/2026-11-15-caret-piu-forte.md, vincolo 1).
pub fn color(t: &Theme) -> rune::Color {
    t.fg
}

/// Il caret più tenue del battito, per il test di contrasto.
#[cfg(test)]
pub fn dimmest(t: &Theme) -> rune::Color {
    let c = color(t);
    let min = OPACITY.iter().copied().fold(1.0f32, f32::min);
    c.alpha(c.a * min)
}

/// Il passo del battito a `t` secondi; fermo con `reduced`.
pub fn step(t: f32, reduced: bool) -> usize {
    if reduced {
        return STILL_STEP;
    }
    ((t.max(0.0) / (PERIOD / STEPS as f32)) as usize) % STEPS
}

/// Il caret su `rect` (pt), `t` secondi dopo che si è acceso.
pub fn layer(rect: Rect, t: f32, dark: bool, reduced: bool) -> FxLayer {
    let c = color(&Theme::for_mode(dark));
    let rgb = Rgb([c.r, c.g, c.b]);
    let fill = DitherParams { pattern: DitherPattern::Bayer4, input: FillInput::Linear, colors: Stops::new(&[rgb, rgb]), contrast: 0.0, ..DitherParams::bayer() };
    let shape = Shape::rounded(rect.x, rect.y, rect.w, rect.h, rect.w * 0.5);
    let t = if reduced { 0.0 } else { t.max(0.0) };
    Fx::fill(fill).on(shape).at(t).reduced(reduced).opacity(OPACITY[step(t, reduced)])
}

/// Fa di `el` lo slot del caret, acceso al tempo `since` dell'orologio di Rune.
pub fn slot(el: El, since: f64, dark: bool, reduced: bool) -> El {
    el.effect(Mark::Caret.id()).effect_data(slot_data(since, dark, reduced))
}

/// Quanto legge il compositor da uno [`slot`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotState {
    /// Secondi da quando il caret è acceso.
    pub t: f32,
    pub dark: bool,
    pub reduced: bool,
}

impl SlotState {
    /// Il caret pulsa: il compositor deve chiedere frame. Fermo con "riduci movimento".
    pub fn moving(&self) -> bool {
        !self.reduced
    }
}

const REDUCED: u32 = 1;
const LIGHT: u32 = 4;

/// `effect_data` di uno [`slot`]: `since` in due f32 (come `fire::slot_data`) e i flag.
pub fn slot_data(since: f64, dark: bool, reduced: bool) -> [f32; 4] {
    let hi = since as f32;
    let flags = (u32::from(reduced) * REDUCED) | (u32::from(!dark) * LIGHT);
    [hi, flags as f32, (since - f64::from(hi)) as f32, 0.0]
}

/// Da `effect_data` di uno [`slot`], al tempo `clock` di Rune. La sottrazione è in f64.
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
