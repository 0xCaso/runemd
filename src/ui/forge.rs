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

use std::path::{Path, PathBuf};

use rune::{El, TextStyle, Tx, col, el, icon, row, text};

use super::theme::{self, Theme};
use crate::App;
use crate::fx::{Mark, forge};

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

/// Between the line and the buttons, pt (the agent line hangs `GAP` under the buttons).
pub const BUTTONS_GAP: f32 = 16.0;
/// The buttons' height, pt (DESIGN.md `ROW` plus room for the key).
pub const BUTTON_H: f32 = 32.0;
/// The agent line's height, pt.
pub const AGENT_H: f32 = 40.0;

/// What the empty screen needs from the app.
pub struct Forge {
    /// Since when (Rune clock) an agent writes a Markdown file in the watched folders; `None` when none does.
    pub writing: Option<f64>,
    /// When the last agent stopped writing (Rune clock), if one wrote since the app started: the fire falls.
    pub stopped: Option<f64>,
    /// The file the agent writes (the last one that started), for the agent line.
    pub file: Option<PathBuf>,
    pub reduced: bool,
    pub active_window: bool,
    /// The pane's width from the last layout (`None` before the first).
    pub pane_w: Option<f32>,
    /// The letters and the strip, decoded (`None` without an image cache, in tests).
    pub images: Option<[rune::Image; 2]>,
}

/// The empty screen: the wordmark, the line, Open file and Add folder, centred in the pane; while an
/// agent writes, "An agent is writing <file> · Open" under the buttons, out of the flow so the
/// wordmark does not move when it comes and goes.
pub fn view(f: Forge, theme: &Theme) -> El {
    let w = plate_w(f.pane_w.unwrap_or(PLATE_W + 2.0 * MARGIN));
    let [_, h] = plate_size(w);
    let [lx, ly, lw, lh] = letters_box(w);
    let m = forge::motion(f.writing.is_some(), f.stopped.is_some(), f.reduced, f.active_window);
    let since = if f.writing.is_some() { f.writing } else { f.stopped };
    let layer = |key: &str| el().key(key).pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0));
    let (letters, plate) = match &f.images {
        Some([l, p]) => (layer("forge-letters").pixels(l.pixels()), layer("forge-plate").pixels(p.pixels())),
        None => (layer("forge-letters"), layer("forge-plate")),
    };
    let fire = forge::slot(el().key("forge-fire").pin(Some(lx), Some(ly), None, None).w(lw).h(lh), m, since.unwrap_or(0.0));
    // A soft shadow under the strip: on the light panel the chrome's light edges would melt into it.
    let shadow = if theme.dark { rune::Color::BLACK.alpha(0.45) } else { rune::Color::BLACK.alpha(0.16) };
    let mark = el().key("forge-mark").w(w).h(h).radius(plate_radius(w)).shadow(2.0, 10.0, shadow).label("RuneMD").child(letters).child(fire).child(plate);
    let line = text(TAGLINE).key("forge-line").font(TextStyle::new(theme::BODY_TEXT, theme::UI_WEIGHT)).color(theme.fg2);
    let buttons = row().key("forge-buttons").gap(8.0).mt(BUTTONS_GAP - GAP).child(open_file(theme)).child(add_folder(theme));
    // A zero-height anchor under the buttons: the agent line hangs from it.
    let agent = f.writing.zip(f.file).map(|(since, path)| agent_line(path, since, f.reduced, theme));
    let anchor = row().key("forge-anchor").w_full().h(0.0).justify_center().children(agent);
    col().key("forge").w_full().h_full().items_center().justify_center().gap(GAP).child(mark).child(line).child(buttons).child(anchor)
}

/// A key cap inside a button ("⌘P").
fn key_cap(label: &'static str, ink: rune::Color, line: rune::Color) -> El {
    el().border(theme::HAIRLINE, line).radius(theme::R_KBD).px(theme::KBD_PAD_X).py(theme::KBD_PAD_Y).child(text(label).font(TextStyle::new(theme::KBD_TEXT, theme::KBD_WEIGHT)).color(ink).a11y_hidden())
}

/// "Open file ⌘P": the ⌘P panel, as the key does. The primary button: `fg` fill, `panel` text.
fn open_file(theme: &Theme) -> El {
    let label = text("Open file").font(TextStyle::new(theme::UI_TEXT + 1.0, theme::UI_SMALL_WEIGHT)).color(theme.panel).a11y_hidden();
    row().key("forge-open-file").h(BUTTON_H).px(12.0).gap(8.0).center_y().radius(theme::R_FIELD).role("button").label("Open file").cursor(rune::Cursor::Pointer)
        .fill(theme.fg)
        .child(label)
        .child(key_cap("⌘P", theme.panel, theme.panel.alpha(0.35)))
        .on_press(|s: &mut App| {
            s.sidebar.new_file.close();
            s.sidebar.quick.show();
            s.requery_sidebar();
        })
}

