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
use rune_fx::{FireParams, Fx, FxLayer, Rect, Shape};

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
pub fn height(phase: Phase, still: bool) -> f32 {
    let span = FIRE_H - REST_H;
    match phase {
        Phase::Rest => REST_H,
        Phase::Rising(_) if still => FIRE_H,
        Phase::Falling(_) if still => REST_H,
        Phase::Rising(t) => REST_H + span * ease(t, RISE),
        Phase::Falling(t) => FIRE_H - span * ease(t, FALL),
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
/// non tiene il display link.
pub fn layer(panel: Rect, phase: Phase, clock: f32, dark: bool, reduced: bool, active_window: bool) -> FxLayer {
    let still = reduced || !active_window;
    let t = if still { 0.0 } else { clock.max(0.0) };
    let fire = FireParams {
        height: height(phase, still),
        scale: 22.0,
        embers: 18,
        ember_rise: 64.0,
        intensity: if dark { 1.0 } else { LIGHT_INTENSITY },
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
pub fn slot(el: El, lit: Option<f64>, dark: bool, reduced: bool, active_window: bool) -> El {
    el.effect(Mark::Fire.id()).effect_data(slot_data(lit, dark, reduced, active_window))
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
}

const REDUCED: u32 = 1;
const INACTIVE: u32 = 2;
const LIGHT: u32 = 4;
const LIT: u32 = 8;

/// `effect_data` di uno [`slot`]: `lit` in due f32 (parte alta e resto, come `ring::slot_data`)
/// e i flag.
pub fn slot_data(lit: Option<f64>, dark: bool, reduced: bool, active_window: bool) -> [f32; 4] {
    let since = lit.unwrap_or(0.0);
    let hi = since as f32;
    let flags = (u32::from(reduced) * REDUCED) | (u32::from(!active_window) * INACTIVE) | (u32::from(!dark) * LIGHT) | (u32::from(lit.is_some()) * LIT);
    [hi, flags as f32, (since - f64::from(hi)) as f32, 0.0]
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
        let l = layer(panel(), Phase::Rising(1.0), 1.0, true, false, true);
        assert_eq!(fire(&l).style, FireStyle::Top, "fiamme dal bordo alto della forma");
        let r = l.shape.rect;
        assert_eq!((r.x, r.y, r.w), (8.0, 8.0 + 984.0, 884.0), "la forma parte dal fondo del pannello");
        assert_eq!(fire(&l).height, FIRE_H);
        assert!(l.is_animating());
    }

    #[test]
    fn fire_rises_over_420_ms_ease_out() {
        let h = |t| fire(&layer(panel(), Phase::Rising(t), t, true, false, true)).height;
        assert_eq!(h(0.0), REST_H);
        let mid = h(RISE / 2.0);
        assert!(mid > FIRE_H / 2.0 && mid < FIRE_H, "ease-out: oltre metà a metà tempo ({mid})");
        assert_eq!(h(RISE), FIRE_H);
        assert_eq!(h(9.0), FIRE_H);
    }

    #[test]
    fn reduced_motion_shows_it_without_rising_and_still() {
        let a = layer(panel(), Phase::Rising(0.0), 0.0, true, true, true);
        let b = layer(panel(), Phase::Rising(5.0), 5.0, true, true, true);
        assert_eq!(fire(&a).height, FIRE_H, "a piena altezza da subito");
        assert!(a.reduced_motion && !a.is_animating());
        assert_eq!(a.time, b.time, "fermo");
    }

    #[test]
    fn an_inactive_window_keeps_it_still_and_visible() {
        let l = layer(panel(), Phase::Rising(0.1), 0.1, true, false, false);
        assert_eq!(fire(&l).height, FIRE_H);
        assert!(l.opacity > 0.99 && !l.is_animating());
    }

    /// Nel tema chiaro le fiamme coprono il testo come nello scuro (decisione 2026-11-14).
    #[test]
    fn light_theme_covers_the_text_like_dark() {
        let (d, l) = (fire(&layer(panel(), Phase::Rising(1.0), 1.0, true, false, true)), fire(&layer(panel(), Phase::Rising(1.0), 1.0, false, false, true)));
        assert_eq!(l.occlusion, d.occlusion);
        assert_eq!(l.occlusion, OCCLUSION);
    }

    #[test]
    fn slot_data_round_trips() {
        let since = 30.0 * 86_400.0 + 0.123_456;
        for (dark, reduced, active) in [(true, false, true), (false, true, false), (false, false, true)] {
            let s = slot_state(slot_data(Some(since), dark, reduced, active), since + 0.2);
            assert_eq!((s.dark, s.reduced, s.active_window), (dark, reduced, active));
            assert!((s.t - 0.2).abs() < 1e-3, "{}", s.t);
        }
        assert_eq!(slot_state(slot_data(Some(10.0), true, false, true), 9.0).t, 0.0, "mai negativo");
    }

    // --- fuoco sempre acceso (v1.1) ---

    #[test]
    fn at_rest_it_stays_low_at_the_panel_bottom() {
        assert_eq!(height(Phase::Rest, false), REST_H);
        const { assert!(REST_H > 0.0 && REST_H < 40.0, "basso: sotto il margine in fondo al documento") };
        let l = layer(panel(), Phase::Rest, 3.0, true, false, true);
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
            let a = layer(panel(), Phase::Rest, 1.0, false, reduced, active);
            let b = layer(panel(), Phase::Rest, 7.0, false, reduced, active);
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
            let s = slot_state(slot_data(lit.then_some(4.0), true, false, true), 5.0);
            assert_eq!(s.lit, lit);
        }
    }
}
