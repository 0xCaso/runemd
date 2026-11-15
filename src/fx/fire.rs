//! Fuoco in fondo al pannello (DESIGN.md, "Segnali live"): sempre acceso e basso (`REST_H`);
//! con testo nuovo sotto la vista sale a `FIRE_H` in `RISE` ease-out, e torna giù in `FALL`
//! quando quel testo entra nella vista.
//!
//! `layer` è una funzione pura del rect del pannello, della fase e del tempo. In entrambi i temi le fiamme
//! coprono il testo sotto di loro (miscela normale, stessa occlusione): decisione di Matteo,
//! `docs/decisions/2026-11-14-fuoco-tema-chiaro.md`. Il Multiply del tema chiaro (DESIGN.md e
//! spec) è scartato: lasciava il testo sopra le fiamme e l'etichetta illeggibile.

use std::time::Duration;

use rune::El;
use rune_fx::{FireParams, Fx, FxLayer, Rect, Rgb, Shape, rgb};
use serde::{Deserialize, Serialize};

use super::Mark;

/// Altezza delle fiamme con testo nuovo sotto la vista, pt (DESIGN.md: 110).
pub const FIRE_H: f32 = 110.0;
/// Altezza delle fiamme a riposo, pt: sempre accese in fondo al pannello, basse. Sta sotto lo
/// spazio in fondo al documento (`doc_view::BOTTOM`, 48): con lo scroll in fondo non coprono testo.
pub const REST_H: f32 = 26.0;
/// Salita: da `REST_H` a `FIRE_H` in 420 ms, ease-out.
pub const RISE: f32 = 0.42;
/// Discesa a riposo quando il testo nuovo entra nella vista: 300 ms, ease-out.
pub const FALL: f32 = 0.30;
/// A riposo le fiamme non tengono il display link (a 120 Hz costano ~11 % di CPU): si
/// ridisegnano a ogni risveglio di `REST_FRAME` (12 frame al secondo).
pub const REST_FRAME: Duration = Duration::from_micros(83_333);
/// Spessore della forma sotto il fondo del pannello da cui partono le fiamme (fuori vista: lo
/// slot è tagliato dal pannello). Le fiamme `Top` salgono dal bordo alto della forma.
const BASE: f32 = 24.0;
/// Quanto la fiamma densa copre ciò che ha dietro (0..1), uguale nei due temi.
pub const OCCLUSION: f32 = 0.85;
/// Intensità nel tema chiaro (1 nello scuro).
const LIGHT_INTENSITY: f32 = 1.0;

/// Le intensità fra cui sceglie il cursore "Intensità" (moltiplica `FireParams::intensity`);
/// contiene 1 (oggi).
pub const STRENGTHS: [f32; 7] = [0.4, 0.6, 0.8, 1.0, 1.25, 1.5, 1.75];
/// Le altezze di "Altezza", come fattore di `FIRE_H` e `REST_H`; contiene 1. Il riposo più alto
/// (`REST_H` × 1,6 = 41,6) resta sotto lo spazio in fondo al documento (48).
pub const HEIGHTS: [f32; 7] = [0.5, 0.65, 0.8, 1.0, 1.2, 1.4, 1.6];
/// Le velocità di "Velocità", come fattore di `SPEED`; contiene 1. Non cambiano i risvegli a
/// riposo (`REST_FRAME`), quindi nemmeno la CPU a riposo.
pub const SPEEDS: [f32; 7] = [0.25, 0.5, 0.75, 1.0, 1.5, 2.0, 3.0];
/// La velocità delle fiamme di oggi (`FireParams::default`).
const SPEED: f32 = 1.5;

/// Il colore delle fiamme. `Fire` è quello di oggi (i colori di `FireParams::default`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FireColor {
    #[default]
    Fire,
    Blue,
    Violet,
    Green,
    /// Il colore d'accento di macOS (Impostazioni di Sistema › Aspetto).
    Accent,
}

/// I colori nell'ordine del pannello, con il nome.
pub const COLORS: [(FireColor, &str); 5] = [
    (FireColor::Fire, "Fire"),
    (FireColor::Blue, "Blue"),
    (FireColor::Violet, "Violet"),
    (FireColor::Green, "Green"),
    (FireColor::Accent, "System accent"),
];

