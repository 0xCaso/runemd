//! forge-empty: the screen with no file open shows the "RuneMD" wordmark, plate tight
//! (docs/decisions/2026-11-16-wordmark-plate.md), with the Rune fire in its letters, and the line
//! under it (docs/decisions/2026-11-16-metal-and-rune-effects.md).
//!
//! Three layers in the same box, in drawing order: the dark letters (`assets/wordmark/letters.svg`),
//! the fire slot over the letters' box (`fx::forge`), the chrome strip with the letters cut out
//! (`assets/wordmark/plate.png`, pre-rendered from `plate.svg`). Both SVGs are Outfit 800 outlines
//! made by `scripts/make_wordmark.py`: no font file at runtime. Rune decodes and rasterizes them
//! once per display scale, at the largest width (`PLATE_W`), on its image worker; a narrower window
//! draws them smaller from the mips, so resizing does not rasterize again.

use rune::{El, TextStyle, col, el, text};

use super::theme::{self, Theme};
use crate::fx::forge;

pub const LETTERS_SVG: &[u8] = include_bytes!("../../assets/wordmark/letters.svg");
/// The strip's source (tests check it against the constants; the app draws `PLATE_PNG`).
#[cfg(test)]
pub const PLATE_SVG: &[u8] = include_bytes!("../../assets/wordmark/plate.svg");
/// `plate.svg` rendered by headless Chrome at 2x `PLATE_W` (resvg loses its displacement filter).
pub const PLATE_PNG: &[u8] = include_bytes!("../../assets/wordmark/plate.png");

/// The strip in SVG units (`viewBox`).
pub const STRIP: [f32; 2] = [1360.0, 400.0];
/// The letters' ink box in SVG units: left, top (cap height), right, bottom (`data-ink`).
pub const INK: [f32; 4] = [113.4, 97.3, 1246.6, 313.4];
/// Baseline and cap height in SVG units (`data-baseline`, `data-cap`).
pub const BASELINE: f32 = 310.0;
pub const CAP: f32 = 212.7;

/// The strip's width on a window with room for it, pt.
pub const PLATE_W: f32 = 440.0;
/// The narrowest strip, pt: below it the pane is narrower than any window the app allows.
pub const PLATE_MIN_W: f32 = 200.0;
/// Room left and right of the strip, pt.
pub const MARGIN: f32 = 48.0;
/// Between the strip and the line under it, pt.
pub const GAP: f32 = 20.0;
/// The line under the wordmark.
pub const TAGLINE: &str = "Read the Markdown your agents write, while they write it.";

/// The strip's width in a pane `pane_w` wide.
pub fn plate_w(pane_w: f32) -> f32 {
    (pane_w - 2.0 * MARGIN).clamp(PLATE_MIN_W, PLATE_W)
}

/// The strip's size for width `w`, keeping the SVG's proportions.
pub fn plate_size(w: f32) -> [f32; 2] {
    [w, w * STRIP[1] / STRIP[0]]
}

/// The strip's corner radius for width `w` (56 SVG units: 14 % of the height).
pub fn plate_radius(w: f32) -> f32 {
    56.0 * w / STRIP[0]
}

/// The fire slot inside a strip `w` wide: the letters from the cap height to the baseline,
/// as [x, y, w, h] from the strip's top left.
pub fn letters_box(w: f32) -> [f32; 4] {
    let k = w / STRIP[0];
    [INK[0] * k, (BASELINE - CAP) * k, (INK[2] - INK[0]) * k, CAP * k]
}

/// What the empty screen needs from the app.
pub struct Forge {
    /// Since when (Rune clock) an agent writes a Markdown file in the watched folders; `None` when none does.
    pub writing: Option<f64>,
    pub reduced: bool,
    pub active_window: bool,
    /// The pane's width from the last layout (`None` before the first).
    pub pane_w: Option<f32>,
    /// The letters and the strip, decoded (`None` without an image cache, in tests).
    pub images: Option<[rune::Image; 2]>,
}

