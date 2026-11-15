//! The app state: the tabs of the open files and what they share.
//!
//! Each [`Tab`] has its own document, its own reread (`live::Follow`), its own signals (fire,
//! mark on changed blocks, caret) and where its view was. [`App`] holds what does not depend on
//! a tab: watcher, "writing" and the app's own writes (`live::Live`), index, sidebar,
//! rings. A background tab keeps reloading and counting the new blocks below
//! its view; the fire shows when it comes back to the front.

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rune::{TextEdit, Tx};
use rune_kit::MarkdownStyle;

use crate::doc::document::{Change, Document};
use crate::edit::SourceEdit;
use crate::live::arriving::Changed;
use crate::live::below::Below;
use crate::live::{Follow, Live};
use crate::settings::{Settings, ThemePref};
use crate::{edit, index, probe, ui};

/// How a tab shows the file: reading, or the source being edited (Cmd+E). Each tab has
/// its own.
#[derive(Debug, Default)]
pub enum Mode {
    #[default]
    Read,
    /// Boxed: the `TextEdit` is large and reading is the common case.
    Source(Box<SourceEdit>),
}

/// An open file.
pub struct Tab {
    pub doc: Document,
    pub mode: Mode,
    /// The fire: the new or changed blocks below this tab's view.
    pub below: Below,
    /// The blocks with the "what changed" mark.
    pub changed: Changed,
    /// The reread of the file.
    pub(crate) follow: Follow,
    /// The id of the last visible block, from the last `on_visible_rows`; 0 before the first.
    pub(crate) visible_last: u64,
    /// The id of the first visible block: when the tab comes back to the front the view starts there.
    pub(crate) visible_first: u64,
    /// The Rune clock at which the fire lit; `None` when it is off.
    pub(crate) fire_since: Option<f64>,
    /// The clock of the reload that brought blocks below the view, until Rune reports the
    /// visible rows after its layout: only then is it known whether they are really out of view.
    pub(crate) fire_armed: Option<f64>,
    /// The wakeup armed to remove the next mark that expires.
    mark_wake: Option<Wake>,
    /// The Rune clock at which the "writing" caret lit; `None` when it is off.
    pub(crate) caret_since: Option<f64>,
    /// The point of the last outside edit (`live::caret`), while the file is writing.
    pub(crate) caret_at: Option<crate::live::caret::At>,
    /// The position of `visible_first` at the last report: after a reload it may no longer hold
    /// (then the id is searched).
    visible_start: usize,
    /// The position of `visible_last` at the last report.
    visible_end: usize,
    /// The headings of the "In this file" outline, recomputed only when the document changes.
    pub(crate) outline: ui::outline::Cache,
    /// Where the scrolling view and the page are (`doc_view::Edge`), and whether the scroll is at the end.
    viewport: [f32; 4],
    page: [f32; 4],
    pub(crate) at_bottom: bool,
    /// The clicked outline heading (block id) and the first visible block after the jump:
    /// it stays highlighted while it shows and there is no manual scroll, even if it does not reach the top.
    picked: Option<(u64, Option<usize>)>,
    /// Cmd+E did not enter because the file is not UTF-8: reading shows the row that says so
    /// (`ui::source::NOT_UTF8_TEXT`) until a Cmd+E enters.
    pub(crate) not_utf8: bool,
    /// The preview tab (like VS Code): opened with a single click in the sidebar, the name in
    /// italics, replaced by the next single click. At most one. It becomes pinned with a double click
    /// (on the row or on the tab), an edit in the source or Cmd+S (`App::pin`).
    pub(crate) preview: bool,
}

impl Tab {
    pub(crate) fn new(doc: Document, follow: Follow) -> Tab {
        Tab { doc, mode: Mode::Read, below: Below::default(), changed: Changed::default(), follow, visible_last: 0, visible_first: 0, fire_since: None, fire_armed: None, mark_wake: None, caret_since: None, caret_at: None, visible_start: 0, visible_end: 0, outline: ui::outline::Cache::default(), viewport: [0.0; 4], page: [0.0; 4], at_bottom: false, picked: None, not_utf8: false, preview: false }
    }

    /// The file as FSEvents reports it (canonical): the key of the rings.
    pub fn path(&self) -> &Path {
        self.follow.path()
    }

    /// The tab's name.
    pub fn title(&self) -> String {
        self.doc.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }

    /// Whether block `id` has the "what changed" mark now.
    pub fn is_marked(&self, id: u64) -> bool {
        self.changed.is_marked(id, Instant::now())
    }

    /// New or changed blocks from a reload get the mark; marks expired at `now` are
    /// removed. True if the view changes. Also returns the wakeup for the next one to expire:
    /// without it, with "reduce motion" or with a still window no frame would remove it.
    pub fn update_marks(&mut self, change: Option<&Change>, clock: f64, now: Instant) -> (bool, Option<Duration>) {
        let mut changed = self.changed.prune(now);
        if let Some(c) = change {
            let ids: Vec<u64> = c.added.iter().chain(&c.changed).copied().collect();
            self.changed.mark(&ids, now);
            changed |= !ids.is_empty();
        }
        (changed, rearm(&mut self.mark_wake, self.changed.left(now), clock, now))
    }

    /// The "writing" caret (record 2026-11-15-caret-at-the-edit-point, constraints 2 and 4): `at`
    /// is the point of the last reload, if it has one, and replaces the previous one. On
    /// while the file is writing and the block of the point still exists; at the end of the window (the same
    /// wakeup as the ring, `update_ring`) it goes off and the point is forgotten. The clock stays
    /// the one of the first lighting, so the pulse does not restart at each write. True if it
    /// changed.
    pub fn caret_step(&mut self, at: Option<&crate::live::caret::At>, writing: bool, clock: f64) -> bool {
        let before = (self.caret_at.clone(), self.caret_since);
        if let Some(at) = at {
            self.caret_at = Some(at.clone());
        }
        let shown = self.caret_at.as_ref().is_some_and(|a| self.doc.blocks.iter().any(|b| b.id == a.block));
        if !writing || !shown {
            self.caret_at = None;
        }
        self.caret_since = match self.caret_at {
            Some(_) => self.caret_since.or(Some(clock)),
            None => None,
        };
        before != (self.caret_at.clone(), self.caret_since)
    }

    /// A reload applied: new or changed blocks after the last visible one enter the
    /// count, but the fire does not light here. `visible_last` comes from the layout before the
    /// reload: in a short document the added block is already in view, and that is known only when
    /// Rune reports the rows of the new layout (`visible_rows`, which lights it then). When
    /// the keys change (blocks added or removed) Rune reports for sure: `VirtualList::set_keys`
    /// clears the last report. With the same keys (only changed blocks) it does not report if the
    /// visible part stays the same, but then the indices of the previous layout still hold and the
    /// fire lights at once. True if it lit.
    ///
    /// The same holds in the background: the count starts from the tab's last view, and an
    /// armed fire lights at the first row report when the tab comes back to the front.
    pub fn note_reload(&mut self, change: &Change, clock: f64) -> bool {
        let order: Vec<u64> = self.doc.blocks.iter().map(|b| b.id).collect();
        self.below.on_change(change, self.visible_last, &order);
        if self.below.count() == 0 || self.fire_since.is_some() {
            return false;
        }
        if change.added.is_empty() && change.removed.is_empty() {
            self.fire_since = Some(clock);
            return true;
        }
        self.fire_armed.get_or_insert(clock);
        false
    }

    /// The visible rows changed (scroll, reload, resize): the last one leaves the
    /// fire count and becomes the anchor of the next reloads. True if the view changes
    /// (label, fire or, with the outline visible `outline`, the highlighted heading). The fire
    /// lights here, at the first report after the reload that armed it (with the clock of that
    /// reload), and goes off when the count reaches 0.
    pub fn visible_rows(&mut self, rows: Range<usize>, outline: bool) -> bool {
        let doc = &self.doc;
        // Rune reports the indices of the last layout: with a reload in between they can go
        // past the current blocks, and then it waits for the next report.
        let Some(last) = rows.end.checked_sub(1).filter(|&i| i < doc.blocks.len() && !rows.is_empty()) else { return false };
        // The outline highlights another heading: the view must redraw. Without an outline it is not computed.
        let heading = if outline { self.active_heading() } else { None };
        self.visible_last = self.doc.blocks[last].id;
        self.visible_first = self.doc.blocks[rows.start].id;
        self.visible_start = rows.start;
        self.visible_end = last;
        // The first report after a click in the outline fixes where the jump landed; a different one
        // is a manual scroll.
        self.picked = match self.picked {
            Some((id, None)) => Some((id, Some(rows.start))),
            Some((id, Some(at))) if at == rows.start => Some((id, Some(at))),
            _ => None,
        };
        let heading_moved = outline && self.active_heading() != heading;
        let before = self.below.count();
        let order: Vec<u64> = self.doc.blocks.iter().map(|b| b.id).collect();
        self.below.on_scroll(last, &order);
        let was = self.fire_since;
        if self.below.count() == 0 {
            self.fire_since = None;
        } else if self.fire_since.is_none() {
            self.fire_since = self.fire_armed;
        }
        self.fire_armed = None;
        heading_moved || self.below.count() != before || self.fire_since != was
    }

    /// The document's view or page has a new rectangle. The scroll is "at the end" if the
    /// page has gone below the top edge (the file does not fit in the view) and ends at the bottom
    /// edge of the view. True if, with the outline visible, the highlighted heading changes.
    pub fn edge(&mut self, edge: ui::doc_view::Edge, outline: bool) -> bool {
        match edge {
            ui::doc_view::Edge::Viewport(r) => self.viewport = r,
            ui::doc_view::Edge::Page(r) => self.page = r,
        }
        let ([_, vy, _, vh], [_, py, _, ph]) = (self.viewport, self.page);
        let at_bottom = vh > 0.0 && py < vy - 0.5 && py + ph <= vy + vh + 0.5;
        if at_bottom == self.at_bottom {
            return false;
        }
        let heading = if outline { self.active_heading() } else { None };
        self.at_bottom = at_bottom;
        outline && self.active_heading() != heading
    }

    /// A click on outline heading `block`: it stays highlighted while it shows (`picked`).
    pub fn pick_heading(&mut self, block: u64) {
        self.picked = Some((block, None));
    }

    /// The position of block `id`, looked up first where the last report put it.
    fn index_of(&self, id: u64, guess: usize) -> Option<usize> {
        let blocks = &self.doc.blocks;
        match blocks.get(guess) {
            Some(b) if b.id == id => Some(guess),
            _ => blocks.iter().position(|b| b.id == id),
        }
    }

    /// The highlighted outline heading. In order: the clicked one, if it shows and there was no
    /// manual scroll; with the scroll at the end, the last heading that shows (short sections at
    /// the end never reach the top); otherwise the one of the section of the first visible
    /// block. It follows block ids, so it holds even after a reload, before Rune's new
    /// report.
    pub fn active_heading(&self) -> Option<usize> {
        if self.visible_first == 0 {
            return None;
        }
        let items = self.outline.get(&self.doc);
        let start = self.index_of(self.visible_first, self.visible_start)?;
        let end = self.index_of(self.visible_last, self.visible_end).unwrap_or(start);
        if let Some((id, _)) = self.picked
            && let Some(k) = items.iter().position(|h| h.id == id)
            && (start..=end).contains(&items[k].index)
        {
            return Some(k);
        }
        ui::outline::active(&items, if self.at_bottom { end } else { start })
    }
}

/// `RUNEMD_GLASS` read: a glass opacity in percent (`60` → 0.60), between 0 and 100; anything else or
/// absent, `None` (the glass of `Theme`).
fn glass_env(v: Option<&str>) -> Option<f32> {
    v?.trim().parse::<f32>().ok().filter(|p| (0.0..=100.0).contains(p)).map(|p| p / 100.0)
}

