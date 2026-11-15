//! "New text further down": the blocks new or changed below the visible part (spec,
//! "Live signals", fire).
//!
//! `on_change` receives each applied reload with the id of the last visible block before the
//! reload and the order of the blocks after; `on_scroll` receives the index of the last visible block
//! at each change of the visible part. A block leaves the count as soon as it enters the view: the fire
//! goes away when the last one enters.

use std::collections::{HashMap, HashSet};

use crate::doc::document::Change;

#[derive(Debug, Default)]
pub struct Below {
    /// The ids of the new or changed blocks still below the view.
    pending: HashSet<u64>,
}

impl Below {
    /// An applied reload. `visible_last` is the id of the last visible block before the
    /// reload, `order` the ids of the blocks after. If `visible_last` is gone (removed, or the
    /// view has not reported rows yet) no block of the reload counts: better a missed
    /// fire than one lit on text already in view.
    pub fn on_change(&mut self, change: &Change, visible_last: u64, order: &[u64]) {
        let at = positions(order);
        self.pending.retain(|id| at.contains_key(id));
        let Some(&last) = at.get(&visible_last) else { return };
        let below = change.added.iter().chain(&change.changed).filter(|id| at.get(id).is_some_and(|&i| i > last));
        self.pending.extend(below);
    }

    /// The visible part changed: `visible_last_index` is the index in `order` of the last
    /// visible block. The blocks up to and including that one leave the count, and so do the ones that
    /// no longer exist.
    pub fn on_scroll(&mut self, visible_last_index: usize, order: &[u64]) {
        if self.pending.is_empty() {
            return;
        }
        let at = positions(order);
        self.pending.retain(|id| at.get(id).is_some_and(|&i| i > visible_last_index));
    }

    /// How many new or changed blocks are below the view.
    pub fn count(&self) -> usize {
        self.pending.len()
    }
}

/// The position of each id in `order`.
fn positions(order: &[u64]) -> HashMap<u64, usize> {
    order.iter().enumerate().map(|(i, id)| (*id, i)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Blocks 1..=10; the view ends at block 5 (index 4).
    fn order(n: u64) -> Vec<u64> {
        (1..=n).collect()
    }

    fn added(ids: &[u64]) -> Change {
        Change { added: ids.to_vec(), ..Change::default() }
    }

    #[test]
    fn counts_new_blocks_below_view() {
        let mut b = Below::default();
        b.on_change(&added(&[11, 12, 13]), 5, &order(13));
        assert_eq!(b.count(), 3);
    }

    #[test]
    fn blocks_above_view_do_not_count() {
        let mut b = Below::default();
        // 3 new blocks at the top: 100, 101, 102 before 1..=10.
        let order: Vec<u64> = [100, 101, 102].into_iter().chain(1..=10).collect();
        b.on_change(&added(&[100, 101, 102]), 5, &order);
        assert_eq!(b.count(), 0);
        // A changed block above the view does not count, one below does.
        b.on_change(&Change { changed: vec![2, 8], ..Change::default() }, 5, &order);
        assert_eq!(b.count(), 1);
    }

    #[test]
    fn clears_when_last_new_block_visible() {
        let mut b = Below::default();
        let order = order(13);
        b.on_change(&added(&[11, 12, 13]), 5, &order);
        // The scroll brings the first of the new ones into view: 2 are left.
        b.on_scroll(10, &order);
        assert_eq!(b.count(), 2);
        // The last one comes into view: the count goes to zero.
        b.on_scroll(12, &order);
        assert_eq!(b.count(), 0);
    }

    #[test]
    fn a_second_reload_does_not_count_twice_and_removed_blocks_leave() {
        let mut b = Below::default();
        b.on_change(&added(&[11, 12]), 5, &order(12));
        b.on_change(&Change { changed: vec![12], ..Change::default() }, 5, &order(12));
        assert_eq!(b.count(), 2, "a block changed twice counts once");
        b.on_change(&Change { removed: vec![12], ..Change::default() }, 5, &order(11));
        assert_eq!(b.count(), 1);
    }

    #[test]
    fn an_unknown_last_visible_block_counts_nothing() {
        let mut b = Below::default();
        b.on_change(&added(&[11]), 999, &order(11));
        assert_eq!(b.count(), 0);
    }
}
