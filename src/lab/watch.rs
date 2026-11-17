//! `--lab` (debug builds only): the bridge with Animation Lab (docs: animation-lab spec
//! "hub-protocollo", §5).
//!
//! A `notify` watcher of its own on the bridge folder, without debounce: `src/watch` keeps only
//! `.md` files and waits 50 ms. At every event on `lab.json` it reads the snapshot, publishes it
//! ([`super::publish`]) and wakes the app. A thread writes `ready.json` every second, the heartbeat
//! the lab reads to know RuneMD is there.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, mpsc};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use notify::{RecommendedWatcher, RecursiveMode, Watcher as _};
use rune::Waker;

use super::{LabState, apply_snapshot, publish};

/// The heartbeat's period: the lab calls it fresh under 2000 ms.
const BEAT: Duration = Duration::from_millis(1000);

/// `~/.animation-lab/bridge/runemd/`, where the lab writes `lab.json`.
#[cfg(debug_assertions)]
pub fn bridge_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".animation-lab").join("bridge").join("runemd"))
}

/// Whether the command line asks for the lab (`--lab`).
#[cfg(debug_assertions)]
pub fn enabled(args: &[String]) -> bool {
    args.iter().any(|a| a == "--lab")
}

/// Keeps the watcher and the heartbeat alive; dropping it stops both.
pub struct LabHandle {
    _watcher: Option<RecommendedWatcher>,
    /// Dropped with the handle: the heartbeat thread sees the channel close and ends.
    _beat: mpsc::Sender<()>,
}

/// Watches `dir` (created if missing) for `lab.json` and writes `ready.json` there every second.
/// A snapshot already there is read once the watcher is registered (the lab may have written it
/// before RuneMD started; a write in between is then seen by one or the other, and `seq` makes a
/// double read harmless).
pub fn start(dir: PathBuf, waker: Waker) -> LabHandle {
    if let Err(e) = std::fs::create_dir_all(&dir) {
        eprintln!("runemd: lab folder {}: {e}", dir.display());
    }
    let lab_json = dir.join("lab.json");
    let state = Arc::new(Mutex::new(LabState::default()));
    let handler = {
        let (lab_json, waker, state) = (lab_json.clone(), waker.clone(), state.clone());
        move |event: notify::Result<notify::Event>| {
            let event = match event {
                Ok(event) => event,
                Err(e) => return eprintln!("runemd: lab watcher error: {e}"),
            };
            if event.kind.is_access() || !event.paths.iter().any(|p| p.file_name() == Some("lab.json".as_ref())) {
                return;
            }
            read(&lab_json, &mut state.lock().unwrap_or_else(PoisonError::into_inner));
            waker();
        }
    };
    let watcher = match notify::recommended_watcher(handler) {
        Ok(mut w) => match w.watch(&dir, RecursiveMode::NonRecursive) {
            Ok(()) => Some(w),
            Err(e) => {
                eprintln!("runemd: lab watcher not started on {}: {e}", dir.display());
                None
            }
        },
        Err(e) => {
            eprintln!("runemd: lab watcher not started: {e}");
            None
        }
    };
    if read(&lab_json, &mut state.lock().unwrap_or_else(PoisonError::into_inner)) {
        waker();
    }
    let (tx, rx) = mpsc::channel::<()>();
    std::thread::Builder::new()
        .name("runemd-lab-beat".into())
        .spawn(move || {
            loop {
                beat(&dir);
                if rx.recv_timeout(BEAT) != Err(mpsc::RecvTimeoutError::Timeout) {
                    return;
                }
            }
        })
        .expect("lab heartbeat thread");
    LabHandle { _watcher: watcher, _beat: tx }
}

/// Reads `lab.json` into `state` and publishes it if it changed. True if it did.
fn read(lab_json: &Path, state: &mut LabState) -> bool {
    let Ok(json) = std::fs::read_to_string(lab_json) else { return false };
    let changed = apply_snapshot(state, &json);
    if changed {
        publish(state);
    }
    changed
}