/// The empty screen: the wordmark and the line, centred in the pane.
pub fn view(f: Forge, theme: &Theme) -> El {
    let w = plate_w(f.pane_w.unwrap_or(PLATE_W + 2.0 * MARGIN));
    let [_, h] = plate_size(w);
    let [lx, ly, lw, lh] = letters_box(w);
    let m = forge::motion(f.writing.is_some(), f.reduced, f.active_window);
    let layer = |key: &str| el().key(key).pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0));
    let (letters, plate) = match &f.images {
        Some([l, p]) => (layer("forge-letters").pixels(l.pixels()), layer("forge-plate").pixels(p.pixels())),
        None => (layer("forge-letters"), layer("forge-plate")),
    };
    let fire = forge::slot(el().key("forge-fire").pin(Some(lx), Some(ly), None, None).w(lw).h(lh), m, f.writing.unwrap_or(0.0));
    // A soft shadow under the strip: on the light panel the chrome's light edges would melt into it.
    let shadow = if theme.dark { rune::Color::BLACK.alpha(0.45) } else { rune::Color::BLACK.alpha(0.16) };
    let mark = el().key("forge-mark").w(w).h(h).radius(plate_radius(w)).shadow(2.0, 10.0, shadow).label("RuneMD").child(letters).child(fire).child(plate);
    let line = text(TAGLINE).key("forge-line").font(TextStyle::new(theme::BODY_TEXT, theme::UI_WEIGHT)).color(theme.fg2);
    col().key("forge").w_full().h_full().items_center().justify_center().gap(GAP).child(mark).child(line)
}

#[cfg(test)]
mod tests {
    use rune::{Env, Runtime};

    use super::*;

    /// The constants are the ones the generator wrote into the SVGs.
    #[test]
    fn the_constants_match_the_svgs() {
        for svg in [LETTERS_SVG, PLATE_SVG] {
            let s = std::str::from_utf8(svg).expect("utf-8");
            assert!(s.contains(&format!("viewBox=\"0 0 {} {}\"", STRIP[0], STRIP[1])));
            assert!(s.contains(&format!("data-ink=\"{:.1} {:.1} {:.1} {:.1}\"", INK[0], INK[1], INK[2], INK[3])));
            assert!(s.contains(&format!("data-baseline=\"{BASELINE}\"")) && s.contains(&format!("data-cap=\"{CAP:.1}\"")));
            assert!(!s.contains("<text") && !s.contains("font-family"), "outlines only, no font");
        }
    }

    /// The strip PNG is 2x the widest strip, with the SVG's proportions.
    #[test]
    fn the_plate_png_is_2x_the_widest_strip() {
        assert_eq!(&PLATE_PNG[1..4], b"PNG");
        let be = |at: usize| u32::from_be_bytes(PLATE_PNG[at..at + 4].try_into().unwrap()) as f32;
        let (w, h) = (be(16), be(20));
        assert_eq!(w, 2.0 * PLATE_W);
        assert!((h - w * STRIP[1] / STRIP[0]).abs() <= 1.0, "{w} x {h}");
    }

    /// Plate tight: corner radius about 14 % of the strip height; the letters centred in the strip.
    #[test]
    fn the_strip_keeps_the_plate_tight_proportions() {
        let s = std::str::from_utf8(PLATE_SVG).unwrap();
        assert!(s.contains("rx=\"56\""));
        let [w, h] = plate_size(PLATE_W);
        assert!((plate_radius(w) / h - 0.14).abs() < 0.001, "the shadow follows the same corners");
        let (left, right) = (INK[0], STRIP[0] - INK[2]);
        assert!((left - right).abs() < 0.5, "letters centred: {left} / {right}");
        assert!((INK[1] - (BASELINE - CAP)).abs() < 0.5, "the ink starts at the cap height");
    }

    #[test]
    fn the_strip_fits_the_pane() {
        assert_eq!(plate_w(2000.0), PLATE_W);
        assert_eq!(plate_w(PLATE_W + 2.0 * MARGIN), PLATE_W);
        assert_eq!(plate_w(400.0), 400.0 - 2.0 * MARGIN);
        assert_eq!(plate_w(10.0), PLATE_MIN_W);
        let [w, h] = plate_size(440.0);
        assert!((w / h - STRIP[0] / STRIP[1]).abs() < 1e-4);
        let [x, y, bw, bh] = letters_box(440.0);
        assert!(x > 0.0 && y > 0.0 && x + bw < w && y + bh < h, "the letters inside the strip");
        assert!((bh - CAP * 440.0 / STRIP[0]).abs() < 1e-3);
    }

    fn empty_app() -> crate::App {
        crate::App::bare(crate::index::Library::load(Vec::new(), None))
    }

    fn settle(rt: &mut Runtime<crate::App>) {
        rt.frame(0.0);
        for _ in 0..200 {
            if !rt.frame(1.0 / 120.0) {
                break;
            }
        }
    }