/// L'aspetto del fuoco scelto nelle impostazioni. `accent`: il colore d'accento di sistema in
/// sRGB 0xRRGGBB (per `FireColor::Accent`). Il default è il fuoco di oggi.
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

/// L'accento di default di macOS (blu), quando non se ne sa un altro.
pub const ACCENT_DEFAULT: u32 = 0x007aff;

/// Un colore di Rune (lineare) in sRGB 0xRRGGBB, per `Look::accent`.
pub fn accent_hex(c: rune::Color) -> u32 {
    let b = |v: f32| (rune::color::srgb_encode(v.clamp(0.0, 1.0)) * 255.0).round() as u32;
    (b(c.r) << 16) | (b(c.g) << 8) | b(c.b)
}

/// I tre colori delle fiamme (basso, medio, caldo) di `color`. Il caldo è sempre chiaro:
/// l'etichetta del testo nuovo sta sopra, in un grigio scuro.
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

/// La posizione di `v` in `steps`, o quella di 1 (il default) se non c'è.
fn step_index(steps: &[f32], v: f32) -> u32 {
    let at = steps.iter().position(|s| *s == v).or_else(|| steps.iter().position(|s| *s == 1.0));
    at.unwrap_or(0) as u32
}

/// Il passo `i` di `steps`; fuori, 1.
fn step_at(steps: &[f32], i: u32) -> f32 {
    steps.get(i as usize).copied().unwrap_or(1.0)
}

/// In che fase è il fuoco, con i secondi da quando ci è entrato.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    /// Nessun testo nuovo sotto la vista: basso, a `REST_H`.
    Rest,
    /// C'è testo nuovo sotto: sale a `FIRE_H` e ci resta.
    Rising(f32),
    /// Il testo nuovo è entrato nella vista: torna a `REST_H`.
    Falling(f32),
}

/// L'altezza delle fiamme in `phase`. `still` ("riduci movimento", finestra non attiva): subito
/// all'altezza di arrivo.
#[cfg(test)]
pub fn height(phase: Phase, still: bool) -> f32 {
    height_scaled(phase, still, 1.0)
}

/// [`height`] con `FIRE_H` e `REST_H` per il fattore `k` ("Altezza").
pub fn height_scaled(phase: Phase, still: bool, k: f32) -> f32 {
    let (fire, rest) = (FIRE_H * k, REST_H * k);
    let span = fire - rest;
    match phase {
        Phase::Rest => rest,
        Phase::Rising(_) if still => fire,
        Phase::Falling(_) if still => rest,
        Phase::Rising(t) => rest + span * ease(t, RISE),
        Phase::Falling(t) => fire - span * ease(t, FALL),
    }
}

/// Se in `phase` il fuoco si ridisegna solo con i risvegli di `REST_FRAME` (riposo, o discesa
/// finita), non con il display link.
pub fn paced(phase: Phase) -> bool {
    match phase {
        Phase::Rest => true,
        Phase::Falling(t) => t >= FALL,
        Phase::Rising(_) => false,
    }
}

/// Il fuoco sul fondo di `panel` (pt) in `phase`; `clock` è l'orologio di Rune, che muove le
/// fiamme (anche a riposo). Con `reduced` è fermo all'altezza di arrivo; con la finestra non
/// attiva (`!active_window`) allo stesso modo. Fermo vuol dire `!is_animating()`: il compositor
/// non tiene il display link. `look`: intensità, altezza, velocità e colore delle impostazioni.
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
        // La fiamma densa copre il testo dietro, nei due temi: l'etichetta sopra resta leggibile.
        occlusion: OCCLUSION,
        ..FireParams::default()
    };
    let shape = Shape::rounded(panel.x, panel.y + panel.h, panel.w, BASE, 0.0);
    Fx::fire(fire).on(shape).at(t).seed(12).reduced(still)
}

