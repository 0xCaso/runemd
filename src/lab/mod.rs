//! Animation Lab tuning (docs: animation-lab spec "hub-protocollo", §5 RuneMD).
//!
//! The ring's and the fire's timings, curves and sizes as a [`LabTuning`], whose default is
//! today's constants (`ring::ENTER`, `ring::TURN`, `ring::RING_W`, `fire::RISE`, `fire::FALL`,
//! `fire::REST_H`, `fire::FIRE_H`). `fx::ring` and `fx::fire` read [`tuning`].
//!
//! In debug builds `--lab` starts [`watch`]: it reads every `lab.json` snapshot into a [`LabState`]
//! ([`apply_snapshot`]) and publishes its tuning ([`tuning`]) and test time ([`lab_time`]). With a
//! test time the app draws the test state: the ring and the fire lit at [`REST_MS`], on the clock
//! `t` ([`ring_at`], [`fire_at`]). In release builds there is no global: [`tuning`] is the default
//! and [`lab_time`] is `None`.

pub mod bezier;
#[cfg(any(debug_assertions, test))]
pub mod watch;

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
/// The test time, ms: `t` of the last snapshot.
#[cfg(any(debug_assertions, test))]
static TIME: RwLock<Option<f64>> = RwLock::new(None);
/// A snapshot was published since `poll` last looked ([`take_changed`]).
#[cfg(any(debug_assertions, test))]
static CHANGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

// `cargo test` runs tests in parallel and many of them read the globals (rings, fire). In tests
// only the thread of a [`test_lab`] sees them; every other thread reads the default.
#[cfg(test)]
thread_local! {
    static SEES_LAB: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(any(debug_assertions, test))]
fn sees_lab() -> bool {
    #[cfg(test)]
    {
        SEES_LAB.get()
    }
    #[cfg(not(test))]
    {
        true
    }
}

/// The tuning the effects use now: the default unless the lab set one ([`set_tuning`]).
pub fn tuning() -> LabTuning {
    #[cfg(any(debug_assertions, test))]
    {
        if sees_lab() { *TUNING.read().unwrap_or_else(|e| e.into_inner()) } else { LabTuning::DEFAULT }
    }
    #[cfg(not(any(debug_assertions, test)))]
    {
        LabTuning::DEFAULT
    }
}

/// Makes `t` the tuning the effects read.
#[cfg(any(debug_assertions, test))]
pub(crate) fn set_tuning(t: LabTuning) {
    *TUNING.write().unwrap_or_else(|e| e.into_inner()) = t;
}

/// The test time the lab set (`t` of `lab.json`, ms), `None` for the real clock and states.
pub fn lab_time() -> Option<f64> {
    #[cfg(any(debug_assertions, test))]
    {
        if sees_lab() { *TIME.read().unwrap_or_else(|e| e.into_inner()) } else { None }
    }
    #[cfg(not(any(debug_assertions, test)))]
    {
        None
    }
}

/// Makes `state` the tuning and the test time the app reads, and marks the change for `poll`.
#[cfg(any(debug_assertions, test))]
pub(crate) fn publish(state: &LabState) {
    set_tuning(state.tuning);
    *TIME.write().unwrap_or_else(|e| e.into_inner()) = state.t;
    CHANGED.store(true, std::sync::atomic::Ordering::Release);
}

/// True once after each [`publish`]: `poll` runs the view again.
pub fn take_changed() -> bool {
    #[cfg(any(debug_assertions, test))]
    {
        sees_lab() && CHANGED.swap(false, std::sync::atomic::Ordering::AcqRel)
    }
    #[cfg(not(any(debug_assertions, test)))]
    {
        false
    }
}

