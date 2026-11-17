//! Animation Lab tuning (docs: animation-lab spec "hub-protocollo", §5 RuneMD).
//!
//! The ring's and the fire's timings, curves and sizes as a [`LabTuning`], whose default is
//! today's constants (`ring::ENTER`, `ring::TURN`, `ring::RING_W`, `fire::RISE`, `fire::FALL`,
//! `fire::REST_H`, `fire::FIRE_H`). `fx::ring` and `fx::fire` read [`tuning`].
//!
//! In debug builds Animation Lab writes `lab.json` snapshots; [`apply_snapshot`] reads one into a
//! [`LabState`] and [`set_tuning`] makes its tuning the global (both wired to `--lab` later). In
//! release builds there is no global: [`tuning`] is the default.

pub mod bezier;

#[cfg(any(debug_assertions, test))]
use std::sync::RwLock;

use crate::fx::{fire, ring};

/// The writing ring: entrance (s) and its curve, one turn (s) and its curve, width (pt).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RingTuning {
    pub enter: f32,
    pub enter_ease: [f32; 4],
    pub turn: f32,
    pub turn_ease: [f32; 4],
    pub width: f32,
}

/// The fire: rise (s) and its curve, fall (s), resting and lit heights (pt, before `Look.height`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FireTuning {
    pub rise: f32,
    pub rise_ease: [f32; 4],
    pub fall: f32,
    pub rest_h: f32,
    pub lit_h: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LabTuning {
    pub ring: RingTuning,
    pub fire: FireTuning,
}

impl LabTuning {
    /// Today's constants.
    pub const DEFAULT: LabTuning = LabTuning {
        ring: RingTuning {
            enter: ring::ENTER,
            enter_ease: bezier::EASE_OUT_CUBIC,
            turn: ring::TURN,
            turn_ease: bezier::LINEAR,
            width: ring::RING_W,
        },
        fire: FireTuning {
            rise: fire::RISE,
            rise_ease: bezier::EASE_OUT_CUBIC,
            fall: fire::FALL,
            rest_h: fire::REST_H,
            lit_h: fire::FIRE_H,
        },
    };
}

impl Default for LabTuning {
    fn default() -> LabTuning {
        LabTuning::DEFAULT
    }
}

#[cfg(any(debug_assertions, test))]
static TUNING: RwLock<LabTuning> = RwLock::new(LabTuning::DEFAULT);

/// The tuning the effects use now: the default unless the lab set one ([`set_tuning`]).
pub fn tuning() -> LabTuning {
    #[cfg(any(debug_assertions, test))]
    {
        *TUNING.read().unwrap_or_else(|e| e.into_inner())
    }
    #[cfg(not(any(debug_assertions, test)))]
    {
        LabTuning::DEFAULT
    }
}

/// Makes `t` the tuning the effects read (the `--lab` watcher calls it).
#[cfg(any(debug_assertions, test))]
#[allow(dead_code)] // wired to `--lab`
pub(crate) fn set_tuning(t: LabTuning) {
    *TUNING.write().unwrap_or_else(|e| e.into_inner()) = t;
}

/// What the lab last sent: the tuning, the test time `t` (ms, `None` for the real clock) and the
/// `seq` of the snapshot it came from.
#[cfg(any(debug_assertions, test))]
#[allow(dead_code)] // wired to `--lab`
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LabState {
    pub tuning: LabTuning,
    pub t: Option<f64>,
    pub seq: u64,
}

/// Durations, ms (the lab's controls).
#[cfg(any(debug_assertions, test))]
const DURATION_MS: std::ops::RangeInclusive<f64> = 50.0..=6000.0;

