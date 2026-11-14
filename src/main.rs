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
//! Le impostazioni (Cmd+,) stanno in `~/Library/Application Support/RuneMD/prefs.json`
//! (`settings`). `RUNEMD_ROOTS=dir1:dir2` sostituisce le cartelle osservate e fa girare l'app con
//! impostazioni in memoria, che non si leggono né si salvano (prove e misure);
//! `RUNEMD_THEME=light|dark` vince sul tema scelto; `RUNEMD_REDUCE_MOTION=1|0` vince su "riduci
//! movimento" di sistema (headless Rune non la legge); `RUNE_DIRS_ROOT` sposta la cache dell'indice
//! e le impostazioni.

mod app;
mod doc;
mod edit;
mod fonts;
mod fx;
mod index;
mod live;
mod open;
mod probe;
mod repo;
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
    // La cache dell'indice si legge prima del primo frame (costo in `docs/perf.md`); la
    // scansione parte in `setup`, su un thread a parte.
    // Le impostazioni danno le cartelle osservate; con `RUNEMD_ROOTS` restano in memoria.
    let prefs = if index::env_roots().is_some() { rune::prefs::Prefs::memory() } else { rune::prefs::Prefs::open(settings::APP_ID) };
    let settings = settings::Settings::load(&prefs);
    let t = Instant::now();
    let library = index::Library::load(index::roots_from(&settings.roots), index::cache_path());
    probe::index_cached(t.elapsed(), library.index.len());
    let mut app = App::new(library, settings, prefs);
    app.scroll = probe::ScrollRun::from_env();
    // Recenti dalla cache: la sidebar è piena dal primo frame.
    app.sidebar.refresh(&app.library.index, &[], SystemTime::now());
    app.open_paths(paths);
    // Le copie di recupero dell'ultima uscita (un conflitto o un errore di scrittura).
    app.open_recovered();
    app
}

fn setup(s: &mut App, waker: Waker) {
    open::queue::set_waker(waker.clone());
    s.find.set_waker(waker.clone());
    s.live.start(waker.clone(), s.library.roots().to_vec());
    s.library.start(waker);
}

