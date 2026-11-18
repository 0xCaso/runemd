//! RuneMD: a Markdown reader for the Mac, in Rune.
//!
//! `runemd FILE.md` opens the file and renders it with rune-kit's `Markdown` component, one
//! top-level block per `virtual_rows` row inside a scroll.
//!
//! Benchmark probes (environment variables, so they do not clash with Rune's flags):
//! - `MDSPIKE_LOG=path`: writes `first_content_epoch_ms` at the first presented frame.
//! - `MDSPIKE_EXIT=1`: exits 300 ms after the first frame (startup measurements).
//! - `MDSPIKE_SCROLL=seconds` (+ `MDSPIKE_SPEED` pt/s, default 3000): after 1 s scrolls at
//!   constant speed and writes the intervals between presented frames to `MDSPIKE_LOG`, then exits.
//! - `MDSPIKE_LATENCY=path`: on every reload writes, at the first frame that shows it, the ms from
//!   the file write (mtime) to the reload and to the frame.
//! - `MDSPIKE_ACTIVE=1`: the window counts as active though not key (virtual screens);
//!   `MDSPIKE_QUICK=1`: the ⌘P panel opens at the first poll.
//! - With `MDSPIKE_LOG` it also writes the cost of the index cache (`index_cache_ms`) and
//!   when the scan ends (`index_scan_epoch_ms`).
//!
//! Settings (Cmd+,) live in `~/Library/Application Support/RuneMD/prefs.json`
//! (`settings`). `RUNEMD_ROOTS=dir1:dir2` replaces the watched folders and runs the app with
//! in-memory settings, which are neither read nor saved (tests and measurements);
//! `RUNEMD_THEME=light|dark` wins over the chosen theme; `RUNEMD_REDUCE_MOTION=1|0` wins over the system
//! "reduce motion" (headless Rune does not read it); `RUNEMD_GLASS=40` gives the glass tint
//! 40 % opacity instead of 60 % and opens the window at (200, 120) (photos); `RUNEMD_SIDEBAR=180` gives
//! the sidebar that width for the session, with no snap and no save (photos); `RUNE_DIRS_ROOT` moves the index cache
//! and the settings.

mod app;
mod doc;
mod edit;
mod fonts;
mod fx;
mod index;
mod lab;
mod live;
mod open;
mod panes;
mod probe;
mod repo;
mod session;
mod settings;
mod ui;
mod watch;

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use rune::{AppSpec, Cx, El, Tx, Waker, el};

pub use app::App;

fn init() -> App {
    let mut paths = open::cli_paths(rune::extra_args());
    if paths.is_empty() {
        paths.extend(std::env::var("MDSPIKE_FILE").ok().map(PathBuf::from));
    }
    // The index cache is read before the first frame (cost in `docs/perf.md`); the
    // scan starts in `setup`, on its own thread.
    // The settings give the watched folders; with `RUNEMD_ROOTS` they stay in memory.
    let prefs = if index::env_roots().is_some() { rune::prefs::Prefs::memory() } else { rune::prefs::Prefs::open(settings::APP_ID) };
    let mut settings = settings::Settings::load(&prefs);
    // `RUNEMD_SIDEBAR` (photos): the sidebar width, within its limits. Dragging
    // snaps to 248 within 8 pt, so 240 cannot be reached by dragging.
    if let Some(w) = std::env::var("RUNEMD_SIDEBAR").ok().and_then(|v| v.parse::<f32>().ok()) {
        settings.sidebar = panes::Pane { width: w, learned: Vec::new() }.sanitized(panes::SIDEBAR);
    }
    // `RUNEMD_SAVED` (photos and tests, separated by `:`): the session's saved folders.
    if let Some(v) = std::env::var_os("RUNEMD_SAVED") {
        settings.saved = std::env::split_paths(&v).filter(|p| !p.as_os_str().is_empty()).collect();
    }
    let t = Instant::now();
    let library = index::Library::load(index::roots_from(&settings.roots), index::cache_path());
    probe::index_cached(t.elapsed(), library.index.len());
    let session = session::Session::load(&prefs);
    let mut app = App::new(library, settings, prefs);
    app.scroll = probe::ScrollRun::from_env();
    // Recent from the cache: the sidebar is full from the first frame.
    app.sidebar.refresh(&app.library.index, &[], SystemTime::now());
    // The tabs of the last launch, then the files from Finder or the terminal (in front).
    app.restore_session(session);
    app.open_paths(paths);
    // The recovery copies from the last exit (a conflict or a write error).
    app.open_recovered();
    app
}