/// A step of the drag of `pane`'s edge: `d.data` holds the width at press,
/// `delta` is the move that widens it. True when the width must be saved (released).
fn drag_pane(pane: &mut crate::panes::Pane, limits: crate::panes::Limits, d: &mut rune::Drag, delta: f32, room: f32) -> bool {
    match d.phase {
        rune::Phase::Down => d.data = pane.width,
        rune::Phase::Move => pane.width = pane.drag(d.data + delta, limits, room),
        rune::Phase::Up if d.cancelled() => pane.width = d.data,
        rune::Phase::Up => {
            pane.learn(pane.width, limits);
            return true;
        }
    }
    false
}

/// Brings block `i` of tab `tab` to the top of the document view, without gliding. The
/// row may not be mounted: `scroll_center_in_instant` mounts it, then `scroll_key_to` puts it
/// at the top, measuring again after layout; Rune makes the two requests in the same frame, in
/// this order (`App::restore_view`, `App::go_to_heading`).
fn scroll_block_to_top(tab: &Tab, i: usize, tx: &mut Tx) {
    let key = ui::doc_view::row_key(i, &tab.doc);
    tx.scroll_center_in_instant("doc", key.clone());
    tx.scroll_key_to(key, 0.0);
}

/// A wakeup for the end of a timed state ("writing", a mark that expires).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Wake {
    /// The end (wall clock, like `Activity` and `Changed`).
    end: Instant,
    /// When Rune fires it, in its clock (`poll` receives it as `clock`).
    at: f64,
}

/// The wakeup to request for a state that ends in `left` (from `now`); `None` if not needed.
///
/// One is armed when the end changes and when the armed one has expired (in Rune's clock)
/// with the state still on: Rune measures `wake_after` from `clock`, the start of the frame, and the end
/// from `now`, later; the wakeup can arrive a little before the end on the wall. A poll in between
/// arms nothing.
pub(crate) fn rearm(wake: &mut Option<Wake>, left: Option<Duration>, clock: f64, now: Instant) -> Option<Duration> {
    let end = left.map(|d| now + d);
    let due = wake.is_some_and(|w| clock >= w.at);
    let next = match left {
        Some(d) if due || end != wake.map(|w| w.end) => Some(d + Duration::from_millis(1)),
        _ => None,
    };
    *wake = match (end, next) {
        (Some(end), Some(d)) => Some(Wake { end, at: clock + d.as_secs_f64() }),
        (Some(_), None) => *wake,
        (None, _) => None,
    };
    next
}

/// The document styles for the light and the dark theme, with the font and size of `s`.
fn styles_for(s: &Settings) -> [MarkdownStyle; 2] {
    let typeface = crate::fonts::Typeface::of(&s.font, s.size);
    [false, true].map(|dark| ui::markdown_style::style_with(&ui::theme::Theme::for_mode(dark), typeface))
}

/// The answers to the "Save changes to …?" row (Cmd+W with unsaved changes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAnswer {
    /// "Save" (also Return): saves and closes.
    Save,
    /// "Don't save": closes and drops the changes.
    Discard,
    /// "Cancel" (also Esc): the row goes away, the tab stays as it was.
    Cancel,
}

pub struct App {
    pub tabs: Vec<Tab>,
    /// The index of the front tab; valid only if `tabs` is not empty.
    pub active: usize,
    /// The document style in the light and in the dark theme (index `dark as usize`), from the font and
    /// the size in the settings: the view picks with the current theme, without rebuilding it.
    styles: [MarkdownStyle; 2],
    /// The settings (Cmd+,) and where they are saved.
    pub(crate) settings: Settings,
    prefs: rune::prefs::Prefs,
    /// The settings panel.
    pub(crate) settings_panel: ui::settings::Panel,
    /// The text width popover (↔ button in the tab bar) is open.
    pub(crate) width_popover: bool,
    /// The shortcuts popover (⌘ button next to ↔) is open.
    pub(crate) keys_popover: bool,
    /// `RUNEMD_THEME` (`light` or `dark`), read once at startup: it wins over the chosen theme.
    theme_env: Option<bool>,
    /// `RUNEMD_REDUCE_MOTION` (`1` or `0`), read once at startup: it wins over the system
    /// "reduce motion" preference.
    pub(crate) motion_env: Option<bool>,
    /// `RUNEMD_GLASS` (glass opacity in percent, `glass_env`), read once at startup:
    /// only for photos, the default stays the one of `Theme`.
    pub(crate) glass_env: Option<f32>,
    /// "Reduce motion" as the last view saw it (`poll` has no `Cx`): the fire at rest
    /// requests its wakeups only if it moves.
    pub(crate) reduced_seen: std::cell::Cell<bool>,
    pub(crate) scroll: Option<probe::ScrollRun>,
    /// Watcher, "writing" and the app's own writes, for all tabs.
    pub(crate) live: Live,
    /// The files that are "writing" and the Rune clock (`poll`) at which their ring
    /// lit: their tab and their rows in Recents and in the tree show it.
    rings: BTreeMap<PathBuf, f64>,
    /// The wakeup armed to turn off the first ring that expires: one per end, not one per frame.
    ring_wake: Option<Wake>,
    /// The index of the Markdown files in the watched folders (Recents, Cmd+P).
    pub(crate) library: index::Library,
    /// Recents and the repo of the front file.
    pub(crate) sidebar: ui::sidebar::Sidebar,
    /// The window is active (`on_window_focus`): without it, the fire stays still.
    pub(crate) window_active: bool,
    /// The window is at least `OUTLINE_MIN_WINDOW` wide: the heading outline shows.
    pub(crate) wide: bool,
    /// The window width (`on_layout` of the root): the room for the dragged panes.
    pub(crate) window_w: f32,
    /// The field that `source_edit` gives when its tab is no longer in the source.
    spare_edit: TextEdit,
    /// Find in file (Cmd+F): one bar per window, on the front tab.
    pub(crate) find: ui::find::Find,
    /// The folder of the recovery copies (`edit::recovery`); `None` in tests that do not give it,
    /// so no test writes into the real one.
    pub(crate) recovery: Option<PathBuf>,
}

impl App {
    pub fn new(library: index::Library, settings: Settings, prefs: rune::prefs::Prefs) -> App {
        let mut app = App {
            tabs: Vec::new(),
            active: 0,
            styles: styles_for(&settings),
            settings,
            prefs,
            settings_panel: ui::settings::Panel::default(),
            width_popover: false,
            keys_popover: false,
            theme_env: match std::env::var("RUNEMD_THEME").as_deref() {
                Ok("light") => Some(false),
                Ok("dark") => Some(true),
                _ => None,
            },
            reduced_seen: std::cell::Cell::new(false),
            motion_env: crate::fx::reduce_motion_env(std::env::var("RUNEMD_REDUCE_MOTION").ok().as_deref()),
            glass_env: glass_env(std::env::var("RUNEMD_GLASS").ok().as_deref()),
            scroll: None,
            live: Live::default(),
            rings: BTreeMap::new(),
            ring_wake: None,
            library,
            sidebar: ui::sidebar::Sidebar::default(),
            window_active: true,
            wide: false,
            window_w: 0.0,
            spare_edit: TextEdit::default(),
            find: ui::find::Find::default(),
            recovery: edit::recovery::default_dir(),
        };
        app.sync_saved();
        app
    }

    /// An app with the default settings, in memory (the tests).
    #[cfg(test)]
    pub fn bare(library: index::Library) -> App {
        let mut app = App::new(library, Settings::default(), rune::prefs::Prefs::memory());
        app.recovery = None;
        app
    }

    /// The document style in the `dark` theme.
    pub fn style(&self, dark: bool) -> &MarkdownStyle {
        &self.styles[usize::from(dark)]
    }

    /// One fixed style for both themes (the view tests).
    #[cfg(test)]
    pub fn set_style(&mut self, style: MarkdownStyle) {
        self.styles = [style.clone(), style];
    }

    /// The current theme, given the system one: `RUNEMD_THEME` (`light` or `dark`, for photos
    /// and checks) wins over the settings, which win over the system.
    pub fn dark(&self, system_dark: bool) -> bool {
        self.theme_env.unwrap_or_else(|| self.settings.theme.dark(system_dark))
    }

    /// The current "reduce motion", given the system preference: `RUNEMD_REDUCE_MOTION` wins.
    pub fn reduced_motion(&self, system: bool) -> bool {
        crate::fx::reduce_motion(self.motion_env, system)
    }

    /// Saves the settings; an error is reported on the terminal and the value stays for this session.
    fn save_settings(&mut self) {
        if let Err(e) = self.settings.save(&mut self.prefs) {
            eprintln!("runemd: settings not saved: {e}");
        }
    }

    /// The document font (body and headings): the document updates at once.
    pub fn set_font(&mut self, font: &str) {
        self.settings_panel.clear_notice();
        if self.settings.font == font {
            return;
        }
        self.settings.font = font.to_owned();
        self.styles = styles_for(&self.settings);
        self.save_settings();
    }

    /// The body size one step smaller (−1) or larger (+1).
    pub fn step_size(&mut self, step: i32) {
        self.settings_panel.clear_notice();
        let size = self.settings.step_size(step);
        if size == self.settings.size {
            return;
        }
        self.settings.size = size;
        self.styles = styles_for(&self.settings);
        self.save_settings();
    }

    pub fn set_theme(&mut self, theme: ThemePref) {
        self.settings_panel.clear_notice();
        if self.settings.theme != theme {
            self.settings.theme = theme;
            self.save_settings();
        }
    }

    /// The width of the text column (one of `settings::COLUMNS`). The row heights
    /// change: the block that was at the top of the view goes back to the top (`scroll_block_to_top`).
    pub fn set_column(&mut self, column: f32, tx: &mut Tx) {
        self.settings_panel.clear_notice();
        if self.settings.column == column || !crate::settings::COLUMNS.contains(&column) {
            return;
        }
        self.settings.column = column;
        self.save_settings();
        if let Some(tab) = self.tab().filter(|t| matches!(t.mode, Mode::Read))
            && let Some(i) = tab.doc.blocks.iter().position(|b| b.id == tab.visible_first).filter(|&i| i > 0)
        {
            scroll_block_to_top(tab, i, tx);
        }
    }

    /// Turns the fire at the bottom of the pane on or off; the new-text label stays.
    pub fn toggle_fire(&mut self) {
        self.settings_panel.clear_notice();
        self.settings.fire = !self.settings.fire;
        self.save_settings();
    }

    /// A step of the fire's intensity, height or speed (one of `knob.steps()`).
    pub fn set_fire_knob(&mut self, knob: crate::settings::FireKnob, v: f32) {
        self.settings_panel.clear_notice();
        if self.settings.set_fire_knob(knob, v) {
            self.save_settings();
        }
    }

    /// The fire colour.
    pub fn set_fire_color(&mut self, color: crate::fx::fire::FireColor) {
        self.settings_panel.clear_notice();
        if self.settings.fire_color != color {
            self.settings.fire_color = color;
            self.save_settings();
        }
    }

    /// One more watched folder, at once: the watcher watches it, the index scans it and
    /// Recents updates when the scan arrives. A folder under one already watched is not
    /// needed and is not added; one above replaces the ones it contains. True if it changed.
    pub fn add_root(&mut self, root: &Path) -> bool {
        self.settings_panel.clear_notice();
        let root = index::canonical(root);
        if !root.is_dir() || self.library.roots().iter().any(|r| root.starts_with(r)) {
            return false;
        }
        let inside: Vec<PathBuf> = self.library.roots().iter().filter(|r| r.starts_with(&root)).cloned().collect();
        for r in inside {
            self.remove_root(&r);
        }
        self.settings.add_root(&root);
        self.library.add_root(root.clone());
        self.live.add_tree(&root);
        self.save_settings();
        true
    }

