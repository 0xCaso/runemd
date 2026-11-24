//! The settings (spec, "Settings"): watched folders, font, size, theme and the fire at the
//! bottom of the panel, saved with `rune::prefs` in `~/Library/Application Support/RuneMD/prefs.json`.
//!
//! Every field has its own key: a value that is missing or cannot be read (another version
//! of the app) takes the default, the others stay.

use std::path::{Path, PathBuf};

use rune::prefs::{Prefs, PrefsError};
use serde::{Deserialize, Serialize};

use crate::fx::fire::{self, FireColor};
use crate::panes::{self, Pane};

/// The app id for `Prefs::open`: the same folder as the index (`index::cache_path`).
pub const APP_ID: &str = "RuneMD";

/// The default document font (DESIGN.md, `typography.doc-body`).
pub const DEFAULT_FONT: &str = "Inter";
/// The default body size, pt (DESIGN.md, `typography.doc-body`).
pub const DEFAULT_SIZE: f32 = 15.5;
/// The body sizes that "−" and "+" choose from, pt. Contains `DEFAULT_SIZE`.
pub const SIZES: [f32; 7] = [13.0, 14.0, 15.5, 17.0, 18.5, 20.0, 22.0];

/// "Full": the text column follows the panel width, minus the row margins.
pub const COLUMN_FULL: f32 = 0.0;
/// The text column widths ⌘⌥+ and ⌘⌥− step through, pt: from 680 (DESIGN.md, `spacing.column`) in
/// steps of 80, the last one "Full". The slider is continuous between `COLUMN_MIN` and `COLUMN_MAX`
/// (per pt, feedback group 3, item 5), then "Full" at its end.
pub const COLUMNS: [f32; 9] = [680.0, 760.0, 840.0, 920.0, 1000.0, 1080.0, 1160.0, 1240.0, COLUMN_FULL];
/// The default column width, pt.
pub const DEFAULT_COLUMN: f32 = 680.0;
/// The narrowest and widest column the slider gives before "Full", pt.
pub const COLUMN_MIN: f32 = 680.0;
pub const COLUMN_MAX: f32 = 1240.0;
/// The share of the slider for `COLUMN_MIN..=COLUMN_MAX`; past it, "Full". Where 1240 sat when the
/// slider had nine steps, so the thumb keeps its places.
const COLUMN_LINEAR: f32 = 7.0 / 8.0;
/// One arrow key on the slider, pt.
pub const COLUMN_NUDGE: f32 = 10.0;

/// A width the setting can hold: "Full" or a whole pt in `COLUMN_MIN..=COLUMN_MAX`.
pub fn valid_column(column: f32) -> bool {
    column == COLUMN_FULL || ((COLUMN_MIN..=COLUMN_MAX).contains(&column) && column.fract() == 0.0)
}

/// The width at slider point `t` (0..1): whole pt from `COLUMN_MIN` to `COLUMN_MAX` along
/// `COLUMN_LINEAR`, then `COLUMN_MAX`, and "Full" in the last half of what is left.
pub fn column_at(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t > (1.0 + COLUMN_LINEAR) / 2.0 {
        return COLUMN_FULL;
    }
    (COLUMN_MIN + (t / COLUMN_LINEAR).min(1.0) * (COLUMN_MAX - COLUMN_MIN)).round()
}

/// The slider point (0..1) of the width `column`.
pub fn column_t(column: f32) -> f32 {
    if column == COLUMN_FULL {
        return 1.0;
    }
    ((column - COLUMN_MIN) / (COLUMN_MAX - COLUMN_MIN)).clamp(0.0, 1.0) * COLUMN_LINEAR
}

/// The arrow keys on the slider: `COLUMN_NUDGE` pt in direction `by`; past `COLUMN_MAX`, "Full", and
/// back from "Full" to `COLUMN_MAX`.
pub fn nudge_column(column: f32, by: i32) -> f32 {
    match (column == COLUMN_FULL, by > 0) {
        (true, true) => COLUMN_FULL,
        (true, false) => COLUMN_MAX,
        (false, true) if column >= COLUMN_MAX => COLUMN_FULL,
        (false, _) => (column + by as f32 * COLUMN_NUDGE).clamp(COLUMN_MIN, COLUMN_MAX),
    }
}

/// How the panel writes the width: "680 pt", or "Full".
pub fn column_label(column: f32) -> String {
    if column == COLUMN_FULL {
        "Full".to_owned()
    } else {
        format!("{column:.0} pt")
    }
}

