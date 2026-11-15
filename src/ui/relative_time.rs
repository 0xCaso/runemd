//! Relative time on Recent rows: "now", "2 min", "3 h", "2 d".

use std::time::{Duration, SystemTime};

/// The elapsed time `age` as Recent shows it.
pub fn format(age: Duration) -> String {
    let secs = age.as_secs();
    match unit(secs) {
        MIN if secs < MIN => "now".to_owned(),
        MIN => format!("{} min", secs / MIN),
        HOUR => format!("{} h", secs / HOUR),
        _ => format!("{} d", secs / DAY),
    }
}

const MIN: u64 = 60;
const HOUR: u64 = 60 * MIN;
const DAY: u64 = 24 * HOUR;

/// The label's unit for `secs` elapsed: minutes under an hour ("now" included), hours under
/// a day, then days.
fn unit(secs: u64) -> u64 {
    if secs < HOUR {
        MIN
    } else if secs < DAY {
        HOUR
    } else {
        DAY
    }
}

/// When the label of a file modified at `mtime` changes: the next full minute, hour or day
/// after `mtime`, starting from `now`. A fixed instant (not "how long from now"), so whoever arms a
/// wakeup does not re-arm it on every poll.
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
        assert_eq!(format(s(30)), "now");
        assert_eq!(format(s(2 * MIN)), "2 min");
        assert_eq!(format(s(3 * HOUR)), "3 h");
        assert_eq!(format(s(2 * DAY)), "2 d");
    }

    #[test]
    fn edges_round_down() {
        assert_eq!(format(s(0)), "now");
        assert_eq!(format(s(59)), "now");
        assert_eq!(format(s(MIN)), "1 min");
        assert_eq!(format(s(HOUR - 1)), "59 min");
        assert_eq!(format(s(HOUR)), "1 h");
        assert_eq!(format(s(DAY - 1)), "23 h");
        assert_eq!(format(s(DAY)), "1 d");
    }

    #[test]
    fn next_change_is_the_next_unit_boundary() {
        let m = SystemTime::UNIX_EPOCH + s(1_000_000);
        assert_eq!(next_change(m, m + s(30)), m + s(MIN), "now → 1 min");
        assert_eq!(next_change(m, m + s(2 * MIN + 5)), m + s(3 * MIN));
        assert_eq!(next_change(m, m + s(3 * HOUR + 10)), m + s(4 * HOUR));
        assert_eq!(next_change(m, m + s(2 * DAY + 10)), m + s(3 * DAY));
        // Fixed between two close polls: the same instant.
        assert_eq!(next_change(m, m + s(125)), next_change(m, m + s(170)));
        // An mtime in the future (different clocks): "now" until one minute after the mtime.
        assert_eq!(next_change(m + s(10), m), m + s(10 + MIN));
    }
}
