//! `DESIGN.md` tokens: OKLCH colors per theme, sizes and radii.

use rune::{Color, oklcha};

pub const FRAME: f32 = 8.0;
pub const SIDEBAR: f32 = 248.0;
pub const COLUMN: f32 = 680.0;

/// The text column is at most `column` wide ("Text width" setting,
/// `settings::COLUMNS`); with "Full" it follows the panel, and the row margins stay.
pub fn column(e: rune::El, column: f32) -> rune::El {
    match crate::settings::max_width(column) {
        Some(w) => e.max_w(w),
        None => e,
    }
}
pub const ROW: f32 = 28.0;
pub const TREE_ROW: f32 = 26.0;
pub const TREE_INDENT: f32 = 16.0;
/// UI text (DESIGN.md, `typography.ui`): 13 pt, weight 400.
pub const UI_TEXT: f32 = 13.0;
/// Small UI (DESIGN.md, `typography.ui-small`): 12 pt, weight 500. Times, sections, branch.
pub const UI_SMALL: f32 = 12.0;
/// UI weights (DESIGN.md, "Typography"): rows 400, repo name 600, small UI 500, key 400.
pub const UI_WEIGHT: f32 = 400.0;
pub const UI_STRONG_WEIGHT: f32 = 600.0;
pub const UI_SMALL_WEIGHT: f32 = 500.0;
pub const KBD_WEIGHT: f32 = 400.0;
/// Tab bar (DESIGN.md, `spacing.tab-bar`, `tab-bar-pad`, `tab-bar-lead`, `tab-top`, `tab-pad`): 44 tall
/// on the `tab_bar` tint, 10 margin on the right, 8 before the first tab. Tabs sit edge to edge (VS Code,
/// record 2026-11-18-vscode-tabs; c6b02b5 had 8 between them, "troppo spazio"), from `TAB_TOP` below the
/// bar's top down to the document, with `TAB_PAD_X` before the name and a `TAB_DIVIDER` line in `line`
/// between back tabs. The front tab is a `sel` pill inside the bar with a closed metal ring
/// (`ui::tabs::PILL_H`, record 2026-11-18-tab-bar-style).
pub const TAB_BAR_H: f32 = 44.0;
pub const TAB_BAR_PAD_X: f32 = 10.0;
pub const TAB_BAR_LEAD: f32 = 8.0;
pub const TAB_TOP: f32 = 8.0;
pub const TAB_PAD_X: f32 = 12.0;
pub const TAB_DIVIDER: f32 = 1.0;
/// A tab's height: from `TAB_TOP` to the bar's bottom, over its 0.5 line.
pub const TAB_H: f32 = TAB_BAR_H + HAIRLINE - TAB_TOP;
/// Find in file (DESIGN.md, `spacing.find-field`): the field in the tab bar is 240 wide.
pub const FIND_W: f32 = 240.0;
/// The inner ring of Find matches, 1 pt above the `sel` fill, in `mark`
/// (DESIGN.md, "Find in file", `find-ring`).
pub const FIND_RING: f32 = 1.0;
/// The active match's ring: in `fg2` and twice as thick (`find-ring-active`), so it
/// stands out at a glance in both themes, even now that `mark` is at 3:1.
pub const FIND_RING_ACTIVE: f32 = 2.0;
/// The focus label sits this far from the bottom of the panel.
pub const FIRE_LABEL_BOTTOM: f32 = 10.0;
/// Document body (DESIGN.md, `typography.doc-body`): 15.5 pt, line height 1.62.
pub const BODY_TEXT: f32 = 15.5;
pub const BODY_LEADING: f32 = 1.62;
/// Code and source (DESIGN.md, `typography.doc-code`): mono 13.5 pt.
pub const CODE_TEXT: f32 = 13.5;
/// Horizontal margin of document rows: the text starts here.
pub const ROW_PAD_X: f32 = 28.0;
/// Vertical margin of document rows.
pub const ROW_PAD_Y: f32 = 7.0;
/// "What changed" mark: 2 pt wide, in `mark`, in the row's left margin.
pub const CHANGED_MARK_W: f32 = 2.0;
/// The mark sits this far from the row's left edge (halfway through the margin).
pub const CHANGED_MARK_X: f32 = (ROW_PAD_X - CHANGED_MARK_W) / 2.0;