/// Reads a `lab.json` snapshot (`{lab: 1, scene: "runemd", values, t, state, seq}`) into `state`.
/// A snapshot whose `seq` is not newer than the last one, or that does not parse, changes nothing.
/// Each snapshot is the whole state: a missing, unknown or out-of-range key leaves the default.
/// True if the tuning or `t` changed.
#[cfg(any(debug_assertions, test))]
#[allow(dead_code)] // wired to `--lab`
pub fn apply_snapshot(state: &mut LabState, json: &str) -> bool {
    use serde_json::Value;

    let Ok(v) = serde_json::from_str::<Value>(json) else { return false };
    if v.get("lab").and_then(Value::as_u64) != Some(1) || v.get("scene").and_then(Value::as_str) != Some("runemd") {
        return false;
    }
    let Some(values) = v.get("values").and_then(Value::as_object) else { return false };
    // `seq` is about an epoch in ms: an integer, or a float from JavaScript.
    let seq = match v.get("seq") {
        Some(n) => match n.as_u64() {
            Some(n) => n,
            None => match n.as_f64() {
                Some(f) if f.is_finite() && f >= 0.0 => f as u64,
                _ => return false,
            },
        },
        None => return false,
    };
    if seq <= state.seq {
        return false;
    }

    let num = |key: &str, range: std::ops::RangeInclusive<f64>| values.get(key).and_then(Value::as_f64).filter(|n| range.contains(n));
    let secs = |key: &str, default: f32| num(key, DURATION_MS).map_or(default, |ms| (ms / 1000.0) as f32);
    let pt = |key: &str, range: std::ops::RangeInclusive<f64>, default: f32| num(key, range).map_or(default, |n| n as f32);
    let curve = |key: &str, default: [f32; 4]| {
        let Some(a) = values.get(key).and_then(Value::as_array) else { return default };
        let n: Vec<f64> = a.iter().filter_map(Value::as_f64).filter(|n| n.is_finite()).collect();
        match n[..] {
            [x1, y1, x2, y2] if a.len() == 4 && (0.0..=1.0).contains(&x1) && (0.0..=1.0).contains(&x2) => [x1 as f32, y1 as f32, x2 as f32, y2 as f32],
            _ => default,
        }
    };

    let d = LabTuning::DEFAULT;
    let tuning = LabTuning {
        ring: RingTuning {
            enter: secs("ringEnter", d.ring.enter),
            enter_ease: curve("ringEnterEase", d.ring.enter_ease),
            turn: secs("ringTurn", d.ring.turn),
            turn_ease: curve("ringTurnEase", d.ring.turn_ease),
            width: pt("ringWidth", 0.5..=6.0, d.ring.width),
        },
        fire: FireTuning {
            rise: secs("fireRise", d.fire.rise),
            rise_ease: curve("fireRiseEase", d.fire.rise_ease),
            fall: secs("fireFall", d.fire.fall),
            rest_h: pt("fireRestH", 0.0..=80.0, d.fire.rest_h),
            lit_h: pt("fireLitH", 20.0..=220.0, d.fire.lit_h),
        },
    };
    let t = v.get("t").and_then(Value::as_f64);
    let changed = tuning != state.tuning || t != state.t;
    *state = LabState { tuning, t, seq };
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(seq: u64, values: &str, t: &str) -> String {
        format!(r#"{{"lab":1,"scene":"runemd","values":{{{values}}},"t":{t},"state":null,"seq":{seq}}}"#)
    }

    #[test]
    fn the_default_is_todays_constants() {
        let d = LabTuning::default();
        assert_eq!(d.ring.enter, ring::ENTER);
        assert_eq!(d.ring.turn, ring::TURN);
        assert_eq!(d.ring.width, ring::RING_W);
        assert_eq!(d.fire.rise, fire::RISE);
        assert_eq!(d.fire.fall, fire::FALL);
        assert_eq!(d.fire.lit_h, fire::FIRE_H);
        assert_eq!(d.fire.rest_h, fire::REST_H);
        assert_eq!(d.ring.enter_ease, [0.33, 1.0, 0.68, 1.0]);
        assert_eq!(d.ring.turn_ease, [0.0, 0.0, 1.0, 1.0]);
        assert_eq!(d.fire.rise_ease, [0.33, 1.0, 0.68, 1.0]);
    }

    #[test]
    fn without_lab_the_global_is_the_default() {
        assert_eq!(tuning(), LabTuning::default());
    }

    #[test]
    fn values_convert_from_ms_and_pt() {
        let mut s = LabState::default();
        let json = snap(5, r#""ringEnter":400,"ringTurn":1500.5,"fireRise":600,"fireFall":250,"ringWidth":4,"fireRestH":10,"fireLitH":150,"ringEnterEase":[0.1,0.2,0.3,0.4],"ringTurnEase":[0.4,0,0.6,1],"fireRiseEase":[0.2,1.2,0.5,1]"#, "null");
        assert!(apply_snapshot(&mut s, &json));
        let t = s.tuning;
        assert!((t.ring.enter - 0.4).abs() < 1e-6 && (t.ring.turn - 1.5005).abs() < 1e-6);
        assert!((t.fire.rise - 0.6).abs() < 1e-6 && (t.fire.fall - 0.25).abs() < 1e-6);
        assert_eq!((t.ring.width, t.fire.rest_h, t.fire.lit_h), (4.0, 10.0, 150.0));
        assert_eq!(t.ring.enter_ease, [0.1, 0.2, 0.3, 0.4]);
        assert_eq!(t.ring.turn_ease, [0.4, 0.0, 0.6, 1.0]);
        assert_eq!(t.fire.rise_ease, [0.2, 1.2, 0.5, 1.0]);
        assert_eq!(s.seq, 5);
    }

    /// Review Focus 1: a seq already seen (or older) is ignored.
    #[test]
    fn an_older_seq_is_ignored() {
        let mut s = LabState::default();
        assert!(apply_snapshot(&mut s, &snap(3, r#""ringTurn":1000"#, "null")));
        assert!(!apply_snapshot(&mut s, &snap(2, r#""ringTurn":2000"#, "null")));
        assert!(!apply_snapshot(&mut s, &snap(3, r#""ringTurn":2000"#, "null")));
        assert!((s.tuning.ring.turn - 1.0).abs() < 1e-6);
        assert_eq!(s.seq, 3);
    }

    #[test]
    fn a_float_epoch_seq_is_read() {
        let mut s = LabState::default();
        assert!(apply_snapshot(&mut s, &snap(0, r#""ringTurn":1000"#, "null").replace(r#""seq":0"#, r#""seq":1763400000123.0"#)));
        assert_eq!(s.seq, 1_763_400_000_123);
    }

    /// Review Focus 2: invalid JSON changes nothing.
    #[test]
    fn invalid_json_changes_nothing() {
        let mut s = LabState::default();
        assert!(apply_snapshot(&mut s, &snap(3, r#""ringTurn":1000"#, "1200")));
        let before = s;
        for bad in ["", "{", "not json", r#"{"lab":1,"scene":"runemd","values":[],"seq":9}"#, r#"{"lab":1,"scene":"other","values":{},"seq":9}"#, r#"{"lab":1,"scene":"runemd","values":{}}"#] {
            assert!(!apply_snapshot(&mut s, bad), "{bad}");
            assert_eq!(s, before, "{bad}");
        }
    }

    #[test]
    fn unknown_keys_are_ignored() {
        let mut s = LabState::default();
        assert!(!apply_snapshot(&mut s, &snap(1, r#""ringColor":"red","speed":3"#, "null")));
        assert_eq!(s.tuning, LabTuning::default());
        assert_eq!(s.seq, 1);
    }

    #[test]
    fn out_of_range_keeps_the_default() {
        let mut s = LabState::default();
        let json = snap(1, r#""ringTurn":99999,"ringEnter":10,"ringWidth":7,"fireRestH":-1,"fireLitH":300,"ringEnterEase":[1.5,0,0.5,1],"fireRiseEase":[0,0,1],"ringTurnEase":[0,"a",1,1]"#, "null");
        assert!(!apply_snapshot(&mut s, &json));
        assert_eq!(s.tuning, LabTuning::default());
    }

    /// A missing key is the default, not the previous value: each snapshot is the whole state.
    #[test]
    fn a_missing_key_goes_back_to_the_default() {
        let mut s = LabState::default();
        assert!(apply_snapshot(&mut s, &snap(1, r#""ringTurn":1000"#, "null")));
        assert!(apply_snapshot(&mut s, &snap(2, "", "null")));
        assert_eq!(s.tuning, LabTuning::default());
    }

    #[test]
    fn t_is_read_or_null() {
        let mut s = LabState::default();
        assert!(apply_snapshot(&mut s, &snap(1, "", "1200")));
        assert_eq!(s.t, Some(1200.0));
        assert!(apply_snapshot(&mut s, &snap(2, "", "null")));
        assert_eq!(s.t, None);
    }
}