/// Writes `ready.json` in `dir`, through a temporary file so the lab never reads half of it.
fn beat(dir: &Path) {
    let at = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
    let ready = serde_json::json!({ "lab": 1, "type": "ready", "scene": "runemd", "caps": { "time": true, "states": [] }, "pid": std::process::id(), "at": at });
    let tmp = dir.join("ready.json.tmp");
    let wrote = std::fs::write(&tmp, ready.to_string()).and_then(|()| std::fs::rename(&tmp, dir.join("ready.json")));
    if let Err(e) = wrote {
        eprintln!("runemd: ready.json: {e}");
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    use super::*;
    use crate::lab::{self, tuning};

    fn write_like_the_lab(dir: &std::path::Path, json: &str) {
        let tmp = dir.join("lab.json.1.1.tmp");
        std::fs::write(&tmp, json).unwrap();
        std::fs::rename(&tmp, dir.join("lab.json")).unwrap();
    }

    /// Waits for the first heartbeat: by then `start` registered the watcher.
    fn wait_for_ready(dir: &std::path::Path) {
        let t = Instant::now();
        while !dir.join("ready.json").exists() {
            assert!(t.elapsed() < Duration::from_secs(5), "no ready.json");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn lab_json_is_applied_within_200_ms_and_ready_json_beats() {
        let _lab = lab::test_lab();
        let dir = tempfile::tempdir().unwrap();
        let woken = Arc::new(AtomicUsize::new(0));
        let w = woken.clone();
        let started = Instant::now();
        let _handle = start(dir.path().to_path_buf(), Arc::new(move || {
            w.fetch_add(1, Ordering::SeqCst);
        }));
        wait_for_ready(dir.path());
        // FSEvents may start delivering a moment after registration: the lab's write is repeated
        // (a new `seq` each time) until it lands, and the first one must land within 200 ms.
        let written = Instant::now();
        let mut seq = 1_763_400_000_123u64;
        while tuning().ring.turn != 2.0 && written.elapsed() < Duration::from_millis(200) {
            write_like_the_lab(dir.path(), &format!(r#"{{"lab":1,"scene":"runemd","values":{{"ringTurn":2000}},"t":600,"state":null,"seq":{seq}}}"#));
            seq += 1;
            std::thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(tuning().ring.turn, 2.0, "applied within 200 ms");
        assert_eq!(lab::lab_time(), Some(600.0));
        assert!(woken.load(Ordering::SeqCst) > 0, "the app was woken");

        std::thread::sleep(Duration::from_millis(1200).saturating_sub(started.elapsed()));
        let ready: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(dir.path().join("ready.json")).expect("ready.json after 1.2 s")).unwrap();
        assert_eq!(ready["lab"], 1);
        assert_eq!(ready["type"], "ready");
        assert_eq!(ready["scene"], "runemd");
        assert_eq!(ready["caps"], serde_json::json!({ "time": true, "states": [] }));
        assert_eq!(ready["pid"], std::process::id());
        let at = ready["at"].as_u64().expect("at in ms");
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64;
        assert!(now.saturating_sub(at) < 1500, "fresh: {}", now - at);
    }

    /// A snapshot the lab wrote before RuneMD started is applied by `start` itself, and wakes the app.
    #[test]
    fn a_snapshot_already_there_is_applied_at_start() {
        let _lab = lab::test_lab();
        let dir = tempfile::tempdir().unwrap();
        write_like_the_lab(dir.path(), r#"{"lab":1,"scene":"runemd","values":{"ringWidth":4},"t":900,"state":null,"seq":7}"#);
        let woken = Arc::new(AtomicUsize::new(0));
        let w = woken.clone();
        let _handle = start(dir.path().to_path_buf(), Arc::new(move || {
            w.fetch_add(1, Ordering::SeqCst);
        }));
        assert_eq!((tuning().ring.width, lab::lab_time()), (4.0, Some(900.0)));
        assert!(woken.load(Ordering::SeqCst) > 0);
    }
}