fn setup(s: &mut App, waker: Waker) {
    open::queue::set_waker(waker.clone());
    s.images = Some(rune::ImageCache::new(waker.clone()));
    s.find.set_waker(waker.clone());
    s.live.start(waker.clone(), s.library.roots().to_vec());
    // `--lab` (debug builds only): the bridge with Animation Lab (`lab::watch`).
    #[cfg(debug_assertions)]
    if lab::watch::enabled(rune::extra_args()) {
        s.lab = lab::watch::bridge_dir().map(|dir| lab::watch::start(dir, waker.clone()));
    }
    s.library.start(waker);
}

fn view(s: &App, cx: &mut Cx<App>) -> El {
    // The theme from the settings (System, Light, Dark): it changes live, the view runs again.
    let dark = s.dark(cx.system_dark());
    let mut theme = ui::theme::Theme::for_mode(dark);
    // `RUNEMD_GLASS` (photos): the opacity of the glass tint, in place of the theme's (60 %).
    if let Some(a) = s.glass_env {
        theme.glass = theme.glass.alpha(a);
    }
    // "Reduce motion": the macOS preference, or `RUNEMD_REDUCE_MOTION`. The override also goes to
    // Rune, so its transitions (hover, pressed scale, scroll) follow the effects.
    let reduced = s.reduced_motion(cx.system_reduced_motion());
    s.reduced_seen.set(reduced);
    cx.reduced_motion(reduced);
    // Keyboard focus like Rune's (3 pt, attached to the edge), but solid
    // (and a bit darker in the light theme).
    let focus = ui::theme::focus_ring(cx.focus_color(), dark);
    cx.focus_ring(3.0, 0.0, focus);
    let hooks = ui::sidebar::Hooks { get: |s: &App| &s.sidebar, get_mut: |s: &mut App| &mut s.sidebar, open: App::open_from_sidebar, pin: App::pin_from_sidebar, ring: App::ring_of, query: App::requery_sidebar, create: App::create_from_sidebar, save: App::save_from_sidebar, unsave: App::unsave_from_sidebar, recent_collapsed: App::recent_collapsed, toggle_recent: App::toggle_recent };
    // With a file open the wordmark is not drawn: its handles go, so the cache frees the decoded
    // pixels (about 6 MB at 2×); going back to the empty screen decodes them again.
    if s.tab().is_some() && s.forge_images.borrow().holds() {
        *s.forge_images.borrow_mut() = Default::default();
    }
    let body = match s.tab() {
        Some(tab @ app::Tab { mode: app::Mode::Source(src), .. }) => {
            let caret = ui::source::caret_lines(cx, tab, src);
            ui::source::view(s, hooks, tab, src, caret, &theme, reduced, !cx.hover_none())
        }
        Some(tab) => {
            // The "is writing" caret at the point of the last edit: where it is comes from the rects of the
            // character and of its line, which scrolling does not move (no view per frame).
            let caret = tab.caret_at.clone().zip(tab.caret_since).map(|(at, since)| {
                let id = tab.doc.id;
                let place = ui::doc_view::caret_point(cx.layout(&ui::doc_view::caret_key(id)), cx.layout(&ui::doc_view::caret_end_key(id)), cx.layout(&ui::doc_view::caret_row_key(id, at.block)), at.side);
                ui::doc_view::Caret { at, place, since, dark, reduced }
            });
            // Labelled blocks in two columns if the text column is wide (from the last
            // layout; Rune runs the view again when the threshold is crossed).
            let columns = cx.layout_with("rows", |r| r[2] - 2.0 * ui::theme::ROW_PAD_X >= ui::doc_view::TWO_COLUMNS).unwrap_or(false);
            let signals = ui::doc_view::Signals { marked: App::is_marked, mark: theme.mark, caret, hits: App::find_hits, edge: App::doc_edge, meta: theme.meta(), fonts: ui::markdown_style::fonts(fonts::Typeface::of(&s.settings.font, s.settings.size)), open: App::open_link, columns, bottom: fx::fire::doc_bottom(s.settings.fire.shows(), s.settings.fire_height), forge: ui::doc_view::Forge { on: s.forge(), look: fx::doc_fx::Look { dark, reduced }, metal: s.metal_block }, image: App::doc_image, images_ready: if s.forge().any() { s.doc_images_ready() } else { 0 } };
            let doc = ui::doc_view::view(s, App::active_document, s.style(dark), s.settings.column, App::visible_rows, signals);
            // The heading outline on the right, only if the window is at least sidebar + column
            // + 200 wide (before the first layout it is not known: no outline). The view reads it from Rune,
            // which runs the view again when the threshold is crossed; `App::wide` is the copy for the
            // handlers, which have no `Cx`. The container is always the same, with or without the outline:
            // crossing the threshold the document does not change place in the tree and keeps scroll and
            // state.
            // With the settings open the outline gives its place to the panel.
            // The threshold follows the dragged widths of sidebar and outline (`App::outline_min_window`).
            let min = s.outline_min_window();
            let wide = cx.layout_with("app", move |r| r[2] >= min).unwrap_or(false);
            let outline_w = s.settings.outline.width;
            let outline = if wide && !s.settings_panel.open { ui::outline::view(tab, hooks, outline_w, &theme, reduced, !cx.hover_none()) } else { None };
            // The outline's left edge can be dragged (`App::drag_outline`): the strip sits in the outline's
            // margin, so it does not cover the document's scroll.
            let outline_edge = outline.is_some().then(|| edge("outline-edge", false).pin(None, Some(0.0), Some(outline_w - ui::theme::EDGE_W), Some(0.0)));
            // Cmd+E on a file that is not UTF-8: the row above the reading view says so.
            let refused = tab.not_utf8.then(|| ui::source::not_utf8_row(&theme, s.settings.column));
            let doc = rune::col().w_full().h_full().children(refused).child(el().w_full().grow(1.0).min_h(0.0).child(doc));
            rune::row().key("doc-pane").transition(ui::theme::mode_transition(reduced)).w_full().grow(1.0).min_h(0.0).child(el().grow(1.0).min_w(0.0).h_full().child(doc)).children(outline).children(outline_edge)
        }
        // No file open: the wordmark with the fire in its letters (forge-empty). The images rasterize
        // at the strip's size step (`ui::forge::raster_w`) and the display scale, so a resize inside a
        // step rasterizes nothing; the handles are shared by content.
        None => {
            let scale = cx.env().scale;
            let pane_w = cx.layout_with("forge", |r| r[2]);
            let raster = ui::forge::raster_w(ui::forge::plate_w(pane_w.unwrap_or(ui::forge::PLATE_W + 2.0 * ui::forge::MARGIN)));
            let images = s.images.as_ref().map(|c| {
                let want = [c.svg(ui::forge::LETTERS_SVG, raster, scale), c.load(ui::forge::plate_png(raster, scale))];
                s.forge_images.borrow_mut().pick(want)
            });
            let forge = ui::forge::Forge { writing: s.writing_since(), stopped: s.writing_stopped(), file: s.writing_file().map(PathBuf::from), reduced, active_window: s.window_active, pane_w, images };
            el().grow(1.0).h_full().child(ui::forge::view(forge, &theme))
        }
    };
    // Full panel with the tab bar and the document, `FRAME` from the top, right and
    // bottom edges; on the left the sidebar, all on the system glass.
    let bar = ui::tabs::view(s, hooks, &theme, reduced, !cx.hover_none());
    // The settings panel (Cmd+,) right of the document. The container is always there,
    // so opening it does not move the document in the tree and does not lose its scroll.
    let aside = s.settings_panel.open.then(|| ui::settings::view(s, hooks, &theme, reduced, !cx.hover_none()));
    let body = rune::row().key("body").w_full().grow(1.0).min_h(0.0).child(el().grow(1.0).min_w(0.0).h_full().child(body)).children(aside);
    let content = rune::col().w_full().h_full().child(bar).child(body);
    let mut panel = el().w_full().h_full().fill(theme.panel).radius(ui::theme::R_PANEL).clip_rounded(ui::theme::R_PANEL).child(content);
    // The front tab's fire: always in reading mode, a strip at the bottom of the panel above
    // the document, low at rest; with new text below the view it rises and carries the label.
    // Out of the flow, so the document and its scroll do not move. The label is a sibling
    // strip after the slot, so Rune paints it above the flames. It sits on the fire's warm base,
    // almost white in both themes: the text takes a dark grey from the tokens (`panel` in dark,
    // `fg` in light).
    if let Some(tab) = s.tab().filter(|t| matches!(t.mode, app::Mode::Read)) {
        let lit = s.fire_of(tab);
        let look = s.settings.fire_look(fx::fire::accent_hex(cx.accent()));
        panel = panel.children(fire_band(lit, s.fire_flames(), look, dark, &theme, reduced, s.window_active));
    }
    // The "is writing" ring sits on the file's tab (`ui::tabs`) and on its rows in
    // Recent and in the tree.
    let sidebar_w = s.settings.sidebar.width;
    let sidebar = ui::sidebar::view(s, hooks, sidebar_w, &theme, reduced, !cx.hover_none());
    // The sidebar's right edge can be dragged (`App::drag_sidebar`); above everything, in the sidebar's margin.
    let sidebar_edge = edge("sidebar-edge", true).pin(Some(sidebar_w - ui::theme::EDGE_W), Some(0.0), None, Some(0.0));
    // Only these bands move the window (`window_chrome`): above the sidebar (the traffic lights), the
    // frame above the panel, and the tab bar (`ui::tabs`). AppKit's own titlebar band would also zoom the
    // window on a double click on a tab (feedback group 3, item 6).
    let drag_top = el().key("drag-sidebar").pin(Some(0.0), Some(0.0), None, None).w(sidebar_w - ui::theme::EDGE_W).h(ui::theme::SIDEBAR_TOP).window_drag();
    let drag_frame = el().key("drag-frame").pin(Some(sidebar_w), Some(0.0), Some(0.0), None).h(ui::theme::FRAME).window_drag();
    let main = el().key("main").grow(1.0).min_w(0.0).h_full().pt(ui::theme::FRAME).pr(ui::theme::FRAME).pb(ui::theme::FRAME).child(panel);
    // The window width for the handlers (`App::wide`: the heading outline is visible); the
    // view reads it from `cx`. It updates without running the view again.
    let width = |s: &mut App, r: [f32; 4], tx: &mut Tx| {
        s.set_window_width(r[2]);
        tx.live_only();
    };
    // The glass: the window is transparent and the system glass is below. Rune does not paint the
    // fill of a material when native glass is there, so the `glass` tint (60 %) sits in
    // a layer above. Without native glass (headless, material off) the root's fill
    // is the solid tint (`glass_fallback`) and the layer, of the same color, changes nothing.
    let tint = el().key("glass-tint").pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).fill(theme.glass);
    rune::row().key("app").on_layout_tx(width).w_full().h_full().system_material(rune::SystemMaterial::Glass).fill(theme.glass_fallback()).child(tint).child(sidebar).child(main).child(drag_top).child(drag_frame).child(sidebar_edge)
        .children(ui::tabs::width_popover(s, &theme))
        .children(ui::quick_open::panel(s, hooks, cx.layout("quick-rows"), s.settings.sidebar.width, &theme, reduced, !cx.hover_none()))
        .on_window_focus(|s: &mut App, focused, _| s.window_focus(focused))
}

