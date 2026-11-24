//! The gear (Settings) button in the tab bar and the "Shortcuts" section of the settings panel
//! (feedback group 3, item 4; before: a ⌘ button with a shortcuts popover,
//! docs/decisions/2026-11-15-shortcuts-popover.md).
//!
//! ⌘ read as "a shortcut", not as a place to set things: the button is now a gear that opens and
//! closes the settings, like ⌘,. It sits left of ↔ and copies its look (28 × 28, glyph in `fg2`, hover
//! `sel`, solid `sel` with the settings open). The shortcut list, with the mockup's groups, moves
//! into the settings panel as its last section, so it is still one click away.

use rune::{El, TextStyle, col, el, icon, row, text, Icon};

use super::sidebar::Motion;

use super::theme::{HAIRLINE, KBD_PAD_X, KBD_PAD_Y, R_KBD, R_ROW, ROW, ROW_PAD, SECTION_PAD_BOTTOM, SECTION_PAD_TOP, Theme, UI_SMALL, UI_TEXT, UI_WEIGHT};
use crate::app::App;

pub const SETTINGS_BUTTON: &str = "settings-button";
/// Button label and tooltip.
pub const BUTTON_LABEL: &str = "Settings";
pub const BUTTON_HELP: &str = "Settings  ⌘,";
/// The section title in the settings panel.
pub const TITLE: &str = "Shortcuts";
/// Row height (mockup: 24).
const KEY_ROW: f32 = 24.0;
/// Key text (mockup: 11.5).
const KEY_TEXT: f32 = 11.5;
/// The gear, pt: as tall as the ↔ glyph.
const GEAR: f32 = 15.0;

/// A gear drawn like the kit icons (round joins, stroke about 1 pt at `GEAR`): eight teeth and the hub.
pub static GEAR_ICON: Icon = Icon {
    name: "runemd-gear",
    view: 24.0,
    body: r#"<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" fill="none"/><circle cx="12" cy="12" r="3" stroke="currentColor" stroke-width="1.6" fill="none"/>"#,
};

/// The app's shortcuts, in the mockup's groups (`shortcuts.md`, from the mockup design/mockups/2026-11-15-scorciatoie/).
pub const GROUPS: &[(&str, &[(&str, &str)])] = &[
    ("File", &[("Open file", "⌘P"), ("Open the selected file", "↑↓  ↵"), ("New file", "⌘N"), ("Save (in source)", "⌘S"), ("Close tab", "⌘W")]),
    ("Tabs", &[("Go to tab", "⌘1 … ⌘8"), ("Last tab", "⌘9"), ("Next tab", "⌘⇧]  ⌃⇥"), ("Previous tab", "⌘⇧[  ⌃⇧⇥")]),
    (
        "Reading",
        &[
            ("Toggle source", "⌘E"),
            ("Find in file", "⌘F"),
            ("Next match, previous match", "↵  ⇧↵"),
            ("Text width", "⌘⌥+  ⌘⌥−"),
            ("Select all", "⌘A"),
        ],
    ),
    ("App", &[("Settings", "⌘,"), ("Close", "Esc")]),
];

/// The gear button, left of ↔: opens and closes the settings (like ⌘,).
pub(super) fn button(s: &App, theme: &Theme, motion: Motion) -> El {
    let open = s.settings_panel.open;
    let mut b = el()
        .key(SETTINGS_BUTTON)
        .size(ROW, ROW)
        .radius(R_ROW)
        .center()
        .role("button")
        .label(BUTTON_LABEL)
        .help(BUTTON_HELP)
        .expanded(open)
        .child(icon(&GEAR_ICON, GEAR).color(theme.fg2).key("settings-button-glyph"))
        .on_press(|s: &mut App| {
            s.width_popover = false;
            s.toggle_settings();
        });
    if open {
        b = b.fill(theme.sel);
    } else if motion.hover {
        b = b.hover(theme.sel);
    }
    b
}

