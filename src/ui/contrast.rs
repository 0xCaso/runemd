//! WCAG contrast of the `DESIGN.md` tokens ("Contrast" section). Tests only.
//!
//! The window draws on `Bgra8Unorm` (`srgb_blend`): a color with alpha is composited over the background
//! in encoded sRGB space, as in browsers. To measure a token with alpha, composite it
//! that way over its background, then compare the relative luminances.
//!
//! `cargo test --profile perf contrast_table -- --nocapture` prints the `DESIGN.md` table.

use rune::Color;

use super::theme::{Theme, focus_ring};

/// `fg` composited over `bg` (opaque) in encoded sRGB.
pub fn over(fg: Color, bg: Color) -> Color {
    super::theme::flatten(fg, bg)
}

/// WCAG 2 contrast ratio between `fg` composited over `bg`, and `bg`.
pub fn ratio(fg: Color, bg: Color) -> f32 {
    let (a, b) = (over(fg, bg).luminance(), bg.luminance());
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// The sidebar ground measured in the real window, system glass plus `glass` tint at 60 %
/// (2026-11-14, mean of 40×40 px; of two measurements the harder one; DESIGN.md "Real glass at 60 %"):
/// `[dark, light]`, each on the worst desktop ground (white in the dark theme, black in the
/// light one) and on the Sonoma wallpaper.
pub const GLASS_GROUNDS: [[&str; 2]; 2] = [["#3e3f40", "#374618"], ["#999a9c", "#becb9d"]];

/// The grounds measured on: the panel (also under `sel`) and the measured glass `glass` (one of
/// `GLASS_GROUNDS`), also under `sel`. In order: panel, panel + sel, glass, glass + sel.
pub fn grounds(t: &Theme, glass: Color) -> [Color; 4] {
    [t.panel, over(t.sel, t.panel), glass, over(t.sel, glass)]
}

/// The measured glass grounds for theme `t`.
pub fn glass_grounds(t: &Theme) -> [Color; 2] {
    GLASS_GROUNDS[usize::from(!t.dark)].map(|h| rune::color::hex(h).expect("hex"))
}

/// A table row: what is measured, the color and the background, and the required minimum.
pub struct Row {
    pub what: &'static str,
    pub fg: Color,
    pub bg: Color,
    pub min: f32,
}

/// The colors that are not tokens: the active Find match ring, the keyboard focus and
/// the double name of the chosen row (second line of Recent and of ⌘P).
pub struct Rings {
    pub active: Color,
    pub focus: Color,
    pub chosen: Color,
    /// The small text inside the sidebar's pressable rows, which take `sel` on hover: the
    /// Recent time, the double name, the "Open System Settings" link
    /// (`sidebar::row_meta`).
    pub row_meta: Color,
}

impl Rings {
    /// The app's ones: active ring in `fg2` (`markdown_style`), focus from [`focus_ring`] with
    /// the default system accent (blue).
    pub fn of(t: &Theme) -> Rings {
        let glass = t.on_glass();
        Rings { active: t.fg2, focus: focus_ring(rune::SystemAccent::DEFAULT.focus(t.dark), t.dark), chosen: glass.fg2, row_meta: super::sidebar::row_meta(&glass) }
    }
}

/// The marks and texts of theme `t`, each on its background, with the minimum (4.5 text, 3 meaningful
/// marks, 1 purely decorative lines and fills). `glass` is the measured glass ground
/// (`glass_grounds`); on glass the text uses `Theme::on_glass`.
pub fn rows(t: &Theme, rings: &Rings, glass: Color) -> Vec<Row> {
    let [panel, panel_sel, glass, glass_sel] = grounds(t, glass);
    let g = t.on_glass();
    let mark = 3.0;
    let row = |what, fg, bg, min| Row { what, fg, bg, min };
    vec![
        row("fg on panel", t.fg, panel, 4.5),
        row("body on panel", t.body, panel, 4.5),
        row("fg2 on panel", t.fg2, panel, 4.5),
        row("fg3 on panel", t.fg3, panel, 4.5),
        // Paths in the text (record 2026-11-16-paths-name-and-link): folders and `:line` in fg3,
        // name in fg; on hover ground sel, dots (line) fg2.
        row("path: folders fg3 on panel + sel (hover)", t.fg3, panel_sel, 4.5),
        row("path: name fg on panel + sel (hover)", t.fg, panel_sel, 4.5),
        row("path: line fg3 on panel", t.fg3, panel, mark),
        row("path: line fg2 on panel + sel (hover)", t.fg2, panel_sel, mark),
        row("fg on glass", t.fg, glass, 4.5),
        row("fg3 on glass", g.fg3, glass, 4.5),
        row("fg2 on glass", g.fg2, glass, 4.5),
        row("fg3 on glass + sel (hover)", g.fg3, glass_sel, 4.5),
        row("fg2 on glass + sel (hover)", g.fg2, glass_sel, 4.5),
        row("double name of the chosen row on glass + sel (fg3 before, fg2 after)", rings.chosen, glass_sel, 4.5),
        row("small text of pressable rows (Recent time, double name, link) on glass + sel (hover)", rings.row_meta, glass_sel, 4.5),
        row("Recent, second line (folder or repo and time) on glass", rings.row_meta, glass, 4.5),
        row("Recent, second line (folder or repo and time) on glass + sel (hover)", rings.row_meta, glass_sel, 4.5),
        row("Recent, name (fg) on glass + sel (hover)", t.fg, glass_sel, 4.5),
        row("tree title: repo name (fg) on glass", t.fg, glass, 4.5),
        row("tree title: branch (fg3) on glass", g.fg3, glass, 4.5),
        row("tree title: repo and branch icons (fg3) on glass", g.fg3, glass, mark),
        row("favourites: root name (fg) on glass + sel (hover)", t.fg, glass_sel, 4.5),
        row("favourites: unsaved root name (fg2) on glass", rings.row_meta, glass, 4.5),
        row("favourites: unsaved root name (fg2) on glass + sel (hover)", rings.row_meta, glass_sel, 4.5),
        row("favourites: root branch (fg3) on glass + sel (hover)", g.fg3, glass_sel, 4.5),
        row("favourites: chevron, repo icons, folder, branch and + (fg3) on glass + sel (hover)", g.fg3, glass_sel, mark),
        row("favourites: empty star (fg3) on glass", g.fg3, glass, mark),
        row("favourites: empty star (fg3) on glass + sel (row hover)", g.fg3, glass_sel, mark),
        row("favourites: filled star edge on glass", t.star().1, glass, mark),
        row("favourites: \"Add folder\" (fg2) on glass", rings.row_meta, glass, 4.5),
        row("favourites: \"Add folder\" (fg2) on glass + sel (hover)", rings.row_meta, glass_sel, 4.5),
        row("highlighted outline heading (fg) on panel + sel (hover)", t.fg, panel_sel, 4.5),
        row("outline headings (fg2) on panel + sel (hover)", t.fg2, panel_sel, 4.5),
        row("mark on panel (Find ring, changed mark)", t.mark, panel, mark),
        row("mark on sel (Find ring, inner side)", t.mark, panel_sel, mark),
        row("active Find ring on sel", rings.active, panel_sel, 3.0),
        row("keyboard focus on glass", rings.focus, glass, mark),
        row("keyboard focus on panel", rings.focus, panel, mark),
        // Settings (Cmd+,, `ui::settings`), on the panel.
        row("settings: chosen option (fg) on panel + sel", t.fg, panel_sel, 4.5),
        row("settings: check of the chosen option (fg) on panel + sel", t.fg, panel_sel, mark),
        row("settings: options and buttons (fg2) on panel + sel (hover)", t.fg2, panel_sel, 4.5),
        row("document: task checkbox off, fg3 border on panel", t.fg3, panel, mark),
        row("document: task checkbox on, fg2 on panel", t.fg2, panel, mark),
        row("document: task checkbox check (panel) on fg2", t.panel, t.fg2, mark),
        row("document: task checkbox off, fg3 border on panel + sel (selection)", t.fg3, panel_sel, mark),
        row("document: task checkbox on, fg2 on panel + sel (selection)", t.fg2, panel_sel, mark),
        row("document: heading 6 (fg3) on panel", t.fg3, panel, 4.5),
        // `Task N` headings (record 2026-11-16-tasks-sections-and-columns, constraint 3): number and
        // progress "2 of 4" in fg3.
        row("document: Task heading number (fg3) on panel", t.fg3, panel, 4.5),
        row("document: Task heading progress (fg3) on panel", t.fg3, panel, 4.5),
        row("document: heading 5, footnotes, callout icon (fg2) on panel", t.fg2, panel, 4.5),
        row("settings: checkbox off, fg3 border on panel", t.fg3, panel, mark),
        row("settings: checkbox on, fg2 on panel", t.fg2, panel, mark),
        row("settings: checkbox check (panel) on fg2", t.panel, t.fg2, mark),
        row("settings: folder icon (fg3) on panel", t.fg3, panel, mark),
        row("settings: checkbox off, fg3 border on panel + sel (hover)", t.fg3, panel_sel, mark),
        row("settings: checkbox on, fg2 on panel + sel (hover)", t.fg2, panel_sel, mark),
        row("settings: fire switch (fg2) on panel + sel (hover)", t.fg2, panel_sel, 4.5),
        row("settings: \"Already watched\" (fg2) on panel", t.fg2, panel, 4.5),
        row("fire-off label (fg2) on panel", t.fg2, panel, 4.5),
        row("frontmatter: text (fg2) on panel + half sel", t.meta().text, over(t.meta().fill, panel), 4.5),
        // Tab names (`ui::tabs::tab_row`): front in `fg` on `sel`, the others in `fg2`, hover `sel`.
        row("tab name, front (fg) on panel + sel", t.fg, panel_sel, 4.5),
        row("tab name, back (fg2) on panel", t.fg2, panel, 4.5),
        row("tab name, back on hover (fg2) on panel + sel", t.fg2, panel_sel, 4.5),
        row("\"⌘E source\" key (fg3) on panel", t.fg3, panel, 4.5),
        // Sidebar "Open file ⌘P" field (`quick_open::launcher`): plain theme on the `field` fill over glass.
        row("Open file field: placeholder (fg2) on field over glass", t.fg2, over(t.field, glass), 4.5),
        row("Open file field: ⌘P key (fg2) on field over glass", t.fg2, over(t.field, glass), 4.5),
        row("Open file field: search icon (fg2) on field over glass", t.fg2, over(t.field, glass), mark),
        row("tab × (fg2) on panel + sel (front tab or hover)", t.fg2, panel_sel, mark),
        row("tab × (fg2) on hover on panel + sel + sel", t.fg2, over(t.sel, panel_sel), mark),
        row("width ↔ button (fg2) on panel", t.fg2, panel, mark),
        row("width ↔ button (fg2) on panel + sel (hover, popover open)", t.fg2, panel_sel, mark),
        row("width popover: title (fg2) on panel", t.fg2, panel, 4.5),
        row("width popover: value (fg) on panel", t.fg, panel, 4.5),
        row("width popover: shortcuts (fg3) on panel", t.fg3, panel, 4.5),
        row("width popover: track (fg3) on panel", t.fg3, panel, mark),
        // Shortcuts (⌘ button and popover, `ui::shortcuts`), on the panel.
        row("⌘ shortcuts button (fg2) on panel", t.fg2, panel, mark),
        row("⌘ shortcuts button (fg2) on panel + sel (hover, popover open)", t.fg2, panel_sel, mark),
        row("shortcuts popover: title and rows (fg2) on panel", t.fg2, panel, 4.5),
        row("settings › shortcuts: group headings (fg3) on panel", super::shortcuts::heading_color(t), panel, 4.5),
        row("shortcuts popover: keys (fg2) on panel", t.fg2, panel, 4.5),
        row("shortcuts popover: key border (line) on panel, decorative", t.line, panel, 1.0),
        // Compact ⌘P (`ui::quick_open::panel`), on the panel: rows with hover and choice `sel`.
        row("⌘P: name (fg) on panel + sel (chosen, hover)", t.fg, panel_sel, 4.5),
        row("⌘P: repo or folder and time (fg2) on panel", t.fg2, panel, 4.5),
        row("⌘P: repo or folder and time (fg2) on panel + sel (chosen, hover)", t.fg2, panel_sel, 4.5),
        row("⌘P: file icon (fg2) on panel + sel", t.fg2, panel_sel, mark),
        row("⌘P: keys at the bottom (fg2) and words (fg3) on panel", t.fg3, panel, 4.5),
        row("⌘P: field placeholder (fg2) on panel", t.fg2, panel, 4.5),
        // "Is writing" caret (`fx::caret`), on the panel: at the faintest step of the pulse, also
        // over a selection.
        row("writing caret (fg at the faintest step) on panel", crate::fx::caret::dimmest(t), panel, 4.5),
        row("writing caret (fg at the faintest step) on panel + sel", crate::fx::caret::dimmest(t), panel_sel, 4.5),
        row("line on panel (decorative lines)", t.line, panel, 1.0),
        row("line on glass (field border, decorative)", t.line, glass, 1.0),
        row("sel on panel (selection, hover)", t.sel, panel, 1.0),
        row("sel next to inline code", over(t.sel, panel), over(inline_code(t), panel), 1.0),
    ]
}

/// The inline code fill: `MarkdownStyle::neutral` uses the kit's text at 8 %.
pub fn inline_code(t: &Theme) -> Color {
    rune::tok::theme(t.dark).text.alpha(0.08)
}

#[cfg(test)]
mod tests {
    use rune::oklcha;

    use super::*;

    /// The rings before the fix: active in `fg3`, system focus at 50 %.
    fn rings_before(dark: bool) -> Rings {
        Rings { active: before(dark).fg3, focus: rune::SystemAccent::DEFAULT.focus(dark), chosen: before(dark).fg3, row_meta: before(dark).fg3 }
    }

    /// Before and after on measured glass `k` (0: worst desktop, 1: Sonoma).
    fn table(dark: bool, k: usize) -> (Vec<Row>, Vec<Row>) {
        let now = Theme::for_mode(dark);
        let glass = glass_grounds(&now)[k];
        (rows(&before(dark), &rings_before(dark), glass), rows(&now, &Rings::of(&now), glass))
    }

    /// The tokens before the 2026-11-14 fix (DESIGN.md, "Contrast", "before" column).
    fn before(dark: bool) -> Theme {
        let now = Theme::for_mode(dark);
        if dark {
            Theme {
                fg2: oklcha(0.80, 0.006, 270.0, 1.0),
                fg3: oklcha(0.69, 0.008, 270.0, 1.0),
                line: oklcha(1.0, 0.0, 0.0, 0.08),
                sel: oklcha(1.0, 0.0, 0.0, 0.09),
                mark: oklcha(1.0, 0.0, 0.0, 0.08),
                ..now
            }
        } else {
            Theme { mark: oklcha(0.0, 0.0, 0.0, 0.09), ..now }
        }
    }

    #[test]
    fn contrast_table() {
        for dark in [true, false] {
            for (k, ground) in GLASS_GROUNDS[usize::from(!dark)].iter().enumerate() {
                println!("{} {ground}", if dark { "dark" } else { "light" });
                let (old, new) = table(dark, k);
                for (b, a) in old.iter().zip(new) {
                    println!("| {} | {:.2} | {:.2} | {} |", a.what, ratio(b.fg, b.bg), ratio(a.fg, a.bg), a.min);
                }
            }
        }
    }

    #[test]
    fn every_row_meets_its_minimum() {
        for dark in [true, false] {
            for k in 0..2 {
                for r in table(dark, k).1 {
                    let c = ratio(r.fg, r.bg);
                    assert!(c >= r.min, "{} ({}, glass {k}): {c:.2} < {}", r.what, if dark { "dark" } else { "light" }, r.min);
                }
            }
        }
    }

    #[test]
    fn light_theme_gets_no_worse() {
        let (old, new) = table(false, 0);
        for (b, a) in old.iter().zip(new) {
            assert!(ratio(a.fg, a.bg) >= ratio(b.fg, b.bg) - 1e-4, "{}", a.what);
        }
    }

    /// Decorative lines show on Retina and `sel` stands out from inline code more than before.
    #[test]
    fn dark_lines_and_selection_are_visible() {
        let (t, old) = (Theme::for_mode(true), before(true));
        assert!(ratio(t.line, t.panel) >= 1.35, "line ≥ about 12 %");
        let code = over(inline_code(&t), t.panel);
        assert!(ratio(t.sel, code) > ratio(old.sel, code) + 0.05);
    }

    /// The active Find match stays more marked than the others.
    #[test]
    fn active_find_ring_is_stronger() {
        for dark in [true, false] {
            let t = Theme::for_mode(dark);
            assert!(ratio(Rings::of(&t).active, t.panel) > ratio(t.mark, t.panel) + 2.0);
        }
    }

    /// On glass the hierarchy holds: `fg` > `fg2` > `fg3` in contrast, on both measured grounds.
    #[test]
    fn glass_text_keeps_its_hierarchy() {
        for dark in [true, false] {
            let t = Theme::for_mode(dark);
            let g = t.on_glass();
            for bg in glass_grounds(&t) {
                assert!(ratio(t.fg, bg) > ratio(g.fg2, bg) && ratio(g.fg2, bg) > ratio(g.fg3, bg) + 0.3, "dark {dark}");
            }
            assert_eq!((g.fg, g.body, g.panel, g.glass), (t.fg, t.body, t.panel, t.glass), "only fg2 and fg3 change");
        }
    }

    #[test]
    fn blending_matches_known_values() {
        assert!((ratio(Color::WHITE, Color::BLACK) - 21.0).abs() < 0.01);
        assert_eq!(ratio(Color::WHITE.alpha(0.0), Color::BLACK), 1.0);
        // White at 50 % on black, in encoded sRGB: grey 0.5 → luminance 0.214.
        assert!((over(Color::WHITE.alpha(0.5), Color::BLACK).luminance() - 0.214).abs() < 0.001);
    }
}
