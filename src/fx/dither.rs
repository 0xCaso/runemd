//! Dither "sta arrivando" (DESIGN.md, "Segnali live"): una barra Bayer 4×4 alta una riga e
//! larga come una riga di testo, in fondo al documento, che pulsa a `STEPS` passi in `PERIOD`.
//!
//! `layer` è una funzione pura del rect della barra e del tempo. Il tono è uniforme (nessuna
//! rampa): ogni passo cambia solo quanti punti accesi ha la matrice. Nel tema scuro i punti sono
//! chiari sul pannello; nel chiaro il dither è invertito, punti scuri sul pannello.
//!
//! Rune-fx non considera "in movimento" un dither a rampa: il compositor tiene il display link
//! con [`SlotState::moving`] finché la barra pulsa.

use rune::El;
use rune_fx::{DitherParams, DitherPattern, FillInput, Fx, FxLayer, Rect, Rgb, Shape, Stops};

use super::Mark;
use crate::ui::theme::Theme;

/// Un ciclo del battito, s (DESIGN.md: 4 passi in 1,6 s).
pub const PERIOD: f32 = 1.6;
pub const STEPS: usize = 4;
/// La parte di punti accesi a ogni passo, su 16 (una matrice 4×4): sale e scende.
const DENSITY: [f32; STEPS] = [3.0 / 16.0, 5.0 / 16.0, 7.0 / 16.0, 5.0 / 16.0];
/// Il passo fermo con "riduci movimento".
const STILL_STEP: usize = 1;
/// Lato di un punto del dither, pt.
const PIXEL: f32 = 2.0;

/// Il passo del battito a `t` secondi; fermo con `reduced`.
pub fn step(t: f32, reduced: bool) -> usize {
    if reduced {
        return STILL_STEP;
    }
    ((t.max(0.0) / (PERIOD / STEPS as f32)) as usize) % STEPS
}

/// La barra su `rect` (pt), `t` secondi dopo che si è accesa.
pub fn layer(rect: Rect, t: f32, dark: bool, reduced: bool) -> FxLayer {
    let theme = Theme::for_mode(dark);
    let rgb = |c: rune::Color| Rgb([c.r, c.g, c.b]);
    // Stops dal scuro al chiaro; il tono è la parte di punti chiari.
    let (colors, tone) = if dark {
        ([rgb(theme.panel), rgb(theme.fg3)], density(t, reduced))
    } else {
        ([rgb(theme.fg3), rgb(theme.panel)], 1.0 - density(t, reduced))
    };
    let dither = DitherParams {
        pattern: DitherPattern::Bayer4,
        input: FillInput::Linear,
        colors: Stops::new(&colors),
        pixel: PIXEL,
        // Contrasto 0: il tono è 0,5 + bias / 2 ovunque (shader `tone_at`).
        contrast: 0.0,
        bias: 2.0 * tone - 1.0,
        ..DitherParams::bayer()
    };
    let shape = Shape::rounded(rect.x, rect.y, rect.w, rect.h, 0.0);
    let t = if reduced { 0.0 } else { t.max(0.0) };
    Fx::fill(dither).on(shape).at(t).reduced(reduced)
}

/// La parte di punti accesi (di inchiostro) al passo di `t`.
pub fn density(t: f32, reduced: bool) -> f32 {
    DENSITY[step(t, reduced)]
}

/// Fa di `el` lo slot del dither, acceso al tempo `since` dell'orologio di Rune.
pub fn slot(el: El, since: f64, dark: bool, reduced: bool) -> El {
    el.effect(Mark::Dither.id()).effect_data(slot_data(since, dark, reduced))
}

/// Quanto legge il compositor da uno [`slot`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotState {
    /// Secondi da quando il dither è acceso.
    pub t: f32,
    pub dark: bool,
    pub reduced: bool,
}

impl SlotState {
    /// La barra pulsa: il compositor deve chiedere frame. Ferma con "riduci movimento".
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
    use rune_fx::{FillParams, FillStyle};

    use super::*;

    fn bar() -> Rect {
        Rect::new(118.0, 400.0, 624.0, 25.0)
    }

    fn dither(l: &FxLayer) -> DitherParams {
        match l.fx {
            Fx::Fill(FillParams { style: FillStyle::Dither(d), .. }) => d,
            other => panic!("non è un dither: {other:?}"),
        }
    }

    #[test]
    fn a_bayer_4x4_bar_on_the_slot_rect() {
        let l = layer(bar(), 0.0, true, false);
        let d = dither(&l);
        assert_eq!(d.pattern, DitherPattern::Bayer4);
        let r = l.shape.rect;
        assert_eq!((r.x, r.y, r.w, r.h), (118.0, 400.0, 624.0, 25.0));
    }

    #[test]
    fn it_pulses_in_4_steps_over_1_6_s() {
        let steps: Vec<usize> = [0.0, 0.41, 0.81, 1.21, 1.61].iter().map(|t| step(*t, false)).collect();
        assert_eq!(steps, [0, 1, 2, 3, 0]);
        let tones: Vec<f32> = (0..STEPS).map(|k| dither(&layer(bar(), k as f32 * 0.4 + 0.01, true, false)).bias).collect();
        assert!(tones.windows(2).any(|w| w[0] != w[1]), "il tono cambia fra i passi: {tones:?}");
    }

    #[test]
    fn light_theme_is_inverted() {
        let (d, l) = (dither(&layer(bar(), 0.0, true, false)), dither(&layer(bar(), 0.0, false, false)));
        let (ds, ls) = (d.colors.as_slice(), l.colors.as_slice());
        let panel = |dark| { let c = Theme::for_mode(dark).panel; Rgb([c.r, c.g, c.b]) };
        assert_eq!((ds[0], ls[1]), (panel(true), panel(false)), "il pannello è lo stop scuro nello scuro, il chiaro nel chiaro");
        assert!((d.bias + l.bias).abs() < 1e-6, "stessa parte di punti d'inchiostro");
    }

    #[test]
    fn reduced_motion_is_still() {
        let a = layer(bar(), 0.0, true, true);
        let b = layer(bar(), 0.9, true, true);
        assert_eq!(dither(&a).bias, dither(&b).bias);
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
