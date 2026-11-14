//! RuneMD: lettore Markdown per Mac in Rune.
//!
//! `runemd FILE.md` apre il file e lo rende con il componente `Markdown` di rune-kit, un blocco
//! top-level per riga di `virtual_rows` dentro uno scroll.
//!
//! Sonde per il benchmark (variabili d'ambiente, così non si scontrano con i flag di Rune):
//! - `MDSPIKE_LOG=path`: scrive `first_content_epoch_ms` al primo frame presentato.
//! - `MDSPIKE_EXIT=1`: esce 300 ms dopo il primo frame (misure d'avvio).
//! - `MDSPIKE_SCROLL=secondi` (+ `MDSPIKE_SPEED` pt/s, default 3000): dopo 1 s scorre a
//!   velocità costante e scrive gli intervalli tra frame presentati in `MDSPIKE_LOG`, poi esce.
//! - `MDSPIKE_LATENCY=path`: a ogni ricaricamento scrive, al primo frame che lo mostra, i ms da
//!   scrittura del file (mtime) a ricarica e a frame.
//! - Con `MDSPIKE_LOG` si scrivono anche il costo della cache dell'indice (`index_cache_ms`) e
//!   quando la scansione finisce (`index_scan_epoch_ms`).
//!
//! `RUNEMD_ROOTS=dir1:dir2` sostituisce le cartelle osservate (default `~/Desktop/GitHub`);
//! `RUNE_DIRS_ROOT` sposta la cache dell'indice.

mod app;
mod doc;
mod edit;
mod fx;
mod index;
mod live;
mod open;
mod probe;
mod repo;
mod ui;
mod watch;

use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use rune::{AppSpec, Cx, El, Tx, Waker, el};

pub use app::App;

/// Tema scuro, salvo `RUNEMD_THEME=light` (finché il tema di sistema non è collegato: serve agli snap).
fn dark_mode() -> bool {
    std::env::var("RUNEMD_THEME").map_or(true, |v| v != "light")
}

fn init() -> App {
    let mut paths = open::cli_paths(rune::extra_args());
    if paths.is_empty() {
        paths.extend(std::env::var("MDSPIKE_FILE").ok().map(PathBuf::from));
    }
    // La cache dell'indice si legge prima del primo frame (costo in `docs/perf.md`); la
    // scansione parte in `setup`, su un thread a parte.
    let t = Instant::now();
    let library = index::Library::load(index::roots(), index::cache_path());
    probe::index_cached(t.elapsed(), library.index.len());
    let mut app = App::new(library, dark_mode());
    app.scroll = probe::ScrollRun::from_env();
    // Recenti dalla cache: la sidebar è piena dal primo frame.
    app.sidebar.refresh(&app.library.index, &[], SystemTime::now());
    app.open_paths(paths);
    app
}

fn setup(s: &mut App, waker: Waker) {
    open::queue::set_waker(waker.clone());
    s.live.start(waker.clone(), s.library.roots().to_vec());
    s.library.start(waker);
}

