//! Effetti GPU di RuneMD: un solo compositor rune-fx per tutta la finestra.

mod compositor;
pub mod dither;
pub mod fire;
mod marks;
pub mod ring;

pub use compositor::FxCompositor;
pub use marks::Mark;
// `FxLayer` è il layer disegnabile di rune-fx, usato così com'è.
#[allow(unused_imports)]
pub use rune_fx::FxLayer;

/// `RUNEMD_REDUCE_MOTION` letto: `1` accende "riduci movimento", `0` lo spegne, altro o assente
/// lascia decidere il sistema.
pub fn reduce_motion_env(value: Option<&str>) -> Option<bool> {
    match value {
        Some("1") => Some(true),
        Some("0") => Some(false),
        _ => None,
    }
}

/// "Riduci movimento" di adesso: l'override di `RUNEMD_REDUCE_MOTION` (foto e prove headless,
/// dove Rune non legge la preferenza di sistema) vince su quella di macOS
/// (`accessibilityDisplayShouldReduceMotion`, che Rune legge all'avvio e quando la finestra
/// prende il fuoco).
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
        assert!(reduce_motion(Some(true), false), "RUNEMD_REDUCE_MOTION=1 senza la preferenza");
        assert!(!reduce_motion(Some(false), true), "RUNEMD_REDUCE_MOTION=0 con la preferenza");
        assert!(reduce_motion(None, true), "senza override decide il sistema");
        assert!(!reduce_motion(None, false));
    }
}