    fn rect(rt: &Runtime<crate::App>, key: &str) -> [f32; 4] {
        rt.elements().iter().find(|e| !e.exiting && e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no {key}")).rect
    }

    /// With no file open the wordmark sits in the middle of the document pane, the fire slot on its letters.
    #[test]
    fn the_wordmark_is_centred_in_the_empty_pane() {
        let mut rt = Runtime::new(empty_app(), crate::view, (1280.0, 800.0), Env::new(true, 2.0));
        settle(&mut rt);
        let pane = rect(&rt, "forge");
        let mark = rect(&rt, "forge-mark");
        let line = rect(&rt, "forge-line");
        let mid = |r: [f32; 4]| r[0] + r[2] / 2.0;
        assert!((mid(mark) - mid(pane)).abs() < 1.0, "centred across: {mark:?} in {pane:?}");
        assert!((mid(line) - mid(pane)).abs() < 1.0, "the line too");
        let top = mark[1] - pane[1];
        let bottom = pane[1] + pane[3] - (line[1] + line[3]);
        assert!((top - bottom).abs() < 1.0, "centred down: {top} / {bottom}");
        assert_eq!(mark[2], PLATE_W);
        let fire = rect(&rt, "forge-fire");
        let [x, y, w, h] = letters_box(PLATE_W);
        for (a, b) in [(fire[0], mark[0] + x), (fire[1], mark[1] + y), (fire[2], w), (fire[3], h)] {
            assert!((a - b).abs() < 0.5, "fire slot on the letters: {fire:?}");
        }
    }

    /// A narrow window shrinks the strip and keeps the margins.
    #[test]
    fn a_narrow_window_shrinks_the_wordmark() {
        let mut rt = Runtime::new(empty_app(), crate::view, (620.0, 700.0), Env::new(true, 2.0));
        settle(&mut rt);
        let pane = rect(&rt, "forge");
        let mark = rect(&rt, "forge-mark");
        assert!(mark[2] < PLATE_W && mark[0] >= pane[0] + MARGIN - 0.5, "{mark:?} in {pane:?}");
    }

    fn motion_of(rt: &Runtime<crate::App>) -> forge::Motion {
        let id = crate::fx::Mark::Forge.id();
        let n = rt.instances.iter().enumerate().position(|(n, i)| i.params[1] == rune::render::KIND_EFFECT && rt.marks.get(n) == Some(&id)).expect("forge slot");
        forge::slot_state(rt.instances[n].uv, rt.clock).motion
    }

    /// The fire is calm with no agent writing and moves while one writes (the "writing" rings);
    /// with "reduce motion" or the window inactive it stays still.
    #[test]
    fn the_fire_follows_the_writing_signal() {
        let mut rt = Runtime::new(empty_app(), crate::view, (1280.0, 800.0), Env::new(true, 2.0));
        rt.track_marks = true;
        settle(&mut rt);
        assert_eq!(motion_of(&rt), forge::Motion::Calm);
        let writing = vec![(std::path::PathBuf::from("/tmp/notes.md"), std::time::Duration::from_secs(10))];
        let clock = rt.clock;
        let step = |rt: &mut Runtime<crate::App>, f: &dyn Fn(&mut crate::App)| {
            rt.update(|s, _| f(s));
            rt.frame(1.0 / 120.0);
            motion_of(rt)
        };
        assert_eq!(step(&mut rt, &|s| _ = s.ring_step(&writing, clock, std::time::Instant::now())), forge::Motion::Moving);
        assert_eq!(step(&mut rt, &|s| s.window_active = false), forge::Motion::Still, "inactive window");
        assert_eq!(step(&mut rt, &|s| (s.window_active, s.motion_env) = (true, Some(true))), forge::Motion::Still, "reduce motion");
        assert_eq!(step(&mut rt, &|s| s.motion_env = None), forge::Motion::Moving);
        assert_eq!(step(&mut rt, &|s| _ = s.ring_step(&[], clock + 11.0, std::time::Instant::now())), forge::Motion::Calm, "calm again when the agent stops");
    }

    /// Opening a file replaces the empty screen.
    #[test]
    fn an_open_file_hides_the_wordmark() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.md");
        std::fs::write(&p, "# A\n").unwrap();
        let mut app = empty_app();
        app.open_paths(vec![p]);
        let mut rt = Runtime::new(app, crate::view, (1280.0, 800.0), Env::new(true, 2.0));
        settle(&mut rt);
        assert!(!rt.elements().iter().any(|e| !e.exiting && e.key.as_deref() == Some("forge-mark")));
    }
}
