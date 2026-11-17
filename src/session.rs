//! The open tabs from one launch to the next: paths in their order, the front tab, which tab is the
//! preview, and the block at the top of each view. Saved in `prefs.json` under `session` when the
//! window closes or the app quits (`App::save_all`), read once at startup (`App::restore_session`).
//! Files that no longer exist are skipped.

use std::path::PathBuf;

use rune::prefs::{Prefs, PrefsError};
use serde::{Deserialize, Serialize};

const KEY: &str = "session";

/// One open tab.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SavedTab {
    pub path: PathBuf,
    /// The preview tab (italic name, replaced by the next single click).
    #[serde(default)]
    pub preview: bool,
    /// The index of the block at the top of the view (0: the start). An index, not a block id: ids
    /// are new at each launch, the order of the blocks is not.
    #[serde(default)]
    pub top: usize,
}

/// The open tabs and the front one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Session {
    pub tabs: Vec<SavedTab>,
    /// Index into `tabs`.
    #[serde(default)]
    pub front: usize,
}

impl Session {
    /// The saved session; none (or an unreadable one) gives an empty session.
    pub fn load(prefs: &Prefs) -> Session {
        prefs.get(KEY).unwrap_or_default()
    }

    pub fn save(&self, prefs: &mut Prefs) -> Result<(), PrefsError> {
        prefs.set(KEY, self)
    }

    /// The tabs whose file still exists, with the front index moved to match: if the front file is
    /// gone, the tab that took its place (or the last one) is in front.
    pub fn existing(self) -> Session {
        let front = self.front;
        let mut kept = Vec::new();
        let mut new_front = None;
        for (i, t) in self.tabs.into_iter().enumerate() {
            if !t.path.is_file() {
                continue;
            }
            if new_front.is_none() && i >= front {
                new_front = Some(kept.len());
            }
            kept.push(t);
        }
        let front = new_front.unwrap_or(kept.len().saturating_sub(1)).min(kept.len().saturating_sub(1));
        // At most one preview tab: the last one wins, as `App::open_preview` keeps it.
        let mut seen = false;
        for t in kept.iter_mut().rev() {
            if t.preview && std::mem::replace(&mut seen, true) {
                t.preview = false;
            }
        }
        Session { tabs: kept, front }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab(path: PathBuf, preview: bool, top: usize) -> SavedTab {
        SavedTab { path, preview, top }
    }

    #[test]
    fn a_session_round_trips_through_prefs_json() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("prefs.json");
        let s = Session { tabs: vec![tab("/a.md".into(), false, 3), tab("/b.md".into(), true, 0)], front: 1 };
        s.save(&mut Prefs::at(&file)).unwrap();
        assert_eq!(Session::load(&Prefs::at(&file)), s);
        assert_eq!(Session::load(&Prefs::memory()), Session::default(), "nothing saved: no tabs");
    }

    #[test]
    fn missing_files_are_skipped_and_the_front_follows() {
        let dir = tempfile::tempdir().unwrap();
        let p = |n: &str| dir.path().join(n);
        for n in ["a.md", "c.md", "d.md"] {
            std::fs::write(p(n), "# x\n").unwrap();
        }
        let s = Session { tabs: vec![tab(p("a.md"), false, 0), tab(p("b.md"), false, 0), tab(p("c.md"), false, 0), tab(p("d.md"), false, 0)], front: 2 };
        let e = s.clone().existing();
        assert_eq!(e.tabs.iter().map(|t| t.path.clone()).collect::<Vec<_>>(), [p("a.md"), p("c.md"), p("d.md")]);
        assert_eq!(e.front, 1, "c.md stays in front");
        let gone_front = Session { front: 1, ..s.clone() }.existing();
        assert_eq!(gone_front.front, 1, "b.md is gone: the next tab, c.md, is in front");
        let last_gone = Session { tabs: vec![tab(p("a.md"), false, 0), tab(p("b.md"), false, 0)], front: 1 }.existing();
        assert_eq!(last_gone.front, 0);
        let none = Session { tabs: vec![tab(p("x.md"), false, 0)], front: 0 }.existing();
        assert!(none.tabs.is_empty() && none.front == 0);
    }

    #[test]
    fn at_most_one_preview_comes_back() {
        let dir = tempfile::tempdir().unwrap();
        let p = |n: &str| dir.path().join(n);
        for n in ["a.md", "b.md"] {
            std::fs::write(p(n), "# x\n").unwrap();
        }
        let e = Session { tabs: vec![tab(p("a.md"), true, 0), tab(p("b.md"), true, 0)], front: 0 }.existing();
        assert_eq!(e.tabs.iter().map(|t| t.preview).collect::<Vec<_>>(), [false, true]);
    }
}