/// Room above a group heading and below it: the sidebar's section labels (`SECTION_PAD_TOP` 14,
/// `SECTION_PAD_BOTTOM` 6), so the groups stand apart while rows follow each other without a gap
/// (record 2026-11-18-small-choices-review, constraint 2). The first heading has the
/// same 14 below the "Shortcuts" title (record 2026-11-18-tab-outline).
const GROUP_GAP: f32 = SECTION_PAD_TOP;
const GROUP_BELOW: f32 = SECTION_PAD_BOTTOM;
/// Group headings at the regular weight: with `fg3` they read lighter than the rows.
const GROUP_WEIGHT: f32 = UI_WEIGHT;

/// The group headings' colour, `fg3`: a step lighter than the rows' `fg2` and still ≥ 4.5:1 on the
/// panel (`contrast::every_row_meets_its_minimum`).
pub(crate) fn heading_color(theme: &Theme) -> rune::Color {
    theme.fg3
}

/// The shortcut list for the settings panel: the mockup's groups in `fg3` 400, rows in `fg2` with the
/// keys in `fg2` inside a `line` border.
pub fn list(theme: &Theme) -> El {
    let mut list = col().key("keys-list").w_full().px(ROW_PAD);
    for (gi, (group, keys)) in GROUPS.iter().enumerate() {
        list = list.child(
            text(*group)
                .key(format!("keys-group-{gi}"))
                .font(TextStyle::new(UI_SMALL, GROUP_WEIGHT))
                .color(heading_color(theme))
                // The first one follows the section title, whose own `SECTION_PAD_BOTTOM` is part of the gap.
                .mt(if gi == 0 { GROUP_GAP - SECTION_PAD_BOTTOM } else { GROUP_GAP })
                .mb(GROUP_BELOW),
        );
        for (ki, (what, keys)) in keys.iter().enumerate() {
            let cap = el()
                .border(HAIRLINE, theme.line)
                .radius(R_KBD)
                .px(KBD_PAD_X)
                .py(KBD_PAD_Y)
                .child(text(*keys).key(format!("keys-cap-{gi}-{ki}")).font(TextStyle::new(KEY_TEXT, UI_WEIGHT)).color(theme.fg2));
            let label = text(*what).font(TextStyle::new(UI_TEXT, UI_WEIGHT)).color(theme.fg2).truncate().grow(1.0).shrink().min_w(0.0);
            list = list.child(row().key(format!("keys-row-{gi}-{ki}")).w_full().h(KEY_ROW).gap(8.0).center_y().child(label).child(cap));
        }
    }
    list
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use rune::{Env, Runtime};

    use super::*;
    use crate::doc::document::Document;
    use crate::ui::tabs::WIDTH_BUTTON;

    fn open() -> Runtime<App> {
        let mut state = App::bare(crate::index::Library::load(Vec::new(), None));
        state.push_tab(Document::from_source(Path::new("t.md"), "# Title\n\nText.\n"));
        let mut rt = Runtime::new(state, crate::view, (1200.0, 900.0), Env::new(true, 2.0));
        rt.on_key = Some(crate::on_key);
        rt.frame(0.0);
        settle(&mut rt);
        rt
    }

    fn settle(rt: &mut Runtime<App>) {
        for _ in 0..2000 {
            if !rt.frame(1.0 / 120.0) {
                return;
            }
        }
        panic!("the view does not settle");
    }

    fn find(rt: &Runtime<App>, key: &str) -> Option<rune::ElementInfo> {
        rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key))
    }

    fn tap_el(rt: &mut Runtime<App>, key: &str) {
        let r = find(rt, key).expect(key).rect;
        tap(rt, r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
    }

    fn tap(rt: &mut Runtime<App>, x: f32, y: f32) {
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        settle(rt);
    }

    /// Every row of shortcuts.md (except Esc, which the mockup writes as "Chiudi popover e
    /// ricerca") has its keys in the list.
    #[test]
    fn the_list_is_the_apps_list() {
        let md = include_str!("shortcuts.md");
        let caps: Vec<&str> = GROUPS.iter().flat_map(|(_, k)| k.iter().map(|(_, c)| *c)).collect();
        for want in ["⌘P", "⌘N", "⌘S", "⌘W", "⌘1 … ⌘8", "⌘9", "⌘E", "⌘F", "⌘A", "⌘,", "Esc", "↑↓  ↵", "⌘⇧]  ⌃⇥", "⌘⇧[  ⌃⇧⇥", "↵  ⇧↵", "⌘⌥+  ⌘⌥−"] {
            assert!(caps.contains(&want), "missing {want}");
        }
        for k in ["⌘P", "⌘N", "⌘S", "⌘W", "⌘9", "⌘E", "⌘F", "⌘A", "⌘,"] {
            assert!(md.contains(k), "{k} is not in shortcuts.md");
        }
        let groups: Vec<&str> = GROUPS.iter().map(|(g, _)| *g).collect();
        assert_eq!(groups, ["File", "Tabs", "Reading", "App"], "mockup groups");
    }

    /// Record 2026-11-18-small-choices-review, constraint 2: each group heading after the first has more
    /// room above it (14, like the sidebar's section labels) than there is between rows (0), and the
    /// headings are a lighter token than the rows (`fg3` against `fg2`) at the regular weight.
    #[test]
    fn the_groups_stand_apart_and_their_headings_are_lighter() {
        let mut rt = open();
        tap_el(&mut rt, SETTINGS_BUTTON);
        let rect = |k: String| find(&rt, &k).expect(&k).rect;
        for (gi, (_, keys)) in GROUPS.iter().enumerate().skip(1) {
            let last = rect(format!("keys-row-{}-{}", gi - 1, GROUPS[gi - 1].1.len() - 1));
            let head = rect(format!("keys-group-{gi}"));
            assert!((head[1] - (last[1] + last[3]) - GROUP_GAP).abs() < 0.01, "group {gi}: {GROUP_GAP} above the heading");
            let (a, b) = (rect(format!("keys-row-{gi}-0")), rect(format!("keys-row-{gi}-1")));
            assert_eq!(b[1], a[1] + a[3], "rows follow each other without a gap");
            assert!(keys.len() > 1);
        }
        for dark in [true, false] {
            let t = Theme::for_mode(dark);
            assert_eq!(heading_color(&t), t.fg3, "headings in fg3");
            assert_ne!(heading_color(&t), t.fg2, "rows are fg2");
        }
        assert_eq!((GROUP_GAP, GROUP_WEIGHT), (14.0, UI_WEIGHT), "14 above; regular weight, lighter than the 500 of before");
    }

    /// Record 2026-11-18-tab-outline (Matteo: "il titoletto Shortcuts è troppo attaccato a File"): the
    /// first group heading sits `GROUP_GAP` below the "Shortcuts" title's text, as far as each later
    /// heading sits below the row before it.
    #[test]
    fn the_first_group_stands_apart_from_the_title() {
        let mut rt = open();
        tap_el(&mut rt, SETTINGS_BUTTON);
        let title = find(&rt, TITLE).expect("title").rect;
        let head = find(&rt, "keys-group-0").expect("File").rect;
        let gap = head[1] - (title[1] + title[3] - SECTION_PAD_BOTTOM);
        assert!((gap - GROUP_GAP).abs() < 0.01, "{gap} between the title and File");
    }

    /// Feedback group 3, item 4: a gear left of ↔, same size, with a tooltip; a click opens the
    /// settings and a second closes them; the shortcut list is in the settings panel.
    #[test]
    fn the_gear_opens_the_settings() {
        let mut rt = open();
        let b = find(&rt, SETTINGS_BUTTON).expect("gear button");
        let w = find(&rt, WIDTH_BUTTON).unwrap();
        assert_eq!(b.role, "button");
        assert_eq!(b.label.as_deref(), Some("Settings"));
        assert_eq!(b.help.as_deref(), Some(BUTTON_HELP), "tooltip");
        assert!(b.rect[0] + b.rect[2] <= w.rect[0], "left of ↔: {:?} {:?}", b.rect, w.rect);
        assert_eq!((b.rect[2], b.rect[3]), (w.rect[2], w.rect[3]), "same size as ↔");
        assert!(!rt.state.settings_panel.open);
        tap_el(&mut rt, SETTINGS_BUTTON);
        assert!(rt.state.settings_panel.open, "the gear opens the settings");
        assert!(find(&rt, "keys-list").is_some(), "the shortcuts are in the panel");
        assert_eq!(find(&rt, "keys-cap-0-0").and_then(|e| e.text).as_deref(), Some("⌘P"));
        tap_el(&mut rt, SETTINGS_BUTTON);
        assert!(!rt.state.settings_panel.open, "and closes them");
    }
}
