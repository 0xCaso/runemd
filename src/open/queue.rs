//! Queue of paths to open: the AppKit hook (main thread, but outside Rune's loop)
//! puts them here, `poll` takes them. A `Waker` wakes the loop when something arrives.

use std::path::PathBuf;
use std::sync::Mutex;

use rune::Waker;

static QUEUE: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static WAKER: Mutex<Option<Waker>> = Mutex::new(None);

/// The waker comes from `AppSpec::setup`. If the queue already has something (opened from Finder before
/// the window existed) it wakes the loop at once.
pub fn set_waker(waker: Waker) {
    let pending = !QUEUE.lock().unwrap().is_empty();
    *WAKER.lock().unwrap() = Some(waker.clone());
    if pending {
        waker();
    }
}

pub fn push(paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    QUEUE.lock().unwrap().extend(paths);
    let waker = WAKER.lock().unwrap().clone();
    if let Some(w) = waker {
        w();
    }
}

pub fn drain() -> Vec<PathBuf> {
    std::mem::take(&mut *QUEUE.lock().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_returns_in_order_and_empties() {
        let p = |s: &str| PathBuf::from(s);
        push(vec![p("a")]);
        push(vec![p("b"), p("c")]);
        assert_eq!(drain(), vec![p("a"), p("b"), p("c")]);
        assert!(drain().is_empty());
    }
}
