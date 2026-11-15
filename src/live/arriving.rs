//! "What changed" (spec, "Live signals"): [`Changed`], the blocks new or changed by an
//! external reload, marked for [`CHANGED_MARK`] (`CHANGED_MARK_MS` in `docs/agent-writes.md`).
//! "Arriving" no longer checks whether the last block is open: the caret sits at the point
//! of the last edit (`live::caret`).

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How long the mark on changed blocks stays (`docs/agent-writes.md`: 8000 ms, `ipotesi`).
pub const CHANGED_MARK: Duration = Duration::from_millis(8000);

/// The blocks marked as changed, each with its expiry.
#[derive(Debug, Default)]
pub struct Changed {
    until: HashMap<u64, Instant>,
}

impl Changed {
    /// Marks `ids` at `now`: the mark stays for [`CHANGED_MARK`]. A block already marked restarts.
    pub fn mark(&mut self, ids: &[u64], now: Instant) {
        self.until.extend(ids.iter().map(|id| (*id, now + CHANGED_MARK)));
    }

    /// Whether block `id` has the mark at `now`.
    pub fn is_marked(&self, id: u64, now: Instant) -> bool {
        self.until.get(&id).is_some_and(|end| now < *end)
    }

    /// Removes the marks expired at `now`; true if it removed at least one (the view changes).
    pub fn prune(&mut self, now: Instant) -> bool {
        let before = self.until.len();
        self.until.retain(|_, end| now < *end);
        self.until.len() != before
    }

    /// How soon the next mark expires; `None` if there are none.
    pub fn left(&self, now: Instant) -> Option<Duration> {
        self.until.values().filter(|end| now < **end).min().map(|end| *end - now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;




    #[test]
    fn changed_mark_expires() {
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        let mut c = Changed::default();
        c.mark(&[7], t0);
        assert!(c.is_marked(7, t0 + ms(7900)), "visible at 7.9 s");
        assert!(!c.is_marked(7, t0 + ms(8100)), "off at 8.1 s");
        assert!(!c.is_marked(8, t0), "only the marked blocks");
    }





    #[test]
    fn prune_and_left_follow_the_earliest_mark() {
        let t0 = Instant::now();
        let s = Duration::from_secs;
        let mut c = Changed::default();
        c.mark(&[1], t0);
        c.mark(&[2], t0 + s(2));
        assert_eq!(c.left(t0), Some(CHANGED_MARK));
        assert!(!c.prune(t0 + s(1)));
        assert!(c.prune(t0 + CHANGED_MARK), "the first expires");
        assert_eq!(c.left(t0 + CHANGED_MARK), Some(s(2)));
        assert!(c.is_marked(2, t0 + CHANGED_MARK));
        assert!(c.prune(t0 + s(10)));
        assert_eq!(c.left(t0 + s(10)), None);
    }
}
