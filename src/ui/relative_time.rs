//! Tempo relativo delle righe di Recenti: "ora", "2 min", "3 h", "2 g".

use std::time::{Duration, SystemTime};

/// Il tempo trascorso `age` come lo mostra Recenti.
pub fn format(age: Duration) -> String {
    let secs = age.as_secs();
    match unit(secs) {
        MIN if secs < MIN => "ora".to_owned(),
        MIN => format!("{} min", secs / MIN),
        HOUR => format!("{} h", secs / HOUR),
        _ => format!("{} g", secs / DAY),
    }
}

const MIN: u64 = 60;
const HOUR: u64 = 60 * MIN;
const DAY: u64 = 24 * HOUR;

/// L'unità dell'etichetta per `secs` trascorsi: sotto l'ora i minuti ("ora" compreso), sotto
/// il giorno le ore, poi i giorni.
fn unit(secs: u64) -> u64 {
    if secs < HOUR {
        MIN
    } else if secs < DAY {
        HOUR
    } else {
        DAY
    }
}

/// Quando cambia l'etichetta di un file modificato a `mtime`: il prossimo minuto, ora o giorno
/// pieno dopo `mtime`, a partire da `now`. Un istante fisso (non "fra quanto"), così chi arma un
/// risveglio non lo riarma a ogni poll.
pub fn next_change(mtime: SystemTime, now: SystemTime) -> SystemTime {
    let secs = now.duration_since(mtime).map_or(0, |d| d.as_secs());
    let u = unit(secs);
    mtime + Duration::from_secs((secs / u + 1) * u)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn brief_examples() {
        assert_eq!(format(s(30)), "ora");
        assert_eq!(format(s(2 * MIN)), "2 min");
        assert_eq!(format(s(3 * HOUR)), "3 h");
        assert_eq!(format(s(2 * DAY)), "2 g");
    }

    #[test]
    fn edges_round_down() {
        assert_eq!(format(s(0)), "ora");
        assert_eq!(format(s(59)), "ora");
        assert_eq!(format(s(MIN)), "1 min");
        assert_eq!(format(s(HOUR - 1)), "59 min");
        assert_eq!(format(s(HOUR)), "1 h");
        assert_eq!(format(s(DAY - 1)), "23 h");
        assert_eq!(format(s(DAY)), "1 g");
    }

    #[test]
    fn next_change_is_the_next_unit_boundary() {
        let m = SystemTime::UNIX_EPOCH + s(1_000_000);
        assert_eq!(next_change(m, m + s(30)), m + s(MIN), "ora → 1 min");
        assert_eq!(next_change(m, m + s(2 * MIN + 5)), m + s(3 * MIN));
        assert_eq!(next_change(m, m + s(3 * HOUR + 10)), m + s(4 * HOUR));
        assert_eq!(next_change(m, m + s(2 * DAY + 10)), m + s(3 * DAY));
        // Fisso fra due poll vicini: lo stesso istante.
        assert_eq!(next_change(m, m + s(125)), next_change(m, m + s(170)));
        // Un mtime nel futuro (orologi diversi): "ora" fino a un minuto dopo l'mtime.
        assert_eq!(next_change(m + s(10), m), m + s(10 + MIN));
    }
}