/// The maximum column width: `None` with "Full".
pub fn max_width(column: f32) -> Option<f32> {
    (column != COLUMN_FULL).then_some(column)
}

/// The default watched folders, relative to home.
pub const DEFAULT_ROOTS: [&str; 1] = ["Desktop/GitHub"];

/// The theme: the system's, or always light or always dark.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemePref {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemePref {
    /// Dark or not, given the system theme.
    pub fn dark(self, system_dark: bool) -> bool {
        match self {
            ThemePref::System => system_dark,
            ThemePref::Light => false,
            ThemePref::Dark => true,
        }
    }
}

/// When the fire at the bottom of the panel shows (decision `docs/decisions/2026-11-17-doc-fire-at-rest.md`).
/// The lit fire, with new text below the view, rises in Brief and Always.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FireMode {
    /// About 5 s when the window becomes active with a document and when a document comes to the
    /// front, and after the lit fire calms; then it fades out and the app stops waking for it.
    Brief,
    /// Always at rest while reading (v1.1): about 8–10 % CPU while the window is active.
    Always,
    /// No flames; the "N new paragraphs below" label stays. The default (feedback 2026-11-24, G4).
    #[default]
    Off,
}

impl FireMode {
    /// The saved value of the old on/off switch (`fire: bool`, before 2026-11-17).
    pub fn from_switch(on: bool) -> FireMode {
        if on { FireMode::Brief } else { FireMode::Off }
    }

    /// The flames can show (Brief or Always).
    pub fn shows(self) -> bool {
        self != FireMode::Off
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// The watched folders, as the user chose them (not canonical).
    pub roots: Vec<PathBuf>,
    /// The family of the body and headings; code stays monospaced.
    pub font: String,
    /// The body size, pt; headings, tables and code scale with it.
    pub size: f32,
    pub theme: ThemePref,
    /// The fire at the bottom of the panel (Matteo's decisions, `docs/decisions/2026-11-14-light-theme-fire.md`
    /// and `2026-11-17-doc-fire-at-rest.md`): Brief, Always or Off. Off means no flames, neither at
    /// rest nor with new text, and the "N new paragraphs below" label stays.
    pub fire: FireMode,
    /// Strength, height and speed of the fire: one of the steps of `fire::STRENGTHS`, `HEIGHTS`,
    /// `SPEEDS` (1 is today's fire).
    pub fire_strength: f32,
    pub fire_height: f32,
    pub fire_speed: f32,
    /// The color of the flames.
    pub fire_color: FireColor,
    /// The text column width, whole pt in `COLUMN_MIN..=COLUMN_MAX`, or `COLUMN_FULL` ("Full").
    pub column: f32,
    /// The sidebar and the heading outline: width and learned widths (`panes`).
    pub sidebar: Pane,
    pub outline: Pane,
    /// The folders saved in the sidebar, in the order they were saved (record
    /// `2026-11-15-folders-instead-of-tree`, constraint 4).
    pub saved: Vec<PathBuf>,
    /// Recent is collapsed in the sidebar (a click on its title, feedback group 3, item 7).
    pub recent_collapsed: bool,
    /// Full forge's charged edge on Warning and Caution callouts (decision
    /// `2026-11-18-full-forge-effects`, constraint 2): off by default, it costs CPU at rest.
    pub electric: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            roots: default_roots(),
            font: DEFAULT_FONT.to_owned(),
            size: DEFAULT_SIZE,
            theme: ThemePref::System,
            fire: FireMode::Off,
            fire_strength: 1.0,
            fire_height: 1.0,
            fire_speed: 1.0,
            fire_color: FireColor::Fire,
            column: DEFAULT_COLUMN,
            sidebar: Pane::new(panes::SIDEBAR),
            outline: Pane::new(panes::OUTLINE),
            saved: Vec::new(),
            recent_collapsed: false,
            electric: false,
        }
    }
}

/// `DEFAULT_ROOTS` under home; none without `HOME`.
pub fn default_roots() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    DEFAULT_ROOTS
        .iter()
        .map(|r| PathBuf::from(&home).join(r))
        .collect()
}

const ROOTS: &str = "roots";
const FONT: &str = "font";
const SIZE: &str = "size";
const THEME: &str = "theme";
/// The old on/off switch: still read when `fire_mode` is missing, and still written for older versions.
const FIRE: &str = "fire";
const FIRE_MODE: &str = "fire_mode";
const FIRE_STRENGTH: &str = "fire_strength";
const FIRE_HEIGHT: &str = "fire_height";
const FIRE_SPEED: &str = "fire_speed";
const FIRE_COLOR: &str = "fire_color";
const COLUMN: &str = "column";
const SIDEBAR: &str = "sidebar";
const OUTLINE: &str = "outline";
const SAVED: &str = "saved_folders";
const RECENT_COLLAPSED: &str = "recent_collapsed";
/// A new key: older prefs files have none (off), and older builds ignore it.
const ELECTRIC: &str = "electric_edge";

