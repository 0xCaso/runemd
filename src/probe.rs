//! Sonde di misura: primo frame con contenuto e intervalli tra frame durante uno scroll.

use std::cell::RefCell;
use std::io::Write;
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use rune::{FrameEncoded, FrameOutcome};

static START: OnceLock<(Instant, f64)> = OnceLock::new();

pub fn mark_start() {
    let epoch = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64() * 1000.0;
    START.get_or_init(|| (Instant::now(), epoch));
}

fn epoch_ms(at: Instant) -> f64 {
    let (i0, e0) = *START.get().expect("mark_start");
    e0 + at.saturating_duration_since(i0).as_secs_f64() * 1000.0
}

fn log(line: &str) {
    let Ok(path) = std::env::var("MDSPIKE_LOG") else { return };
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).expect("log");
    writeln!(f, "{line}").expect("log");
}

#[derive(Default)]
struct Frames {
    first: bool,
    recording: bool,
    /// (fine encoding, target del display link) dei frame `Drawn` durante lo scroll.
    drawn: Vec<(Instant, Option<Instant>)>,
    skipped: usize,
    refresh_hz: Option<f64>,
}

thread_local! { static FRAMES: RefCell<Frames> = RefCell::default(); }

pub fn on_frame(f: &FrameEncoded) {
    FRAMES.with_borrow_mut(|fr| {
        if !fr.first && f.outcome() == FrameOutcome::Drawn {
            fr.first = true;
            log(&format!("first_content_epoch_ms={:.1}", epoch_ms(f.at())));
            if std::env::var_os("MDSPIKE_EXIT").is_some() {
                std::thread::spawn(|| { std::thread::sleep(std::time::Duration::from_millis(300)); std::process::exit(0) });
            }
        }
        if fr.recording {
            fr.refresh_hz = fr.refresh_hz.or(f.refresh_hz());
            match f.outcome() {
                FrameOutcome::Drawn => fr.drawn.push((f.at(), f.presents_at())),
                FrameOutcome::Skipped => fr.skipped += 1,
                _ => {}
            }
        }
    });
}

/// Uno scroll a velocità costante, passo dopo passo da `poll`.
pub struct ScrollRun {
    secs: f64,
    speed: f64,
    start: Option<f64>,
    done: bool,
}

impl ScrollRun {
    pub fn from_env() -> Option<ScrollRun> {
        let secs = std::env::var("MDSPIKE_SCROLL").ok()?.parse().ok()?;
        let speed = std::env::var("MDSPIKE_SPEED").ok().and_then(|v| v.parse().ok()).unwrap_or(3000.0);
        Some(ScrollRun { secs, speed, start: None, done: false })
    }

    /// L'offset da applicare in questo frame, o `None` prima dell'inizio e dopo la fine.
    pub fn step(&mut self, clock: f64) -> Option<f32> {
        let start = *self.start.get_or_insert(clock + 1.0);
        if clock < start || self.done { return None; }
        let t = clock - start;
        if t > self.secs { self.done = true; return None; }
        FRAMES.with_borrow_mut(|fr| fr.recording = true);
        Some((t * self.speed) as f32)
    }

    pub fn finished(&self) -> bool { self.done }
}

pub fn finish_scroll() {
    FRAMES.with_borrow_mut(|fr| {
        fr.recording = false;
        let at: Vec<f64> = fr.drawn.iter().map(|d| epoch_ms(d.0)).collect();
        let dt: Vec<f64> = at.windows(2).map(|w| w[1] - w[0]).collect();
        let hz = fr.refresh_hz.unwrap_or(120.0);
        let period = 1000.0 / hz;
        // Un frame perso = un intervallo oltre 1,5 periodi; conta quanti refresh mancano.
        let missed: f64 = dt.iter().filter(|d| **d > period * 1.5).map(|d| (d / period).round() - 1.0).sum();
        let mut sorted = dt.clone();
        sorted.sort_by(f64::total_cmp);
        let pct = |p: f64| sorted.get(((sorted.len() as f64 - 1.0) * p).round() as usize).copied().unwrap_or(0.0);
        let span = at.last().unwrap_or(&0.0) - at.first().unwrap_or(&0.0);
        log(&format!(
            "scroll frames={} span_ms={span:.0} refresh_hz={hz:.0} dt_p50={:.2} dt_p95={:.2} dt_p99={:.2} dt_max={:.2} missed_refreshes={missed:.0} skipped={}",
            at.len(), pct(0.5), pct(0.95), pct(0.99), pct(1.0), fr.skipped
        ));
        log(&format!("scroll_dt_ms={}", dt.iter().map(|d| format!("{d:.2}")).collect::<Vec<_>>().join(",")));
    });
    std::process::exit(0);
}