/// The strip that drags the edge of the sidebar (`sidebar`) or of the outline: `EDGE_W` wide, inside the
/// panel's margin, ↔ cursor, double click for the default width. Invisible: the edge is already there.
fn edge(key: &str, sidebar: bool) -> El {
    let label = if sidebar { "Sidebar width" } else { "Outline width" };
    el().key(key)
        .w(ui::theme::EDGE_W)
        .cursor(rune::Cursor::ResizeColumn)
        .label(label)
        .on_drag(move |s: &mut App, d: &mut rune::Drag, tx: &mut Tx| {
            tx.instant();
            if sidebar { s.drag_sidebar(d) } else { s.drag_outline(d) }
        })
        .on_double_press_tx(move |s: &mut App, tx: &mut Tx| {
            tx.instant();
            s.reset_pane(!sidebar);
        })
}

/// The fire and its label at the bottom of the panel. `lit`: since when there is new text below the
/// view, and how many blocks; `None` at rest (low flames, no label). `flames` (`App::fire_flames`):
/// whether the flames show and when they fade at rest (Brief). Without flames (Off, or Brief
/// faded out) there is nothing at rest, and with new text (Off) only
/// the label, in `fg2` on a `panel` strip with the `line` line above: without the flames below,
/// the document text must not pass behind it.
fn fire_band(lit: Option<(f64, usize)>, flames: Option<Option<f64>>, look: fx::fire::Look, dark: bool, theme: &ui::theme::Theme, reduced: bool, active: bool) -> Vec<El> {
    let label = |count, ink| rune::text(ui::doc_view::fire_label(count)).key("fire-text").font(rune::TextStyle::new(ui::theme::UI_TEXT, ui::theme::UI_WEIGHT)).color(ink);
    let Some(fade_at) = flames else {
        let Some((_, count)) = lit else { return Vec::new() };
        let line = el().w_full().h(ui::theme::HAIRLINE).fill(theme.line);
        let strip = rune::col().w_full().h(ui::theme::TAB_BAR_H).fill(theme.panel).items_center().justify_center().child(label(count, theme.fg2));
        return vec![rune::col().key("fire-label").pin(Some(0.0), None, Some(0.0), Some(0.0)).child(line).child(strip)];
    };
    let band = |key: &str, h: f32| rune::col().key(key).pin(Some(0.0), None, Some(0.0), Some(0.0)).h(h);
    // The slot as tall as the lit fire at the chosen height; the label stays where it was.
    let flames = fx::fire::slot(band("fire", fx::fire::lit_height(look.height)), lit.map(|(since, _)| since), fade_at, dark, reduced, active, look);
    let Some((_, count)) = lit else { return vec![flames] };
    let ink = if dark { theme.panel } else { theme.fg };
    let text = band("fire-label", fx::fire::FIRE_H).items_center().justify_end().pb(ui::theme::FIRE_LABEL_BOTTOM).child(label(count, ink));
    // Two sibling strips: the slot first, the label after (Rune paints it above the flames).
    vec![flames, text]
}