fn view(s: &App, cx: &mut Cx<App>) -> El {
    let dark = dark_mode();
    let theme = ui::theme::Theme::for_mode(dark);
    let reduced = cx.is_reduced_motion();
    let body = match s.tab() {
        Some(tab @ app::Tab { mode: app::Mode::Source(src), .. }) => ui::source::view(tab, src, &theme),
        Some(tab) => {
            // Il dither: una barra alta una riga e larga come il testo, nello spazio sotto l'ultimo blocco.
            let tail = tab.dither_since.map(|since| fx::dither::slot(el().w_full().h(ui::theme::BODY_LINE), since, dark, reduced));
            let signals = ui::doc_view::Signals { marked: App::is_marked, mark: theme.line, tail };
            ui::doc_view::view(s, App::active_document, &s.style, App::visible_rows, signals)
        }
        None => el().grow(1.0),
    };
    // Pannello pieno con la barra delle schede e il documento, a `FRAME` dai bordi alto, destro e
    // basso; a sinistra la sidebar, tutto sul vetro di sistema.
    let hooks = ui::sidebar::Hooks { get: |s: &App| &s.sidebar, get_mut: |s: &mut App| &mut s.sidebar, open: App::open_from_sidebar, ring: App::ring_of, query: App::requery_sidebar };
    let bar = ui::tabs::view(s, hooks, &theme, reduced, !cx.hover_none());
    let content = rune::col().w_full().h_full().child(bar).child(body);
    let mut panel = el().w_full().h_full().fill(theme.panel).radius(ui::theme::R_PANEL).clip_rounded(ui::theme::R_PANEL).child(content);
    // Il fuoco della scheda davanti: una fascia sul fondo del pannello, sopra il documento, con
    // l'etichetta. Fuori dal flusso, così il documento e il suo scroll non si muovono quando
    // compare. L'etichetta è una fascia sorella dopo lo slot, così Rune la dipinge sopra le
    // fiamme. Sta sulla base calda del fuoco, quasi bianca nei due temi: il testo prende un
    // grigio scuro dei token (`panel` nello scuro, `fg` nel chiaro).
    if let Some((since, count)) = s.tab().filter(|t| matches!(t.mode, app::Mode::Read)).and_then(|t| Some((t.fire_since?, t.below.count()))) {
        let band = |key: &str| rune::col().key(key).pin(Some(0.0), None, Some(0.0), Some(0.0)).h(fx::fire::FIRE_H);
        panel = panel.child(fx::fire::slot(band("fire"), since, dark, reduced, s.window_active));
        let ink = if dark { theme.panel } else { theme.fg };
        let label = rune::text(ui::doc_view::fire_label(count)).font(rune::TextStyle::new(ui::theme::UI_TEXT, 400.0)).color(ink);
        panel = panel.child(band("fire-label").items_center().justify_end().pb(ui::theme::FIRE_LABEL_BOTTOM).child(label));
    }
    // L'anello "sta scrivendo" sta sulla scheda del file (`ui::tabs`) e sulle sue righe in
    // Recenti e nell'albero.
    let sidebar = ui::sidebar::view(s, hooks, &theme, reduced, !cx.hover_none());
    let main = el().key("main").grow(1.0).min_w(0.0).h_full().pt(ui::theme::FRAME).pr(ui::theme::FRAME).pb(ui::theme::FRAME).child(panel);
    rune::row().w_full().h_full().system_material(rune::SystemMaterial::Glass).fill(theme.glass).child(sidebar).child(main)
        .on_window_focus(|s: &mut App, focused, _| s.window_active = focused)
}