fn view(s: &App, cx: &mut Cx<App>) -> El {
    // Il tema delle impostazioni (Sistema, Chiaro, Scuro): cambia dal vivo, la vista rigira.
    let dark = s.dark(cx.system_dark());
    let theme = ui::theme::Theme::for_mode(dark);
    // "Riduci movimento": la preferenza di macOS, o `RUNEMD_REDUCE_MOTION`. L'override va anche a
    // Rune, così le sue transizioni (hover, scala premuta, scroll) seguono gli effetti.
    let reduced = s.reduced_motion(cx.system_reduced_motion());
    s.reduced_seen.set(reduced);
    cx.reduced_motion(reduced);
    // Il fuoco della tastiera come quello di Rune (3 pt, attaccato al bordo), ma pieno
    // (e un po' più scuro nel tema chiaro).
    let focus = ui::theme::focus_ring(cx.focus_color(), dark);
    cx.focus_ring(3.0, 0.0, focus);
    let hooks = ui::sidebar::Hooks { get: |s: &App| &s.sidebar, get_mut: |s: &mut App| &mut s.sidebar, open: App::open_from_sidebar, ring: App::ring_of, query: App::requery_sidebar, create: App::create_from_sidebar };
    let body = match s.tab() {
        Some(tab @ app::Tab { mode: app::Mode::Source(src), .. }) => {
            let caret = ui::source::caret_lines(cx, tab, src);
            ui::source::view(s, hooks, tab, src, caret, &theme, reduced, !cx.hover_none())
        }
        Some(tab) => {
            // Il dither: una barra alta una riga e larga come il testo, nello spazio sotto l'ultimo blocco.
            let tail = tab.dither_since.map(|since| fx::dither::slot(el().w_full().h(s.settings.size * ui::theme::BODY_LEADING), since, dark, reduced));
            let signals = ui::doc_view::Signals { marked: App::is_marked, mark: theme.mark, tail, hits: App::find_hits, edge: App::doc_edge };
            let doc = ui::doc_view::view(s, App::active_document, s.style(dark), App::visible_rows, signals);
            // L'indice dei titoli a destra, solo se la finestra è larga almeno sidebar + colonna
            // + 200 (prima del primo layout non si sa: niente indice). La vista la legge da Rune,
            // che la rigira quando la soglia si attraversa; `App::wide` ne è la copia per i
            // gestori, che non hanno `Cx`. Il contenitore è sempre lo stesso, con o senza indice:
            // attraversando la soglia il documento non cambia posto nell'albero e tiene scroll e
            // stato.
            // Con le impostazioni aperte l'indice lascia il posto al pannello.
            let wide = cx.layout_with("app", |r| r[2] >= ui::theme::OUTLINE_MIN_WINDOW).unwrap_or(false);
            let outline = if wide && !s.settings_panel.open { ui::outline::view(tab, hooks, &theme, reduced, !cx.hover_none()) } else { None };
            // Cmd+E su un file che non è UTF-8: la riga sopra la lettura lo dice.
            let refused = tab.not_utf8.then(|| ui::source::not_utf8_row(&theme));
            let doc = rune::col().w_full().h_full().children(refused).child(el().w_full().grow(1.0).min_h(0.0).child(doc));
            rune::row().key("doc-pane").w_full().grow(1.0).min_h(0.0).child(el().grow(1.0).min_w(0.0).h_full().child(doc)).children(outline)
        }
        None => el().grow(1.0),
    };
    // Pannello pieno con la barra delle schede e il documento, a `FRAME` dai bordi alto, destro e
    // basso; a sinistra la sidebar, tutto sul vetro di sistema.
    let bar = ui::tabs::view(s, hooks, &theme, reduced, !cx.hover_none());
    // Il pannello delle impostazioni (Cmd+,) a destra del documento. Il contenitore c'è sempre,
    // così aprirlo non sposta il documento nell'albero e non ne perde lo scroll.
    let aside = s.settings_panel.open.then(|| ui::settings::view(s, hooks, &theme, reduced, !cx.hover_none()));
    let body = rune::row().key("body").w_full().grow(1.0).min_h(0.0).child(el().grow(1.0).min_w(0.0).h_full().child(body)).children(aside);
    let content = rune::col().w_full().h_full().child(bar).child(body);
    let mut panel = el().w_full().h_full().fill(theme.panel).radius(ui::theme::R_PANEL).clip_rounded(ui::theme::R_PANEL).child(content);
    // Il fuoco della scheda davanti: sempre in lettura, una fascia sul fondo del pannello sopra
    // il documento, bassa a riposo; con testo nuovo sotto la vista sale e porta l'etichetta.
    // Fuori dal flusso, così il documento e il suo scroll non si muovono. L'etichetta è una fascia
    // sorella dopo lo slot, così Rune la dipinge sopra le fiamme. Sta sulla base calda del fuoco,
    // quasi bianca nei due temi: il testo prende un grigio scuro dei token (`panel` nello scuro,
    // `fg` nel chiaro).
    if let Some(tab) = s.tab().filter(|t| matches!(t.mode, app::Mode::Read)) {
        let lit = tab.fire_since.map(|since| (since, tab.below.count()));
        panel = panel.children(fire_band(lit, s.settings.fire, dark, &theme, reduced, s.window_active));
    }
    // L'anello "sta scrivendo" sta sulla scheda del file (`ui::tabs`) e sulle sue righe in
    // Recenti e nell'albero.
    let sidebar = ui::sidebar::view(s, hooks, &theme, reduced, !cx.hover_none());
    let main = el().key("main").grow(1.0).min_w(0.0).h_full().pt(ui::theme::FRAME).pr(ui::theme::FRAME).pb(ui::theme::FRAME).child(panel);
    // La larghezza della finestra per i gestori (`App::wide`: l'indice dei titoli si vede); la
    // vista la legge da `cx`. Si aggiorna senza far rigirare la vista.
    let width = |s: &mut App, r: [f32; 4], tx: &mut Tx| {
        s.set_window_width(r[2]);
        tx.live_only();
    };
    rune::row().key("app").on_layout_tx(width).w_full().h_full().system_material(rune::SystemMaterial::Glass).fill(theme.glass).child(sidebar).child(main)
        .on_window_focus(|s: &mut App, focused, _| s.window_active = focused)
}

/// Il fuoco e la sua etichetta sul fondo del pannello. `lit`: da quando c'è testo nuovo sotto la
/// vista, e quanti blocchi; `None` a riposo (fiamme basse, nessuna etichetta). Con il fuoco
/// spento dalle impostazioni (`Settings::fire`) non c'è niente a riposo, e con testo nuovo solo
/// l'etichetta, in `fg2` su una fascia `panel` con la linea `line` sopra: senza le fiamme sotto,
/// il testo del documento non deve passarle dietro.
fn fire_band(lit: Option<(f64, usize)>, fire: bool, dark: bool, theme: &ui::theme::Theme, reduced: bool, active: bool) -> Vec<El> {
    let label = |count, ink| rune::text(ui::doc_view::fire_label(count)).key("fire-text").font(rune::TextStyle::new(ui::theme::UI_TEXT, ui::theme::UI_WEIGHT)).color(ink);
    if !fire {
        let Some((_, count)) = lit else { return Vec::new() };
        let line = el().w_full().h(ui::theme::HAIRLINE).fill(theme.line);
        let strip = rune::col().w_full().h(ui::theme::TAB_BAR_H).fill(theme.panel).items_center().justify_center().child(label(count, theme.fg2));
        return vec![rune::col().key("fire-label").pin(Some(0.0), None, Some(0.0), Some(0.0)).child(line).child(strip)];
    }
    let band = |key: &str| rune::col().key(key).pin(Some(0.0), None, Some(0.0), Some(0.0)).h(fx::fire::FIRE_H);
    let flames = fx::fire::slot(band("fire"), lit.map(|(since, _)| since), dark, reduced, active);
    let Some((_, count)) = lit else { return vec![flames] };
    let ink = if dark { theme.panel } else { theme.fg };
    let text = band("fire-label").items_center().justify_end().pb(ui::theme::FIRE_LABEL_BOTTOM).child(label(count, ink));
    // Due fasce sorelle: lo slot prima, l'etichetta dopo (Rune la dipinge sopra le fiamme).
    vec![flames, text]
}