/// Tab keys: Cmd+W closes the front one (with unsaved changes it asks,
/// `App::close`), Cmd+1…8 go to the tab, Cmd+9 to the last one (like Safari and Finder), Ctrl+Tab
/// and Ctrl+Shift+Tab to the next and the previous. True if the key belongs to the tabs; Cmd+W with no tabs
/// is left to Rune, which closes the window.
fn tab_key(s: &mut App, key: &rune::KeyEvent, tx: &mut Tx) -> bool {
    let m = key.mods;
    let changed = match key.key {
        rune::Key::Command('w') if m.cmd && !m.shift && !m.alt && !m.ctrl => {
            if s.tabs.is_empty() {
                return false;
            }
            let before = s.tab().map(|t| t.doc.id);
            s.close_active();
            s.tab().map(|t| t.doc.id) != before
        }
        rune::Key::Command(c @ '1'..='9') if m.cmd && !m.shift && !m.alt && !m.ctrl => {
            let n = c as usize - '1' as usize;
            let i = if c == '9' { s.tabs.len().saturating_sub(1) } else { n };
            if i >= s.tabs.len() {
                return true;
            }
            s.select(i)
        }
        rune::Key::Tab if m.ctrl && !m.cmd && !m.alt => s.cycle(if m.shift { -1 } else { 1 }),
        _ => return false,
    };
    if changed {
        s.restore_view(tx);
    }
    true
}

