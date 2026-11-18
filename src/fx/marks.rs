//! Ids of the `El::effect` slots: one range per kind, so two kinds never clash.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mark {
    TabRing(u32),
    RecentRing(u32),
    TreeRing(u32),
    /// The whole ⌘P panel (docs/decisions/2026-11-16-metal-tab-quick-open-panel.md): one slot.
    QuickPanel,
    Fire,
    Caret,
    /// The fire inside the wordmark on the empty screen (forge-empty): one slot.
    Forge,
    /// The empty screen's "An agent is writing <file>" line: one ring.
    ForgeLine,
    /// The front tab's metal, joined to the page's (`fx::outline`): one ring.
    FrontTab,
    /// The page's part of the front tab's outline (`fx::outline`): one slot, not a ring of its own.
    Page,
}

const FIRE: u32 = 1;
const CARET: u32 = 2;
const FORGE: u32 = 3;
const PAGE: u32 = 4;
// Tab rings start at 100: below are the fixed ids (fire 1, caret 2, forge 3).
const TAB: (u32, u32) = (100, 4_999);
const QUICK: u32 = 5_000;
const FORGE_LINE: u32 = 5_001;
const FRONT_TAB: u32 = 5_002;
const RECENT: u32 = 10_000;
const TREE: u32 = 20_000;

impl Mark {
    // Tabs from 100, not from 1: 1 and 2 are fire and caret. No pipeline warm-up: see docs/perf.md.
    pub fn id(self) -> u32 {
        match self {
            Mark::TabRing(n) => TAB.0 + n.min(TAB.1 - TAB.0),
            Mark::RecentRing(n) => RECENT + n.min(9_999),
            Mark::TreeRing(n) => TREE + n,
            Mark::QuickPanel => QUICK,
            Mark::Fire => FIRE,
            Mark::Caret => CARET,
            Mark::Forge => FORGE,
            Mark::ForgeLine => FORGE_LINE,
            Mark::FrontTab => FRONT_TAB,
            Mark::Page => PAGE,
        }
    }

    /// Whether a slot id is a ring (tab, ⌘P panel, row in Recent, row in the tree).
    pub fn is_ring(id: u32) -> bool {
        id >= TAB.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_and_stable() {
        assert_ne!(Mark::TabRing(3).id(), Mark::RecentRing(3).id());
        assert_ne!(Mark::RecentRing(3).id(), Mark::TreeRing(3).id());
        assert!(Mark::TabRing(1_000_000).id() < Mark::QuickPanel.id());
        assert!(Mark::QuickPanel.id() < Mark::RecentRing(0).id());
        assert!(Mark::QuickPanel.id() < Mark::ForgeLine.id() && Mark::ForgeLine.id() < Mark::RecentRing(0).id());
        assert_ne!(Mark::Fire.id(), Mark::Caret.id());
        assert_eq!(Mark::Fire.id(), Mark::Fire.id());
        assert_eq!(Mark::Caret.id(), 2);
        assert_eq!(Mark::Forge.id(), 3);
        // No ring collides with the fixed ids, at the edges of the ranges.
        for n in [0, 1, 9_899, 50_000] {
            assert!(![1, 2, 3].contains(&Mark::TabRing(n).id()));
            assert!(Mark::TabRing(n).id() <= 4_999);
        }
    }

    #[test]
    fn rings_are_told_apart_from_fire_and_caret() {
        for m in [Mark::TabRing(0), Mark::QuickPanel, Mark::ForgeLine, Mark::RecentRing(4), Mark::TreeRing(70_000)] {
            assert!(Mark::is_ring(m.id()), "{m:?}");
        }
        assert!(!Mark::is_ring(Mark::Fire.id()));
        assert!(!Mark::is_ring(Mark::Caret.id()));
        assert!(!Mark::is_ring(Mark::Forge.id()));
        assert!(!Mark::is_ring(Mark::Page.id()));
        assert!(Mark::is_ring(Mark::FrontTab.id()) && Mark::FrontTab.id() > Mark::ForgeLine.id() && Mark::FrontTab.id() < Mark::RecentRing(0).id());
        assert!(!Mark::is_ring(0), "slot without mark");
    }
}