impl Settings {
    /// The settings saved in `prefs`; missing or invalid values take the default.
    pub fn load(prefs: &Prefs) -> Settings {
        let d = Settings::default();
        Settings {
            roots: prefs.get(ROOTS).unwrap_or(d.roots),
            font: prefs
                .get::<String>(FONT)
                .filter(|f| !f.trim().is_empty())
                .unwrap_or(d.font),
            size: prefs
                .get::<f32>(SIZE)
                .filter(|s| (SIZES[0]..=SIZES[SIZES.len() - 1]).contains(s))
                .unwrap_or(d.size),
            theme: prefs.get(THEME).unwrap_or(d.theme),
            fire: prefs
                .get(FIRE_MODE)
                .or_else(|| prefs.get(FIRE).map(FireMode::from_switch))
                .unwrap_or(d.fire),
            fire_strength: step_or(prefs, FIRE_STRENGTH, &fire::STRENGTHS, d.fire_strength),
            fire_height: step_or(prefs, FIRE_HEIGHT, &fire::HEIGHTS, d.fire_height),
            fire_speed: step_or(prefs, FIRE_SPEED, &fire::SPEEDS, d.fire_speed),
            fire_color: prefs.get(FIRE_COLOR).unwrap_or(d.fire_color),
            column: prefs
                .get::<f32>(COLUMN)
                .filter(|c| valid_column(*c))
                .unwrap_or(d.column),
            sidebar: prefs.get::<Pane>(SIDEBAR).map_or(d.sidebar, |p| p.sanitized(panes::SIDEBAR)),
            outline: prefs.get::<Pane>(OUTLINE).map_or(d.outline, |p| p.sanitized(panes::OUTLINE)),
            saved: prefs.get(SAVED).unwrap_or(d.saved),
            recent_collapsed: prefs.get(RECENT_COLLAPSED).unwrap_or(d.recent_collapsed),
            electric: prefs.get(ELECTRIC).unwrap_or(d.electric),
        }
    }

    /// Writes all the fields to `prefs` (and to disk, if `prefs` has a file).
    pub fn save(&self, prefs: &mut Prefs) -> Result<(), PrefsError> {
        prefs.set(ROOTS, &self.roots)?;
        prefs.set(FONT, &self.font)?;
        prefs.set(SIZE, &self.size)?;
        prefs.set(THEME, &self.theme)?;
        prefs.set(FIRE_MODE, &self.fire)?;
        prefs.set(FIRE, &self.fire.shows())?;
        prefs.set(FIRE_STRENGTH, &self.fire_strength)?;
        prefs.set(FIRE_HEIGHT, &self.fire_height)?;
        prefs.set(FIRE_SPEED, &self.fire_speed)?;
        prefs.set(FIRE_COLOR, &self.fire_color)?;
        prefs.set(COLUMN, &self.column)?;
        prefs.set(SIDEBAR, &self.sidebar)?;
        prefs.set(OUTLINE, &self.outline)?;
        prefs.set(SAVED, &self.saved)?;
        prefs.set(RECENT_COLLAPSED, &self.recent_collapsed)?;
        prefs.set(ELECTRIC, &self.electric)
    }

    /// The next width of `COLUMNS` in the direction `step` (−1 or +1) from the current one, which may
    /// sit between two (the slider is continuous); clamped at the ends.
    pub fn step_column(&self, step: i32) -> f32 {
        let c = self.column;
        let steps = &COLUMNS[..COLUMNS.len() - 1];
        if step > 0 {
            if c == COLUMN_FULL { COLUMN_FULL } else { steps.iter().copied().find(|x| *x > c).unwrap_or(COLUMN_FULL) }
        } else if c == COLUMN_FULL {
            COLUMN_MAX
        } else {
            steps.iter().copied().rev().find(|x| *x < c).unwrap_or(COLUMN_MIN)
        }
    }

    /// The current value of `knob`.
    pub fn fire_knob(&self, knob: FireKnob) -> f32 {
        match knob {
            FireKnob::Strength => self.fire_strength,
            FireKnob::Height => self.fire_height,
            FireKnob::Speed => self.fire_speed,
        }
    }