/// I tasti delle schede: Cmd+W chiude quella davanti (con modifiche non salvate chiede,
/// `App::close`), Cmd+1…8 vanno alla scheda, Cmd+9 all'ultima (come Safari e Finder), Ctrl+Tab
/// e Ctrl+Shift+Tab alla dopo e alla prima. Vero se il tasto è delle schede; Cmd+W senza schede
/// resta a Rune, che chiude la finestra.
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

/// Ogni tasto non preso da un controllo, prima della selezione di Rune: il Cmd+A va segnato.
fn on_key(s: &mut App, key: &rune::KeyEvent, tx: &mut Tx) -> bool {
    // Cmd+,: apre e chiude le impostazioni; Esc le chiude.
    if matches!(key.key, rune::Key::Command(',')) && key.mods.cmd && !key.mods.shift && !key.mods.alt && !key.mods.ctrl && !tx.key_repeat() {
        s.toggle_settings();
        return true;
    }
    if matches!(key.key, rune::Key::Escape) && !key.mods.cmd && !key.mods.shift && !key.mods.alt && !key.mods.ctrl && s.settings_panel.close() {
        return true;
    }
    // Cmd+P: il fuoco nel campo "Apri file" della sidebar. Un solo campo aperto alla volta.
    if matches!(key.key, rune::Key::Command('p')) && key.mods.cmd && !key.mods.shift && !key.mods.alt && !tx.key_repeat() {
        s.find.close();
        s.sidebar.new_file.close();
        s.sidebar.quick.focus();
        return true;
    }
    // Cmd+N: il campo "Nuovo file" prende il posto di "Apri file".
    if matches!(key.key, rune::Key::Command('n')) && key.mods.cmd && !key.mods.shift && !key.mods.alt && !key.mods.ctrl && !tx.key_repeat() {
        s.find.close();
        s.sidebar.quick.close();
        s.sidebar.quick_rows.clear();
        s.sidebar.new_file.show();
        return true;
    }
    // Cmd+F: Cerca nel file, solo in lettura (nel sorgente non fa niente).
    if matches!(key.key, rune::Key::Command('f')) && key.mods.cmd && !key.mods.shift && !key.mods.alt && !key.mods.ctrl && !tx.key_repeat() {
        s.open_find();
        return true;
    }
    if tab_key(s, key, tx) {
        return true;
    }
    // Esc con la riga "Salvare le modifiche…?" aperta e il fuoco fuori dal campo: "Annulla" (dal
    // campo lo prende `ui::source::ask_key`).
    if let Some(id) = s.tab().map(|t| t.doc.id)
        && ui::source::ask_key(s, id, key, tx)
    {
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
    // Cerca: una ricarica della scheda davanti rigira la ricerca (senza spostare la vista), e i
    // risultati arrivati dal thread della ricerca si applicano.
    let found = s.poll_find(tx);
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
    // Il fuoco a riposo si muove senza display link: un risveglio ogni `REST_FRAME` lo ridisegna.
    let resting = s.fire_rests();
    if resting {
        tx.wake_after(fx::fire::REST_FRAME);
    }
    // Gli anelli si spengono con un solo risveglio a fine finestra, non con frame continui.
    let now = Instant::now();
    let (ring, wake) = s.update_ring(clock, now);
    if let Some(d) = wake {
        tx.wake_after(d);
    }
    // Il file appena aperto: la sua riga dell'albero in vista.
    if let Some(id) = s.sidebar.take_reveal() {
        tx.scroll_into_view_in("tree", id);
        side = true;
    }
    let blurred = s.sidebar.blur_close() | s.settings_panel.expire(Instant::now());
    let changed = opened || tabs || ring || side || blurred || found;
    // Solo il fuoco a riposo da ridisegnare: un frame senza rigirare la vista.
    if resting && !changed {
        tx.live_only();
    }
    let changed = changed || resting;
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
    // Chiudere la finestra o uscire salva le modifiche aperte nel sorgente (o ne fa una copia di
    // recupero). Cmd+Q fa `on_quit` e poi `on_close`: il secondo, a uscita già partita, salta.
    rune::App::new(spec).on_close(|s, _| if !rune::quitting() { s.save_all() }).on_quit(|s, _| s.save_all()).compositor(fx::FxCompositor::default()).on_frame_encoded(probe::on_frame).run();
}
