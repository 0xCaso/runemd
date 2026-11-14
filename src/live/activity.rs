//! Stato "sta scrivendo": quali file un agente ha scritto da poco.
//!
//! `Live` chiama `touch` per una ricarica esterna applicata del file aperto (mai per le scritture
//! dell'app, le letture saltate o quelle sospette trattenute), per un `Changed` di un altro file
//! scritto da poco (le righe di Recenti e dell'albero) e `git_head` per ogni cambio di
//! `.git/HEAD`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Quanto resta acceso l'anello dopo l'ultima scrittura. Decisione di Matteo:
/// `docs/decisions/2026-11-13-anello-sta-scrivendo-10s.md`. Unica costante, non ripeterla.
pub const WRITING_WINDOW_MS: u64 = 10_000;
pub const WRITING_WINDOW: Duration = Duration::from_millis(WRITING_WINDOW_MS);

/// Un cambio di `.git/HEAD` nello stesso repo in questo intervallo spegne le scritture: è un
/// checkout o un pull, non un agente (spec, "Scritture che non sono di un agente").
pub const GIT_QUIET: Duration = Duration::from_secs(2);

#[derive(Debug, Default)]
pub struct Activity {
    /// Ultima scrittura esterna per file.
    touched: HashMap<PathBuf, Instant>,
    /// Ultimo cambio di `.git/HEAD` per radice di repo.
    heads: HashMap<PathBuf, Instant>,
}

/// `a` e `b` distano al più `d`, in qualunque ordine.
fn within(a: Instant, b: Instant, d: Duration) -> bool {
    a.max(b).duration_since(a.min(b)) <= d
}

impl Activity {
    /// Un agente ha scritto `path` a `now`. Ignorato se il repo del file ha appena cambiato HEAD.
    pub fn touch(&mut self, path: &Path, now: Instant) {
        self.prune(now);
        if self.heads.iter().any(|(repo, at)| path.starts_with(repo) && within(*at, now, GIT_QUIET)) {
            return;
        }
        self.touched.insert(path.to_owned(), now);
    }

    /// `.git/HEAD` di `repo` è cambiato a `now`: le scritture del repo negli ultimi `GIT_QUIET`
    /// (arrivate prima dell'evento di HEAD) e nei prossimi non accendono l'anello.
    pub fn git_head(&mut self, repo: &Path, now: Instant) {
        self.prune(now);
        self.touched.retain(|p, at| !(p.starts_with(repo) && within(*at, now, GIT_QUIET)));
        self.heads.insert(repo.to_owned(), now);
    }

    /// I file che "stanno scrivendo" a `now` e fra quanto smettono (le righe di Recenti e
    /// dell'albero).
    pub fn writing(&self, now: Instant) -> impl Iterator<Item = (&Path, Duration)> {
        self.touched.keys().filter_map(move |p| Some((p.as_path(), self.left(p, now)?)))
    }

    /// Fra quanto `path` smette di essere "sta scrivendo", se lo è ora.
    pub fn left(&self, path: &Path, now: Instant) -> Option<Duration> {
        let end = *self.touched.get(path)? + WRITING_WINDOW;
        (now < end).then(|| end - now)
    }

    /// Dimentica scritture e HEAD che non contano più.
    fn prune(&mut self, now: Instant) {
        self.touched.retain(|_, at| now.saturating_duration_since(*at) < WRITING_WINDOW);
        self.heads.retain(|_, at| now.saturating_duration_since(*at) <= GIT_QUIET);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn writing_for_window_after_touch() {
        let t0 = Instant::now();
        let p = Path::new("/r/a.md");
        let mut a = Activity::default();
        assert!(a.left(p, t0).is_none());
        a.touch(p, t0);
        assert!(a.left(p, t0).is_some());
        assert!(a.left(p, t0 + ms(WRITING_WINDOW_MS - 100)).is_some(), "9,9 s");
        assert!(a.left(p, t0 + ms(WRITING_WINDOW_MS + 100)).is_none(), "10,1 s");
        assert!(a.left(Path::new("/r/b.md"), t0).is_none(), "solo il file toccato");
    }

    #[test]
    fn a_new_touch_extends_the_window() {
        let t0 = Instant::now();
        let p = Path::new("/r/a.md");
        let mut a = Activity::default();
        a.touch(p, t0);
        a.touch(p, t0 + ms(5_000));
        assert!(a.left(p, t0 + ms(WRITING_WINDOW_MS + 100)).is_some());
        assert_eq!(a.left(p, t0 + ms(5_000)), Some(WRITING_WINDOW));
    }

    #[test]
    fn git_checkout_does_not_light_ring() {
        let t0 = Instant::now();
        let (repo, p) = (Path::new("/r"), Path::new("/r/x.md"));
        let mut a = Activity::default();
        a.git_head(repo, t0);
        a.touch(p, t0 + ms(1_000));
        assert!(a.left(p, t0 + ms(1_000)).is_none());
        // Dopo 2 s una scrittura nel repo è di nuovo di un agente.
        a.touch(p, t0 + ms(2_100));
        assert!(a.left(p, t0 + ms(2_100)).is_some());
    }

    #[test]
    fn git_head_after_the_reload_turns_the_ring_off() {
        let t0 = Instant::now();
        let p = Path::new("/r/x.md");
        let mut a = Activity::default();
        // La ricarica gira su un altro thread: può arrivare prima dell'evento di HEAD.
        a.touch(p, t0);
        a.git_head(Path::new("/r"), t0 + ms(300));
        assert!(a.left(p, t0 + ms(300)).is_none());
    }

    #[test]
    fn git_head_of_another_repo_does_not_matter() {
        let t0 = Instant::now();
        let p = Path::new("/r/x.md");
        let mut a = Activity::default();
        a.git_head(Path::new("/s"), t0);
        a.git_head(Path::new("/rr"), t0);
        a.touch(p, t0 + ms(100));
        assert!(a.left(p, t0 + ms(100)).is_some());
    }
}
