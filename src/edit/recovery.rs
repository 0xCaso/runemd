//! Recovery copy: when closing the window or quitting with a conflict or a write error,
//! Rune does not allow cancelling the close, so the source text goes into
//! `~/Library/Application Support/RuneMD/Recovered/<name>-<YYYYMMDD-HHMMSS>.md` (outside the repos:
//! agents, index and git do not see it). The original file stays as it is. At the next startup each
//! new copy opens in a tab; the copy stays until the user deletes it
//! (`docs/decisions/2026-11-14-recovery-copy.md`).

use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

/// The copies to open at the next startup, one per line (the file name in the folder).
const PENDING: &str = ".to-open";

/// The Italian names used before 2026-11-15, moved by [`migrate`].
const OLD_DIR: &str = "Recuperati";
const OLD_PENDING: &str = ".da-aprire";

/// The folder of the copies: `Recovered` in the app's data folder (under `RUNE_DIRS_ROOT` if
/// set, for checks). `None` without a home folder.
pub fn default_dir() -> Option<PathBuf> {
    rune::dirs::AppDirs::new(crate::settings::APP_ID).map(|d| d.data().join("Recovered"))
}

/// One-time move from the old Italian names: the folder `Recuperati` next to `dir` becomes
/// `dir`, and the list `.da-aprire` in it becomes `.to-open`. It renames only when the old name
/// exists and the new one does not; errors are ignored (the copies stay where they are).
pub fn migrate(dir: &Path) {
    if let Some(old) = dir.parent().map(|p| p.join(OLD_DIR))
        && old.is_dir()
        && !dir.exists()
    {
        let _ = fs::rename(&old, dir);
    }
    let (old, new) = (dir.join(OLD_PENDING), dir.join(PENDING));
    if old.is_file() && !new.exists() {
        let _ = fs::rename(old, new);
    }
}

/// Writes `text` into a new copy in `dir` for the file `original`, with the time `stamp`
/// (`YYYYMMDD-HHMMSS`), and adds it to those to open at the next startup. It never overwrites:
/// two copies with the same name in the same second get `-2`, `-3`…
pub fn write(dir: &Path, original: &Path, text: &str, stamp: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let stem = original
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "untitled".to_owned());
    for n in 1.. {
        let name = if n == 1 {
            format!("{stem}-{stamp}.md")
        } else {
            format!("{stem}-{stamp}-{n}.md")
        };
        let path = dir.join(&name);
        let mut f = match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        };
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
        let mut list = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(PENDING))?;
        writeln!(list, "{name}")?;
        return Ok(path);
    }
    unreachable!()
}

/// The copies to open at this startup (the ones that still exist), and the list empties: a
/// later startup does not reopen them.
pub fn take_pending(dir: &Path) -> Vec<PathBuf> {
    let list = dir.join(PENDING);
    // A list with invalid bytes is read anyway (without loss for the good lines), so
    // it is removed and the later copies reopen. The copies on disk are never touched.
    let Ok(bytes) = fs::read(&list) else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&bytes);
    let _ = fs::remove_file(&list);
    text.lines()
        .filter(|l| !l.is_empty() && !l.contains('/'))
        .map(|l| dir.join(l))
        .filter(|p| p.is_file())
        .collect()
}

/// The current local time, as `YYYYMMDD-HHMMSS`.
pub fn stamp_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let offset = objc2_foundation::NSTimeZone::localTimeZone().secondsFromGMT() as i64;
    stamp(secs + offset)
}

/// `secs` since the epoch (already shifted to the local time zone) as `YYYYMMDD-HHMMSS`.
pub fn stamp(secs: i64) -> String {
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // From days since the epoch to a civil date (Howard Hinnant, `civil_from_days`).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_formats_a_date() {
        assert_eq!(stamp(0), "19700101-000000");
        // 2026-11-14 15:30:07
        assert_eq!(stamp(1_794_670_207), "20261114-153007");
        assert_eq!(stamp(951_782_400), "20000229-000000", "29 February");
    }

    #[test]
    fn same_name_same_second_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(
            dir.path(),
            Path::new("/x/README.md"),
            "one",
            "20261114-153007",
        )
        .unwrap();
        let b = write(
            dir.path(),
            Path::new("/y/README.md"),
            "two",
            "20261114-153007",
        )
        .unwrap();
        assert_eq!(a.file_name().unwrap(), "README-20261114-153007.md");
        assert_eq!(b.file_name().unwrap(), "README-20261114-153007-2.md");
        assert_eq!(
            (
                fs::read_to_string(&a).unwrap(),
                fs::read_to_string(&b).unwrap()
            ),
            ("one".into(), "two".into())
        );
        assert_eq!(take_pending(dir.path()), vec![a, b]);
        assert!(take_pending(dir.path()).is_empty(), "only once");
    }

    #[test]
    fn a_list_with_invalid_bytes_is_still_read_and_removed() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), Path::new("/x/a.md"), "one", "20261114-153007").unwrap();
        let mut list = fs::read(dir.path().join(PENDING)).unwrap();
        list.extend_from_slice(b"\xff\xfe broken\n");
        fs::write(dir.path().join(PENDING), list).unwrap();
        assert_eq!(take_pending(dir.path()), vec![a.clone()]);
        assert!(!dir.path().join(PENDING).exists(), "the list is removed");
        assert!(a.is_file(), "the copy stays");
        let b = write(dir.path(), Path::new("/x/b.md"), "two", "20261114-153008").unwrap();
        assert_eq!(take_pending(dir.path()), vec![b], "the later copies reopen");
    }

    #[test]
    fn migrate_moves_the_old_italian_names() {
        let support = tempfile::tempdir().unwrap();
        let old = support.path().join(OLD_DIR);
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join("a-20261114-153007.md"), "one").unwrap();
        fs::write(old.join(OLD_PENDING), "a-20261114-153007.md\n").unwrap();
        let dir = support.path().join("Recovered");
        migrate(&dir);
        assert!(!old.exists(), "the old folder moves");
        assert_eq!(fs::read_to_string(dir.join("a-20261114-153007.md")).unwrap(), "one");
        assert!(!dir.join(OLD_PENDING).exists());
        assert_eq!(take_pending(&dir), vec![dir.join("a-20261114-153007.md")]);
    }

    #[test]
    fn migrate_never_overwrites_the_new_names() {
        let support = tempfile::tempdir().unwrap();
        let old = support.path().join(OLD_DIR);
        let dir = support.path().join("Recovered");
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&dir).unwrap();
        fs::write(old.join("old.md"), "old").unwrap();
        fs::write(dir.join(OLD_PENDING), "x.md\n").unwrap();
        fs::write(dir.join(PENDING), "y.md\n").unwrap();
        migrate(&dir);
        assert!(old.join("old.md").is_file(), "the old folder stays when the new one exists");
        assert_eq!(fs::read_to_string(dir.join(PENDING)).unwrap(), "y.md\n");
        assert!(dir.join(OLD_PENDING).is_file());
        migrate(&support.path().join("missing").join("Recovered"));
    }

    #[test]
    fn a_file_without_a_name_is_untitled() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), Path::new("/"), "one", "20261114-153007").unwrap();
        assert_eq!(a.file_name().unwrap(), "untitled-20261114-153007.md");
    }
}