/// Fa di `el` lo slot del fuoco: acceso (testo nuovo sotto) dal tempo `lit` dell'orologio di
/// Rune, a riposo con `None`. Il compositor lo dipinge con [`layer`] sul rect di `el`, che deve
/// toccare il fondo del pannello e averne la larghezza.
pub fn slot(el: El, lit: Option<f64>, dark: bool, reduced: bool, active_window: bool, look: Look) -> El {
    el.effect(Mark::Fire.id()).effect_data(slot_data(lit, dark, reduced, active_window, look))
}

/// Quanto legge il compositor da uno [`slot`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotState {
    /// C'è testo nuovo sotto la vista.
    pub lit: bool,
    /// Secondi da quando il fuoco è acceso (0 a riposo).
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

/// Dove stanno nei flag i passi dell'aspetto (4 bit ciascuno, sopra i 4 bit dei flag): un intero
/// sotto 2^24, esatto in un f32.
const STRENGTH_AT: u32 = 4;
const HEIGHT_AT: u32 = 8;
const SPEED_AT: u32 = 12;
const COLOR_AT: u32 = 16;

/// `effect_data` di uno [`slot`]: `lit` in due f32 (parte alta e resto, come `ring::slot_data`),
/// i flag con i passi dell'aspetto, e l'accento (0xRRGGBB, esatto in un f32).
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

