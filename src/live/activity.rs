//! "Writing" state: which files an agent wrote recently.
//!
//! `Live` calls `touch` for an applied external reload of the open file (never for the app's
//! own writes, skipped reads or held suspicious ones), for a `Changed` of another file
//! written recently (the rows of Recents and of the tree) and `git_head` for each change of
//! `.git/HEAD`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long the ring stays lit after the last write. Matteo's decision:
/// `docs/decisions/2026-11-13-writing-ring-10s.md`. The only constant, do not repeat it.
pub const WRITING_WINDOW_MS: u64 = 10_000;
pub const WRITING_WINDOW: Duration = Duration::from_millis(WRITING_WINDOW_MS);

/// A change of `.git/HEAD` in the same repo within this interval turns the writes off: it is a
/// checkout or a pull, not an agent (spec, "Writes that are not from an agent").
pub const GIT_QUIET: Duration = Duration::from_secs(2);

#[derive(Debug, Default)]
pub struct Activity {
    /// Last external write per file.
    touched: HashMap<PathBuf, Instant>,
    /// Last change of `.git/HEAD` per repo root.
    heads: HashMap<PathBuf, Instant>,
}

/// `a` and `b` are at most `d` apart, in any order.
fn within(a: Instant, b: Instant, d: Duration) -> bool {
    a.max(b).duration_since(a.min(b)) <= d
}

impl Activity {
    /// An agent wrote `path` at `now`. Ignored if the file's repo has just changed HEAD.
    pub fn touch(&mut self, path: &Path, now: Instant) {
        self.prune(now);
        if self.heads.iter().any(|(repo, at)| path.starts_with(repo) && within(*at, now, GIT_QUIET)) {
            return;
        }
        self.touched.insert(path.to_owned(), now);
    }

    /// `.git/HEAD` of `repo` changed at `now`: the repo's writes in the last `GIT_QUIET`
    /// (arrived before the HEAD event) and in the next ones do not light the ring.
    pub fn git_head(&mut self, repo: &Path, now: Instant) {
        self.prune(now);
        self.touched.retain(|p, at| !(p.starts_with(repo) && within(*at, now, GIT_QUIET)));
        self.heads.insert(repo.to_owned(), now);
    }

    /// The files that are "writing" at `now` and how soon they stop (the rows of Recents and
    /// of the tree).
    pub fn writing(&self, now: Instant) -> impl Iterator<Item = (&Path, Duration)> {
        self.touched.keys().filter_map(move |p| Some((p.as_path(), self.left(p, now)?)))
    }

    /// How soon `path` stops "writing", if it is now.
    pub fn left(&self, path: &Path, now: Instant) -> Option<Duration> {
        let end = *self.touched.get(path)? + WRITING_WINDOW;
        (now < end).then(|| end - now)
    }

    /// Forgets writes and HEADs that no longer count.
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
        assert!(a.left(Path::new("/r/b.md"), t0).is_none(), "only the touched file");
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
        // After 2 s a write in the repo is from an agent again.
        a.touch(p, t0 + ms(2_100));
        assert!(a.left(p, t0 + ms(2_100)).is_some());
    }

    #[test]
    fn git_head_after_the_reload_turns_the_ring_off() {
        let t0 = Instant::now();
        let p = Path::new("/r/x.md");
        let mut a = Activity::default();
        // The reload runs on another thread: it can arrive before the HEAD event.
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