    /// Puts `v` in `knob` if it is one of its steps. True if it changed.
    pub fn set_fire_knob(&mut self, knob: FireKnob, v: f32) -> bool {
        if !knob.steps().contains(&v) || self.fire_knob(knob) == v {
            return false;
        }
        match knob {
            FireKnob::Strength => self.fire_strength = v,
            FireKnob::Height => self.fire_height = v,
            FireKnob::Speed => self.fire_speed = v,
        }
        true
    }

    /// The fire's look for the compositor, with the system accent `accent` (0xRRGGBB).
    pub fn fire_look(&self, accent: u32) -> fire::Look {
        fire::Look { strength: self.fire_strength, height: self.fire_height, speed: self.fire_speed, color: self.fire_color, accent }
    }

    /// Adds `root` if it is not already there. True if it changed.
    pub fn add_root(&mut self, root: &Path) -> bool {
        if self.roots.iter().any(|r| same(r, root)) {
            return false;
        }
        self.roots.push(root.to_owned());
        true
    }

    /// Removes `root`. True if it was there.
    pub fn remove_root(&mut self, root: &Path) -> bool {
        let before = self.roots.len();
        self.roots.retain(|r| !same(r, root));
        self.roots.len() != before
    }

    /// Saves the folder `root` in the sidebar, at the end, if it is not already there. True if it changed.
    pub fn save_folder(&mut self, root: &Path) -> bool {
        if self.saved.iter().any(|r| same(r, root)) {
            return false;
        }
        self.saved.push(root.to_owned());
        true
    }

    /// Removes the saved folder `root`. True if it was there.
    pub fn unsave_folder(&mut self, root: &Path) -> bool {
        let before = self.saved.len();
        self.saved.retain(|r| !same(r, root));
        self.saved.len() != before
    }

    /// The size after the current one in the direction `step` (−1 or +1); clamped at the ends.
    pub fn step_size(&self, step: i32) -> f32 {
        let at = SIZES
            .iter()
            .position(|s| *s >= self.size)
            .unwrap_or(SIZES.len() - 1);
        let at = if SIZES[at] > self.size && step > 0 {
            at
        } else {
            (at as i32 + step).clamp(0, SIZES.len() as i32 - 1) as usize
        };
        SIZES[at]
    }
}

/// A stepped slider of the fire (settings, fire section).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FireKnob {
    Strength,
    Height,
    Speed,
}

impl FireKnob {
    pub fn steps(self) -> &'static [f32] {
        match self {
            FireKnob::Strength => &fire::STRENGTHS,
            FireKnob::Height => &fire::HEIGHTS,
            FireKnob::Speed => &fire::SPEEDS,
        }
    }

    /// The slider point (0..1) of the value `v`; off-step, the default's (1).
    pub fn t(self, v: f32) -> f32 {
        let steps = self.steps();
        let at = steps.iter().position(|s| *s == v).or_else(|| steps.iter().position(|s| *s == 1.0)).unwrap_or(0);
        at as f32 / (steps.len() - 1) as f32
    }

    /// The step nearest to the slider point `t` (0..1).
    pub fn at(self, t: f32) -> f32 {
        let steps = self.steps();
        steps[(t.clamp(0.0, 1.0) * (steps.len() - 1) as f32).round() as usize]
    }

    /// The step after `v` in the direction `step` (−1 or +1), clamped at the ends.
    pub fn step(self, v: f32, step: i32) -> f32 {
        let steps = self.steps();
        let at = steps.iter().position(|s| *s == v).unwrap_or(0) as i32;
        steps[(at + step).clamp(0, steps.len() as i32 - 1) as usize]
    }

    /// How the panel writes the value: strength in percent, height in pt (of the lit fire),
    /// speed as "1.5×".
    pub fn label(self, v: f32) -> String {
        match self {
            FireKnob::Strength => format!("{:.0} %", v * 100.0),
            FireKnob::Height => format!("{:.0} pt", fire::lit_height(v)),
            FireKnob::Speed => format!("{}×", format!("{v}").replace('.', ",")),
        }
    }
}

/// The value of `key` in `prefs` if it is one of `steps`, otherwise `default`.
fn step_or(prefs: &Prefs, key: &str, steps: &[f32], default: f32) -> f32 {
    prefs.get::<f32>(key).filter(|v| steps.contains(v)).unwrap_or(default)
}

