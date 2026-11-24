//! Crash log: a panic writes `~/Library/Logs/RuneMD/panic-<YYYYMMDD-HHMMSS>.log` (message, place,
//! version, backtrace) before the default hook runs, so a user can send it. Console.app lists that
//! folder under "Log Reports". Audit `docs/audit/2026-11-24-publish-readiness.md`, gap 5.

use std::fmt::Write as _;
use std::io::Write as _;
use std::panic::PanicHookInfo;
use std::path::{Path, PathBuf};

/// Installs the hook. Call once, first thing in `main`.
pub fn install() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Some(dir) = log_dir() {
            let text = report(info, &std::backtrace::Backtrace::force_capture().to_string());
            if let Ok(path) = write(&dir, &crate::edit::recovery::stamp_now(), &text) {
                eprintln!("runemd: crash log in {}", path.display());
            }
        }
        default(info);
    }));
}

fn log_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Logs/RuneMD"))
}

fn report(info: &PanicHookInfo<'_>, backtrace: &str) -> String {
    let message = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "(no message)".to_owned());
    let place = info.location().map_or_else(|| "(unknown)".to_owned(), |l| format!("{}:{}:{}", l.file(), l.line(), l.column()));
    let thread = std::thread::current().name().unwrap_or("(unnamed)").to_owned();
    let mut text = String::new();
    let _ = writeln!(text, "RuneMD {} crashed", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(text, "message: {message}");
    let _ = writeln!(text, "at: {place}");
    let _ = writeln!(text, "thread: {thread}");
    let _ = writeln!(text, "macOS: {}", os_version());
    let _ = write!(text, "\nbacktrace:\n{backtrace}");
    text
}

fn os_version() -> String {
    objc2_foundation::NSProcessInfo::processInfo().operatingSystemVersionString().to_string()
}

/// Writes `panic-<stamp>.log` in `dir` (made if missing). Never overwrites: a second crash in
/// the same second gets `-2`, `-3`, ….
fn write(dir: &Path, stamp: &str, text: &str) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    for n in 1.. {
        let name = if n == 1 { format!("panic-{stamp}.log") } else { format!("panic-{stamp}-{n}.log") };
        let path = dir.join(name);
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut f) => {
                f.write_all(text.as_bytes())?;
                return Ok(path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crash_log_is_written_and_never_overwritten() {
        let dir = std::env::temp_dir().join(format!("runemd-crash-{}", std::process::id())).join("Logs");
        let _ = std::fs::remove_dir_all(&dir);
        let a = write(&dir, "20261124-120000", "first").unwrap();
        let b = write(&dir, "20261124-120000", "second").unwrap();
        assert_eq!(a.file_name().unwrap(), "panic-20261124-120000.log");
        assert_eq!(b.file_name().unwrap(), "panic-20261124-120000-2.log");
        assert_eq!(std::fs::read_to_string(&a).unwrap(), "first");
        assert_eq!(std::fs::read_to_string(&b).unwrap(), "second");
        std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn the_report_names_the_message_and_the_place() {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let out = seen.clone();
        // The hook is global: only this thread's panic is recorded, others go to the old hook.
        let text = std::thread::Builder::new().name("crash-test".into()).spawn(move || {
            let hook = std::sync::Arc::new(std::panic::take_hook());
            let old = hook.clone();
            std::panic::set_hook(Box::new(move |info| {
                if std::thread::current().name() == Some("crash-test") {
                    *out.lock().unwrap() = report(info, "frames");
                } else {
                    old(info);
                }
            }));
            let _ = std::panic::catch_unwind(|| panic!("boom {}", 42));
            let _ = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| hook(info)));
            seen.lock().unwrap().clone()
        })
        .unwrap()
        .join()
        .unwrap();
        assert!(text.starts_with("RuneMD "), "{text}");
        assert!(text.contains("message: boom 42"), "{text}");
        assert!(text.contains("at: src/crash.rs:"), "{text}");
        assert!(text.ends_with("backtrace:\nframes"), "{text}");
    }
}
