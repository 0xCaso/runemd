//! Copia di recupero: chiudendo la finestra o uscendo con un conflitto o un errore di scrittura,
//! Rune non lascia annullare la chiusura, quindi il testo del sorgente va in
//! `~/Library/Application Support/RuneMD/Recuperati/<nome>-<AAAAMMGG-HHMMSS>.md` (fuori dai repo:
//! agenti, indice e git non la vedono). Il file originale resta com'è. Al prossimo avvio ogni
//! copia nuova si apre in una scheda; la copia resta finché l'utente la cancella
//! (`docs/decisions/2026-11-14-copia-di-recupero.md`).

use std::fs::{self, OpenOptions};
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

/// Le copie da aprire al prossimo avvio, una per riga (il nome del file nella cartella).
const PENDING: &str = ".da-aprire";

/// La cartella delle copie: `Recuperati` nella cartella dati dell'app (sotto `RUNE_DIRS_ROOT` se
/// impostata, per le prove). `None` senza una cartella home.
pub fn default_dir() -> Option<PathBuf> {
    rune::dirs::AppDirs::new(crate::settings::APP_ID).map(|d| d.data().join("Recuperati"))
}

/// Scrive `text` in una copia nuova in `dir` per il file `original`, con l'ora `stamp`
/// (`AAAAMMGG-HHMMSS`), e la mette fra quelle da aprire al prossimo avvio. Non sovrascrive mai:
/// due copie con lo stesso nome nello stesso secondo prendono `-2`, `-3`…
pub fn write(dir: &Path, original: &Path, text: &str, stamp: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let stem = original
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "senza-nome".to_owned());
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

/// Le copie da aprire a questo avvio (quelle che esistono ancora), e la lista si svuota: un
/// avvio dopo non le riapre.
pub fn take_pending(dir: &Path) -> Vec<PathBuf> {
    let list = dir.join(PENDING);
    let Ok(text) = fs::read_to_string(&list) else {
        return Vec::new();
    };
    let _ = fs::remove_file(&list);
    text.lines()
        .filter(|l| !l.is_empty() && !l.contains('/'))
        .map(|l| dir.join(l))
        .filter(|p| p.is_file())
        .collect()
}

/// L'ora di adesso, locale, come `AAAAMMGG-HHMMSS`.
pub fn stamp_now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let offset = objc2_foundation::NSTimeZone::localTimeZone().secondsFromGMT() as i64;
    stamp(secs + offset)
}

/// `secs` dall'epoca (già spostati sul fuso locale) come `AAAAMMGG-HHMMSS`.
pub fn stamp(secs: i64) -> String {
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Da giorni dall'epoca a data civile (Howard Hinnant, `civil_from_days`).
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
        assert_eq!(stamp(951_782_400), "20000229-000000", "29 febbraio");
    }

    #[test]
    fn same_name_same_second_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(
            dir.path(),
            Path::new("/x/README.md"),
            "uno",
            "20261114-153007",
        )
        .unwrap();
        let b = write(
            dir.path(),
            Path::new("/y/README.md"),
            "due",
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
            ("uno".into(), "due".into())
        );
        assert_eq!(take_pending(dir.path()), vec![a, b]);
        assert!(take_pending(dir.path()).is_empty(), "una volta sola");
    }
}