/// Two paths are the same folder (even if one is canonical and the other is not).
fn same(a: &Path, b: &Path) -> bool {
    a == b
        || a.canonicalize()
            .ok()
            .zip(b.canonicalize().ok())
            .is_some_and(|(a, b)| a == b)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn empty_prefs_give_the_defaults() {
        let s = Settings::load(&Prefs::memory());
        assert_eq!(s.font, "Inter");
        assert_eq!(s.size, 15.5);
        assert_eq!(s.theme, ThemePref::System);
        assert_eq!(s.fire, FireMode::Off, "the fire is off by default");
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        assert_eq!(s.roots, vec![home.join("Desktop/GitHub")]);
    }

    #[test]
    fn settings_round_trip_through_prefs() {
        let mut prefs = Prefs::memory();
        let s = Settings {
            roots: vec![PathBuf::from("/a"), PathBuf::from("/b c")],
            font: "New York".into(),
            size: 17.0,
            theme: ThemePref::Dark,
            fire: FireMode::Always,
            fire_strength: 1.5,
            fire_height: 0.8,
            fire_speed: 2.0,
            fire_color: FireColor::Violet,
            column: 920.0,
            sidebar: Pane { width: 300.0, learned: vec![300.0, 210.0] },
            outline: Pane { width: 240.0, learned: vec![240.0] },
            saved: vec![PathBuf::from("/a/runemd")],
            recent_collapsed: true,
            electric: true,
        };
        s.save(&mut prefs).unwrap();
        assert_eq!(Settings::load(&prefs), s);
    }

    #[test]
    fn settings_round_trip_through_a_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        let s = Settings {
            theme: ThemePref::Light,
            fire: FireMode::Off,
            ..Settings::default()
        };
        s.save(&mut Prefs::at(&path)).unwrap();
        assert_eq!(
            Settings::load(&Prefs::at(&path)),
            s,
            "persists across restarts"
        );
    }

    #[test]
    fn bad_values_fall_back_one_by_one() {
        let mut prefs = Prefs::memory();
        prefs.set(SIZE, &200.0f32).unwrap();
        prefs.set(FONT, "").unwrap();
        prefs.set(THEME, "Viola").unwrap();
        prefs.set(FIRE, &false).unwrap();
        let s = Settings::load(&prefs);
        assert_eq!(
            (s.size, s.font.as_str(), s.theme, s.fire),
            (15.5, "Inter", ThemePref::System, FireMode::Off)
        );
    }

    /// Decision 2026-11-18-full-forge-effects, constraint 2: the charged edge switch is off by
    /// default and in prefs files that predate it, and a save keeps it (and the other keys).
    #[test]
    fn the_electric_switch_is_off_by_default_and_saved() {
        assert!(!Settings::default().electric);
        let mut prefs = Prefs::memory();
        prefs.set(FIRE_MODE, &FireMode::Always).unwrap();
        let old = Settings::load(&prefs);
        assert!(!old.electric, "an older prefs file: off");
        assert_eq!(old.fire, FireMode::Always, "the rest reads as before");
        Settings { electric: true, ..old }.save(&mut prefs).unwrap();
        assert_eq!(prefs.get::<bool>(ELECTRIC), Some(true));
        assert!(Settings::load(&prefs).electric);
        prefs.set(ELECTRIC, &"yes").unwrap();
        assert!(!Settings::load(&prefs).electric, "unreadable: the default");
    }

    /// Decision 2026-11-17-doc-fire-at-rest, constraint 1: the old on/off switch (`fire`) reads as
    /// Brief (on) and Off (off); the new key wins over it, a value that cannot be read takes the
    /// default (Off), and saving keeps the old key for older versions (on unless Off).
    #[test]
    fn the_old_fire_switch_becomes_brief_or_off() {
        let old = |v: bool| {
            let mut prefs = Prefs::memory();
            prefs.set(FIRE, &v).unwrap();
            Settings::load(&prefs).fire
        };
        assert_eq!(old(true), FireMode::Brief);
        assert_eq!(old(false), FireMode::Off);
        let mut prefs = Prefs::memory();
        prefs.set(FIRE, &false).unwrap();
        prefs.set(FIRE_MODE, &FireMode::Always).unwrap();
        assert_eq!(Settings::load(&prefs).fire, FireMode::Always, "the new key wins");
        prefs.set(FIRE_MODE, &"Sometimes").unwrap();
        assert_eq!(Settings::load(&prefs).fire, FireMode::Off, "unreadable: the old key");
        let mut prefs = Prefs::memory();
        prefs.set(FIRE_MODE, &"Sometimes").unwrap();
        assert_eq!(Settings::load(&prefs).fire, FireMode::Off, "neither: the default");
        for (mode, on) in [(FireMode::Brief, true), (FireMode::Always, true), (FireMode::Off, false)] {
            let mut prefs = Prefs::memory();
            Settings { fire: mode, ..Settings::default() }.save(&mut prefs).unwrap();
            assert_eq!(prefs.get::<bool>(FIRE), Some(on), "{mode:?}: the old key for older versions");
            assert_eq!(Settings::load(&prefs).fire, mode);
        }
    }

    /// The text width: 680 by default; the shortcuts step by 80 up to 1240 and then "Full" (0), also from
    /// a width between two steps; the slider is continuous per pt (feedback group 3, item 5).
    #[test]
    fn text_width_steps_from_680_to_full() {
        let mut s = Settings::default();
        assert_eq!(s.column, 680.0);
        assert_eq!(COLUMNS[0], 680.0);
        assert_eq!(*COLUMNS.last().unwrap(), COLUMN_FULL);
        assert_eq!(s.step_column(-1), 680.0, "stops at the first");
        assert_eq!(s.step_column(1), 760.0);
        s.column = 1240.0;
        assert_eq!(s.step_column(1), COLUMN_FULL);
        s.column = COLUMN_FULL;
        assert_eq!(s.step_column(1), COLUMN_FULL, "stops at the last");
        assert_eq!(s.step_column(-1), 1240.0);
        s.column = 803.0;
        assert_eq!((s.step_column(1), s.step_column(-1)), (840.0, 760.0), "between two steps: the next one each way");
        s.column = 840.0;
        assert_eq!((s.step_column(1), s.step_column(-1)), (920.0, 760.0));
        assert_eq!(column_at(0.0), 680.0);
        assert_eq!(column_at(1.0), COLUMN_FULL);
        assert_eq!(column_at(-3.0), 680.0, "below 0: the first");
        assert_eq!(column_at(7.0), COLUMN_FULL, "above 1: Full");
        assert_eq!(column_t(COLUMN_FULL), 1.0);
        assert_eq!(column_t(123.0), 0.0, "below the range: the start");
    }

    /// The slider is continuous: every whole pt from 680 to 1240 has its own point, one pt apart is a
    /// small move, and the round trip gives the same width; 760 and 840 are no longer neighbours.
    #[test]
    fn the_width_slider_is_continuous_per_pt() {
        for c in (680..=1240).map(|c| c as f32) {
            assert_eq!(column_at(column_t(c)), c, "round trip at {c}");
            assert!(valid_column(c));
        }
        assert!(column_at(column_t(760.0) + 0.01) > 760.0 && column_at(column_t(760.0) + 0.01) < 840.0, "between the old steps");
        let one_pt = column_t(801.0) - column_t(800.0);
        assert!(one_pt > 0.0 && one_pt < 0.01, "one pt is a small move: {one_pt}");
        assert_eq!(column_at(0.5), 1000.0, "linear along the range");
        assert_eq!(column_t(1240.0), COLUMN_LINEAR, "1240 where the old step sat");
        assert_eq!(column_at(0.9), 1240.0, "past the range, before Full: 1240");
        assert!(!valid_column(679.0) && !valid_column(1241.0) && !valid_column(800.5));
        assert_eq!(nudge_column(800.0, 1), 810.0);
        assert_eq!(nudge_column(685.0, -1), 680.0, "clamped");
        assert_eq!(nudge_column(1240.0, 1), COLUMN_FULL);
        assert_eq!(nudge_column(COLUMN_FULL, -1), 1240.0);
    }

    /// A width between the old steps survives a restart; a broken one goes back to the default.
    #[test]
    fn a_continuous_width_is_saved() {
        let mut prefs = Prefs::memory();
        let s = Settings { column: 803.0, ..Settings::default() };
        s.save(&mut prefs).unwrap();
        assert_eq!(Settings::load(&prefs).column, 803.0);
        prefs.set(COLUMN, &803.5f32).unwrap();
        assert_eq!(Settings::load(&prefs).column, DEFAULT_COLUMN);
    }

    #[test]
    fn column_labels() {
        assert_eq!(column_label(680.0), "680 pt");
        assert_eq!(column_label(COLUMN_FULL), "Full");
        assert_eq!(max_width(680.0), Some(680.0));
        assert_eq!(max_width(COLUMN_FULL), None);
    }

    #[test]
    fn text_width_persists_and_bad_values_fall_back() {
        let mut prefs = Prefs::memory();
        let s = Settings { column: COLUMN_FULL, ..Settings::default() };
        s.save(&mut prefs).unwrap();
        assert_eq!(Settings::load(&prefs).column, COLUMN_FULL);
        prefs.set(COLUMN, &1300.0f32).unwrap();
        assert_eq!(Settings::load(&prefs).column, 680.0, "out of the range");
    }

    /// Pane widths come back after a restart; a saved value out of the limits comes back
    /// within the limits, one that cannot be read takes the default.
    #[test]
    fn pane_widths_persist_and_come_back_in_the_limits() {
        let mut prefs = Prefs::memory();
        let mut s = Settings::default();
        s.sidebar.learn(330.0, panes::SIDEBAR);
        s.outline.learn(260.0, panes::OUTLINE);
        s.save(&mut prefs).unwrap();
        let back = Settings::load(&prefs);
        assert_eq!(back.sidebar, Pane { width: 330.0, learned: vec![330.0] });
        assert_eq!(back.outline, Pane { width: 260.0, learned: vec![260.0] });
        prefs.set(SIDEBAR, &Pane { width: 5000.0, learned: vec![1.0, 200.0] }).unwrap();
        prefs.set(OUTLINE, &"largo").unwrap();
        let back = Settings::load(&prefs);
        assert_eq!(back.sidebar, Pane { width: panes::SIDEBAR.max, learned: vec![200.0] });
        assert_eq!(back.outline, Pane::new(panes::OUTLINE));
    }

    /// The default fire is today's; strength, height, speed and color come back after a
    /// restart; a value that is not a step (or a color that cannot be read) goes back to the default, the
    /// others stay.
    #[test]
    fn fire_look_persists_and_bad_values_fall_back() {
        let d = Settings::default();
        assert_eq!(d.fire_look(fire::ACCENT_DEFAULT), fire::Look::default());
        let mut prefs = Prefs::memory();
        let s = Settings { fire: FireMode::Off, fire_strength: 0.6, fire_height: 1.6, fire_speed: 0.25, fire_color: FireColor::Accent, ..Settings::default() };
        s.save(&mut prefs).unwrap();
        let back = Settings::load(&prefs);
        assert_eq!((back.fire_strength, back.fire_height, back.fire_speed, back.fire_color), (0.6, 1.6, 0.25, FireColor::Accent));
        prefs.set(FIRE_STRENGTH, &7.0f32).unwrap();
        prefs.set(FIRE_HEIGHT, &"tall").unwrap();
        prefs.set(FIRE_SPEED, &-1.0f32).unwrap();
        prefs.set(FIRE_COLOR, &"Orange").unwrap();
        let back = Settings::load(&prefs);
        assert_eq!((back.fire_strength, back.fire_height, back.fire_speed, back.fire_color), (1.0, 1.0, 1.0, FireColor::Fire));
        assert_eq!(back.fire, FireMode::Off, "the rest stays");
    }

    #[test]
    fn fire_knobs_step_clamp_and_read_in_italian() {
        let mut s = Settings::default();
        for knob in [FireKnob::Strength, FireKnob::Height, FireKnob::Speed] {
            let steps = knob.steps();
            assert_eq!(knob.at(-2.0), steps[0], "below 0: the first");
            assert_eq!(knob.at(5.0), *steps.last().unwrap(), "above 1: the last");
            assert_eq!(knob.at(knob.t(1.0)), 1.0);
            assert_eq!(knob.step(steps[0], -1), steps[0], "stops at the first");
            assert_eq!(knob.step(*steps.last().unwrap(), 1), *steps.last().unwrap(), "stops at the last");
            assert!(!s.set_fire_knob(knob, 0.123), "not a step");
            assert!(s.set_fire_knob(knob, steps[0]));
            assert_eq!(s.fire_knob(knob), steps[0]);
            assert!(!s.set_fire_knob(knob, steps[0]), "already so");
        }
        assert_eq!(FireKnob::Strength.label(1.25), "125 %");
        assert_eq!(FireKnob::Height.label(1.0), "110 pt");
        assert_eq!(FireKnob::Speed.label(1.5), "1,5×");
        assert_eq!(FireKnob::Speed.label(1.0), "1×");
    }

    #[test]
    fn theme_pref_overrides_the_system() {
        assert!(ThemePref::System.dark(true));
        assert!(!ThemePref::System.dark(false));
        assert!(!ThemePref::Light.dark(true));
        assert!(ThemePref::Dark.dark(false));
    }

    /// Saved folders stay from one launch to the next, in their order (record
    /// `2026-11-15-folders-instead-of-tree`, constraint 4); a folder is saved once and can be removed.
    #[test]
    fn saved_folders_round_trip_through_prefs_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("prefs.json");
        let mut s = Settings::default();
        assert!(s.save_folder(Path::new("/b/note")));
        assert!(s.save_folder(Path::new("/a/runemd")));
        assert!(!s.save_folder(Path::new("/b/note")), "already saved");
        s.save(&mut Prefs::at(&path)).unwrap();
        let back = Settings::load(&Prefs::at(&path));
        assert_eq!(back.saved, [PathBuf::from("/b/note"), PathBuf::from("/a/runemd")]);
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.contains("saved_folders"), "{raw}");
        let mut back = back;
        assert!(back.unsave_folder(Path::new("/b/note")));
        assert!(!back.unsave_folder(Path::new("/b/note")));
        back.save(&mut Prefs::at(&path)).unwrap();
        assert_eq!(Settings::load(&Prefs::at(&path)).saved, [PathBuf::from("/a/runemd")]);
        assert!(Settings::load(&Prefs::memory()).saved.is_empty(), "none by default");
    }

    #[test]
    fn roots_add_once_and_remove() {
        let mut s = Settings {
            roots: Vec::new(),
            ..Settings::default()
        };
        assert!(s.add_root(Path::new("/x")));
        assert!(!s.add_root(Path::new("/x")), "already there");
        assert!(s.remove_root(Path::new("/x")));
        assert!(!s.remove_root(Path::new("/x")));
        assert!(s.roots.is_empty());
    }

    #[test]
    fn size_steps_stop_at_the_ends() {
        let s = |size| Settings {
            size,
            ..Settings::default()
        };
        assert_eq!(s(15.5).step_size(1), 17.0);
        assert_eq!(s(15.5).step_size(-1), 14.0);
        assert_eq!(s(13.0).step_size(-1), 13.0);
        assert_eq!(s(22.0).step_size(1), 22.0);
        assert_eq!(
            s(16.0).step_size(1),
            17.0,
            "an off-scale value goes to the next step"
        );
        assert_eq!(s(16.0).step_size(-1), 15.5);
    }

    /// Adding a folder from the settings passes it to the watcher (`Watcher::add_root`, with its
    /// subfolders): a write below arrives as an event.
    #[test]
    fn adding_a_root_reaches_the_watcher() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        let mut app = crate::App::new(
            crate::index::Library::load(Vec::new(), None),
            Settings {
                roots: Vec::new(),
                ..Settings::default()
            },
            Prefs::memory(),
        );
        app.live.start(std::sync::Arc::new(|| {}), Vec::new());
        assert!(app.add_root(&root));
        assert_eq!(app.library.roots(), std::slice::from_ref(&root));
        assert_eq!(app.settings.roots, std::slice::from_ref(&root));
        std::thread::sleep(Duration::from_millis(300));
        let file = root.join("sub/a.md");
        std::fs::write(&file, "# a\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut seen = Vec::new();
        while Instant::now() < deadline
            && !seen.contains(&crate::watch::FsEvent::Changed(file.clone()))
        {
            app.live.drain(&[], Instant::now());
            seen.extend(app.live.drained().iter().cloned());
            std::thread::sleep(Duration::from_millis(16));
        }
        assert!(
            seen.contains(&crate::watch::FsEvent::Changed(file)),
            "{seen:?}"
        );
    }

    /// Removing a folder removes it from the watcher, from the library and from Recent, at once.
    #[test]
    fn removing_a_root_drops_its_recent_rows() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let (a, b) = (root.join("a"), root.join("b"));
        for d in [&a, &b] {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join("x.md"), "# x\n").unwrap();
        }
        let entries = crate::index::scan::scan(&[a.clone(), b.clone()]);
        let mut app = crate::App::new(
            crate::index::Library::load(vec![a.clone(), b.clone()], None),
            Settings {
                roots: vec![a.clone(), b.clone()],
                ..Settings::default()
            },
            Prefs::memory(),
        );
        app.library.index = crate::index::Index::from_entries(entries);
        app.live
            .start(std::sync::Arc::new(|| {}), vec![a.clone(), b.clone()]);
        app.sidebar
            .refresh(&app.library.index, &[], std::time::SystemTime::now());
        assert_eq!(app.sidebar.recent.len(), 2);
        assert!(app.remove_root(&a));
        assert_eq!(app.library.roots(), std::slice::from_ref(&b));
        assert_eq!(app.settings.roots, std::slice::from_ref(&b));
        assert!(!app.live.watched_roots().iter().any(|(r, _)| r == &a));
        assert_eq!(
            app.sidebar
                .recent
                .iter()
                .map(|r| r.path.clone())
                .collect::<Vec<_>>(),
            [b.join("x.md")]
        );
    }
}