/// I tasti delle schede: Cmd+W chiude quella davanti, Cmd+1…8 vanno alla scheda, Cmd+9
/// all'ultima (come Safari e Finder), Ctrl+Tab e Ctrl+Shift+Tab alla dopo e alla prima. Vero se
/// il tasto è delle schede; Cmd+W senza schede resta a Rune, che chiude la finestra.
fn tab_key(s: &mut App, key: &rune::KeyEvent, tx: &mut Tx) -> bool {
    let m = key.mods;
    let changed = match key.key {
        rune::Key::Command('w') if m.cmd && !m.shift && !m.alt && !m.ctrl => {
            if s.tabs.is_empty() {
                return false;
            }
            s.close_active()
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

/// Ogni tasto non preso da un controllo, prima della selezione di Rune: il Cmd+A va segnato.
fn on_key(s: &mut App, key: &rune::KeyEvent, tx: &mut Tx) -> bool {
    // Cmd+P: il fuoco nel campo "Apri file" della sidebar.
    if matches!(key.key, rune::Key::Command('p')) && key.mods.cmd && !key.mods.shift && !key.mods.alt && !tx.key_repeat() {
        s.sidebar.quick.focus();
        return true;
    }
    if tab_key(s, key, tx) {
        return true;
    }
    // Cmd+E: sorgente e ritorno alla lettura; Cmd+S: salva il sorgente. Il campo non li prende.
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
    let queued = open::queue::drain();
    let opened = !queued.is_empty();
    if opened {
        let before = s.tab().map(|t| t.doc.id);
        s.open_paths(queued);
        if s.tab().map(|t| t.doc.id) != before {
            s.restore_view(tx);
        }
    }
    // Il watcher va svuotato a ogni risveglio (`Watcher::drain`); poi ogni scheda raccoglie la
    // sua rilettura e aggiorna fuoco, segni e dither. Il fuoco si accende dopo una ricarica che
    // porta testo sotto la vista (`Tab::note_reload`, o il primo `visible_rows` del layout nuovo)
    // e si spegne quando l'ultimo di quei blocchi entra nella vista: nessun risveglio serve.
    let now = Instant::now();
    let (tabs, wakes) = s.poll_tabs(clock, now);
    for d in wakes {
        tx.wake_after(d);
    }
    // L'indice legge gli stessi eventi; Recenti e l'albero si ricalcolano solo se cambia.
    let was_scanned = s.library.scanned;
    let indexed = s.library.poll(s.live.drained(), Instant::now());
    if s.library.scanned && !was_scanned {
        probe::index_scanned(s.library.index.len());
    }
    if indexed {
        s.sidebar.no_access = matches!(s.library.access, index::Access::Denied(_));
    }
    let wall = SystemTime::now();
    let t_side = Instant::now();
    let mut side = if indexed {
        // L'albero si ricontrolla solo per i percorsi toccati sotto (o sopra) la radice del repo.
        let mut touched: Vec<PathBuf> = s.live.drained().iter().map(|ev| match ev {
            watch::FsEvent::Changed(p) | watch::FsEvent::Removed(p) | watch::FsEvent::GitHead(p) | watch::FsEvent::Rescan(p) => p.clone(),
        }).collect();
        touched.extend(s.library.landed.iter().cloned());
        s.sidebar.refresh(&s.library.index, &touched, wall)
    } else {
        s.sidebar.tick(wall)
    };
    // Il branch: un cambio di HEAD del repo del file davanti (worktree compresi, `repo::head_path`).
    for ev in s.live.drained() {
        if let watch::FsEvent::GitHead(root) = ev {
            side |= s.sidebar.git_head(root);
        }
    }
    if indexed {
        probe::sidebar_refreshed(t_side.elapsed());
    }
    // I tempi di Recenti cambiano col tempo: un risveglio al prossimo cambio, non frame continui.
    if let Some(d) = s.sidebar.label_wake(wall, clock) {
        tx.wake_after(d);
    }
    // Gli anelli si spengono con un solo risveglio a fine finestra, non con frame continui.
    let now = Instant::now();
    let (ring, wake) = s.update_ring(clock, now);
    if let Some(d) = wake {
        tx.wake_after(d);
    }
    let blurred = s.sidebar.blur_close();
    let changed = opened || tabs || ring || side || blurred;
    let Some(run) = s.scroll.as_mut() else { return changed };
    if let Some(y) = run.step(clock) {
        tx.scroll_to_instant("doc", 0.0, y);
        tx.wake_after(Duration::from_millis(1));
        // Senza una barra glass che anima, nulla chiede un frame: lo scroll di misura lo chiede qui.
        return true;
    } else if run.finished() {
        probe::finish_scroll();
    } else {
        tx.wake_after(Duration::from_millis(16));
    }
    changed
}

fn main() {
    open::install();
    rune::prewarm_gpu();
    probe::mark_start();
    // Lo slot mono di default è Inter: il codice va in SF Mono.
    let spec = AppSpec { setup: Some(setup), poll: Some(poll), on_key: Some(on_key), families: ("Inter", "ui-monospace, Menlo"), ..AppSpec::new("runemd", (900.0, 1000.0), init, view) };
    // Chiudere la finestra o uscire salva le modifiche aperte nel sorgente.
    rune::App::new(spec).on_close(|s, _| s.save_all()).on_quit(|s, _| s.save_all()).compositor(fx::FxCompositor::default()).on_frame_encoded(probe::on_frame).run();
}