/// "In this file" heading outline (DESIGN.md, `spacing.outline*`, mockup `scheda`): column
/// 196 wide including the line, text 16 from the line (10 of margin plus 6 inside the row, which
/// so has room for hover), rows 25 tall (12.5 pt with line height 2), 10 of indent per
/// level, 4 between the label and the first heading.
pub const OUTLINE_W: f32 = 196.0;
pub const OUTLINE_PAD_X: f32 = 10.0;
pub const OUTLINE_ROW_PAD: f32 = 6.0;
pub const OUTLINE_ROW: f32 = 25.0;
pub const OUTLINE_TEXT: f32 = 12.5;
pub const OUTLINE_INDENT: f32 = 10.0;
pub const OUTLINE_LABEL_GAP: f32 = 4.0;
/// Below this window width the outline hides: sidebar + column + 200.
pub const OUTLINE_MIN_WINDOW: f32 = SIDEBAR + COLUMN + 200.0;

/// The outline threshold with sidebar and outline `sidebar` and `outline` wide (dragged,
/// `panes`): it moves by how much wider or narrower they are than the default.
pub fn outline_min_window(sidebar: f32, outline: f32) -> f32 {
    OUTLINE_MIN_WINDOW + (sidebar - SIDEBAR) + (outline - OUTLINE_W)
}
/// Without the outline, the sidebar widens as long as the panel stays at least this wide.
pub const DOC_MIN: f32 = 400.0;
/// The strip you drag on the edge of the sidebar and the outline, in their margin (8 and 10): cursor ↔.
pub const EDGE_W: f32 = 8.0;

/// The widest tab (`tab-max`): a longer name is cut in the middle.
pub const TAB_MAX_W: f32 = 240.0;
/// Sidebar (DESIGN.md, `spacing`): space above the field for the traffic lights, side and bottom margins.
pub const SIDEBAR_TOP: f32 = 44.0;
pub const SIDEBAR_PAD: f32 = 8.0;
pub const SIDEBAR_BOTTOM: f32 = 10.0;
/// "Open file" field and repo row (`spacing.field`); field margin (`field-margin`).
pub const FIELD_H: f32 = 30.0;
pub const FIELD_MARGIN_X: f32 = 2.0;
pub const FIELD_MARGIN_BOTTOM: f32 = 6.0;
/// Horizontal margin inside sidebar rows (`row-pad`) and tree rows (`tree-row-pad`).
pub const ROW_PAD: f32 = 10.0;
pub const TREE_ROW_PAD: f32 = 8.0;
/// Space between icon, name and time (`row-gap`), and between name and repo of a double name (`qualifier-gap`).
pub const ROW_GAP: f32 = 8.0;
/// Field and repo icons (`icon`), folder chevron (`chevron`).
pub const ICON: f32 = 14.0;
pub const CHEVRON: f32 = 10.0;
/// Section labels: 14 above, 6 below, `ROW_PAD` on the sides (`section-pad`).
pub const SECTION_PAD_TOP: f32 = 14.0;
pub const SECTION_PAD_BOTTOM: f32 = 6.0;
/// The "⌘P" key: text `typography.kbd` (11, 400) and inner margin `kbd-pad`.
pub const KBD_TEXT: f32 = 11.0;
pub const KBD_PAD_X: f32 = 5.0;
pub const KBD_PAD_Y: f32 = 1.0;
/// Lines are 0.5 pt (`hairline`, DESIGN.md "Shapes").
pub const HAIRLINE: f32 = 0.5;
/// Pressed row: scale 0.98 in 160 ms ease-out; hover in and out in 120 ms (`motion`).
pub const PRESS_SCALE: f32 = 0.98;
pub const PRESS_TIME: f32 = 0.16;
pub const HOVER_FADE: f32 = 0.12;
/// `motion.ease-out`: cubic-bezier(0.23, 1, 0.32, 1).
pub const EASE_OUT: rune::Easing = rune::Easing::cubic(0.23, 1.0, 0.32, 1.0);

/// Reading ↔ source (Cmd+E): cross-fade with critically damped springs (Rune animates
/// enters and exits only with springs). Enters in 0.22 s of perceived duration (95 % at ~166 ms; the
/// velocity peaks at 35 ms and then drops, a strong ease-out); exits in 0.17 s (under 5 % at
/// ~128 ms). It used to be Rune's default fade, 0.30 / 0.24 s (95 % at ~227 ms).
pub const MODE_ENTER: f32 = 0.22;
pub const MODE_EXIT: f32 = 0.17;

