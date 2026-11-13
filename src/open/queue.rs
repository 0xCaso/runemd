//! Coda dei percorsi da aprire: l'hook AppKit (thread principale, ma fuori dal ciclo di Rune)
//! li mette qui, `poll` li prende. Un `Waker` sveglia il ciclo quando arriva qualcosa.

use std::path::PathBuf;
use std::sync::Mutex;

use rune::Waker;

static QUEUE: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());
static WAKER: Mutex<Option<Waker>> = Mutex::new(None);

/// Il waker arriva da `AppSpec::setup`. Se la coda ha già qualcosa (apertura da Finder prima
/// che la finestra esistesse) sveglia subito il ciclo.
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
