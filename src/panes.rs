//! The widths of the sidebar and of the heading outline, changed by dragging the edge.
//!
//! The snap learns widths, like Snappy: releasing the edge remembers the width; a
//! drag that passes within `SNAP` of a remembered width (or the default one)
//! stops there. The last `LEARNED_MAX` stay, the most recent first.

use serde::{Deserialize, Serialize};

/// The distance within which the edge snaps to a remembered width, pt.
pub const SNAP: f32 = 8.0;
/// How many remembered widths stay per pane.
pub const LEARNED_MAX: usize = 4;

/// The limits of a pane, pt.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Limits {
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

/// The sidebar: 248 by default (DESIGN.md, `spacing.sidebar`).
pub const SIDEBAR: Limits = Limits { min: 180.0, max: 420.0, default: crate::ui::theme::SIDEBAR };
/// The heading outline: 196 by default (DESIGN.md, `spacing.outline`).
pub const OUTLINE: Limits = Limits { min: 150.0, max: 360.0, default: crate::ui::theme::OUTLINE_W };

/// A pane's width and the ones it has learned.
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

    /// The pane read from the settings, put back within the limits: a width out of the limits
    /// clamped to the ends, remembered widths out of the limits or in excess removed.
    pub fn sanitized(mut self, limits: Limits) -> Pane {
        self.width = if self.width.is_finite() { self.width.clamp(limits.min, limits.max).round() } else { limits.default };
        self.learned.retain(|w| w.is_finite() && (limits.min..=limits.max).contains(w));
        self.learned.truncate(LEARNED_MAX);
        self
    }

    /// The width while dragging: `raw` clamped between `min` and the narrower of
    /// `max` and `room` (the space the window leaves), then the snap to the nearest remembered
    /// (or default) width within `SNAP`, if it is within the limits.
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

    /// The edge released at `width`: it becomes the width, and is remembered. A width already
    /// remembered (within `SNAP`) moves to the front; the default one is not remembered (it always snaps).
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
        assert_eq!(p.drag(900.0, L, 300.0), 300.0, "the window leaves less room");
        assert_eq!(p.drag(900.0, L, 50.0), 180.0, "never below the minimum");
        assert_eq!(p.drag(310.4, L, 2000.0), 310.0, "whole points");
    }

    #[test]
    fn drag_snaps_to_the_default_and_to_learned_widths() {
        let mut p = Pane::new(L);
        assert_eq!(p.drag(254.0, L, 2000.0), 248.0, "the default snaps");
        assert_eq!(p.drag(257.0, L, 2000.0), 257.0, "not beyond SNAP");
        p.learn(320.0, L);
        assert_eq!(p.drag(314.0, L, 2000.0), 320.0);
        assert_eq!(p.drag(327.0, L, 2000.0), 320.0);
        assert_eq!(p.drag(330.0, L, 2000.0), 330.0);
        p.learn(336.0, L);
        assert_eq!(p.drag(329.0, L, 2000.0), 336.0, "the nearer of the two");
        assert_eq!(p.drag(326.0, L, 330.0), 320.0, "336 does not fit: it does not snap there");
    }

    #[test]
    fn learn_keeps_the_latest_widths_once() {
        let mut p = Pane::new(L);
        p.learn(250.0, L);
        assert_eq!(p.width, 250.0);
        assert!(p.learned.is_empty(), "near the default it is not remembered");
        for w in [200.0, 300.0, 350.0, 400.0, 220.0] {
            p.learn(w, L);
        }
        assert_eq!(p.learned, vec![220.0, 400.0, 350.0, 300.0], "the last LEARNED_MAX");
        p.learn(352.0, L);
        assert_eq!(p.learned, vec![352.0, 220.0, 400.0, 300.0], "an already remembered one moves to the front, once");
    }

    #[test]
    fn sanitized_puts_a_saved_pane_back_in_the_limits() {
        let p = Pane { width: 9000.0, learned: vec![300.0, 10.0, f32::NAN, 200.0, 210.0, 220.0, 230.0] }.sanitized(L);
        assert_eq!(p.width, 420.0);
        assert_eq!(p.learned, vec![300.0, 200.0, 210.0, 220.0]);
        assert_eq!(Pane { width: f32::NAN, learned: vec![] }.sanitized(L).width, 248.0);
    }
}
