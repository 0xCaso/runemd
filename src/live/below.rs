//! "Testo nuovo più in basso": i blocchi nuovi o cambiati sotto la parte visibile (spec,
//! "Segnali live", fuoco).
//!
//! `on_change` riceve ogni ricarica applicata con l'id dell'ultimo blocco visibile prima della
//! ricarica e l'ordine dei blocchi dopo; `on_scroll` riceve l'indice dell'ultimo blocco visibile
//! a ogni cambio della parte visibile. Un blocco esce dal conto appena entra nella vista: il fuoco
//! sparisce quando ci entra l'ultimo.

use std::collections::{HashMap, HashSet};

use crate::doc::document::Change;

#[derive(Debug, Default)]
pub struct Below {
    /// Gli id dei blocchi nuovi o cambiati ancora sotto la vista.
    pending: HashSet<u64>,
}

impl Below {
    /// Una ricarica applicata. `visible_last` è l'id dell'ultimo blocco visibile prima della
    /// ricarica, `order` gli id dei blocchi dopo. Se `visible_last` non c'è più (rimosso, o la
    /// vista non ha ancora riportato righe) nessun blocco della ricarica conta: meglio un fuoco
    /// mancato che uno acceso su testo già in vista.
    pub fn on_change(&mut self, change: &Change, visible_last: u64, order: &[u64]) {
        let at = positions(order);
        self.pending.retain(|id| at.contains_key(id));
        let Some(&last) = at.get(&visible_last) else { return };
        let below = change.added.iter().chain(&change.changed).filter(|id| at.get(id).is_some_and(|&i| i > last));
        self.pending.extend(below);
    }

    /// La parte visibile è cambiata: `visible_last_index` è l'indice in `order` dell'ultimo
    /// blocco visibile. Escono dal conto i blocchi fino a quello compreso e quelli che non ci
    /// sono più.
    pub fn on_scroll(&mut self, visible_last_index: usize, order: &[u64]) {
        if self.pending.is_empty() {
            return;
        }
        let at = positions(order);
        self.pending.retain(|id| at.get(id).is_some_and(|&i| i > visible_last_index));
    }

    /// Quanti blocchi nuovi o cambiati sono sotto la vista.
    pub fn count(&self) -> usize {
        self.pending.len()
    }
}

/// La posizione di ogni id in `order`.
fn positions(order: &[u64]) -> HashMap<u64, usize> {
    order.iter().enumerate().map(|(i, id)| (*id, i)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Blocchi 1..=10; la vista finisce al blocco 5 (indice 4).
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
        // 3 blocchi nuovi in testa: 100, 101, 102 prima di 1..=10.
        let order: Vec<u64> = [100, 101, 102].into_iter().chain(1..=10).collect();
        b.on_change(&added(&[100, 101, 102]), 5, &order);
        assert_eq!(b.count(), 0);
        // Un blocco cambiato sopra la vista non conta, uno sotto sì.
        b.on_change(&Change { changed: vec![2, 8], ..Change::default() }, 5, &order);
        assert_eq!(b.count(), 1);
    }

    #[test]
    fn clears_when_last_new_block_visible() {
        let mut b = Below::default();
        let order = order(13);
        b.on_change(&added(&[11, 12, 13]), 5, &order);
        // Lo scroll porta in vista il primo dei nuovi: ne restano 2.
        b.on_scroll(10, &order);
        assert_eq!(b.count(), 2);
        // Entra in vista l'ultimo: il conto si azzera.
        b.on_scroll(12, &order);
        assert_eq!(b.count(), 0);
    }

    #[test]
    fn a_second_reload_does_not_count_twice_and_removed_blocks_leave() {
        let mut b = Below::default();
        b.on_change(&added(&[11, 12]), 5, &order(12));
        b.on_change(&Change { changed: vec![12], ..Change::default() }, 5, &order(12));
        assert_eq!(b.count(), 2, "un blocco cambiato due volte conta una");
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