/// The transition of the reading view and the source view; with "reduce motion" it is a
/// hard cut.
pub fn mode_transition(reduced: bool) -> rune::Transition {
    let springs = if reduced { rune::SpringPair::NONE } else { rune::SpringPair::new(rune::Spring::new(MODE_ENTER, 0.0), rune::Spring::new(MODE_EXIT, 0.0)) };
    rune::Transition::opacity().springs(springs)
}

pub const R_KBD: f32 = 4.0;
pub const R_ROW: f32 = 6.0;
pub const R_FIELD: f32 = 7.0;
pub const R_PANEL: f32 = 10.0;

/// `fg` composited over `bg` (opaque) in encoded sRGB, as the window composites it (`srgb_blend`).
/// Used where rune-kit wants an opaque color (diagram fills) and to measure contrast.
pub fn flatten(fg: Color, bg: Color) -> Color {
    fn encode(c: f32) -> f32 {
        if c <= 0.003_130_8 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
    }
    fn decode(c: f32) -> f32 {
        if c <= 0.040_45 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    }
    let mix = |f: f32, b: f32| decode(encode(f) * fg.a + encode(b) * (1.0 - fg.a));
    Color { r: mix(fg.r, bg.r), g: mix(fg.g, bg.g), b: mix(fg.b, bg.b), a: 1.0 }
}

/// The keyboard focus color (`Cx::focus_ring`), from the system color `system`
/// (`Cx::focus_color`, which AppKit gives at 50 %), solid in both themes: at 50 % on glass
/// it stays under 3:1 (DESIGN.md, "Contrast"). In the light theme it is also darkened by `FOCUS_DARKEN`
/// toward black (in linear light): on real glass at 60 % (black desktop, ground `999a9c`) the blue
/// darkened by 20 % gave 1.70:1. In the dark theme it is
/// lightened by `FOCUS_LIGHTEN` toward white: on real glass at 60 % (white desktop) the solid
/// blue gave 2.94:1.
pub fn focus_ring(system: Color, dark: bool) -> Color {
    let solid = system.alpha(1.0);
    if dark { solid.mix(Color::WHITE, FOCUS_LIGHTEN) } else { solid.mix(Color::BLACK, FOCUS_DARKEN) }
}

/// How much the keyboard focus is lightened in the dark theme.
pub const FOCUS_LIGHTEN: f32 = 0.1;

/// How much the keyboard focus is darkened in the light theme.
pub const FOCUS_DARKEN: f32 = 0.68;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub dark: bool,
    pub panel: Color,
    pub fg: Color,
    pub body: Color,
    pub fg2: Color,
    pub fg3: Color,
    pub glass: Color,
    pub line: Color,
    pub sel: Color,
    pub field: Color,
    pub mark: Color,
}

impl Theme {
    /// The YAML frontmatter block: text `fg2` on half `sel` (quieter than code).
    pub fn meta(&self) -> crate::ui::doc_view::Meta {
        let rule = rune_kit::Kit::neutral(self.dark).c.layer_03;
        crate::ui::doc_view::Meta { text: self.fg2, fill: self.sel.alpha(self.sel.a * 0.5), label: self.fg, icon: self.fg2, rule, line: self.line, headings: [self.body, self.fg2, self.fg3], path: crate::ui::doc_view::PathColors { name: self.fg, dim: self.fg3, hover_line: self.fg2, hover_bg: self.sel } }
    }

    /// Text colors on glass (sidebar): `fg2` and `fg3` stronger than on the panel, because
    /// with the tint at 60 % real glass lets the desktop through. They are ≥ 4.5:1 on the
    /// measured grounds (`contrast::GLASS_GROUNDS`), under `sel` too; `fg` > `fg2` > `fg3` holds. The
    /// document panel keeps its own.
    pub fn on_glass(&self) -> Theme {
        if self.dark {
            Theme { fg2: oklcha(0.90, 0.006, 270.0, 1.0), fg3: oklcha(0.86, 0.008, 270.0, 1.0), ..*self }
        } else {
            Theme { fg2: oklcha(0.25, 0.008, 270.0, 1.0), fg3: oklcha(0.28, 0.008, 270.0, 1.0), ..*self }
        }
    }

    /// The tab bar's tint over `panel` (record 2026-11-18-vscode-tabs): a shade darker than the page, so
    /// the front tab, which has the page's `panel` fill, reads as part of the document below (VS Code's
    /// `editorGroupHeader.tabsBackground`).
    pub fn tab_bar(&self) -> Color {
        if self.dark { oklcha(0.0, 0.0, 0.0, 0.4) } else { oklcha(0.0, 0.0, 0.0, 0.04) }
    }