/// Every key not taken by a control, before Rune's selection: Cmd+A must be marked.
fn on_key(s: &mut App, key: &rune::KeyEvent, tx: &mut Tx) -> bool {
    // Cmd+,: opens and closes the settings; Esc closes them.
    if matches!(key.key, rune::Key::Command(',')) && key.mods.cmd && !key.mods.shift && !key.mods.alt && !key.mods.ctrl && !tx.key_repeat() {
        s.toggle_settings();
        return true;
    }
    // Esc first closes the width popover, then the settings.
    let esc = matches!(key.key, rune::Key::Escape) && !key.mods.cmd && !key.mods.shift && !key.mods.alt && !key.mods.ctrl;
    if esc && s.width_popover {
        s.width_popover = false;
        return true;
    }
    if esc && s.settings_panel.close() {
        return true;
    }
    // Cmd+Opt+= / Cmd+Opt+-: one step of the text width (↔ button, settings).
    if let Some(step) = ui::tabs::width_key(key, tx.key_base()) {
        s.set_column(s.settings.step_column(step), tx);
        return true;
    }
    // Cmd+P: opens and closes the Open file panel (`ui::quick_open::panel`). Only one field open at a time.
    if matches!(key.key, rune::Key::Command('p')) && key.mods.cmd && !key.mods.shift && !key.mods.alt && !tx.key_repeat() {
        s.sidebar.new_file.close();
        if s.sidebar.quick.active() {
            s.sidebar.quick.close();
        } else {
            s.sidebar.quick.show();
            s.requery_sidebar();
        }
        return true;
    }
    // Cmd+N: the "New file" field takes the place of "Open file".
    if matches!(key.key, rune::Key::Command('n')) && key.mods.cmd && !key.mods.shift && !key.mods.alt && !key.mods.ctrl && !tx.key_repeat() {
        s.find.close();
        s.sidebar.quick.close();
        s.sidebar.quick_rows.clear();
        s.sidebar.new_file.show();
        return true;
    }
    // Cmd+F: Find in file, in reading mode only (in the source it does nothing).
    if matches!(key.key, rune::Key::Command('f')) && key.mods.cmd && !key.mods.shift && !key.mods.alt && !key.mods.ctrl && !tx.key_repeat() {
        s.open_find();
        return true;
    }
    if tab_key(s, key, tx) {
        return true;
    }
    // Esc with the "Save changes…?" row open and the focus outside the field: "Cancel" (from the
    // field `ui::source::ask_key` takes it).
    if let Some(id) = s.tab().map(|t| t.doc.id)
        && ui::source::ask_key(s, id, key, tx)
    {
        return true;
    }
    // Cmd+E: source and back to reading; Cmd+S: saves the source. The field does not take them.
    let plain = key.mods.cmd && !key.mods.shift && !key.mods.alt && !key.mods.ctrl;
    if plain && !tx.key_repeat() {
        match key.key {
            rune::Key::Command('e') => return s.toggle_source(tx),
            rune::Key::Command('s') => return s.save_source(),
            _ => {}
        }
    }
    if let Some(tab) = s.tab() {
        ui::doc_view::note_key(&tab.doc, key, tx.key_repeat());
    }
    false
}

