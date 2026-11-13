//! Anello di metallo liquido "sta scrivendo" (DESIGN.md, "Segnali live"): un bordo di metallo
//! liquido di `RING_W` attorno a una riga, che entra in `ENTER` e gira una volta ogni `TURN`.
//!
//! `layer` è una funzione pura del rect della riga e del tempo: la stessa per la scheda, la riga
//! in Recenti e la riga nell'albero (task 16 e 19).

use std::f32::consts::TAU;

use rune::El;
use rune_fx::{Fx, FxLayer, LiquidMetalParams, Rect, Shape};

use super::Mark;

/// Larghezza dell'anello, pt (DESIGN.md: 1,5).
pub const RING_W: f32 = 1.5;
/// Entrata: opacità da 0 a 1 in 220 ms, ease-out.
pub const ENTER: f32 = 0.22;
/// Un giro dello studio riflesso ogni 3 s.
pub const TURN: f32 = 3.0;
#[cfg_attr(not(test), allow(dead_code))] // le righe dei task 16 e 19; oggi l'ospite è il pannello
/// Raggio della riga (`theme::R_ROW`): l'anello lo allarga di `RING_W`.
pub const ROW_RADIUS: f32 = crate::ui::theme::R_ROW;

#[cfg_attr(not(test), allow(dead_code))] // le righe dei task 16 e 19; oggi l'ospite è il pannello
/// L'anello attorno a `rect` (la riga, in pt), `t` secondi dopo che si è acceso.
/// `.border` dipinge dentro la forma (rune-fx README, "Fills"): la forma è il rect allargato di
/// `RING_W` per lato, così la banda sta tutta fuori dalla riga. Con `reduced` è fermo e
/// visibile da subito.
pub fn layer(rect: Rect, t: f32, reduced: bool) -> FxLayer {
    layer_rounded(rect, ROW_RADIUS, t, reduced)
}

/// Come [`layer`], per una riga con raggio `radius` (il pannello del lettore, oggi).
pub fn layer_rounded(rect: Rect, radius: f32, t: f32, reduced: bool) -> FxLayer {
    let t = if reduced { 0.0 } else { t.max(0.0) };
    let base = LiquidMetalParams::chrome();
    let metal = LiquidMetalParams { angle: base.angle + TAU * (t / TURN).fract(), ..base };
    let r = rect.inflate(RING_W);
    let shape = Shape::rounded(r.x, r.y, r.w, r.h, radius + RING_W);
    Fx::fill(metal.border(RING_W)).on(shape).at(t).reduced(reduced).opacity(if reduced { 1.0 } else { enter(t) })
}

/// Fa di `el` lo slot dell'anello `mark`, acceso al tempo `since` dell'orologio di Rune
/// (`poll` lo riceve, il compositor legge `Runtime::clock`). Il compositor lo dipinge con
/// [`layer_rounded`] sul rect e il raggio di `el`.
pub fn slot(el: El, mark: Mark, since: f64, reduced: bool) -> El {
    el.effect(mark.id()).effect_bleed(RING_W).effect_data([since as f32, if reduced { 1.0 } else { 0.0 }, 0.0, 0.0])
}

/// Da `effect_data` di uno [`slot`]: secondi da quando l'anello è acceso e "riduci movimento".
pub fn slot_time(data: [f32; 4], clock: f64) -> (f32, bool) {
    ((clock as f32 - data[0]).max(0.0), data[1] > 0.5)
}

/// Ease-out cubica da 0 a 1 in `ENTER`.
fn enter(t: f32) -> f32 {
    let x = (t / ENTER).clamp(0.0, 1.0);
    1.0 - (1.0 - x).powi(3)
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
            other => panic!("non è un fill: {other:?}"),
        }
    }

    fn metal(l: &FxLayer) -> LiquidMetalParams {
        match fill(l).style {
            FillStyle::LiquidMetal(m) => m,
            other => panic!("non è metallo liquido: {other:?}"),
        }
    }

    #[test]
    fn ring_is_a_liquid_metal_border_outside_the_row() {
        let l = layer(row(), 1.0, false);
        metal(&l);
        assert_eq!(fill(&l).border, RING_W);
        let r = l.shape.rect;
        assert_eq!((r.x, r.y, r.w, r.h), (10.0 - RING_W, 20.0 - RING_W, 200.0 + 2.0 * RING_W, 28.0 + 2.0 * RING_W));
        assert!(l.is_animating());
    }

    #[test]
    fn ring_fades_in_over_220_ms_ease_out() {
        assert_eq!(layer(row(), 0.0, false).opacity, 0.0);
        let mid = layer(row(), ENTER / 2.0, false).opacity;
        assert!(mid > 0.5 && mid < 1.0, "ease-out: oltre metà a metà tempo ({mid})");
        assert_eq!(layer(row(), ENTER, false).opacity, 1.0);
        assert_eq!(layer(row(), 5.0, false).opacity, 1.0);
    }

    #[test]
    fn ring_turns_once_every_3_s() {
        let tau = std::f32::consts::TAU;
        let angle = |t| metal(&layer(row(), t, false)).angle.rem_euclid(tau);
        assert!((angle(1.0) - angle(1.0 + TURN / 2.0)).abs() > 3.0, "mezzo giro in TURN / 2");
        assert!((angle(1.0) - angle(1.0 + TURN)).abs() < 1e-3, "un giro in TURN");
    }

    #[test]
    fn slot_data_round_trips() {
        assert_eq!(slot_time([12.5, 1.0, 0.0, 0.0], 13.0), (0.5, true));
        assert_eq!(slot_time([12.5, 0.0, 0.0, 0.0], 12.0), (0.0, false), "mai negativo");
    }

    #[test]
    fn reduced_motion_is_still_and_visible() {
        let a = layer(row(), 0.0, true);
        let b = layer(row(), 7.0, true);
        assert_eq!(a.opacity, 1.0, "visibile subito, senza entrata");
        assert!(a.reduced_motion);
        assert!(!a.is_animating());
        assert_eq!((a.time, metal(&a)), (b.time, metal(&b)), "fermo");
    }
}
