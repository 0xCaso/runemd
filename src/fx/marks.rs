//! Id degli slot `El::effect`: un intervallo per tipo, così due tipi non si scontrano mai.

// Costruiti dai task che accendono i layer (11 anelli, 12 fuoco, 13 dither).
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mark {
    TabRing(u32),
    RecentRing(u32),
    TreeRing(u32),
    Fire,
    Dither,
}

const FIRE: u32 = 1;
const DITHER: u32 = 2;
// Gli anelli delle schede partono da 100: sotto ci sono gli id fissi (fuoco 1, dither 2).
const TAB: (u32, u32) = (100, 9999);
const RECENT: u32 = 10_000;
const TREE: u32 = 20_000;

impl Mark {
    // Tab da 100, non da 1: 1 e 2 sono fuoco e dither. Nessun warm-up delle pipeline: vedi docs/perf.md.
    pub fn id(self) -> u32 {
        match self {
            Mark::TabRing(n) => TAB.0 + n.min(TAB.1 - TAB.0),
            Mark::RecentRing(n) => RECENT + n.min(9_999),
            Mark::TreeRing(n) => TREE + n,
            Mark::Fire => FIRE,
            Mark::Dither => DITHER,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_stable() {
        assert_ne!(Mark::TabRing(3).id(), Mark::RecentRing(3).id());
        assert_ne!(Mark::RecentRing(3).id(), Mark::TreeRing(3).id());
        assert_ne!(Mark::Fire.id(), Mark::Dither.id());
        assert_eq!(Mark::Fire.id(), Mark::Fire.id());
        assert_eq!(Mark::Dither.id(), 2);
        // Nessun anello collide con gli id fissi, ai bordi dei range.
        for n in [0, 1, 9_899, 50_000] {
            assert!(![1, 2].contains(&Mark::TabRing(n).id()));
            assert!(Mark::TabRing(n).id() <= 9_999);
        }
    }
}