fn poll(s: &mut App, clock: f64, tx: &mut Tx) -> bool {
    // The front tab of the last session goes back to its reading place.
    if std::mem::take(&mut s.restore_pending) {
        s.restore_view(tx);
    }
    let quick = probe::quick_at_start();
    if quick {
        s.sidebar.quick.show();
        s.requery_sidebar();
    }
    let mut queued = open::queue::drain();
    queued.extend(probe::late_open(clock, tx));
    let opened = !queued.is_empty();
    if opened {
        let before = s.tab().map(|t| t.doc.id);
        s.open_paths(queued);
        if s.tab().map(|t| t.doc.id) != before {
            s.restore_view(tx);
        }
    }
    // The watcher must be drained on every wake-up (`Watcher::drain`); then every tab collects its
    // reread and updates fire, marks and caret. The fire lights up after a reload that
    // brings text below the view (`Tab::note_reload`, or the first `visible_rows` of the new layout)
    // and goes out when the last of those blocks enters the view: no wake-up is needed.
    let now = Instant::now();
    let (tabs, wakes) = s.poll_tabs(clock, now);
    for d in wakes {
        tx.wake_after(d);
    }
    // Find: a reload of the front tab runs the search again (without moving the view), and the
    // results that arrived from the search thread are applied.
    let found = s.poll_find(tx);
    // The index reads the same events; Recent and the tree are recomputed only if it changes.
    let was_scanned = s.library.scanned;
    let indexed = s.library.poll(s.live.drained(), Instant::now());
    if s.library.scanned && !was_scanned {
        probe::index_scanned(s.library.index.len());
    }
    if indexed {
        s.sidebar.no_access = match &s.library.access {
            index::Access::Denied(root) => Some(root.clone()),
            _ => None,
        };
    }
    let wall = SystemTime::now();
    let t_side = Instant::now();
    let mut side = if indexed {
        // The tree is checked again only for paths touched below (or above) the repo root.
        let mut touched: Vec<PathBuf> = s.live.drained().iter().map(|ev| match ev {
            watch::FsEvent::Changed(p) | watch::FsEvent::Removed(p) | watch::FsEvent::GitHead(p) | watch::FsEvent::Rescan(p) => p.clone(),
        }).collect();
        touched.extend(s.library.landed.iter().cloned());
        s.sidebar.refresh(&s.library.index, &touched, wall)
    } else {
        s.sidebar.tick(wall)
    };
    // The branch: a HEAD change in the front file's repo (worktrees included, `repo::head_path`).
    for ev in s.live.drained() {
        if let watch::FsEvent::GitHead(root) = ev {
            side |= s.sidebar.git_head(root);
        }
    }
    if indexed {
        probe::sidebar_refreshed(t_side.elapsed());
    }
    // Recent's times change with time: one wake-up at the next change, not continuous frames.
    if let Some(d) = s.sidebar.label_wake(wall, clock) {
        tx.wake_after(d);
    }
    // The Brief fire: a show asked for since the last poll starts now, and one wake-up ends it.
    let (brief, end) = s.tick_fire(clock);
    if let Some(end) = end {
        tx.wake_at(end);
    }
    // The fire at rest moves without a display link: a wake-up every `REST_FRAME` redraws it.
    let resting = s.fire_rests(clock);
    if resting {
        tx.wake_after(fx::fire::REST_FRAME);
    }
    // The rings go out with a single wake-up at the end of the window, not with continuous frames.
    let now = Instant::now();
    let (ring, wake) = s.update_ring(clock, now);
    if let Some(d) = wake {
        tx.wake_after(d);
    }
    // The file just opened: its tree row in view.
    if let Some(id) = s.sidebar.take_reveal() {
        tx.scroll_into_view_in("tree", id);
        side = true;
    }
    let blurred = s.sidebar.blur_close() | s.settings_panel.expire(Instant::now());
    // The wordmark's next size is decoded: the view swaps it in.
    let wordmark = s.forge_images.borrow().next_ready();
    // A new `lab.json` (`--lab`): the view draws the new tuning, or the test state at the new time.
    let lab = lab::take_changed();
    let changed = opened || quick || tabs || ring || side || blurred || found || wordmark || brief || lab;
    // Only the fire at rest to redraw: the `REST_FRAME` wake-up commits to get a frame (a
    // resting window draws only for a poll that changed something; `Tx::live_only` does not hold
    // for `poll`). Inside a stream of frames (a scroll) the compositor already moves the flames
    // on every frame, and committing there would run the whole view on each one (~2 ms on a long
    // file). The empty screen's fire keeps committing on every poll (its CPU is by design).
    let rest_wake = s.rest_wake(clock);
    let changed = changed || (resting && (rest_wake || s.tab().is_none()));
    // The moving metal edges at rest (`App::metal_moves`): one frame every `METAL_FRAME`. When one
    // is due, `wake_after(ZERO)` asks Rune for the next frame without committing the view
    // (`Runtime::resting_wake`); otherwise a wake-up when it is due. In a stream of frames the
    // compositor draws them on every frame, so none is due.
    if s.metal_moves() && !changed {
        match s.metal_due(clock) {
            Ok(()) => tx.wake_after(Duration::ZERO),
            Err(at) => tx.wake_at(at),
        }
    }
    let Some(run) = s.scroll.as_mut() else { return changed };
    if let Some(y) = run.step(clock) {
        tx.scroll_to_instant("doc", 0.0, y);
        tx.wake_after(Duration::from_millis(1));
        // Like a wheel scroll: the view runs again only when its rows change or something else
        // does. `MDSPIKE_SCROLL_VIEW=1` runs it on every frame (the probe before 2026-11-17).
        return changed || std::env::var_os("MDSPIKE_SCROLL_VIEW").is_some();
    } else if run.finished() {
        probe::finish_scroll();
        s.scroll = None;
    } else {
        tx.wake_after(Duration::from_millis(16));
    }
    changed
}

