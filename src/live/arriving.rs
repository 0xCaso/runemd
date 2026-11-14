//! "Sta arrivando" e "cosa è cambiato" (spec, "Segnali live").
//!
//! - [`last_block_open`]: l'ultimo blocco del file è ancora aperto (fence non chiusa, oppure il
//!   file non finisce con un a capo). Con il file "sta scrivendo" accende il
//!   dither in fondo al documento.
//! - [`Changed`]: i blocchi nuovi o cambiati da una ricarica esterna, segnati per
//!   [`CHANGED_MARK`] (`CHANGED_MARK_MS` in `docs/agent-writes.md`).

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Per quanto resta il segno dei blocchi cambiati (`docs/agent-writes.md`: 8000 ms, `ipotesi`).
pub const CHANGED_MARK: Duration = Duration::from_millis(8000);

/// L'ultimo blocco di `source` (il file intero) è aperto: dentro una fence ``` o ~~~ non chiusa
/// (qualunque a capo ci sia dopo), oppure il file non finisce con un a capo. Un solo a capo
/// finale chiude il blocco: quasi tutti i file degli agenti finiscono così, e con la riga vuota
/// della spec il dither si vedrebbe a ogni scrittura, non "di rado" (decisione "Dither raro").
/// Un file vuoto o di soli spazi non ha blocchi: chiuso.
pub fn last_block_open(source: &str) -> bool {
    let body = source.trim_end();
    if body.is_empty() {
        return false;
    }
    let ends_line = source[body.len()..].contains('\n');
    open_fence(body) || !ends_line
}

/// Se `source` finisce dentro una fence di codice. Come CommonMark: fino a 3 spazi prima, almeno
/// 3 ``` o ~~~; chiude una riga con lo stesso carattere, almeno altrettanti, e nient'altro.
/// Le fence dentro citazioni ed elenchi non si contano (`ipotesi`: per l'ultimo blocco basta).
fn open_fence(source: &str) -> bool {
    let mut open: Option<(char, usize)> = None;
    for line in source.lines() {
        let indent = line.len() - line.trim_start_matches(' ').len();
        if indent > 3 {
            continue;
        }
        let rest = &line[indent..];
        let Some(c) = rest.chars().next().filter(|c| matches!(c, '`' | '~')) else { continue };
        let run = rest.len() - rest.trim_start_matches(c).len();
        if run < 3 {
            continue;
        }
        let after = &rest[run..];
        open = match open {
            None if c == '~' || !after.contains('`') => Some((c, run)),
            Some((oc, on)) if oc == c && run >= on && after.trim().is_empty() => None,
            other => other,
        };
    }
    open.is_some()
}

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
    fn open_fence_is_open() {
        assert!(last_block_open("a\n\n```rs\nfn"));
    }

    #[test]
    fn closed_fence_with_blank_line_is_closed() {
        assert!(!last_block_open("a\n\n```rs\nfn\n```\n\n"));
    }

    #[test]
    fn no_trailing_blank_line_is_open() {
        assert!(last_block_open("a\n\nparagrafo"));
    }

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
    fn a_final_newline_closes_the_last_block() {
        assert!(!last_block_open("a\n\nparagrafo\n"));
        assert!(!last_block_open("a\n\n```rs\nfn\n```\n"), "fence chiusa e a capo finale");
    }

    #[test]
    fn an_open_fence_stays_open_after_a_final_newline() {
        assert!(last_block_open("a\n\n```rs\nfn\n"));
        assert!(last_block_open("a\n\n```rs\nfn\n\n"), "anche con una riga vuota");
    }

    #[test]
    fn a_tilde_fence_and_a_longer_fence_count() {
        assert!(last_block_open("~~~\nx\n```\n\n"), "``` non chiude ~~~");
        assert!(last_block_open("````\nx\n```\n\n"), "``` non chiude ````");
        assert!(!last_block_open("````\nx\n`````\n\n"));
    }

    #[test]
    fn an_empty_file_is_closed() {
        assert!(!last_block_open(""));
        assert!(!last_block_open(" \n\n"));
        assert!(!last_block_open("a\n  \n"), "riga di soli spazi");
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