/// "Add folder": the macOS folder dialog; the picked folder is saved, as from the sidebar.
fn add_folder(theme: &Theme) -> El {
    row().key("forge-add-folder").h(BUTTON_H).px(12.0).gap(8.0).center_y().radius(theme::R_FIELD).role("button").label("Add folder").cursor(rune::Cursor::Pointer)
        .border(theme::HAIRLINE, theme.line)
        .child(icon(&super::sidebar::PLUS_ICON, theme::ICON).color(theme.fg2))
        .child(text("Add folder").font(TextStyle::new(theme::UI_TEXT + 1.0, theme::UI_SMALL_WEIGHT)).color(theme.fg).a11y_hidden())
        .on_press_tx(|_: &mut App, tx: &mut Tx| {
            let directory = std::env::var_os("HOME").map(PathBuf::from);
            tx.open_folder(rune::FileDialogOptions { title: "Add folder".into(), directory, ..Default::default() }, |s: &mut App, result, _: &mut Tx| match result {
                Ok(Some(path)) => s.save_from_sidebar(path),
                Ok(None) => {}
                Err(e) => eprintln!("runemd: folder dialog: {e}"),
            });
        })
}

/// The file name the agent line shows.
pub fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// "An agent is writing <file> · Open", with the moving metal edge of a place that shows a file being
/// written (metal-tab, constraint 3). Open opens the file in a pinned tab.
fn agent_line(path: PathBuf, since: f64, reduced: bool, theme: &Theme) -> El {
    let font = TextStyle::new(theme::UI_TEXT, theme::UI_WEIGHT);
    let name = file_name(&path);
    let open = row().key("forge-agent-open").h(26.0).px(10.0).center_y().radius(theme::R_ROW).role("button").label(format!("Open {name}")).cursor(rune::Cursor::Pointer)
        .border(theme::HAIRLINE, theme.line)
        .child(text("Open").font(font).color(theme.fg).a11y_hidden())
        .on_press_tx(move |s: &mut App, tx: &mut Tx| s.pin_from_sidebar(path.clone(), tx));
    let body = row().key("forge-agent").h(AGENT_H).pl(14.0).pr(7.0).gap(10.0).center_y().radius(theme::R_FIELD).fill(theme.panel).max_w(560.0)
        .child(text("An agent is writing").font(font).color(theme.fg2))
        .child(text(name).font(TextStyle::new(theme::UI_TEXT, theme::UI_STRONG_WEIGHT)).color(theme.fg).truncate_middle().shrink().min_w(0.0))
        .child(open);
    crate::fx::ring::slot(body, Mark::ForgeLine, crate::fx::ring::edge(Some(since), false), reduced, theme.dark)
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
        let buttons = rect(&rt, "forge-buttons");
        assert!((mid(buttons) - mid(pane)).abs() < 1.0, "the buttons too");
        let anchor = rect(&rt, "forge-anchor");
        let top = mark[1] - pane[1];
        let bottom = pane[1] + pane[3] - (anchor[1] + anchor[3]);
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
        assert_eq!(step(&mut rt, &|s| _ = s.ring_step(&[], clock + 11.0, std::time::Instant::now())), forge::Motion::Falling, "it falls back when the agent stops");
        assert_eq!(step(&mut rt, &|s| s.motion_env = Some(true)), forge::Motion::Calm, "reduce motion: calm at once");
    }

    fn press(rt: &mut Runtime<crate::App>, key: &str) {
        let r = rect(rt, key);
        let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        settle(rt);
    }

    fn has(rt: &Runtime<crate::App>, key: &str) -> bool {
        rt.elements().iter().any(|e| !e.exiting && e.key.as_deref() == Some(key))
    }

    /// "Open file ⌘P" opens the ⌘P panel, as the key does.
    #[test]
    fn open_file_opens_the_quick_open_panel() {
        let mut rt = Runtime::new(empty_app(), crate::view, (1280.0, 800.0), Env::new(true, 2.0));
        settle(&mut rt);
        assert!(!rt.state.sidebar.quick.active());
        press(&mut rt, "forge-open-file");
        assert!(rt.state.sidebar.quick.active(), "the ⌘P panel is open");
    }

    /// While an agent writes, "An agent is writing <file> · Open" shows under the buttons without
    /// moving the wordmark, with the moving metal edge; Open opens the file in a pinned tab.
    #[test]
    fn the_agent_line_names_the_file_and_opens_it() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("2026-11-16-paths-name-and-link.md");
        std::fs::write(&p, "# Paths\n").unwrap();
        let mut rt = Runtime::new(empty_app(), crate::view, (1280.0, 800.0), Env::new(true, 2.0));
        rt.track_marks = true;
        settle(&mut rt);
        assert!(!has(&rt, "forge-agent"), "no line while calm");
        let before = rect(&rt, "forge-mark");
        let clock = rt.clock;
        let writing = vec![(p.clone(), std::time::Duration::from_secs(10))];
        rt.update(|s, _| _ = s.ring_step(&writing, clock, std::time::Instant::now()));
        settle(&mut rt);
        assert!(has(&rt, "forge-agent"));
        assert_eq!(rect(&rt, "forge-mark"), before, "the wordmark does not move");
        let texts: Vec<String> = rt.elements().iter().filter(|e| e.path.contains("forge-agent")).filter_map(|e| e.text.clone()).collect();
        assert!(texts.iter().any(|t| t == "An agent is writing") && texts.iter().any(|t| t == "2026-11-16-paths-name-and-link.md"), "{texts:?}");
        let line = rect(&rt, "forge-agent");
        let buttons = rect(&rt, "forge-buttons");
        assert!(line[1] >= buttons[1] + buttons[3], "under the buttons");
        assert!(crate::fx::edges(&rt).iter().any(|(m, calm)| *m == crate::fx::Mark::ForgeLine.id() && !calm), "moving edge on the line");
        press(&mut rt, "forge-agent-open");
        let tab = rt.state.tab().expect("a tab");
        assert_eq!(tab.doc.path, p);
        assert!(!tab.preview, "pinned");
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