/// The window frame: full-size content as before, and only `.window_drag()` regions move it (the
/// tab bar, the band above the sidebar and the frame above the panel), so presses on tabs are the app's.
fn window_chrome() -> rune::Chrome {
    rune::Chrome { drag_regions: true, ..rune::Chrome::DEFAULT }
}

fn main() {
    open::install();
    rune::prewarm_gpu();
    probe::mark_start();
    // The default mono slot is Inter: code goes in SF Mono.
    let spec = AppSpec { setup: Some(setup), poll: Some(poll), on_key: Some(on_key), families: ("Inter", "ui-monospace, Menlo"), ..AppSpec::new("runemd", (900.0, 1000.0), init, view) };
    // Closing the window or quitting saves the open changes in the source (or makes a recovery
    // copy). Cmd+Q runs `on_quit` and then `on_close`: the second, with the exit already started, is skipped.
    // Transparent window: Rune turns on the system glass only in transparent windows. With
    // `RUNEMD_GLASS` (photos) it also opens at the top left, at (200, 120), above the photo's background.
    let photo = std::env::var_os("RUNEMD_GLASS").is_some();
    let options = rune::WindowOptions { transparent: true, position: photo.then_some((200, 120)), ..rune::WindowOptions::default() };
    rune::App::new(spec).window_options(options).chrome(window_chrome()).on_close(|s, _| if !rune::quitting() { s.save_all() }).on_quit(|s, _| s.save_all()).compositor(fx::FxCompositor::default()).on_frame_encoded(probe::on_frame).run();
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    /// While frames stream (a scroll at 120 Hz), the resting fire does not run the view again on
    /// each one: a `poll` that returns true commits the whole view (~2 ms on `big.md`).
    #[test]
    fn the_resting_fire_does_not_run_the_view_on_every_frame() {
        let mut app = crate::App::bare(crate::index::Library::load(Vec::new(), None));
        app.settings.fire = crate::settings::FireMode::Always;
        app.open_paths(vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/normal.md")]);
        let mut rt = rune::Runtime::new(app, crate::view, (1280.0, 800.0), rune::Env::new(true, 2.0));
        rt.poll = Some(crate::poll);
        rt.frame(0.0);
        for _ in 0..400 {
            if !rt.frame(1.0 / 120.0) { break; }
        }
        assert!(rt.state.fire_rests(rt.clock), "the fire is at rest");
        let commits = rt.stats.commits;
        // Half a second of frames.
        for _ in 0..60 {
            rt.frame(1.0 / 120.0);
        }
        assert_eq!(rt.stats.commits, commits, "the view did not run again");
    }

    /// The empty screen's wordmark images are let go once a file is open: the cache frees
    /// their decoded pixels only when no handle is left.
    #[test]
    fn opening_a_file_lets_the_wordmark_images_go() {
        let mut app = crate::App::bare(crate::index::Library::load(Vec::new(), None));
        app.images = Some(rune::ImageCache::new(std::sync::Arc::new(|| {})));
        let mut rt = rune::Runtime::new(app, crate::view, (1280.0, 800.0), rune::Env::new(true, 2.0));
        rt.frame(0.0);
        assert!(rt.state.forge_images.borrow().holds(), "the empty screen draws the wordmark");
        rt.state.open_paths(vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/normal.md")]);
        rt.invalidate();
        rt.frame(1.0 / 120.0);
        assert!(!rt.state.forge_images.borrow().holds(), "no handle left with a file open");
    }

    /// The forge pieces drawn for `fixtures/forge.md` with `forge` on: (piece, still).
    fn forge_pieces(forge: crate::fx::doc_fx::Directions) -> Vec<crate::fx::doc_fx::Piece> {
        forge_pieces_with(forge, None)
    }

    /// [`forge_pieces`] with `metal` the block kind that keeps a metal frame.
    fn forge_pieces_with(forge: crate::fx::doc_fx::Directions, metal: Option<crate::fx::doc_fx::BlockKind>) -> Vec<crate::fx::doc_fx::Piece> {
        let mut app = crate::App::bare(crate::index::Library::load(Vec::new(), None));
        app.forge_lab = Some(forge);
        app.metal_block = metal;
        app.images = Some(rune::ImageCache::new(std::sync::Arc::new(|| {})));
        app.open_paths(vec![PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/forge.md")]);
        let mut rt = rune::Runtime::new(app, crate::view, (1280.0, 2400.0), rune::Env::new(true, 2.0));
        rt.track_marks = true;
        rt.frame(0.0);
        rt.frame(1.0 / 60.0);
        rt.instances
            .iter()
            .enumerate()
            .filter(|(n, i)| i.params[1] == rune::render::KIND_EFFECT && rt.marks.get(*n).copied() == Some(crate::fx::Mark::DocFx.id()))
            .filter_map(|(_, i)| crate::fx::doc_fx::read(i.uv).map(|(p, _)| p))
            .collect()
    }

    /// Full forge off: no forge slot in the document, so nothing is drawn differently.
    #[test]
    fn full_forge_off_draws_no_forge_piece() {
        assert!(forge_pieces(crate::fx::doc_fx::Directions::default()).is_empty());
    }

    /// Each direction alone puts only its own pieces in the document; all together put every kind.
    #[test]
    fn each_forge_direction_dresses_its_blocks() {
        use crate::fx::doc_fx::{Directions, Piece};
        use crate::fx::doc_fx::BlockKind;
        let alone = |name: &str| forge_pieces(Directions::parse(Some(name)).unwrap());
        let metal = |name: &str, kind| forge_pieces_with(Directions::parse(Some(name)).unwrap(), Some(kind));
        let edges = |v: &[Piece]| v.iter().filter(|p| matches!(p, Piece::Edge { .. })).count();
        // By default no block keeps metal: Steel and Forged draw quiet edges, no effect layer.
        assert_eq!(edges(&alone("steel")), 0, "{:?}", alone("steel"));
        assert_eq!(edges(&alone("forged")), 0, "{:?}", alone("forged"));
        // The one metal kind: code (one card), the five callouts, the frontmatter, the table.
        assert_eq!(edges(&metal("steel", BlockKind::Code)), 1);
        assert_eq!(edges(&metal("steel", BlockKind::Callout)), 5);
        assert_eq!(edges(&metal("steel", BlockKind::Frontmatter)), 1);
        assert_eq!(edges(&metal("steel", BlockKind::Table)), 0, "tables: Forged only");
        assert_eq!(edges(&metal("forged", BlockKind::Table)), 1);
        assert!(alone("engraved").iter().all(|p| *p == Piece::Rod) && alone("engraved").len() == 2);
        assert!(alone("ember").iter().all(|p| *p == Piece::Fire) && alone("ember").len() == 2);
        let glass = alone("glass");
        assert_eq!(glass.iter().filter(|p| matches!(p, Piece::Dither { .. })).count(), 5, "{glass:?}");
        assert!(glass.contains(&Piece::Fluted));
        let tasks = alone("tasks");
        assert_eq!(tasks.iter().filter(|p| matches!(p, Piece::Bead { .. })).count(), 4, "{tasks:?}");
        assert!(tasks.contains(&Piece::Tube) && tasks.contains(&Piece::TubeFill));
        assert_eq!(alone("electric"), vec![Piece::Electric { red: false }, Piece::Electric { red: true }]);
        let all = forge_pieces_with(Directions::ALL, Some(BlockKind::Callout));
        assert!(all.iter().any(|p| matches!(p, Piece::Edge { chrome: true, .. })), "Full forge edges are chrome");
        assert!(all.iter().all(|p| !matches!(p, Piece::Edge { chrome: false, .. })));
    }

    /// A poll after a `REST_FRAME` of quiet is a resting wake-up; polls a frame apart are a
    /// stream of frames.
    #[test]
    fn a_rest_wake_is_a_poll_after_a_quiet_rest_frame() {
        let mut app = crate::App::bare(crate::index::Library::load(Vec::new(), None));
        let rest = crate::fx::fire::REST_FRAME.as_secs_f64();
        assert!(app.rest_wake(1.0), "the first poll");
        assert!(!app.rest_wake(1.0 + 1.0 / 120.0), "the next frame");
        assert!(!app.rest_wake(1.0 + 2.0 / 120.0));
        assert!(app.rest_wake(1.0 + 2.0 / 120.0 + rest), "the wake-up after a quiet REST_FRAME");
        assert!(app.rest_wake(0.5), "a clock that went back");
    }
}
