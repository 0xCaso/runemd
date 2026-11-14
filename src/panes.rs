//! Le larghezze della sidebar e dell'indice dei titoli, che si cambiano trascinando il bordo.
//!
//! Lo scatto impara le larghezze, come Snappy: lasciando il bordo la larghezza si ricorda; un
//! trascinamento che passa a meno di `SNAP` da una larghezza ricordata (o da quella di default)
//! si ferma lì. Le ultime `LEARNED_MAX` restano, la più recente prima.

use serde::{Deserialize, Serialize};

/// La distanza entro cui il bordo scatta a una larghezza ricordata, pt.
pub const SNAP: f32 = 8.0;
/// Quante larghezze ricordate restano per pannello.
pub const LEARNED_MAX: usize = 4;

/// I limiti di un pannello, pt.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

/// La sidebar: 248 di default (DESIGN.md, `spacing.sidebar`).
pub const SIDEBAR: Limits = Limits { min: 180.0, max: 420.0, default: crate::ui::theme::SIDEBAR };
/// L'indice dei titoli: 196 di default (DESIGN.md, `spacing.outline`).
pub const OUTLINE: Limits = Limits { min: 150.0, max: 360.0, default: crate::ui::theme::OUTLINE_W };

/// La larghezza di un pannello e quelle che ha imparato.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pane {
    pub width: f32,
    #[serde(default)]
    pub learned: Vec<f32>,
}

impl Pane {
    pub fn new(limits: Limits) -> Pane {
        Pane { width: limits.default, learned: Vec::new() }
    }

    /// Il pannello letto dalle impostazioni, rimesso nei limiti: larghezza fuori dai limiti
    /// fermata ai capi, larghezze ricordate fuori dai limiti o in più tolte.
    pub fn sanitized(mut self, limits: Limits) -> Pane {
        self.width = if self.width.is_finite() { self.width.clamp(limits.min, limits.max).round() } else { limits.default };
        self.learned.retain(|w| w.is_finite() && (limits.min..=limits.max).contains(w));
        self.learned.truncate(LEARNED_MAX);
        self
    }

    /// La larghezza durante il trascinamento: `raw` fermata fra `min` e il più stretto fra
    /// `max` e `room` (lo spazio che la finestra lascia), poi lo scatto alla larghezza ricordata
    /// (o di default) più vicina entro `SNAP`, se sta nei limiti.
    pub fn drag(&self, raw: f32, limits: Limits, room: f32) -> f32 {
        let max = limits.max.min(room).max(limits.min);
        let w = raw.clamp(limits.min, max);
        let near = self
            .learned
            .iter()
            .copied()
            .chain(std::iter::once(limits.default))
            .filter(|t| (limits.min..=max).contains(t) && (t - w).abs() <= SNAP)
            .min_by(|a, b| (a - w).abs().total_cmp(&(b - w).abs()));
        near.unwrap_or(w).round()
    }

    /// Il bordo lasciato a `width`: diventa la larghezza, e si ricorda. Una larghezza già
    /// ricordata (entro `SNAP`) torna prima; quella di default non si ricorda (scatta sempre).
    pub fn learn(&mut self, width: f32, limits: Limits) {
        self.width = width;
        if (width - limits.default).abs() <= SNAP {
            return;
        }
        self.learned.retain(|w| (w - width).abs() > SNAP);
        self.learned.insert(0, width);
        self.learned.truncate(LEARNED_MAX);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const L: Limits = Limits { min: 180.0, max: 420.0, default: 248.0 };

    #[test]
    fn drag_clamps_to_the_limits_and_the_room() {
        let p = Pane::new(L);
        assert_eq!(p.drag(100.0, L, 2000.0), 180.0);
        assert_eq!(p.drag(900.0, L, 2000.0), 420.0);
        assert_eq!(p.drag(900.0, L, 300.0), 300.0, "la finestra lascia meno spazio");
        assert_eq!(p.drag(900.0, L, 50.0), 180.0, "mai sotto il minimo");
        assert_eq!(p.drag(310.4, L, 2000.0), 310.0, "punti interi");
    }

    #[test]
    fn drag_snaps_to_the_default_and_to_learned_widths() {
        let mut p = Pane::new(L);
        assert_eq!(p.drag(254.0, L, 2000.0), 248.0, "il default scatta");
        assert_eq!(p.drag(257.0, L, 2000.0), 257.0, "oltre SNAP no");
        p.learn(320.0, L);
        assert_eq!(p.drag(314.0, L, 2000.0), 320.0);
        assert_eq!(p.drag(327.0, L, 2000.0), 320.0);
        assert_eq!(p.drag(330.0, L, 2000.0), 330.0);
        p.learn(336.0, L);
        assert_eq!(p.drag(329.0, L, 2000.0), 336.0, "la più vicina delle due");
        assert_eq!(p.drag(326.0, L, 330.0), 320.0, "336 non ci sta: non scatta lì");
    }

    #[test]
    fn learn_keeps_the_latest_widths_once() {
        let mut p = Pane::new(L);
        p.learn(250.0, L);
        assert_eq!(p.width, 250.0);
        assert!(p.learned.is_empty(), "vicino al default non si ricorda");
        for w in [200.0, 300.0, 350.0, 400.0, 220.0] {
            p.learn(w, L);
        }
        assert_eq!(p.learned, vec![220.0, 400.0, 350.0, 300.0], "le ultime LEARNED_MAX");
        p.learn(352.0, L);
        assert_eq!(p.learned, vec![352.0, 220.0, 400.0, 300.0], "una già ricordata torna prima, una volta");
    }

    #[test]
    fn sanitized_puts_a_saved_pane_back_in_the_limits() {
        let p = Pane { width: 9000.0, learned: vec![300.0, 10.0, f32::NAN, 200.0, 210.0, 220.0, 230.0] }.sanitized(L);
        assert_eq!(p.width, 420.0);
        assert_eq!(p.learned, vec![300.0, 200.0, 210.0, 220.0]);
        assert_eq!(Pane { width: f32::NAN, learned: vec![] }.sanitized(L).width, 248.0);
    }
}