/// Da `effect_data` di uno [`slot`], al tempo `clock` di Rune. La sottrazione è in f64.
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
            other => panic!("non è fuoco: {other:?}"),
        }
    }

    #[test]
    fn fire_rises_from_the_panel_bottom_across_its_width() {
        let l = layer(panel(), Phase::Rising(1.0), 1.0, true, false, true, Look::default());
        assert_eq!(fire(&l).style, FireStyle::Top, "fiamme dal bordo alto della forma");
        let r = l.shape.rect;
        assert_eq!((r.x, r.y, r.w), (8.0, 8.0 + 984.0, 884.0), "la forma parte dal fondo del pannello");
        assert_eq!(fire(&l).height, FIRE_H);
        assert!(l.is_animating());
    }

    #[test]
    fn fire_rises_over_420_ms_ease_out() {
        let h = |t| fire(&layer(panel(), Phase::Rising(t), t, true, false, true, Look::default())).height;
        assert_eq!(h(0.0), REST_H);
        let mid = h(RISE / 2.0);
        assert!(mid > FIRE_H / 2.0 && mid < FIRE_H, "ease-out: oltre metà a metà tempo ({mid})");
        assert_eq!(h(RISE), FIRE_H);
        assert_eq!(h(9.0), FIRE_H);
    }

    #[test]
    fn reduced_motion_shows_it_without_rising_and_still() {
        let a = layer(panel(), Phase::Rising(0.0), 0.0, true, true, true, Look::default());
        let b = layer(panel(), Phase::Rising(5.0), 5.0, true, true, true, Look::default());
        assert_eq!(fire(&a).height, FIRE_H, "a piena altezza da subito");
        assert!(a.reduced_motion && !a.is_animating());
        assert_eq!(a.time, b.time, "fermo");
    }

    #[test]
    fn an_inactive_window_keeps_it_still_and_visible() {
        let l = layer(panel(), Phase::Rising(0.1), 0.1, true, false, false, Look::default());
        assert_eq!(fire(&l).height, FIRE_H);
        assert!(l.opacity > 0.99 && !l.is_animating());
    }

    /// Nel tema chiaro le fiamme coprono il testo come nello scuro (decisione 2026-11-14).
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
        assert_eq!(slot_state(slot_data(Some(10.0), true, false, true, Look::default()), 9.0).t, 0.0, "mai negativo");
    }

    // --- fuoco sempre acceso (v1.1) ---

    #[test]
    fn at_rest_it_stays_low_at_the_panel_bottom() {
        assert_eq!(height(Phase::Rest, false), REST_H);
        const { assert!(REST_H > 0.0 && REST_H < 40.0, "basso: sotto il margine in fondo al documento") };
        let l = layer(panel(), Phase::Rest, 3.0, true, false, true, Look::default());
        assert_eq!(fire(&l).height, REST_H);
        assert!(l.is_animating(), "a riposo si muove ancora (a pochi frame al secondo)");
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
            assert!(!a.is_animating() && a.time == b.time, "fermo");
        }
    }

    /// A riposo il fuoco non tiene il display link: lo ridisegnano i risvegli di `REST_FRAME`.
    #[test]
    fn only_the_rest_is_paced_by_wakes() {
        assert!(paced(Phase::Rest));
        assert!(!paced(Phase::Rising(0.1)) && !paced(Phase::Falling(0.1)));
        assert!(paced(Phase::Falling(FALL + 0.01)), "caduta finita = riposo");
        assert!(REST_FRAME >= Duration::from_secs(1) / 15, "al massimo 15 frame al secondo a riposo");
    }

    #[test]
    fn slot_data_says_whether_it_is_lit() {
        for lit in [true, false] {
            let s = slot_state(slot_data(lit.then_some(4.0), true, false, true, Look::default()), 5.0);
            assert_eq!(s.lit, lit);
        }
    }

    // --- impostazioni del fuoco ---

    /// L'aspetto di default è il fuoco di oggi, parametro per parametro.
    #[test]
    fn the_default_look_is_todays_fire() {
        let p = fire(&layer(panel(), Phase::Rising(9.0), 9.0, true, false, true, Look::default()));
        let d = FireParams::default();
        assert_eq!((p.low, p.mid, p.hot), (d.low, d.mid, d.hot));
        assert_eq!((p.height, p.speed, p.intensity), (FIRE_H, 1.5, 1.0));
        assert_eq!((p.scale, p.embers, p.ember_rise, p.occlusion), (22.0, 18, 64.0, OCCLUSION));
        for steps in [&STRENGTHS[..], &HEIGHTS, &SPEEDS] {
            assert!(steps.contains(&1.0), "il default è un passo");
            assert!(steps.windows(2).all(|w| w[0] < w[1]), "in ordine");
            assert!(steps.len() <= 16, "sta in 4 bit");
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
        assert!((rest - REST_H * 1.4).abs() < 1e-3, "anche il riposo");
    }

    /// Il riposo più alto resta sotto lo spazio in fondo al documento: con lo scroll in fondo non
    /// copre testo.
    #[test]
    fn the_highest_rest_stays_below_the_document_bottom() {
        let top = HEIGHTS.iter().fold(0.0f32, |a, k| a.max(*k));
        assert!(height_scaled(Phase::Rest, false, top) < crate::ui::doc_view::BOTTOM);
    }

    #[test]
    fn the_look_round_trips_through_the_slot() {
        for (i, (color, _)) in COLORS.iter().enumerate() {
            let look = Look { strength: STRENGTHS[i % STRENGTHS.len()], height: HEIGHTS[(i + 2) % HEIGHTS.len()], speed: SPEEDS[(i + 4) % SPEEDS.len()], color: *color, accent: 0xff2d55 };
            let s = slot_state(slot_data(Some(3.0), false, true, false, look), 4.0);
            assert_eq!(s.look, look);
            assert_eq!((s.dark, s.reduced, s.active_window, s.lit), (false, true, false, true));
        }
        // Un valore che non è un passo arriva come il default.
        let odd = Look { strength: 0.33, height: 9.0, speed: -1.0, ..Look::default() };
        assert_eq!(slot_state(slot_data(None, true, false, true, odd), 0.0).look, Look::default());
    }

    /// In ogni colore la parte calda è chiara: l'etichetta scura sopra resta leggibile (≥ 4,5:1
    /// col grigio più chiaro dei due inchiostri, `panel` scuro e `fg` chiaro).
    #[test]
    fn every_colour_keeps_a_light_hot_base_for_the_label() {
        let ink = [crate::ui::theme::Theme::for_mode(true).panel, crate::ui::theme::Theme::for_mode(false).fg];
        for accent in [ACCENT_DEFAULT, 0xff2d55, 0x8e8e93, 0xffcc00] {
            for (color, name) in COLORS {
                let [_, _, hot] = palette(color, accent);
                let hot = rune::Color { r: hot.0[0], g: hot.0[1], b: hot.0[2], a: 1.0 };
                for ink in ink {
                    let r = crate::ui::contrast::ratio(ink, hot);
                    assert!(r >= 4.5, "{name} (accento {accent:06x}): {r:.2}");
                }
            }
        }
    }
}