    /// "Add folder…" with the folder `root` chosen in the dialog: if it is not added (already
    /// watched, or under a watched one) the panel says "Already watched" until `now + 3 s`.
    /// True if it was added.
    pub fn pick_root(&mut self, root: &Path, now: Instant) -> bool {
        let added = self.add_root(root);
        if !added {
            self.settings_panel.notice_already(now);
        }
        added
    }

    /// One watched folder less, at once: it leaves the watcher and the index, and its rows
    /// leave Recents. True if it was there.
    pub fn remove_root(&mut self, root: &Path) -> bool {
        self.settings_panel.clear_notice();
        let canonical = index::canonical(root);
        let known = self.settings.remove_root(root) | self.settings.remove_root(&canonical);
        let dropped = self.library.remove_root(&canonical);
        if !known && !dropped {
            return false;
        }
        self.live.remove_tree(&canonical);
        self.sidebar.refresh(&self.library.index, &[], std::time::SystemTime::now());
        self.save_settings();
        true
    }

    /// The sidebar takes back the saved folders from the settings (canonical).
    fn sync_saved(&mut self) {
        let saved = self.settings.saved.iter().map(|r| index::canonical(r)).collect();
        self.sidebar.set_saved(saved, &self.library.index);
    }

    /// "Save" on the root of the open file, or the folder of "Add folder": it joins the
    /// saved ones (at the end) and prefs.json; if no watched folder covers it, it becomes
    /// watched (`add_root`), so its tree has its Markdown files. True if it changed.
    pub fn save_folder(&mut self, root: &Path) -> bool {
        let root = index::canonical(root);
        if !root.is_dir() || !self.settings.save_folder(&root) {
            return false;
        }
        if !self.library.roots().iter().any(|r| root.starts_with(r)) {
            self.add_root(&root);
        }
        self.save_settings();
        self.sync_saved();
        true
    }

    /// "Remove" on the row of a saved folder: it leaves the saved ones and prefs.json. It stays
    /// watched (Recents does not lose its files): it is removed from the settings. True if it was there.
    pub fn unsave_folder(&mut self, root: &Path) -> bool {
        if !self.settings.unsave_folder(root) {
            return false;
        }
        self.save_settings();
        self.sync_saved();
        true
    }

    /// Sidebar hook for "Save" and "Add folder".
    pub fn save_from_sidebar(&mut self, root: PathBuf) {
        self.save_folder(&root);
    }

    /// Sidebar hook for "Remove".
    pub fn unsave_from_sidebar(&mut self, root: PathBuf) {
        self.unsave_folder(&root);
    }

    /// Cmd+,: opens or closes the settings panel.
    pub fn toggle_settings(&mut self) {
        self.settings_panel.toggle();
    }

