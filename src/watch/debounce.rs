//! Raggruppa raffiche di eventi sulla stessa chiave: esce una volta sola, a quiete.

use std::collections::HashMap;
use std::hash::Hash;
use std::time::{Duration, Instant};

pub struct Debounce<K> {
    window: Duration,
    last_seen: HashMap<K, Instant>,
}

impl<K: Hash + Eq + Clone> Debounce<K> {
    pub fn new(ms: u64) -> Self {
        Debounce { window: Duration::from_millis(ms), last_seen: HashMap::new() }
    }

    /// Registra un evento su `key` a `at`; ogni evento riparte la finestra.
    pub fn push(&mut self, key: K, at: Instant) {
        let slot = self.last_seen.entry(key).or_insert(at);
        if at > *slot {
            *slot = at;
        }
    }

    /// Le chiavi quiete da almeno la finestra a `now`; escono una volta sola.
    pub fn ready(&mut self, now: Instant) -> Vec<K> {
        let window = self.window;
        let done: Vec<K> = self
            .last_seen
            .iter()
            .filter(|(_, seen)| now.saturating_duration_since(**seen) >= window)
            .map(|(k, _)| k.clone())
            .collect();
        for key in &done {
            self.last_seen.remove(key);
        }
        done
    }

    pub fn is_empty(&self) -> bool {
        self.last_seen.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn burst_on_same_path_fires_once_after_quiet() {
        let t0 = Instant::now();
        let at = |ms| t0 + Duration::from_millis(ms);
        let mut d = Debounce::new(50);
        for ms in [0, 20, 40] {
            d.push("a.md", at(ms));
        }
        assert!(d.ready(at(60)).is_empty(), "la finestra riparte a ogni evento");
        assert_eq!(d.ready(at(90)), vec!["a.md"]);
        assert!(d.ready(at(200)).is_empty());
        assert!(d.is_empty());
    }
}
