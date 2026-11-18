//! RuneMD's GPU effects: a single rune-fx compositor for the whole window.

mod compositor;
pub mod caret;
pub mod doc_fx;
pub mod fire;
pub mod forge;
mod marks;
pub mod ring;

pub use compositor::FxCompositor;
#[cfg(test)]
pub use compositor::{edge_fills, edges, fire_layer, ring_layers};
pub use marks::Mark;
// `FxLayer` is rune-fx's drawable layer, used as it is.
#[allow(unused_imports)]
pub use rune_fx::FxLayer;

/// `RUNEMD_REDUCE_MOTION` as read: `1` turns "reduce motion" on, `0` turns it off, anything else or missing
/// lets the system decide.
pub fn reduce_motion_env(value: Option<&str>) -> Option<bool> {
    match value {
        Some("1") => Some(true),
        Some("0") => Some(false),
        _ => None,
    }
}

/// "Reduce motion" right now: the `RUNEMD_REDUCE_MOTION` override (photos and headless tests,
/// where Rune does not read the system preference) wins over the macOS one
/// (`accessibilityDisplayShouldReduceMotion`, which Rune reads at startup and when the window
/// takes focus).
pub fn reduce_motion(env: Option<bool>, system: bool) -> bool {
    env.unwrap_or(system)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_env_override_reads_1_and_0_only() {
        assert_eq!(reduce_motion_env(Some("1")), Some(true));
        assert_eq!(reduce_motion_env(Some("0")), Some(false));
        assert_eq!(reduce_motion_env(Some("")), None);
        assert_eq!(reduce_motion_env(Some("si")), None);
        assert_eq!(reduce_motion_env(None), None);
    }

    #[test]
    fn the_env_override_wins_over_the_system() {
        assert!(reduce_motion(Some(true), false), "RUNEMD_REDUCE_MOTION=1 without the preference");
        assert!(!reduce_motion(Some(false), true), "RUNEMD_REDUCE_MOTION=0 with the preference");
        assert!(reduce_motion(None, true), "without an override the system decides");
        assert!(!reduce_motion(None, false));
    }
}
