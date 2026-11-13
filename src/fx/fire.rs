//! Fuoco "testo nuovo più in basso" (DESIGN.md, "Segnali live"): fiamme e braci alte `FIRE_H`
//! dal fondo del pannello, che salgono in `RISE` ease-out.
//!
//! `layer` è una funzione pura del rect del pannello e del tempo. In entrambi i temi le fiamme
//! coprono il testo sotto di loro (miscela normale, stessa occlusione): decisione di Matteo,
//! `docs/decisions/2026-11-14-fuoco-tema-chiaro.md`. Il Multiply del tema chiaro (DESIGN.md e
//! spec) è scartato: lasciava il testo sopra le fiamme e l'etichetta illeggibile.

use rune::El;
use rune_fx::{FireParams, Fx, FxLayer, Rect, Shape};

use super::Mark;

/// Altezza delle fiamme, pt (DESIGN.md: 110).
pub const FIRE_H: f32 = 110.0;
/// Salita: l'altezza va da 0 a `FIRE_H` in 420 ms, ease-out.
pub const RISE: f32 = 0.42;
/// Spessore della forma sotto il fondo del pannello da cui partono le fiamme (fuori vista: lo
/// slot è tagliato dal pannello). Le fiamme `Top` salgono dal bordo alto della forma.
const BASE: f32 = 24.0;
/// Quanto la fiamma densa copre ciò che ha dietro (0..1), uguale nei due temi.
pub const OCCLUSION: f32 = 0.85;
/// Intensità nel tema chiaro (1 nello scuro).
const LIGHT_INTENSITY: f32 = 1.0;

/// Il fuoco sul fondo di `panel` (pt), `t` secondi dopo che si è acceso.
/// Con `reduced` compare a piena altezza senza salire e resta fermo; con la finestra non attiva
/// (`!active_window`) è fermo e visibile allo stesso modo. Fermo vuol dire `!is_animating()`:
/// il compositor non tiene il display link.
pub fn layer(panel: Rect, t: f32, dark: bool, reduced: bool, active_window: bool) -> FxLayer {
    let still = reduced || !active_window;
    let t = if still { 0.0 } else { t.max(0.0) };
    let height = if still { FIRE_H } else { FIRE_H * rise(t) };
    let fire = FireParams {
        height,
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

/// Fa di `el` lo slot del fuoco, acceso al tempo `since` dell'orologio di Rune. Il compositor lo
/// dipinge con [`layer`] sul rect di `el`, che deve toccare il fondo del pannello e averne la
/// larghezza.
pub fn slot(el: El, since: f64, dark: bool, reduced: bool, active_window: bool) -> El {
    el.effect(Mark::Fire.id()).effect_data(slot_data(since, dark, reduced, active_window))
}

/// Quanto legge il compositor da uno [`slot`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotState {
    /// Secondi da quando il fuoco è acceso.
    pub t: f32,
    pub dark: bool,
    pub reduced: bool,
    pub active_window: bool,
}

const REDUCED: u32 = 1;
const INACTIVE: u32 = 2;
const LIGHT: u32 = 4;

/// `effect_data` di uno [`slot`]: `since` in due f32 (parte alta e resto, come `ring::slot_data`)
/// e i flag.
pub fn slot_data(since: f64, dark: bool, reduced: bool, active_window: bool) -> [f32; 4] {
    let hi = since as f32;
    let flags = (u32::from(reduced) * REDUCED) | (u32::from(!active_window) * INACTIVE) | (u32::from(!dark) * LIGHT);
    [hi, flags as f32, (since - f64::from(hi)) as f32, 0.0]
}

/// Da `effect_data` di uno [`slot`], al tempo `clock` di Rune. La sottrazione è in f64.
pub fn slot_state(data: [f32; 4], clock: f64) -> SlotState {
    let since = f64::from(data[0]) + f64::from(data[2]);
    let flags = data[1] as u32;
    SlotState {
        t: (clock - since).max(0.0) as f32,
        dark: flags & LIGHT == 0,
        reduced: flags & REDUCED != 0,
        active_window: flags & INACTIVE == 0,
    }
}

/// Ease-out cubica da 0 a 1 in `RISE`.
fn rise(t: f32) -> f32 {
    let x = (t / RISE).clamp(0.0, 1.0);
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
        let l = layer(panel(), 1.0, true, false, true);
        assert_eq!(fire(&l).style, FireStyle::Top, "fiamme dal bordo alto della forma");
        let r = l.shape.rect;
        assert_eq!((r.x, r.y, r.w), (8.0, 8.0 + 984.0, 884.0), "la forma parte dal fondo del pannello");
        assert_eq!(fire(&l).height, FIRE_H);
        assert!(l.is_animating());
    }

    #[test]
    fn fire_rises_over_420_ms_ease_out() {
        let h = |t| fire(&layer(panel(), t, true, false, true)).height;
        assert_eq!(h(0.0), 0.0);
        let mid = h(RISE / 2.0);
        assert!(mid > FIRE_H / 2.0 && mid < FIRE_H, "ease-out: oltre metà a metà tempo ({mid})");
        assert_eq!(h(RISE), FIRE_H);
        assert_eq!(h(9.0), FIRE_H);
    }

    #[test]
    fn reduced_motion_shows_it_without_rising_and_still() {
        let a = layer(panel(), 0.0, true, true, true);
        let b = layer(panel(), 5.0, true, true, true);
        assert_eq!(fire(&a).height, FIRE_H, "a piena altezza da subito");
        assert!(a.reduced_motion && !a.is_animating());
        assert_eq!(a.time, b.time, "fermo");
    }

    #[test]
    fn an_inactive_window_keeps_it_still_and_visible() {
        let l = layer(panel(), 0.1, true, false, false);
        assert_eq!(fire(&l).height, FIRE_H);
        assert!(l.opacity > 0.99 && !l.is_animating());
    }

    /// Nel tema chiaro le fiamme coprono il testo come nello scuro (decisione 2026-11-14).
    #[test]
    fn light_theme_covers_the_text_like_dark() {
        let (d, l) = (fire(&layer(panel(), 1.0, true, false, true)), fire(&layer(panel(), 1.0, false, false, true)));
        assert_eq!(l.occlusion, d.occlusion);
        assert_eq!(l.occlusion, OCCLUSION);
    }

    #[test]
    fn slot_data_round_trips() {
        let since = 30.0 * 86_400.0 + 0.123_456;
        for (dark, reduced, active) in [(true, false, true), (false, true, false), (false, false, true)] {
            let s = slot_state(slot_data(since, dark, reduced, active), since + 0.2);
            assert_eq!((s.dark, s.reduced, s.active_window), (dark, reduced, active));
            assert!((s.t - 0.2).abs() < 1e-3, "{}", s.t);
        }
        assert_eq!(slot_state(slot_data(10.0, true, false, true), 9.0).t, 0.0, "mai negativo");
    }
}