    /// The window fill when there is no system glass (headless, material
    /// off): the solid glass tint, so the desktop does not show through without blur.
    pub fn glass_fallback(&self) -> Color {
        self.glass.alpha(1.0)
    }

    /// The favourite star on a root row (feedback group 3, item 3): yellow fill and its edge. In dark the
    /// yellow alone is 3:1 on the glass; in light no yellow is, so a dark amber edge carries the shape and
    /// the yellow fills it. The empty star is `fg3`.
    pub fn star(&self) -> (Color, Color) {
        if self.dark {
            (oklcha(0.88, 0.17, 95.0, 1.0), oklcha(0.88, 0.17, 95.0, 1.0))
        } else {
            (oklcha(0.84, 0.16, 88.0, 1.0), oklcha(0.38, 0.08, 65.0, 1.0))
        }
    }

    pub fn for_mode(dark: bool) -> Theme {
        if dark {
            Theme {
                dark,
                panel: oklcha(0.155, 0.004, 270.0, 1.0),
                fg: oklcha(0.95, 0.003, 270.0, 1.0),
                body: oklcha(0.90, 0.004, 270.0, 1.0),
                fg2: oklcha(0.82, 0.006, 270.0, 1.0),
                fg3: oklcha(0.72, 0.008, 270.0, 1.0),
                glass: oklcha(0.18, 0.01, 260.0, 0.60),
                line: oklcha(1.0, 0.0, 0.0, 0.14),
                sel: oklcha(1.0, 0.0, 0.0, 0.12),
                field: oklcha(0.0, 0.0, 0.0, 0.22),
                mark: oklcha(0.72, 0.008, 270.0, 0.65),
            }
        } else {
            Theme {
                dark,
                panel: oklcha(0.995, 0.001, 270.0, 1.0),
                fg: oklcha(0.22, 0.006, 270.0, 1.0),
                body: oklcha(0.28, 0.006, 270.0, 1.0),
                fg2: oklcha(0.36, 0.008, 270.0, 1.0),
                fg3: oklcha(0.45, 0.008, 270.0, 1.0),
                glass: oklcha(0.965, 0.004, 260.0, 0.60),
                line: oklcha(0.0, 0.0, 0.0, 0.09),
                sel: oklcha(0.0, 0.0, 0.0, 0.06),
                field: oklcha(1.0, 0.0, 0.0, 0.55),
                mark: oklcha(0.45, 0.008, 270.0, 0.70),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_match_design_md() {
        assert_eq!((FIND_RING, FIND_RING_ACTIVE), (1.0, 2.0), "DESIGN.md, spacing.find-ring and find-ring-active");
        assert_eq!(TAB_MAX_W, 240.0, "DESIGN.md, spacing.tab-max");
        assert_eq!((TAB_BAR_PAD_X, TAB_BAR_LEAD, TAB_TOP, TAB_PAD_X, TAB_DIVIDER), (10.0, 8.0, 8.0, 12.0, 1.0), "DESIGN.md, spacing.tab-bar-pad, tab-bar-lead, tab-top, tab-pad and tab-divider");
        assert_eq!((crate::ui::sidebar::RECENT_GAP, crate::ui::sidebar::RECENT_SHARE), (3.0, 0.4), "DESIGN.md, spacing.recent-gap and recent-share");
        assert_eq!(crate::ui::sidebar::RECENT_ROW, 44.0, "DESIGN.md, spacing.recent-row");
        let dark = Theme::for_mode(true);
        assert_eq!(dark.fg2, oklcha(0.82, 0.006, 270.0, 1.0));
        assert_eq!(dark.fg3, oklcha(0.72, 0.008, 270.0, 1.0));
        assert_eq!(dark.line, oklcha(1.0, 0.0, 0.0, 0.14));
        assert_eq!(dark.sel, oklcha(1.0, 0.0, 0.0, 0.12));
        assert_eq!(dark.mark, oklcha(0.72, 0.008, 270.0, 0.65));
        assert_eq!(Theme::for_mode(false).mark, oklcha(0.45, 0.008, 270.0, 0.70));
        assert_eq!(Theme::for_mode(false).glass, oklcha(0.965, 0.004, 260.0, 0.60));
        assert_eq!(dark.glass, oklcha(0.18, 0.01, 260.0, 0.60));
    }
}