    /// The front tab.
    pub fn tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub fn tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active)
    }

    /// A tab for `doc`, in front (the view tests, without disk).
    #[cfg(test)]
    pub fn push_tab(&mut self, doc: Document) {
        let follow = self.live.follow(&doc.path.clone());
        self.tabs.push(Tab::new(doc, follow));
        self.active = self.tabs.len() - 1;
    }

    /// The document of the front tab; the view calls it only when there is a tab.
    pub fn active_document(&self) -> &Document {
        &self.tab().expect("document view without tabs").doc
    }

    /// The tab of document `id` (this is how the view's closures find their tab).
    fn tab_of(&mut self, id: u64) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.doc.id == id)
    }

    /// Opens the received paths (Finder, command line, sidebar): one tab per path,
    /// the last one in front. A file already open is brought to the front, not duplicated or reread.
    pub fn open_paths(&mut self, paths: Vec<PathBuf>) {
        if !paths.is_empty() && std::env::var_os("RUNEMD_LOG_OPEN").is_some() {
            eprintln!("runemd: open_paths {paths:?}");
        }
        let before = self.tab().map(|t| t.doc.id);
        for path in &paths {
            let canonical = path.canonicalize().unwrap_or_else(|_| path.to_owned());
            if let Some(i) = self.tabs.iter().position(|t| t.path() == canonical || t.doc.path == *path) {
                self.active = i;
                // Opened from Finder or from the terminal: the tab stays.
                self.tabs[i].preview = false;
                continue;
            }
            match Document::load(path) {
                Ok(doc) => {
                    let follow = self.live.follow(path);
                    self.tabs.push(Tab::new(doc, follow));
                    self.active = self.tabs.len() - 1;
                }
                Err(e) => eprintln!("runemd: {}: {e}", path.display()),
            }
        }
        if self.tab().map(|t| t.doc.id) != before {
            self.fronted();
        }
    }

    /// A single click on a file in the sidebar (Recents, tree, Cmd+P results): if it is already
    /// open its tab comes to the front; otherwise it goes into the preview tab, which takes the
    /// place of the previous one (in the same position) or, if there is none, is added at the end.
    /// True if another tab came to the front.
    pub fn open_preview(&mut self, path: PathBuf) -> bool {
        let before = self.tab().map(|t| t.doc.id);
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        if let Some(i) = self.tabs.iter().position(|t| t.path() == canonical || t.doc.path == path) {
            self.active = i;
        } else {
            let doc = match Document::load(&path) {
                Ok(doc) => doc,
                Err(e) => {
                    eprintln!("runemd: {}: {e}", path.display());
                    return false;
                }
            };
            let follow = self.live.follow(&path);
            let mut tab = Tab::new(doc, follow);
            tab.preview = true;
            // A preview tab with unsaved changes is not thrown away: it stays (`pin`).
            let old = self.tabs.iter().position(|t| t.preview && !matches!(&t.mode, Mode::Source(s) if s.dirty));
            match old {
                Some(i) => {
                    let old = std::mem::replace(&mut self.tabs[i], tab);
                    self.live.unfollow(old.follow);
                    self.active = i;
                }
                None => {
                    for t in &mut self.tabs {
                        t.preview = false;
                    }
                    self.tabs.push(tab);
                    self.active = self.tabs.len() - 1;
                }
            }
        }
        let fronted = self.tab().map(|t| t.doc.id) != before;
        if fronted {
            self.fronted();
        }
        fronted
    }

    /// Tab `id` becomes pinned: the next single click does not replace it.
    pub fn pin(&mut self, id: u64) {
        if let Some(t) = self.tab_of(id) {
            t.preview = false;
        }
    }

    /// Pins the tab of file `path`, if it is open (double click on a sidebar row).
    pub fn pin_path(&mut self, path: &Path) {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_owned());
        if let Some(t) = self.tabs.iter_mut().find(|t| t.path() == canonical || t.doc.path == path) {
            t.preview = false;
        }
    }

    /// Closes the front tab; the one to its right comes to the front, or the last one. The
    /// roots watched only for that file leave the watcher. True if it closed.
    pub fn close_active(&mut self) -> bool {
        self.close(self.active)
    }

    /// Closes tab `i` (Cmd+W for the front one, middle click for the others). A tab
    /// in the source with unsaved changes does not close and does not write: it comes to the front with the
    /// "Save changes to …?" row ([`SourceEdit::asking`]), which
    /// [`App::answer_close`] answers. With the row already open it stays as it is. True if it closed.
    pub fn close(&mut self, i: usize) -> bool {
        let Some(tab) = self.tabs.get_mut(i) else { return false };
        if let Mode::Source(s) = &mut tab.mode
            && s.dirty
        {
            s.asking = true;
            self.select(i);
            return false;
        }
        self.remove(i);
        true
    }

    /// The answer to the "Save changes to …?" row of tab `id`. "Save" closes only if
    /// the save writes; otherwise the row becomes the conflict or error row. "Don't
    /// save" closes without writing; "Cancel" removes the row and leaves everything as it was. A tab
    /// that no longer asks (a button from an old frame) stays as it is.
    pub fn answer_close(&mut self, id: u64, answer: CloseAnswer) {
        let Some(i) = self.tabs.iter().position(|t| t.doc.id == id) else { return };
        let tab = &mut self.tabs[i];
        let Mode::Source(s) = &mut tab.mode else { return };
        if !s.asking {
            return;
        }
        match answer {
            CloseAnswer::Save => {
                if edit::save(tab, &mut self.live.own).is_ok() {
                    self.remove(i);
                }
            }
            CloseAnswer::Discard => self.remove(i),
            CloseAnswer::Cancel => s.asking = false,
        }
    }

    /// [`App::answer_close`] from a button or a key: if another tab comes to the front, its
    /// view starts again from where it was.
    pub fn answer_close_tx(&mut self, id: u64, answer: CloseAnswer, tx: &mut Tx) {
        let before = self.tab().map(|t| t.doc.id);
        self.answer_close(id, answer);
        if self.tab().map(|t| t.doc.id) != before {
            self.restore_view(tx);
        }
    }

    /// Tab `id` shows the "Save changes to …?" row.
    pub fn asking(&self, id: u64) -> bool {
        self.tabs.iter().any(|t| t.doc.id == id && matches!(&t.mode, Mode::Source(s) if s.asking))
    }

    /// Removes tab `i`, without saving; the one to its right comes to the front, or
    /// the last one. The roots watched only for that file leave the watcher.
    fn remove(&mut self, i: usize) {
        let tab = self.tabs.remove(i);
        self.live.unfollow(tab.follow);
        let front = self.active == i;
        if self.active > i || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
        if self.tabs.is_empty() {
            self.sidebar.clear_open();
            self.find.close();
        } else if front {
            self.fronted();
        }
    }

    /// The window closes or the app quits (`on_close`, `on_quit`): each tab in the source with
    /// changes saves them. Rune does not allow cancelling the close: if the save does not write (a
    /// conflict or an error) the text goes into a recovery copy ([`edit::recovery`]) and the file
    /// on disk stays as it is. The tab has no more changes to save: a second call (Cmd+Q
    /// does `on_quit` and then `on_close`) does not write a second copy.
    pub fn save_all(&mut self) {
        let stamp = edit::recovery::stamp_now();
        for tab in &mut self.tabs {
            if !matches!(&tab.mode, Mode::Source(s) if s.dirty) {
                continue;
            }
            let why = match edit::save(tab, &mut self.live.own) {
                Ok(()) => continue,
                Err(edit::SaveError::Conflict) => "the file changed on disk".to_owned(),
                Err(edit::SaveError::Io(e)) => format!("save failed ({e})"),
                Err(edit::SaveError::NotUtf8) => "the file is not UTF-8".to_owned(),
            };
            let path = tab.doc.path.display().to_string();
            match self.recovery.as_deref().map(|dir| edit::recover(tab, dir, &stamp)) {
                Some(Ok(copy)) => eprintln!("runemd: {path}: {why}; changes in {}", copy.display()),
                Some(Err(e)) => eprintln!("runemd: {path}: {why}; recovery copy not written: {e}"),
                None => eprintln!("runemd: {path}: {why}; changes not saved"),
            }
        }
    }

    /// At startup: the recovery copies written at the last quit open in a tab.
    pub fn open_recovered(&mut self) {
        let Some(dir) = self.recovery.as_deref() else { return };
        let copies = edit::recovery::take_pending(dir);
        self.open_paths(copies);
    }

    /// Brings tab `i` to the front; true if it changed.
    pub fn select(&mut self, i: usize) -> bool {
        if i >= self.tabs.len() || i == self.active {
            return false;
        }
        self.active = i;
        self.fronted();
        true
    }

    /// The tab after (`step` 1) or before (-1) the front one, wrapping around (Ctrl+Tab).
    pub fn cycle(&mut self, step: isize) -> bool {
        let n = self.tabs.len() as isize;
        n > 1 && self.select((self.active as isize + step).rem_euclid(n) as usize)
    }

    /// Another tab is in front: the sidebar shows its repo.
    fn fronted(&mut self) {
        if let Some(tab) = self.tabs.get(self.active) {
            self.sidebar.set_open(&tab.doc.path, &self.library.index);
        }
    }

    /// After a change of the front tab, in the same handler: the view starts again from the first
    /// block the tab showed, at the top, or from the start if it was never in front; its
    /// fire, if lit or armed, rises from now (`tx.clock()`), not from the reload back then.
    ///
    /// The tab's view is a new element (`ui::doc_view::view`), which starts from 0 with the
    /// estimated heights, and `Tx::scroll_key_to` wants a mounted row. Rune makes the two requests
    /// in the same frame, in this order: first the reveals (`scroll_center_in_instant`
    /// mounts the row), then the scrolls (`scroll_key_to` puts it at the top, measuring again after
    /// layout). The first frame after the change is already the right one.
    pub fn restore_view(&mut self, tx: &mut Tx) {
        let clock = tx.clock();
        if let Some(tab) = self.tab_mut() {
            if tab.fire_since.is_some() {
                tab.fire_since = Some(clock);
            }
            if tab.fire_armed.is_some() {
                tab.fire_armed = Some(clock);
            }
        }
        // With Find open the search runs again on the front file; the view goes to the first result
        // when it arrives (`poll_find`).
        self.sync_find();
        let Some(tab) = self.tab_mut() else { return };
        match tab.doc.blocks.iter().position(|b| b.id == tab.visible_first) {
            Some(i) if i > 0 => scroll_block_to_top(tab, i, tx),
            _ => tx.scroll_to_instant("doc", 0.0, 0.0),
        }
    }

    /// A click on a link in the document. A path that exists (`runemd-open:`, `doc::paths`)
    /// opens the file in a pinned tab (or brings its tab to the front) and, with `:line`, puts the
    /// block of that line at the top. `#heading` puts that heading at the top; a relative link to a file that
    /// exists opens it like a path; the others (`https:`, `mailto:`) are opened by the system.
    pub fn open_link(&mut self, url: &str, tx: &mut Tx) {
        if url.starts_with('#') {
            let Some(tab) = self.tab_mut().filter(|t| matches!(t.mode, Mode::Read)) else { return };
            if let Some(i) = crate::doc::anchors::find(tab.doc.blocks.iter().map(|b| &*b.source), url) {
                scroll_block_to_top(tab, i, tx);
            }
            return;
        }
        let url = match crate::doc::paths::parse_target(url) {
            Some(_) => url.to_owned(),
            None => {
                let file = url.split('#').next().unwrap_or(url);
                let local = !file.is_empty() && !file.contains(':');
                match self.tab().filter(|_| local).and_then(|t| crate::doc::paths::Places::of(&t.doc.path).resolve(file)) {
                    Some(f) => crate::doc::paths::open_target(&f, None),
                    None => {
                        if let Err(e) = tx.open_url(url) {
                            eprintln!("runemd: link {url}: {e}");
                        }
                        return;
                    }
                }
            }
        };
        let Some((path, line)) = crate::doc::paths::parse_target(&url) else { return };
        let before = self.tab().map(|t| t.doc.id);
        self.open_paths(vec![path.clone()]);
        if self.tab().map(|t| t.doc.id) != before {
            self.restore_view(tx);
        }
        let (Some(line), Ok(text)) = (line, std::fs::read_to_string(&path)) else { return };
        let Some(tab) = self.tab_mut().filter(|t| matches!(t.mode, Mode::Read)) else { return };
        let i = crate::doc::paths::block_of_line(&text, tab.doc.blocks.iter().map(|b| &*b.source), line);
        scroll_block_to_top(tab, i, tx);
    }

    /// A click on an outline heading: its block goes to the top of the view, without gliding (like
    /// `restore_view`: the row may not be mounted and its height is only estimated). Only if
    /// tab `doc` is still in front and in reading.
    pub fn go_to_heading(&mut self, doc: u64, block: u64, tx: &mut Tx) {
        let Some(tab) = self.tab_mut().filter(|t| t.doc.id == doc && matches!(t.mode, Mode::Read)) else { return };
        let Some(i) = tab.doc.blocks.iter().position(|b| b.id == block) else { return };
        tab.pick_heading(block);
        scroll_block_to_top(tab, i, tx);
    }

    /// A click on a sidebar row: the file goes into the preview tab (or its tab comes
    /// to the front, `open_preview`).
    pub fn open_from_sidebar(&mut self, path: PathBuf, tx: &mut Tx) {
        if self.open_preview(path) {
            self.restore_view(tx);
        }
    }

    /// A double click on a sidebar row: the file opens (the first click already opened it)
    /// and its tab becomes pinned.
    pub fn pin_from_sidebar(&mut self, path: PathBuf, tx: &mut Tx) {
        self.open_from_sidebar(path.clone(), tx);
        self.pin_path(&path);
    }

    /// The folder for Cmd+N: the one of the front file; without tabs, the first watched folder.
    pub fn new_file_dir(&self) -> Option<PathBuf> {
        let beside = self.tab().and_then(|t| t.doc.path.parent()).filter(|d| !d.as_os_str().is_empty());
        beside.map(Path::to_path_buf).or_else(|| self.library.roots().first().cloned())
    }

    /// Cmd+N, Return: creates `name` (empty, never overwritten) and brings it to the front already in the source; a
    /// name that exists opens that file (or its tab), also in the source. `None` if nothing was
    /// created or opened (invalid name, no folder, error): the field stays open.
    /// `Some(true)` if another tab came to the front.
    pub fn create_file(&mut self, name: &str) -> Option<bool> {
        edit::new_file::normalize(name)?;
        let dir = self.new_file_dir()?;
        let path = match edit::new_file::create(&dir, name) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("runemd: {}: {e}", dir.display());
                return None;
            }
        };
        let before = self.tab().map(|t| t.doc.id);
        self.open_paths(vec![path.clone()]);
        let tab = self.tabs.get_mut(self.active).filter(|t| t.doc.path == path || t.path() == path.canonicalize().unwrap_or_else(|_| path.clone()))?;
        if matches!(tab.mode, Mode::Read) {
            if edit::editable(tab.follow.path()) {
                tab.mode = Mode::Source(Box::new(edit::enter(&tab.doc)));
            } else {
                tab.not_utf8 = true;
            }
        }
        Some(self.tab().map(|t| t.doc.id) != before)
    }

    /// Cmd+N, Return from the field: like `create_file`, and if another tab came to the front it
    /// restores its view. True if the field closes.
    pub fn create_from_sidebar(&mut self, name: String, tx: &mut Tx) -> bool {
        match self.create_file(&name) {
            Some(fronted) => {
                if fronted {
                    self.restore_view(tx);
                }
                true
            }
            None => false,
        }
    }

    /// The Cmd+P query changed. Only one field open at a time: Find closes.
    pub fn requery_sidebar(&mut self) {
        self.find.close();
        self.sidebar.requery(&self.library.index);
    }

    /// Cmd+F: the Find bar opens with focus, and the Cmd+P and Cmd+N fields close. Only
    /// in reading: in the source (Cmd+E) and without tabs it does nothing. True if it opened.
    pub fn open_find(&mut self) -> bool {
        if !self.tab().is_some_and(|t| matches!(t.mode, Mode::Read)) {
            return false;
        }
        self.sidebar.quick.close();
        self.sidebar.quick_rows.clear();
        self.sidebar.new_file.close();
        self.find.show();
        true
    }

    /// The Find query changed: the results are requested, and the active one will be the first from the
    /// first visible block down (the view brings it to the centre when they arrive).
    pub fn find_changed(&mut self) {
        let Some(tab) = self.tabs.get(self.active).filter(|t| matches!(t.mode, Mode::Read)) else { return };
        let from = tab.doc.blocks.iter().position(|b| b.id == tab.visible_first).unwrap_or(0);
        self.find.ask(&tab.doc, ui::find::Goal::From(from));
    }

    /// Return (`1`) and Shift+Return (`-1`) in Find: the next or previous result, wrapping around, in the centre
    /// of the view from the first frame after the key. With the results still on the thread the step
    /// waits for them to arrive (`poll_find`).
    pub fn find_step(&mut self, delta: isize, tx: &mut Tx) {
        if self.find.enter(delta) {
            self.reveal_find(tx);
        }
    }

    /// Brings the active result to the centre of the view, even in an unmounted block, without
    /// gliding. True if there was a result.
    fn reveal_find(&self, tx: &mut Tx) -> bool {
        let Some(key) = self.tab().and_then(|t| self.find.reveal_key(&t.doc)) else { return false };
        tx.scroll_center_in_instant("doc", key);
        true
    }

    /// Like `reveal_find`, but the view moves only as much as needed, and not at all if the result
    /// already shows: while the query is typed the text does not jump at every letter.
    fn show_find(&self, tx: &mut Tx) -> bool {
        let Some(key) = self.tab().and_then(|t| self.find.reveal_key(&t.doc)) else { return false };
        tx.scroll_into_view_in("doc", key);
        true
    }

    /// Keeps Find in step with the front tab: another tab runs the search again from the first
    /// result; a reload runs it again keeping the active one near where it was. In the source or without
    /// tabs the bar closes.
    pub fn sync_find(&mut self) {
        if !self.find.open {
            return;
        }
        let Some(tab) = self.tabs.get(self.active).filter(|t| matches!(t.mode, Mode::Read)) else {
            self.find.close();
            return;
        };
        if let Some(goal) = self.find.stale(&tab.doc) {
            self.find.ask(&tab.doc, goal);
        }
    }

    /// From `poll`: follows the front tab (`sync_find`) and applies the results that arrived from the
    /// search thread; for a new search or another tab the view shows the active one. True if the
    /// view changes.
    pub fn poll_find(&mut self, tx: &mut Tx) -> bool {
        self.sync_find();
        let Some(tab) = self.tabs.get(self.active) else { return false };
        match self.find.poll(&tab.doc) {
            Some(reveal) => {
                match reveal {
                    ui::find::Reveal::Show => {
                        self.show_find(tx);
                    }
                    ui::find::Reveal::Center => {
                        self.reveal_find(tx);
                    }
                    ui::find::Reveal::No => {}
                }
                true
            }
            None => false,
        }
    }

    /// The visible rows reported by the view of document `id`: they hold for its tab,
    /// even if another one came to the front in the meantime.
    pub fn visible_rows(&mut self, id: u64, rows: Range<usize>) -> bool {
        let wide = self.wide;
        self.tab_of(id).is_some_and(|t| {
            let outline = wide && matches!(t.mode, Mode::Read);
            t.visible_rows(rows, outline)
        })
    }

    /// The view or page of document `id` moved (`doc_view::Edge`).
    pub fn doc_edge(&mut self, id: u64, edge: ui::doc_view::Edge) -> bool {
        let wide = self.wide;
        self.tab_of(id).is_some_and(|t| {
            let outline = wide && matches!(t.mode, Mode::Read);
            t.edge(edge, outline)
        })
    }

    /// The window is `w` wide (`on_layout` of the root): the heading outline shows if it is
    /// at least `outline_min_window`. The view does not redraw for this: it decides from `Cx::layout_with`.
    pub fn set_window_width(&mut self, w: f32) {
        self.window_w = w;
        self.wide = w >= self.outline_min_window();
    }

    /// The narrowest window with the outline in view, with the current pane widths.
    pub fn outline_min_window(&self) -> f32 {
        ui::theme::outline_min_window(self.settings.sidebar.width, self.settings.outline.width)
    }

    /// The right edge of the sidebar dragged (`panes`): it follows the pointer and snaps to the
    /// remembered widths; when released, the width is remembered and saved; Esc puts it back as it was.
    /// With the outline possible (wide window) it widens as long as the outline stays, otherwise as long as the
    /// pane stays at least `DOC_MIN` wide.
    pub fn drag_sidebar(&mut self, d: &mut rune::Drag) {
        let fixed = if self.wide { self.outline_min_window() - self.settings.sidebar.width } else { ui::theme::FRAME + ui::theme::DOC_MIN };
        let room = self.window_w - fixed;
        if drag_pane(&mut self.settings.sidebar, crate::panes::SIDEBAR, d, d.dx, room) {
            self.save_settings();
        }
    }

    /// The left edge of the outline dragged: like `drag_sidebar`, it widens towards the left;
    /// never so much that the outline hides.
    pub fn drag_outline(&mut self, d: &mut rune::Drag) {
        let room = self.window_w - (self.outline_min_window() - self.settings.outline.width);
        if drag_pane(&mut self.settings.outline, crate::panes::OUTLINE, d, -d.dx, room) {
            self.save_settings();
        }
    }

    /// Double click on the edge: the default width.
    pub fn reset_pane(&mut self, outline: bool) {
        let (pane, limits) = if outline { (&mut self.settings.outline, crate::panes::OUTLINE) } else { (&mut self.settings.sidebar, crate::panes::SIDEBAR) };
        pane.width = limits.default;
        self.save_settings();
    }

    /// The Find marks of block `block` of the front tab, in row `row_key`.
    pub fn find_hits(&self, block: &crate::doc::document::Block, row_key: &str) -> ui::find::Marks {
        self.find.highlights(block.id, block.hash, row_key)
    }

    /// Whether block `id` of the front tab has the "what changed" mark.
    pub fn is_marked(&self, id: u64) -> bool {
        self.tab().is_some_and(|t| t.is_marked(id))
    }

    /// A "writing" ring is lit: only then does the compositor walk the instances.
    pub fn ring_lit(&self) -> bool {
        !self.rings.is_empty()
    }

    /// A per-slot effect is lit (ring, fire or caret of the front tab): only then does the compositor
    /// walk the instances. The source shows no fire and no caret.
    pub fn effects_lit(&self) -> bool {
        let fire = self.settings.fire;
        self.ring_lit() || self.tab().is_some_and(|t| matches!(t.mode, Mode::Read) && (fire || t.caret_since.is_some()))
    }

    /// The fire of the front tab is at rest and moving: without a display link, `poll` requests a
    /// wakeup every `fx::fire::REST_FRAME` to redraw it. Still (inactive window, "reduce
    /// motion") or lit (the display link moves it): no.
    pub fn fire_rests(&self) -> bool {
        self.settings.fire && self.window_active && !self.reduced_seen.get() && self.tab().is_some_and(|t| matches!(t.mode, Mode::Read) && t.fire_since.is_none())
    }

    /// Cmd+E on the front tab: enters the source or, if already there, saves if there are changes and
    /// goes back to reading, from the block where reading had stopped. If the save does not write (the
    /// file changed on disk, or an error) it stays in the source with the row. True if there was a
    /// tab.
    pub fn toggle_source(&mut self, tx: &mut Tx) -> bool {
        let Some(tab) = self.tabs.get_mut(self.active) else { return false };
        match tab.mode {
            Mode::Read => {
                if !edit::editable(tab.follow.path()) {
                    tab.not_utf8 = true;
                    return true;
                }
                tab.not_utf8 = false;
                tab.mode = Mode::Source(Box::new(edit::enter(&tab.doc)));
                // Find works only in reading.
                self.find.close();
            }
            Mode::Source(_) => {
                if edit::leave(tab, &mut self.live.own).is_ok() {
                    self.restore_view(tx);
                }
            }
        }
        true
    }

    /// Cmd+S: saves the source of the front tab (if it does not write, the row says so). True if the
    /// tab was in the source.
    pub fn save_source(&mut self) -> bool {
        let Some(tab) = self.tabs.get_mut(self.active).filter(|t| matches!(t.mode, Mode::Source(_))) else { return false };
        tab.preview = false;
        let _ = edit::save(tab, &mut self.live.own);
        true
    }

    /// "Reload" in the conflict row of tab `id`: discards the changes and starts again from disk.
    pub fn reload_source(&mut self, id: u64) {
        if let Some(tab) = self.tab_of(id) {
            edit::reload(tab);
        }
    }

    /// "Keep mine" in the conflict row of tab `id`: writes the source text.
    pub fn keep_mine(&mut self, id: u64) {
        if let Some(i) = self.tabs.iter().position(|t| t.doc.id == id) {
            let _ = edit::keep_mine(&mut self.tabs[i], &mut self.live.own);
        }
    }

    /// The field of piece `piece` of the source of tab `id` (`on_edit` of the view). If the
    /// tab is gone, went back to reading or the piece merged with another, a throwaway
    /// field: the key is lost, no other text changes.
    pub fn source_edit(&mut self, id: u64, piece: u64) -> &mut TextEdit {
        let src = self.tabs.iter_mut().find(|t| t.doc.id == id).and_then(|t| match &mut t.mode {
            Mode::Source(s) => s.pieces.edit_mut(piece),
            Mode::Read => None,
        });
        match src {
            Some(edit) => edit,
            None => &mut self.spare_edit,
        }
    }

    /// A key at the edges of piece `piece` of the source of tab `id` (`on_input_key`): true
    /// if it took it (merged two pieces or passed to the neighbour). A merge removes a `\n`: the text
    /// changed. Rune makes the scroll follow the cursor only after a key that goes through the field:
    /// an empty key to the field that now has focus requests it without changing anything.
    pub fn source_edge(&mut self, id: u64, piece: u64, edge: edit::blocks::Edge, first: bool, last: bool, tx: &mut Tx) -> bool {
        let Some(Mode::Source(s)) = self.tab_of(id).map(|t| &mut t.mode) else { return false };
        let took = s.pieces.edge(piece, edge, first, last);
        let joined = took && matches!(edge, edit::blocks::Edge::JoinBack | edit::blocks::Edge::JoinForward);
        if joined {
            s.dirty = true;
        }
        if took {
            tx.send_key(rune::KeyEvent { key: rune::Key::Text(String::new()), mods: rune::Mods::default() });
        }
        if joined {
            self.pin(id);
        }
        took
    }

    /// The source text of tab `id` changed.
    pub fn source_changed(&mut self, id: u64) {
        if let Some(t) = self.tab_of(id)
            && let Mode::Source(s) = &mut t.mode
        {
            s.dirty = true;
            // An edit pins the preview tab.
            t.preview = false;
        }
    }

    /// The clock at which the ring of `path` lit, if it is "writing" (tab and sidebar).
    pub fn ring_of(&self, path: &Path) -> Option<f64> {
        self.rings.get(path).copied()
    }

    /// Turns the rings on or off according to `Live` at `now`; true if it changed. Also returns the
    /// wakeup to request: at the end of the window `poll` turns them off even if no frame went by
    /// in between (with "reduce motion" none go by).
    pub fn update_ring(&mut self, clock: f64, now: Instant) -> (bool, Option<Duration>) {
        self.ring_step(&self.live.writing(now), clock, now)
    }

    /// `update_ring` with the files that are writing and how soon they stop (at `now`) already
    /// computed. A file already lit keeps its clock; the wakeup, for the first one to
    /// expire, comes from [`rearm`].
    pub(crate) fn ring_step(&mut self, writing: &[(PathBuf, Duration)], clock: f64, now: Instant) -> (bool, Option<Duration>) {
        let wake = rearm(&mut self.ring_wake, writing.iter().map(|(_, d)| *d).min(), clock, now);
        let rings: BTreeMap<PathBuf, f64> = writing.iter().map(|(p, _)| (p.clone(), self.rings.get(p).copied().unwrap_or(clock))).collect();
        let changed = rings != self.rings;
        self.rings = rings;
        (changed, wake)
    }

    /// The reloads of all tabs, at each `poll`: drains the watcher, rereads the changed
    /// files and updates fire, marks and caret of each tab. Returns whether the view changes and the
    /// wakeups to request.
    pub fn poll_tabs(&mut self, clock: f64, now: Instant) -> (bool, Vec<Duration>) {
        let open: Vec<&Path> = self.tabs.iter().map(Tab::path).collect();
        self.live.drain(&open, now);
        let (mut changed, mut wakes) = (false, Vec::new());
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            // In the source the view does not reload by itself: the events of this file are lost
            // and `edit::leave` makes it reread. A write from outside (the disk no longer has the
            // content the edit starts from, nor the one of our last save) still lights
            // "writing", so it shows before the conflict at save.
            if let Mode::Source(src) = &tab.mode {
                if self.live.changed(tab.follow.path()) && edit::changed_on_disk(tab.follow.path(), src.base_hash) {
                    self.live.touch_open(tab.follow.path(), now);
                }
                continue;
            }
            let (reloaded, recheck) = self.live.poll(&mut tab.follow, &mut tab.doc, now);
            wakes.extend(recheck);
            if let Some(r) = &reloaded {
                // The latency probe measures up to the frame that shows the reload: only the front
                // tab has one.
                if i == self.active {
                    probe::reloaded(r);
                }
                changed = true;
                changed |= tab.note_reload(&r.change, clock);
            }
            // The mark on changed blocks: each applied reload comes from outside (the app's
            // own writes are skipped in `reload::commit`). One wakeup per mark that expires.
            let (marks, wake) = tab.update_marks(reloaded.as_ref().map(|r| &r.change), clock, now);
            wakes.extend(wake);
            let writing = self.live.writing_left(tab.follow.path(), now).is_some();
            let at = reloaded.as_ref().and_then(|r| r.change.at.as_ref());
            changed |= marks | tab.caret_step(at, writing, clock);
        }
        (changed, wakes)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::live;

    use super::*;

    #[test]
    fn glass_env_reads_a_percentage() {
        assert_eq!(glass_env(Some("60")), Some(0.6));
        assert_eq!(glass_env(Some(" 40 ")), Some(0.4));
        assert_eq!(glass_env(Some("140")), None);
        assert_eq!(glass_env(Some("glass")), None);
        assert_eq!(glass_env(None), None);
    }

    pub(crate) fn empty_app() -> App {
        App::bare(index::Library::load(Vec::new(), None))
    }

    fn files(names: &[&str]) -> (tempfile::TempDir, Vec<PathBuf>) {
        let dir = tempfile::tempdir().unwrap();
        let paths = names.iter().map(|n| {
            let p = dir.path().join(n);
            fs::write(&p, format!("# {n}\n")).unwrap();
            p
        }).collect();
        (dir, paths)
    }

    fn app_with_root(root: &Path) -> App {
        App::bare(index::Library::load(vec![root.to_path_buf()], None))
    }

    /// "Reduce motion": the system preference, unless `RUNEMD_REDUCE_MOTION`.
    #[test]
    fn reduced_motion_follows_the_system_unless_the_env_says_otherwise() {
        let mut app = empty_app();
        app.motion_env = None;
        assert!(app.reduced_motion(true) && !app.reduced_motion(false));
        app.motion_env = Some(true);
        assert!(app.reduced_motion(false), "RUNEMD_REDUCE_MOTION=1");
        app.motion_env = Some(false);
        assert!(!app.reduced_motion(true), "RUNEMD_REDUCE_MOTION=0");
    }

    #[test]
    fn new_file_without_tabs_goes_to_the_first_root_in_source() {
        let root = tempfile::tempdir().unwrap();
        let mut app = app_with_root(root.path());
        assert_eq!(app.create_file("test"), Some(true));
        let path = root.path().join("test.md");
        assert_eq!(fs::read_to_string(&path).unwrap(), "");
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.tabs[0].title(), "test.md");
        assert!(matches!(app.tabs[0].mode, Mode::Source(_)));
    }

    #[test]
    fn new_file_goes_beside_the_open_file() {
        let (dir, p) = files(&["a.md"]);
        let root = tempfile::tempdir().unwrap();
        let mut app = app_with_root(root.path());
        app.open_paths(vec![p[0].clone()]);
        assert_eq!(app.create_file("b.md"), Some(true));
        assert!(dir.path().join("b.md").exists() && !root.path().join("b.md").exists());
        assert_eq!(app.tabs.len(), 2);
        assert!(matches!(app.tabs[0].mode, Mode::Read), "the other tab does not change");
        assert!(matches!(app.tabs[1].mode, Mode::Source(_)));
    }

    #[test]
    fn an_existing_name_opens_that_file_in_source_without_touching_it() {
        let (dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(vec![p[0].clone(), p[1].clone()]);
        // Already open in a tab behind: it comes to the front, in the source, without a new tab.
        assert_eq!(app.create_file("a"), Some(true));
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active, 0);
        assert!(matches!(app.tabs[0].mode, Mode::Source(_)));
        assert_eq!(fs::read_to_string(dir.path().join("a.md")).unwrap(), "# a.md\n");
        // Not open: a new tab, in the source.
        fs::write(dir.path().join("c.md"), "# c\n").unwrap();
        assert_eq!(app.create_file("c.md"), Some(true));
        assert_eq!(app.tabs.len(), 3);
        assert!(matches!(app.tabs[2].mode, Mode::Source(_)));
        assert_eq!(fs::read_to_string(dir.path().join("c.md")).unwrap(), "# c\n");
    }

    #[test]
    fn invalid_names_create_and_open_nothing() {
        let root = tempfile::tempdir().unwrap();
        let mut app = app_with_root(root.path());
        for bad in ["", "  ", "a/b", ".x"] {
            assert_eq!(app.create_file(bad), None, "{bad:?}");
        }
        assert!(app.tabs.is_empty() && fs::read_dir(root.path()).unwrap().count() == 0);
        // With no watched folder and no tab there is nowhere to create.
        assert_eq!(empty_app().create_file("test"), None);
    }

    #[test]
    fn creating_a_file_lights_no_ring() {
        let root = tempfile::tempdir().unwrap();
        let mut app = app_with_root(root.path());
        app.create_file("test");
        assert!(!app.ring_lit() && !app.effects_lit());
        assert!(app.live.writing(Instant::now()).is_empty(), "no \"writing\"");
    }

    #[test]
    fn opening_the_same_file_twice_keeps_one_tab() {
        let (_dir, p) = files(&["a.md"]);
        let mut app = empty_app();
        app.open_paths(vec![p[0].clone()]);
        let id = app.tabs[0].doc.id;
        app.open_paths(vec![p[0].clone()]);
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.tabs[0].doc.id, id, "not reread");
    }

    #[test]
    fn opening_two_files_makes_two_tabs_with_the_last_in_front() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.tab().unwrap().doc.path, p[1]);
        // Reopened, `a` comes back to the front without a new tab.
        app.open_paths(vec![p[0].clone()]);
        assert_eq!((app.tabs.len(), app.active), (2, 0));
    }

    #[test]
    fn closing_the_last_tab_leaves_no_tabs() {
        let (_dir, p) = files(&["a.md"]);
        let mut app = empty_app();
        app.open_paths(p);
        app.close_active();
        assert!(app.tabs.is_empty() && app.tab().is_none());
        app.close_active();
        assert!(app.tabs.is_empty());
        assert!(app.live.open_roots().is_empty(), "root released");
    }

    /// Once the last tab is closed the sidebar no longer shows the repo of the closed file.
    #[test]
    fn closing_the_last_tab_clears_the_sidebar() {
        let (_dir, p) = files(&["a.md"]);
        let mut app = empty_app();
        app.open_paths(p);
        assert!(app.sidebar.repo.is_some());
        app.close_active();
        assert!(app.sidebar.repo.is_none());
    }

    /// When the front tab closes, the one to its right comes to the front; when one on the left closes, the
    /// front tab stays the same.
    #[test]
    fn closing_keeps_a_sensible_tab_in_front() {
        let (_dir, p) = files(&["a.md", "b.md", "c.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        app.select(1);
        app.close_active();
        assert_eq!(app.tab().unwrap().doc.path, p[2]);
        app.close(0);
        assert_eq!(app.tab().unwrap().doc.path, p[2]);
        app.close_active();
        assert!(app.tabs.is_empty());
    }

    #[test]
    fn ctrl_tab_cycles() {
        let (_dir, p) = files(&["a.md", "b.md", "c.md"]);
        let mut app = empty_app();
        app.open_paths(p);
        assert!(app.cycle(1));
        assert_eq!(app.active, 0, "after the last, the first");
        assert!(app.cycle(-1));
        assert_eq!(app.active, 2);
    }

    /// The row keys differ between two tabs, even on the same content and after
    /// closing and reopening the same file: Rune does not pass measured heights from one to the other.
    #[test]
    fn row_keys_differ_across_tabs_and_reopens() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        let key = |app: &App, i: usize| ui::doc_view::row_key(0, &app.tabs[i].doc);
        assert_ne!(key(&app, 0), key(&app, 1));
        let before = key(&app, 1);
        app.close_active();
        app.open_paths(vec![p[1].clone()]);
        assert_ne!(before, key(&app, 1), "reopened: new keys");
    }

    /// A background tab that receives new text does not light the fire of the front
    /// tab; its count stays, and the fire lights when it comes back to the front and the view
    /// reports the rows.
    #[test]
    fn a_background_reload_does_not_light_the_front_tab() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(p);
        // `a` was in front and showed its first block.
        app.select(0);
        app.tabs[0].visible_rows(0..1, false);
        app.select(1);
        let change = app.tabs[0].doc.update("# a.md\n\none\n\ntwo\n");
        assert!(!app.tabs[0].note_reload(&change, 3.0));
        assert_eq!(app.tabs[0].below.count(), 2, "counts in the background");
        assert!(app.tab().unwrap().fire_since.is_none(), "the front tab stays off");
        assert!(app.fire_rests(), "the front tab stays at rest");
        app.select(0);
        assert!(app.visible_rows(app.tabs[0].doc.id, 0..1));
        assert_eq!(app.tab().unwrap().fire_since, Some(3.0));
    }

    /// The visible rows reported by a tab's view hold for that tab.
    #[test]
    fn visible_rows_go_to_the_tab_that_reported_them() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(p);
        let a = app.tabs[0].doc.id;
        app.visible_rows(a, 0..1);
        assert_eq!(app.tabs[0].visible_last, app.tabs[0].doc.blocks[0].id);
        assert_eq!(app.tabs[1].visible_last, 0);
    }

    /// With "reduce motion" no frame goes by: `poll` runs only at wakeups. Rune measures
    /// `wake_after` from the frame-start clock, older than `Instant::now()`: the wakeup
    /// can arrive before the end of the window. That wakeup arms another one, and
    /// the ring goes off at the end.
    #[test]
    fn an_early_wake_rearms_and_the_ring_goes_off() {
        let t0 = Instant::now();
        let ms = |n| Duration::from_millis(n);
        let window = live::activity::WRITING_WINDOW;
        let mut app = empty_app();
        let w = |d| vec![(PathBuf::from("/r/a.md"), d)];
        let (changed, wake) = app.ring_step(&w(window), 100.0, t0);
        assert!(changed && app.ring_lit());
        let wake = wake.expect("a wakeup at the end");
        let clock = 100.0 + wake.as_secs_f64();
        let now = t0 + window - ms(5);
        let (changed, again) = app.ring_step(&w(ms(5)), clock, now);
        assert!(!changed && app.ring_lit());
        let again = again.expect("the early wakeup arms another one");
        assert!(again >= ms(5));
        let (changed, last) = app.ring_step(&[], clock + again.as_secs_f64(), now + again);
        assert!(changed && !app.ring_lit(), "off at the end of the window");
        assert_eq!(last, None, "no other wakeup");
    }

    /// Any poll with the ring lit (a frame) does not arm a new wakeup.
    #[test]
    fn one_wake_per_window() {
        let t0 = Instant::now();
        let window = live::activity::WRITING_WINDOW;
        let mut app = empty_app();
        let w = |d| vec![(PathBuf::from("/r/a.md"), d)];
        assert!(app.ring_step(&w(window), 0.0, t0).1.is_some());
        let dt = Duration::from_millis(16);
        assert_eq!(app.ring_step(&w(window - dt), 0.016, t0 + dt).1, None);
        assert!(app.ring_step(&w(window), 0.032, t0 + 2 * dt).1.is_some());
    }

    /// Two files writing: each keeps the clock at which it lit, the wakeup is for the
    /// first that ends, and when it ends only its ring goes off.
    #[test]
    fn each_writing_file_has_its_own_ring() {
        let t0 = Instant::now();
        let window = live::activity::WRITING_WINDOW;
        let ms = |n| Duration::from_millis(n);
        let (a, b) = (PathBuf::from("/r/a.md"), PathBuf::from("/r/b.md"));
        let mut app = empty_app();
        app.ring_step(&[(a.clone(), window)], 1.0, t0);
        let (changed, wake) = app.ring_step(&[(a.clone(), window - ms(2000)), (b.clone(), window)], 3.0, t0 + ms(2000));
        assert!(changed);
        assert_eq!(wake, None, "the first that ends is still `a`: the armed wakeup holds");
        assert_eq!((app.ring_of(&a), app.ring_of(&b)), (Some(1.0), Some(3.0)));
        let (changed, wake) = app.ring_step(&[(b.clone(), ms(2000))], 11.0, t0 + window);
        assert!(changed && app.ring_of(&a).is_none() && app.ring_of(&b) == Some(3.0));
        assert_eq!(wake, Some(ms(2001)), "then the one of `b`");
    }

    /// Saving the source goes back to reading with the saved text: the watcher's reread
    /// is an app write, so no changed-block mark, fire, caret
    /// or "writing".
    #[test]
    fn saving_the_source_lights_nothing() {
        let (_dir, p) = files(&["a.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        app.tabs[0].visible_rows(0..1, false);
        let tab = &mut app.tabs[0];
        tab.mode = Mode::Source(Box::new(edit::enter(&tab.doc)));
        let mine = "# a.md\n\none\n\ntwo\n\n```rs\nfn";
        if let Mode::Source(s) = &mut tab.mode {
            s.set_text(mine);
            s.dirty = true;
        }
        assert!(edit::leave(tab, &mut app.live.own).is_ok());
        assert_eq!(fs::read_to_string(&p[0]).unwrap(), mine);
        let hash = crate::doc::ids::hash_str(mine);
        assert!(app.live.own.is_own(&app.tabs[0].doc.path, hash));
        // The reread requested by `leave` runs on a thread: `poll` runs until it consumes it.
        let t0 = Instant::now();
        while app.live.own.is_own(&app.tabs[0].doc.path, hash) {
            assert!(t0.elapsed() < Duration::from_secs(5), "reread never arrived");
            let (changed, _) = app.poll_tabs(1.0, Instant::now());
            assert!(!changed, "the view does not change for our write");
            std::thread::sleep(Duration::from_millis(5));
        }
        let tab = &app.tabs[0];
        assert_eq!((tab.doc.file_hash, tab.doc.blocks.len()), (hash, 4), "reading shows the saved text");
        assert!(tab.doc.blocks.iter().all(|b| !tab.is_marked(b.id)), "no mark");
        assert_eq!((tab.fire_since, tab.fire_armed, tab.below.count()), (None, None, 0), "no fire");
        assert_eq!(tab.caret_since, None, "no caret");
        assert!(app.live.writing_left(tab.path(), Instant::now()).is_none() && !app.ring_lit(), "no ring");
    }

    /// "Keep mine" after a conflict: like a save, the watcher's reread is an
    /// app write; back in reading, no marks, fire, caret or ring.
    #[test]
    fn keeping_mine_lights_nothing() {
        let (_dir, p) = files(&["a.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        app.tabs[0].visible_rows(0..1, false);
        let mine = "# a.md\n\none\n\ntwo\n";
        edit_tab(&mut app, 0, mine);
        fs::write(&p[0], "# a.md\n\nby the agent\n").unwrap();
        assert!(app.save_source(), "Cmd+S on the front tab, in the source");
        let id = app.tabs[0].doc.id;
        assert!(matches!(&app.tabs[0].mode, Mode::Source(s) if s.notice == Some(edit::Notice::Conflict)));
        app.keep_mine(id);
        assert_eq!(fs::read_to_string(&p[0]).unwrap(), mine);
        let hash = crate::doc::ids::hash_str(mine);
        assert!(edit::leave(&mut app.tabs[0], &mut app.live.own).is_ok());
        let t0 = Instant::now();
        while app.live.own.is_own(&app.tabs[0].doc.path, hash) {
            assert!(t0.elapsed() < Duration::from_secs(5), "reread never arrived");
            let (changed, _) = app.poll_tabs(1.0, Instant::now());
            assert!(!changed, "the view does not change for our write");
            std::thread::sleep(Duration::from_millis(5));
        }
        let tab = &app.tabs[0];
        assert_eq!((tab.doc.file_hash, tab.doc.blocks.len()), (hash, 3), "reading shows my text");
        assert!(tab.doc.blocks.iter().all(|b| !tab.is_marked(b.id)), "no mark");
        assert_eq!((tab.fire_since, tab.fire_armed, tab.below.count()), (None, None, 0), "no fire");
        assert_eq!(tab.caret_since, None, "no caret");
        assert!(app.live.writing_left(tab.path(), Instant::now()).is_none() && !app.ring_lit(), "no ring");
    }

    /// Puts tab `i` in the source with the edited text `text`.
    fn edit_tab(app: &mut App, i: usize, text: &str) {
        let tab = &mut app.tabs[i];
        let mut src = edit::enter(&tab.doc);
        src.set_text(text);
        src.dirty = true;
        tab.mode = Mode::Source(Box::new(src));
    }

    fn asking(app: &App, i: usize) -> bool {
        matches!(&app.tabs[i].mode, Mode::Source(s) if s.asking)
    }

    /// An mtime in the past: a write would bring it to now.
    fn age(path: &Path) -> std::time::SystemTime {
        let old = std::time::SystemTime::now() - Duration::from_secs(3600);
        fs::File::options().write(true).open(path).unwrap().set_modified(old).unwrap();
        old
    }

    /// Without changes (even in the source) the tab closes at once, without asking.
    #[test]
    fn closing_without_edits_closes_at_once() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        app.tabs[0].mode = Mode::Source(Box::new(edit::enter(&app.tabs[0].doc)));
        assert!(app.close(0), "closed");
        assert!(app.close_active(), "closed");
        assert!(app.tabs.is_empty());
    }

    /// With unsaved changes Cmd+W (or the middle click) does not close and does not write: the tab
    /// comes to the front with the question. A second Cmd+W leaves it as it is.
    #[test]
    fn closing_a_tab_with_edits_asks() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        let old = age(&p[0]);
        edit_tab(&mut app, 0, "# a.md\n\nmine\n");
        assert_eq!(app.active, 1);
        assert!(!app.close(0), "not closed");
        assert_eq!((app.tabs.len(), app.active), (2, 0), "open and in front");
        assert!(asking(&app, 0), "asks");
        assert!(!app.close_active(), "the second Cmd+W does not close");
        assert!(asking(&app, 0), "still asking");
        assert_eq!(fs::metadata(&p[0]).unwrap().modified().unwrap(), old, "nothing written");
    }

    /// "Save": writes and closes.
    #[test]
    fn save_answer_writes_and_closes() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        edit_tab(&mut app, 0, "# a.md\n\nmine\n");
        app.close(0);
        let id = app.tabs[0].doc.id;
        app.answer_close(id, CloseAnswer::Save);
        assert_eq!(fs::read_to_string(&p[0]).unwrap(), "# a.md\n\nmine\n");
        assert_eq!(app.tabs.len(), 1, "closed");
        assert_eq!(app.tabs[0].doc.path, p[1]);
    }

    /// "Save" with the file changed on disk: nothing is written, the tab stays with the
    /// conflict row instead of the question.
    #[test]
    fn save_answer_with_a_conflict_keeps_the_tab() {
        let (_dir, p) = files(&["a.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        edit_tab(&mut app, 0, "# a.md\n\nmine\n");
        app.close(0);
        fs::write(&p[0], "# a.md\n\nby the agent\n").unwrap();
        let id = app.tabs[0].doc.id;
        app.answer_close(id, CloseAnswer::Save);
        assert_eq!(app.tabs.len(), 1, "not closed");
        assert!(matches!(&app.tabs[0].mode, Mode::Source(s) if !s.asking && s.notice == Some(edit::Notice::Conflict) && s.dirty && s.text() == "# a.md\n\nmine\n"));
        assert_eq!(fs::read_to_string(&p[0]).unwrap(), "# a.md\n\nby the agent\n");
    }

    /// "Save" with a write error: the tab stays with "Save failed".
    #[test]
    fn save_answer_with_a_write_error_keeps_the_tab() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, p) = files(&["a.md"]);
        if edit::tests::as_root(dir.path()) {
            return;
        }
        let mut app = empty_app();
        app.open_paths(p.clone());
        edit_tab(&mut app, 0, "# a.md\n\nmine\n");
        app.close(0);
        let id = app.tabs[0].doc.id;
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
        app.answer_close(id, CloseAnswer::Save);
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(app.tabs.len(), 1, "not closed");
        assert!(matches!(&app.tabs[0].mode, Mode::Source(s) if !s.asking && s.notice == Some(edit::Notice::SaveFailed) && s.dirty));
    }

    /// "Don't save": closes and drops the changes; the file on disk does not change.
    #[test]
    fn discard_answer_closes_without_writing() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        let before = fs::read(&p[0]).unwrap();
        let old = age(&p[0]);
        edit_tab(&mut app, 0, "# a.md\n\nmine\n");
        app.close(0);
        let id = app.tabs[0].doc.id;
        app.answer_close(id, CloseAnswer::Discard);
        assert_eq!(app.tabs.len(), 1, "closed");
        assert_eq!(fs::read(&p[0]).unwrap(), before, "same bytes");
        assert_eq!(fs::metadata(&p[0]).unwrap().modified().unwrap(), old, "nothing written");
    }

    /// "Cancel" (and Esc): the question goes away, the tab stays with changes and cursor.
    #[test]
    fn cancel_answer_keeps_edits_and_caret() {
        let (_dir, p) = files(&["a.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        edit_tab(&mut app, 0, "# a.md\n\nmine\n");
        if let Mode::Source(s) = &mut app.tabs[0].mode {
            s.pieces.set_caret(9);
        }
        app.close(0);
        let id = app.tabs[0].doc.id;
        app.answer_close(id, CloseAnswer::Cancel);
        assert_eq!(app.tabs.len(), 1);
        assert!(matches!(&app.tabs[0].mode, Mode::Source(s) if !s.asking && s.dirty && s.text() == "# a.md\n\nmine\n" && s.pieces.caret() == 9));
        assert_eq!(fs::read_to_string(&p[0]).unwrap(), "# a.md\n");
    }

    /// An answer for a tab that no longer asks (the button of an old frame) does nothing.
    #[test]
    fn a_stale_answer_does_nothing() {
        let (_dir, p) = files(&["a.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        edit_tab(&mut app, 0, "# a.md\n\nmine\n");
        let id = app.tabs[0].doc.id;
        app.answer_close(id, CloseAnswer::Discard);
        assert_eq!(app.tabs.len(), 1, "it was not asking: it stays");
    }

    /// Cmd+S with the question open: saves and the question goes away (there is nothing left to save).
    #[test]
    fn saving_while_asking_ends_the_ask() {
        let (_dir, p) = files(&["a.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        edit_tab(&mut app, 0, "# a.md\n\nmine\n");
        app.close(0);
        assert!(app.save_source());
        assert!(!asking(&app, 0));
        assert_eq!(app.tabs.len(), 1);
    }

    /// When the app quits (`on_close`, `on_quit`) the changes of each tab are saved.
    #[test]
    fn quitting_saves_every_tab() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        edit_tab(&mut app, 0, "# a.md\n\none\n");
        edit_tab(&mut app, 1, "# b.md\n\ntwo\n");
        app.save_all();
        assert_eq!(fs::read_to_string(&p[0]).unwrap(), "# a.md\n\none\n");
        assert_eq!(fs::read_to_string(&p[1]).unwrap(), "# b.md\n\ntwo\n");
    }

    /// The recovery copies in `dir` (the `.md` files, without the list of copies to open).
    fn copies(dir: &Path) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = fs::read_dir(dir).map(|r| r.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e == "md")).collect()).unwrap_or_default();
        v.sort();
        v
    }

    /// Quitting with a conflict does not lose the changes. The text goes into a recovery copy
    /// (only one, even if `on_quit` and `on_close` both arrive), the file on disk stays
    /// the agent's one, and at the next startup the copy opens in a tab (only once).
    #[test]
    fn quitting_with_a_conflict_keeps_a_recovery_copy() {
        let (_dir, p) = files(&["a.md"]);
        let support = tempfile::tempdir().unwrap();
        let rec = support.path().join("Recuperati");
        let mut app = empty_app();
        app.recovery = Some(rec.clone());
        app.open_paths(p.clone());
        edit_tab(&mut app, 0, "# a.md\n\nmine\n");
        fs::write(&p[0], "# a.md\n\nby the agent\n").unwrap();
        app.save_all();
        app.save_all();
        assert_eq!(fs::read_to_string(&p[0]).unwrap(), "# a.md\n\nby the agent\n", "the file stays as it is");
        let found = copies(&rec);
        assert_eq!(found.len(), 1, "only one copy: {found:?}");
        assert_eq!(fs::read_to_string(&found[0]).unwrap(), "# a.md\n\nmine\n", "the exact text");
        let name = found[0].file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("a-") && name.len() == "a-AAAAMMGG-HHMMSS.md".len(), "{name}");
        // The next startup: the copy opens in a tab and stays on disk.
        let mut next = empty_app();
        next.recovery = Some(rec.clone());
        next.open_recovered();
        assert_eq!(next.tabs.len(), 1);
        assert_eq!(next.tabs[0].title(), name);
        assert!(found[0].exists(), "the copy stays until the user deletes it");
        // The startup after does not reopen it.
        let mut later = empty_app();
        later.recovery = Some(rec);
        later.open_recovered();
        assert!(later.tabs.is_empty());
    }

    /// A write error at quit also leaves a copy; the file stays as it was.
    #[test]
    fn quitting_with_a_write_error_keeps_a_recovery_copy() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, p) = files(&["a.md"]);
        if edit::tests::as_root(dir.path()) {
            return;
        }
        let support = tempfile::tempdir().unwrap();
        let rec = support.path().join("Recuperati");
        let mut app = empty_app();
        app.recovery = Some(rec.clone());
        app.open_paths(p.clone());
        edit_tab(&mut app, 0, "# a.md\n\nmine\n");
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
        app.save_all();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(fs::read_to_string(&p[0]).unwrap(), "# a.md\n");
        let found = copies(&rec);
        assert_eq!(found.len(), 1);
        assert_eq!(fs::read_to_string(&found[0]).unwrap(), "# a.md\n\nmine\n");
    }

    /// An agent writing the file being edited (Cmd+E) lights "writing" as in
    /// reading, before the save finds the conflict; our save does not.
    #[test]
    fn an_external_write_during_source_lights_the_ring_and_our_save_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().canonicalize().unwrap().join("a.md");
        fs::write(&p, "# a\n").unwrap();
        let mut app = empty_app();
        app.open_paths(vec![p.clone()]);
        app.live.start(std::sync::Arc::new(|| {}), Vec::new());
        edit_tab(&mut app, 0, "# a\n\nmine\n");
        std::thread::sleep(Duration::from_millis(300));
        // Our save: the event arrives, but the ring stays off.
        assert!(app.save_source());
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_millis(1500) {
            app.poll_tabs(1.0, Instant::now());
            assert!(app.live.writing_left(&p, Instant::now()).is_none(), "our write does not light the ring");
            std::thread::sleep(Duration::from_millis(16));
        }
        // The agent writes: the ring lights, the tab stays in the source with the changes.
        fs::write(&p, "# a\n\nby the agent\n").unwrap();
        let t0 = Instant::now();
        while app.live.writing_left(&p, Instant::now()).is_none() {
            assert!(t0.elapsed() < Duration::from_secs(5), "ring never lit");
            app.poll_tabs(1.0, Instant::now());
            std::thread::sleep(Duration::from_millis(16));
        }
        assert!(matches!(&app.tabs[0].mode, Mode::Source(s) if s.text() == "# a\n\nmine\n"));
    }

    /// End to end with the real watcher: the folder of the open file disappears (a `git
    /// checkout` without `docs/`) and comes back with the file changed; the tab reloads.
    #[test]
    fn a_tab_whose_folder_is_deleted_and_recreated_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let docs = dir.path().canonicalize().unwrap().join("docs");
        fs::create_dir(&docs).unwrap();
        let p = docs.join("a.md");
        fs::write(&p, "# a\n").unwrap();
        let mut app = empty_app();
        app.open_paths(vec![p.clone()]);
        app.live.start(std::sync::Arc::new(|| {}), Vec::new());
        std::thread::sleep(Duration::from_millis(300));
        fs::remove_dir_all(&docs).unwrap();
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_millis(1500) {
            app.poll_tabs(1.0, Instant::now());
            std::thread::sleep(Duration::from_millis(16));
        }
        fs::create_dir(&docs).unwrap();
        fs::write(&p, "# a\n\nback\n").unwrap();
        let t0 = Instant::now();
        while app.tabs[0].doc.blocks.len() != 2 {
            assert!(t0.elapsed() < Duration::from_secs(6), "the tab did not reload again");
            app.poll_tabs(1.0, Instant::now());
            std::thread::sleep(Duration::from_millis(16));
        }
    }

    /// In the source the tab does not reload; back in reading without changes, it rereads and
    /// shows the write that arrived in the meantime. Each tab has its own mode.
    #[test]
    fn the_source_suspends_reload_and_leaving_rereads() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        let tab = &mut app.tabs[0];
        tab.mode = Mode::Source(Box::new(edit::enter(&tab.doc)));
        assert!(matches!(app.tabs[1].mode, Mode::Read), "the other tab stays in reading");
        fs::write(&p[0], "# a.md\n\nby the agent\n").unwrap();
        app.tabs[0].follow.reread();
        for _ in 0..20 {
            app.poll_tabs(1.0, Instant::now());
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(app.tabs[0].doc.blocks.len(), 1, "suspended in the source");
        assert!(edit::leave(&mut app.tabs[0], &mut app.live.own).is_ok());
        let t0 = Instant::now();
        while app.tabs[0].doc.blocks.len() != 2 {
            assert!(t0.elapsed() < Duration::from_secs(5), "reread never arrived");
            app.poll_tabs(1.0, Instant::now());
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn tab_of(source: &str) -> Tab {
        let mut live = Live::default();
        let path = Path::new("/r/t.md");
        Tab::new(Document::from_source(path, source), live.follow(path))
    }

    /// Like the ring: the wakeup for the expiring mark arrives early on the wall, arms
    /// another one, and the mark is removed at the end.
    #[test]
    fn an_early_wake_rearms_and_the_changed_mark_goes_off() {
        let t0 = Instant::now();
        let ms = |n| Duration::from_millis(n);
        let mut tab = tab_of("# t\n");
        let change = Change { added: vec![3], changed: vec![4], ..Default::default() };
        let (changed, wake) = tab.update_marks(Some(&change), 100.0, t0);
        assert!(changed && tab.is_marked(3) && tab.is_marked(4));
        let wake = wake.expect("a wakeup at expiry");
        let clock = 100.0 + wake.as_secs_f64();
        let now = t0 + live::arriving::CHANGED_MARK - ms(5);
        let (changed, again) = tab.update_marks(None, clock, now);
        assert!(!changed && tab.changed.is_marked(3, now));
        let again = again.expect("the early wakeup arms another one");
        let (changed, last) = tab.update_marks(None, clock + again.as_secs_f64(), now + again);
        assert!(changed && !tab.changed.is_marked(3, now + again), "removed at expiry");
        assert_eq!(last, None, "no other wakeup");
    }

    /// Any poll with a mark lit does not arm a new wakeup.
    #[test]
    fn one_wake_per_mark_expiry() {
        let t0 = Instant::now();
        let mut tab = tab_of("# t\n");
        let change = Change { added: vec![1], ..Default::default() };
        assert!(tab.update_marks(Some(&change), 0.0, t0).1.is_some());
        let dt = Duration::from_millis(16);
        assert_eq!(tab.update_marks(None, 0.016, t0 + dt).1, None);
    }

    /// The caret: lit with the point of a reload while the file is writing, even in the middle of the
    /// file; it stays where it is on a reload without a new point; off at the end of the window (constraint 4).
    #[test]
    fn the_caret_follows_the_last_change_while_writing() {
        let mut tab = tab_of("# T\n\nOne.\n\nTwo.\n");
        let old = tab.doc.blocks.clone();
        let change = tab.doc.update("# T\n\nOne bis.\n\nTwo.\n");
        let at = live::caret::locate(&old, &tab.doc.blocks, &change).expect("a point");
        assert!(!tab.caret_step(Some(&at), false, 1.0) && tab.caret_since.is_none(), "not writing");
        assert!(tab.caret_step(Some(&at), true, 1.0) && tab.caret_since == Some(1.0));
        assert_eq!(tab.caret_at.as_ref().map(|a| a.block), Some(tab.doc.blocks[1].id), "in the middle of the file");
        assert!(!tab.caret_step(None, true, 2.0) && tab.caret_since == Some(1.0), "stays where it is");
        assert!(tab.caret_step(None, false, 3.0) && tab.caret_since.is_none() && tab.caret_at.is_none(), "off at the end of the window");
    }

    /// The block of the point disappears: the caret goes off instead of staying on a block that does not exist.
    #[test]
    fn the_caret_goes_off_when_its_block_goes() {
        let mut tab = tab_of("One.\n\nTwo.\n");
        let old = tab.doc.blocks.clone();
        let change = tab.doc.update("One.\n\nTwo bis.\n");
        let at = live::caret::locate(&old, &tab.doc.blocks, &change).unwrap();
        tab.caret_step(Some(&at), true, 1.0);
        tab.doc.update("One.\n");
        assert!(tab.caret_step(None, true, 2.0) && tab.caret_at.is_none());
    }

    // The preview tab (like VS Code): a single click in the sidebar opens the file in a single
    // tab that the next click replaces.

    fn previews(app: &App) -> Vec<bool> {
        app.tabs.iter().map(|t| t.preview).collect()
    }

    #[test]
    fn a_single_click_opens_one_preview_tab_that_the_next_replaces() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        assert!(app.open_preview(p[0].clone()));
        assert_eq!(previews(&app), [true]);
        assert!(app.open_preview(p[1].clone()));
        assert_eq!(app.tabs.len(), 1, "replaced, not added");
        assert_eq!(app.tab().unwrap().doc.path, p[1]);
        assert_eq!(previews(&app), [true]);
        assert_eq!(app.live.open_roots().len(), 1, "the reread of a.md is released");
    }

    #[test]
    fn the_preview_is_replaced_in_place_and_pinned_tabs_stay() {
        let (_dir, p) = files(&["a.md", "b.md", "c.md", "d.md"]);
        let mut app = empty_app();
        app.open_paths(vec![p[0].clone()]);
        app.open_preview(p[1].clone());
        app.open_paths(vec![p[2].clone()]);
        assert_eq!(previews(&app), [false, true, false], "Finder and terminal open pinned");
        app.open_preview(p[3].clone());
        let names: Vec<String> = app.tabs.iter().map(Tab::title).collect();
        assert_eq!(names, ["a.md", "d.md", "c.md"]);
        assert_eq!((app.active, previews(&app)), (1, vec![false, true, false]));
    }

    #[test]
    fn a_click_on_an_open_file_only_switches_to_its_tab() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(vec![p[0].clone()]);
        app.open_preview(p[1].clone());
        let ids: Vec<u64> = app.tabs.iter().map(|t| t.doc.id).collect();
        assert!(app.open_preview(p[0].clone()));
        assert_eq!(app.active, 0);
        assert_eq!(app.tabs.iter().map(|t| t.doc.id).collect::<Vec<_>>(), ids, "nothing reread or replaced");
        assert_eq!(previews(&app), [false, true]);
        assert!(!app.open_preview(p[0].clone()), "already in front");
    }

    #[test]
    fn pinning_keeps_the_tab_when_the_next_preview_opens() {
        let (_dir, p) = files(&["a.md", "b.md", "c.md"]);
        let mut app = empty_app();
        app.open_preview(p[0].clone());
        app.pin_path(&p[0]);
        assert_eq!(previews(&app), [false]);
        app.open_preview(p[1].clone());
        assert_eq!(previews(&app), [false, true]);
        let id = app.tabs[1].doc.id;
        app.pin(id);
        app.open_preview(p[2].clone());
        assert_eq!(previews(&app), [false, false, true]);
    }

    #[test]
    fn opening_from_finder_pins_a_preview_tab() {
        let (_dir, p) = files(&["a.md"]);
        let mut app = empty_app();
        app.open_preview(p[0].clone());
        app.open_paths(vec![p[0].clone()]);
        assert_eq!(previews(&app), [false]);
    }

    #[test]
    fn editing_or_saving_pins_the_preview_tab() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_preview(p[0].clone());
        let tab = app.tab_mut().unwrap();
        tab.mode = Mode::Source(Box::new(edit::enter(&tab.doc)));
        assert_eq!(previews(&app), [true], "Cmd+E without changes stays a preview");
        let id = app.tabs[0].doc.id;
        app.source_changed(id);
        assert_eq!(previews(&app), [false], "an edit pins it");
        app.open_preview(p[1].clone());
        assert_eq!(previews(&app), [false, true]);
        let tab = app.tab_mut().unwrap();
        tab.mode = Mode::Source(Box::new(edit::enter(&tab.doc)));
        assert!(app.save_source());
        assert_eq!(previews(&app), [false, false], "Cmd+S pins it");
    }

    #[test]
    fn closing_the_preview_tab_leaves_no_preview() {
        let (_dir, p) = files(&["a.md", "b.md", "c.md"]);
        let mut app = empty_app();
        app.open_paths(vec![p[0].clone()]);
        app.open_preview(p[1].clone());
        app.close_active();
        app.open_preview(p[2].clone());
        assert_eq!(previews(&app), [false, true]);
        assert_eq!(app.tabs.len(), 2);
    }
}