/// A test that publishes to the globals holds this: one at a time, its thread sees them, and at the
/// end they go back to the default.
#[cfg(test)]
pub(crate) struct TestLab(#[allow(dead_code)] std::sync::MutexGuard<'static, ()>);

#[cfg(test)]
pub(crate) fn test_lab() -> TestLab {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    publish(&LabState::default());
    CHANGED.store(false, std::sync::atomic::Ordering::Release);
    SEES_LAB.set(true);
    TestLab(guard)
}

#[cfg(test)]
impl Drop for TestLab {
    fn drop(&mut self) {
        publish(&LabState::default());
        CHANGED.store(false, std::sync::atomic::Ordering::Release);
        SEES_LAB.set(false);
    }
}

/// What the lab last sent: the tuning, the test time `t` (ms, `None` for the real clock) and the
/// `seq` of the snapshot it came from.
#[cfg(any(debug_assertions, test))]
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

/// The lab's rest before the effects start, ms (`lib/scenes/runemd-live.ts`): fixed in the
/// protocol, not a parameter. In the test state the ring and the fire lit at `REST_MS`.
pub const REST_MS: f64 = 300.0;

/// The ring `tau_ms` after it lit: opacity `enter_ease(τ / enter)` and the turn's phase
/// `turn_ease((τ / turn) mod 1)`, 0..1. Before 0 it is off: `(0, 0)`. `fx::ring` draws with the same
/// two functions.
pub fn ring_at(tau_ms: f64, t: &RingTuning) -> (f32, f32) {
    if tau_ms < 0.0 {
        return (0.0, 0.0);
    }
    let s = (tau_ms / 1000.0) as f32;
    (ring::enter(s, *t), ring::phase(s, *t))
}

/// The fire's height, pt, `tau_ms` after it lit: `lerp(rest_h, lit_h, rise_ease(τ / rise))` up to
/// `rise`, then `lit_h`; `rest_h` before 0. `fx::fire` passes the heights already scaled by
/// `Look.height`.
pub fn fire_at(tau_ms: f64, t: &FireTuning) -> f32 {
    if tau_ms < 0.0 {
        return t.rest_h;
    }
    let s = (tau_ms / 1000.0) as f32;
    if s >= t.rise {
        return t.lit_h;
    }
    t.rest_h + (t.lit_h - t.rest_h) * bezier::ease(t.rise_ease, s / t.rise)
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

    /// The ten keys at their defaults, as the lab writes them, give the default bit for bit.
    #[test]
    fn the_defaults_as_json_are_the_default() {
        let mut s = LabState::default();
        let json = snap(1, r#""ringEnter":220,"ringEnterEase":[0.33,1,0.68,1],"ringTurn":3000,"ringTurnEase":[0,0,1,1],"ringWidth":2,"fireRise":420,"fireRiseEase":[0.33,1,0.68,1],"fireFall":300,"fireRestH":26,"fireLitH":110"#, "null");
        assert!(!apply_snapshot(&mut s, &json), "nothing changed");
        assert_eq!(s.seq, 1);
        assert_eq!(s.tuning, LabTuning::DEFAULT);
        let bits = |t: LabTuning| {
            let (r, f) = (t.ring, t.fire);
            let mut v = vec![r.enter, r.turn, r.width, f.rise, f.fall, f.rest_h, f.lit_h];
            v.extend(r.enter_ease.into_iter().chain(r.turn_ease).chain(f.rise_ease));
            v.into_iter().map(f32::to_bits).collect::<Vec<_>>()
        };
        assert_eq!(bits(s.tuning), bits(LabTuning::DEFAULT));
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

    /// §5 "time": for τ < 0 the ring is off.
    #[test]
    fn ring_at_before_rest_is_off() {
        assert_eq!(ring_at(-1.0, &LabTuning::DEFAULT.ring), (0.0, 0.0));
    }

    /// Opacity = enter_ease(τ / enter): at half the entrance, the curve at 0.5. The default curve
    /// is the exact ease-out cubic (`bezier::ease`); a tuned one is the CSS bezier.
    #[test]
    fn ring_at_half_the_entrance_is_the_curve_at_half() {
        let d = LabTuning::DEFAULT.ring;
        let half = f64::from(d.enter) * 1000.0 / 2.0;
        assert!((ring_at(half, &d).0 - bezier::ease(d.enter_ease, 0.5)).abs() < 1e-5);
        assert!((ring_at(110.0, &d).0 - bezier::ease(d.enter_ease, 0.5)).abs() < 1e-5);
        let tuned = RingTuning { enter_ease: [0.4, 0.0, 0.2, 1.0], ..d };
        assert!((ring_at(110.0, &tuned).0 - bezier::cubic_bezier(tuned.enter_ease, 0.5)).abs() < 1e-5);
    }

    /// Phase = turn_ease((τ / turn) mod 1): one turn later, the same phase.
    #[test]
    fn ring_at_one_turn_later_is_the_same_phase() {
        let d = LabTuning::DEFAULT.ring;
        let turn_ms = f64::from(d.turn) * 1000.0;
        let (a, b) = (ring_at(1500.0, &d).1, ring_at(1500.0 + turn_ms, &d).1);
        assert!((a - b).abs() < 1e-4, "{a} {b}");
        assert!((a - 0.5).abs() < 1e-4, "half a turn at 1.5 s of 3 s: {a}");
        let eased = RingTuning { turn_ease: [0.4, 0.0, 0.6, 1.0], ..d };
        assert!((ring_at(750.0, &eased).1 - bezier::cubic_bezier(eased.turn_ease, 0.25)).abs() < 1e-4);
    }

    /// Height = lerp(rest_h, lit_h, rise_ease(τ / rise)) up to `rise`, then lit_h; at rest before 0.
    #[test]
    fn fire_at_rises_from_rest_to_lit() {
        let d = LabTuning::DEFAULT.fire;
        let rise_ms = f64::from(d.rise) * 1000.0;
        assert_eq!(fire_at(-1.0, &d), d.rest_h);
        assert_eq!(fire_at(rise_ms, &d), d.lit_h);
        assert_eq!(fire_at(rise_ms * 3.0, &d), d.lit_h);
        let mid = d.rest_h + (d.lit_h - d.rest_h) * bezier::ease(d.rise_ease, 0.5);
        assert!((fire_at(rise_ms / 2.0, &d) - mid).abs() < 1e-4);
        let tuned = FireTuning { rise_ease: [0.4, 0.0, 0.2, 1.0], ..d };
        let mid = d.rest_h + (d.lit_h - d.rest_h) * bezier::cubic_bezier(tuned.rise_ease, 0.5);
        assert!((fire_at(rise_ms / 2.0, &tuned) - mid).abs() < 1e-4);
    }
}
