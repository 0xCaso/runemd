//! "Cosa è cambiato" (spec, "Segnali live"): [`Changed`], i blocchi nuovi o cambiati da una
//! ricarica esterna, segnati per [`CHANGED_MARK`] (`CHANGED_MARK_MS` in `docs/agent-writes.md`).
//! "Sta arrivando" non guarda più se l'ultimo blocco è aperto: il caret sta nel punto
//! dell'ultima modifica (`live::caret`).

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Per quanto resta il segno dei blocchi cambiati (`docs/agent-writes.md`: 8000 ms, `ipotesi`).
pub const CHANGED_MARK: Duration = Duration::from_millis(8000);

/// I blocchi segnati come cambiati, ognuno con la sua scadenza.
#[derive(Debug, Default)]
pub struct Changed {
    until: HashMap<u64, Instant>,
}

impl Changed {
    /// Segna `ids` a `now`: il segno resta per [`CHANGED_MARK`]. Un blocco già segnato riparte.
    pub fn mark(&mut self, ids: &[u64], now: Instant) {
        self.until.extend(ids.iter().map(|id| (*id, now + CHANGED_MARK)));
    }

    /// Se il blocco `id` ha il segno a `now`.
    pub fn is_marked(&self, id: u64, now: Instant) -> bool {
        self.until.get(&id).is_some_and(|end| now < *end)
    }

    /// Toglie i segni scaduti a `now`; vero se ne ha tolto almeno uno (la vista cambia).
    pub fn prune(&mut self, now: Instant) -> bool {
        let before = self.until.len();
        self.until.retain(|_, end| now < *end);
        self.until.len() != before
    }

    /// Fra quanto scade il prossimo segno; `None` se non ce ne sono.
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
        assert!(c.is_marked(7, t0 + ms(7900)), "visibile a 7,9 s");
        assert!(!c.is_marked(7, t0 + ms(8100)), "spento a 8,1 s");
        assert!(!c.is_marked(8, t0), "solo i blocchi segnati");
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
        assert!(c.prune(t0 + CHANGED_MARK), "il primo scade");
        assert_eq!(c.left(t0 + CHANGED_MARK), Some(s(2)));
        assert!(c.is_marked(2, t0 + CHANGED_MARK));
        assert!(c.prune(t0 + s(10)));
        assert_eq!(c.left(t0 + s(10)), None);
    }
}
