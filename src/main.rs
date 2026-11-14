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

mod doc;
mod fx;
mod index;
mod live;
mod open;
mod probe;
mod repo;
mod ui;
mod watch;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use rune::{App, AppSpec, Cx, El, Tx, Waker, el};
use rune_kit::MarkdownStyle;

use doc::document::Document;

pub struct Doc {
    /// `None` finché non si apre un file.
    document: Option<Document>,
    style: MarkdownStyle,
    scroll: Option<probe::ScrollRun>,
    /// Cresce a ogni documento aperto: le chiavi delle righe cambiano e Rune scarta le altezze misurate.
    generation: u64,
    /// Watcher e rilettura del file aperto.
    live: live::Live,
    /// I file che "stanno scrivendo" e l'orologio di Rune (`poll`) a cui si è acceso il loro
    /// anello: le loro righe in Recenti e nell'albero lo mostrano.
    rings: BTreeMap<PathBuf, f64>,
    /// L'indice dei Markdown nelle cartelle osservate (Recenti, Cmd+P).
    library: index::Library,
    /// Recenti e il repo del file aperto.
    sidebar: ui::sidebar::Sidebar,
    /// Il risveglio armato per spegnere il primo anello che scade: uno per fine, non uno a frame.
    ring_wake: Option<Wake>,
    /// Il fuoco: i blocchi nuovi o cambiati sotto la vista.
    below: live::below::Below,
    /// L'id dell'ultimo blocco visibile, dall'ultimo `on_visible_rows`; 0 prima del primo.
    visible_last: u64,
    /// L'orologio di Rune a cui si è acceso il fuoco; `None` quando è spento.
    fire_since: Option<f64>,
    /// L'orologio della ricarica che ha portato blocchi sotto la vista, finché Rune non riporta
    /// le righe visibili dopo il suo layout: solo allora si sa se sono davvero fuori vista.
    fire_armed: Option<f64>,
    /// La finestra è attiva (`on_window_focus`): senza, il fuoco resta fermo.
    window_active: bool,
    /// I blocchi con il segno "cosa è cambiato".
    changed: live::arriving::Changed,
    /// Il risveglio armato per togliere il prossimo segno che scade.
    mark_wake: Option<Wake>,
    /// L'orologio di Rune a cui si è acceso il dither "sta arrivando"; `None` quando è spento.
    dither_since: Option<f64>,
}

/// Un risveglio per la fine di uno stato a tempo ("sta scrivendo", un segno che scade).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Wake {
    /// La fine (orologio del muro, come `Activity` e `Changed`).
    end: Instant,
    /// Quando Rune lo fa scattare, nel suo orologio (`poll` lo riceve come `clock`).
    at: f64,
}

/// Il risveglio da chiedere per uno stato che finisce fra `left` (da `now`); `None` se non serve.
///
/// Se ne arma uno quando la fine cambia e quando quello armato è scaduto (nell'orologio di Rune)
/// con lo stato ancora acceso: Rune misura `wake_after` da `clock`, l'inizio del frame, e la fine
/// da `now`, più tardi; il risveglio può arrivare un po' prima della fine sul muro. Un poll a metà
/// non arma niente.
fn rearm(wake: &mut Option<Wake>, left: Option<Duration>, clock: f64, now: Instant) -> Option<Duration> {
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

impl Doc {
    /// Un anello "sta scrivendo" è acceso: il compositor scorre le istanze solo allora.
    pub fn ring_lit(&self) -> bool {
        !self.rings.is_empty()
    }

    /// Un effetto per slot è acceso (anello, fuoco o dither): il compositor scorre le istanze
    /// solo allora.
    pub fn effects_lit(&self) -> bool {
        self.ring_lit() || self.fire_since.is_some() || self.dither_since.is_some()
    }

    /// Se il blocco `id` ha il segno "cosa è cambiato" adesso (per la vista).
    fn is_marked(&self, id: u64) -> bool {
        self.changed.is_marked(id, Instant::now())
    }

    /// I blocchi nuovi o cambiati da una ricarica prendono il segno; i segni scaduti a `now` si
    /// tolgono. Vero se la vista cambia. Ritorna anche il risveglio per il prossimo che scade:
    /// senza, con "riduci movimento" o a finestra ferma nessun frame lo toglierebbe.
    fn update_marks(&mut self, change: Option<&doc::document::Change>, clock: f64, now: Instant) -> (bool, Option<Duration>) {
        let mut changed = self.changed.prune(now);
        if let Some(c) = change {
            let ids: Vec<u64> = c.added.iter().chain(&c.changed).copied().collect();
            self.changed.mark(&ids, now);
            changed |= !ids.is_empty();
        }
        (changed, rearm(&mut self.mark_wake, self.changed.left(now), clock, now))
    }

    /// Accende o spegne il dither: solo se l'ultimo blocco è aperto e il file "sta scrivendo".
    /// Si spegne a fine finestra con il risveglio dell'anello (stessa finestra, `update_ring`) o
    /// alla ricarica che chiude il blocco. Vero se è cambiato.
    fn dither_step(&mut self, writing: bool, clock: f64) -> bool {
        let lit = writing && self.document.as_ref().is_some_and(|d| d.last_open && !d.blocks.is_empty());
        if lit == self.dither_since.is_some() {
            return false;
        }
        self.dither_since = lit.then_some(clock);
        true
    }

    /// Una ricarica applicata: i blocchi nuovi o cambiati dopo l'ultimo visibile entrano nel
    /// conto, ma il fuoco non si accende qui. `visible_last` viene dal layout di prima della
    /// ricarica: in un documento corto il blocco aggiunto è già in vista, e lo si sa solo quando
    /// Rune riporta le righe del layout nuovo (`visible_rows`, che si accende allora). Quando
    /// le chiavi cambiano (blocchi aggiunti o tolti) Rune riporta di sicuro: `VirtualList::set_keys`
    /// azzera l'ultimo rapporto. Con le stesse chiavi (solo blocchi cambiati) non riporta se la
    /// parte visibile resta quella, ma allora gli indici del layout di prima valgono ancora e il
    /// fuoco si accende subito. Vero se si è acceso.
    fn note_reload(&mut self, change: &doc::document::Change, clock: f64) -> bool {
        let Some(doc) = &self.document else { return false };
        let order: Vec<u64> = doc.blocks.iter().map(|b| b.id).collect();
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

    /// Le righe visibili sono cambiate (scroll, ricarica, ridimensionamento): l'ultima esce dal
    /// conto del fuoco e diventa l'ancora delle ricariche seguenti. Vero se la vista cambia
    /// (etichetta o fuoco). Il fuoco si accende qui, al primo rapporto dopo la ricarica che l'ha
    /// armato (con l'orologio di quella ricarica), e si spegne quando il conto arriva a 0.
    fn visible_rows(&mut self, rows: std::ops::Range<usize>) -> bool {
        let Some(doc) = self.document.as_ref() else { return false };
        // Rune riporta gli indici dell'ultimo layout: con una ricarica in mezzo possono andare
        // oltre i blocchi di adesso, e allora si aspetta il prossimo rapporto.
        let Some(last) = rows.end.checked_sub(1).filter(|&i| i < doc.blocks.len() && !rows.is_empty()) else { return false };
        self.visible_last = doc.blocks[last].id;
        let before = self.below.count();
        let order: Vec<u64> = doc.blocks.iter().map(|b| b.id).collect();
        self.below.on_scroll(last, &order);
        let was = self.fire_since;
        if self.below.count() == 0 {
            self.fire_since = None;
        } else if self.fire_since.is_none() {
            self.fire_since = self.fire_armed;
        }
        self.fire_armed = None;
        self.below.count() != before || self.fire_since != was
    }

    /// Accende o spegne gli anelli secondo `Live` a `now`; vero se è cambiato. Ritorna anche il
    /// risveglio da chiedere: a fine finestra `poll` li spegne anche se nessun frame è passato
    /// in mezzo (con "riduci movimento" non ne passano).
    fn update_ring(&mut self, clock: f64, now: Instant) -> (bool, Option<Duration>) {
        self.ring_step(&self.live.writing(now), clock, now)
    }

    /// `update_ring` con i file che stanno scrivendo e fra quanto smettono (a `now`) già
    /// calcolati. Un file già acceso tiene il suo orologio; il risveglio, per il primo che
    /// scade, viene da [`rearm`].
    fn ring_step(&mut self, writing: &[(PathBuf, Duration)], clock: f64, now: Instant) -> (bool, Option<Duration>) {
        let wake = rearm(&mut self.ring_wake, writing.iter().map(|(_, d)| *d).min(), clock, now);
        let rings: BTreeMap<PathBuf, f64> = writing.iter().map(|(p, _)| (p.clone(), self.rings.get(p).copied().unwrap_or(clock))).collect();
        let changed = rings != self.rings;
        self.rings = rings;
        (changed, wake)
    }

    /// L'orologio a cui si è acceso l'anello di `path`, se "sta scrivendo" (per la sidebar).
    fn ring_of(&self, path: &Path) -> Option<f64> {
        self.rings.get(path).copied()
    }

    /// Un clic su una riga della sidebar: il file va nel lettore (per ora al posto di quello
    /// aperto; le schede sono il task 19). Il file già aperto non si ricarica.
    fn open_from_sidebar(&mut self, path: PathBuf, tx: &mut Tx) {
        let same = self.document.as_ref().is_some_and(|d| d.path == path || d.path.canonicalize().ok().as_ref() == Some(&path));
        if !same {
            self.open_paths(vec![path]);
            tx.scroll_to_instant("doc", 0.0, 0.0);
        }
    }

    /// La ricerca di Cmd+P è cambiata.
    fn requery_sidebar(&mut self) {
        self.sidebar.requery(&self.library.index);
    }

    /// Apre i percorsi ricevuti (Finder, riga di comando). Per ora un solo documento: vale
    /// l'ultimo percorso e sostituisce quello corrente. Il task 19 sposta questo metodo su
    /// `App` e apre una scheda per percorso.
    fn open_paths(&mut self, paths: Vec<PathBuf>) {
        if !paths.is_empty() && std::env::var_os("RUNEMD_LOG_OPEN").is_some() {
            eprintln!("runemd: open_paths {paths:?}");
        }
        let Some(path) = paths.last() else { return };
        match Document::load(path) {
            Ok(document) => {
                self.live.watch(path);
                self.sidebar.set_open(path, &self.library.index);
                self.document = Some(document);
                self.generation += 1;
                // Un altro file: niente fuoco e nessuna ancora finché la vista non riporta righe.
                self.below = live::below::Below::default();
                self.visible_last = 0;
                self.fire_since = None;
                self.fire_armed = None;
                // I segni erano dell'altro file; il dither lo riaccende il prossimo `poll`.
                self.changed = live::arriving::Changed::default();
                self.mark_wake = None;
                self.dither_since = None;
            }
            Err(e) => eprintln!("runemd: {}: {e}", path.display()),
        }
    }
}

/// Tema scuro, salvo `RUNEMD_THEME=light` (finché il tema di sistema non è collegato: serve agli snap).
fn dark_mode() -> bool {
    std::env::var("RUNEMD_THEME").map_or(true, |v| v != "light")
}

fn init() -> Doc {
    let mut paths = open::cli_paths(rune::extra_args());
    if paths.is_empty() {
        paths.extend(std::env::var("MDSPIKE_FILE").ok().map(PathBuf::from));
    }
    // La cache dell'indice si legge prima del primo frame (costo in `docs/perf.md`); la
    // scansione parte in `setup`, su un thread a parte.
    let t = Instant::now();
    let library = index::Library::load(index::roots(), index::cache_path());
    probe::index_cached(t.elapsed(), library.index.len());
    let mut doc = Doc { document: None, style: ui::markdown_style::style(&ui::theme::Theme::for_mode(dark_mode()), 15.5), scroll: probe::ScrollRun::from_env(), generation: 0, live: live::Live::default(), library, sidebar: ui::sidebar::Sidebar::default(), rings: BTreeMap::new(), ring_wake: None, below: live::below::Below::default(), visible_last: 0, fire_since: None, fire_armed: None, window_active: true, changed: live::arriving::Changed::default(), mark_wake: None, dither_since: None };
    // Recenti dalla cache: la sidebar è piena dal primo frame.
    doc.sidebar.refresh(&doc.library.index, &[], SystemTime::now());
    doc.open_paths(paths);
    doc
}

fn setup(s: &mut Doc, waker: Waker) {
    open::queue::set_waker(waker.clone());
    s.live.start(waker.clone(), s.library.roots().to_vec());
    s.library.start(waker);
}

/// Il documento aperto; `view` chiama la vista del documento solo quando c'è.
fn open_document(s: &Doc) -> &Document {
    s.document.as_ref().expect("vista del documento senza documento")
}

fn view(s: &Doc, cx: &mut Cx<Doc>) -> El {
    let dark = dark_mode();
    let theme = ui::theme::Theme::for_mode(dark);
    let body = if s.document.is_some() {
        // Il dither: una barra alta una riga e larga come il testo, nello spazio sotto l'ultimo blocco.
        let tail = s.dither_since.map(|since| fx::dither::slot(el().w_full().h(ui::theme::BODY_LINE), since, dark, cx.is_reduced_motion()));
        let signals = ui::doc_view::Signals { marked: Doc::is_marked, mark: theme.line, tail };
        ui::doc_view::view(s, open_document, s.generation, &s.style, Doc::visible_rows, signals)
    } else {
        el()
    };
    // Pannello pieno con il documento, a `FRAME` dai bordi alto, destro e basso; a sinistra la
    // sidebar, tutto sul vetro di sistema.
    let mut panel = el().w_full().h_full().fill(theme.panel).radius(ui::theme::R_PANEL).clip_rounded(ui::theme::R_PANEL).child(body);
    // Il fuoco: una fascia sul fondo del pannello, sopra il documento, con l'etichetta. Fuori
    // dal flusso, così il documento e il suo scroll non si muovono quando compare.
    // L'etichetta è una fascia sorella dopo lo slot, così Rune la dipinge sopra le fiamme. Sta
    // sulla base calda del fuoco, quasi bianca nei due temi: il testo prende un grigio scuro dei
    // token (`panel` nello scuro, `fg` nel chiaro).
    if let Some(since) = s.fire_since {
        let band = |key: &str| rune::col().key(key).pin(Some(0.0), None, Some(0.0), Some(0.0)).h(fx::fire::FIRE_H);
        panel = panel.child(fx::fire::slot(band("fire"), since, dark, cx.is_reduced_motion(), s.window_active));
        let ink = if dark { theme.panel } else { theme.fg };
        let label = rune::text(ui::doc_view::fire_label(s.below.count())).font(rune::TextStyle::new(ui::theme::UI_TEXT, 400.0)).color(ink);
        panel = panel.child(band("fire-label").items_center().justify_end().pb(ui::theme::FIRE_LABEL_BOTTOM).child(label));
    }
    // L'anello "sta scrivendo" sta sulle righe del file in Recenti e nell'albero; la scheda è il
    // task 19.
    let hooks = ui::sidebar::Hooks { get: |s: &Doc| &s.sidebar, get_mut: |s: &mut Doc| &mut s.sidebar, open: Doc::open_from_sidebar, ring: Doc::ring_of, query: Doc::requery_sidebar };
    let sidebar = ui::sidebar::view(s, hooks, &theme, cx.is_reduced_motion(), !cx.hover_none());
    let main = el().key("main").grow(1.0).min_w(0.0).h_full().pt(ui::theme::FRAME).pr(ui::theme::FRAME).pb(ui::theme::FRAME).child(panel);
    rune::row().w_full().h_full().system_material(rune::SystemMaterial::Glass).fill(theme.glass).child(sidebar).child(main)
        .on_window_focus(|s: &mut Doc, focused, _| s.window_active = focused)
}

/// Ogni tasto non preso da un controllo, prima della selezione di Rune: il Cmd+A va segnato.
fn on_key(s: &mut Doc, key: &rune::KeyEvent, tx: &mut Tx) -> bool {
    // Cmd+P: il fuoco nel campo "Apri file" della sidebar.
    if matches!(key.key, rune::Key::Command('p')) && key.mods.cmd && !key.mods.shift && !key.mods.alt && !tx.key_repeat() {
        s.sidebar.quick.focus();
        return true;
    }
    if let Some(doc) = &s.document {
        ui::doc_view::note_key(doc, key, tx.key_repeat());
    }
    false
}

fn poll(s: &mut Doc, clock: f64, tx: &mut Tx) -> bool {
    let queued = open::queue::drain();
    let opened = !queued.is_empty();
    if opened {
        s.open_paths(queued);
        tx.scroll_to_instant("doc", 0.0, 0.0);
    }
    // Il watcher va svuotato a ogni risveglio (`Watcher::drain`).
    let (reloaded, recheck) = s.live.poll(s.document.as_mut(), Instant::now());
    if let Some(d) = recheck {
        tx.wake_after(d);
    }
    if let Some(r) = &reloaded {
        probe::reloaded(r);
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
    // Il branch: un cambio di HEAD del repo del file aperto (worktree compresi, `repo::head_path`).
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
    let fire = reloaded.as_ref().is_some_and(|r| s.note_reload(&r.change, clock));
    // Il fuoco si accende dopo una ricarica che porta testo sotto la vista (`note_reload`, o il
    // primo `visible_rows` del layout nuovo) e si spegne quando l'ultimo di quei blocchi entra
    // nella vista. Non serve un risveglio: lo accende e lo spegne un rapporto di righe visibili,
    // e il conto non scade col tempo.

    // L'anello si spegne con un solo risveglio a fine finestra, non con frame continui.
    let now = Instant::now();
    let (ring, wake) = s.update_ring(clock, now);
    if let Some(d) = wake {
        tx.wake_after(d);
    }
    let dither = s.dither_step(s.live.writing_left(now).is_some(), clock);
    // Il segno dei blocchi cambiati: ogni ricarica applicata viene da fuori (le scritture
    // dell'app si saltano in `reload::commit`). Un risveglio per segno che scade.
    let (marks, wake) = s.update_marks(reloaded.as_ref().map(|r| &r.change), clock, now);
    if let Some(d) = wake {
        tx.wake_after(d);
    }
    let blurred = s.sidebar.blur_close();
    let changed = opened || reloaded.is_some() || ring || fire || dither || marks || side || blurred;
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
    App::new(spec).compositor(fx::FxCompositor::default()).on_frame_encoded(probe::on_frame).run();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_doc() -> Doc {
        Doc { document: None, style: ui::markdown_style::style(&ui::theme::Theme::for_mode(true), 15.5), scroll: None, generation: 0, live: live::Live::default(), library: index::Library::load(Vec::new(), None), sidebar: ui::sidebar::Sidebar::default(), rings: BTreeMap::new(), ring_wake: None, below: live::below::Below::default(), visible_last: 0, fire_since: None, fire_armed: None, window_active: true, changed: live::arriving::Changed::default(), mark_wake: None, dither_since: None }
    }

    /// Con "riduci movimento" non passa nessun frame: `poll` gira solo ai risvegli. Rune misura
    /// `wake_after` dall'orologio di inizio frame, più vecchio di `Instant::now()`: il risveglio
    /// può arrivare prima della fine della finestra. Quel risveglio ne arma un altro, e
    /// l'anello si spegne alla fine.
    #[test]
    fn an_early_wake_rearms_and_the_ring_goes_off() {
        let t0 = Instant::now();
        let ms = |n| Duration::from_millis(n);
        let window = live::activity::WRITING_WINDOW;
        let mut doc = empty_doc();
        let w = |d| vec![(PathBuf::from("/r/a.md"), d)];
        // Ricarica applicata a t0, al clock 100 s; il frame finisce 5 ms dopo l'inizio.
        let (changed, wake) = doc.ring_step(&w(window), 100.0, t0);
        assert!(changed && doc.ring_lit());
        let wake = wake.expect("un risveglio alla fine");
        // Nessun frame in mezzo: solo il risveglio, che arriva 5 ms prima della fine sul muro.
        let clock = 100.0 + wake.as_secs_f64();
        let now = t0 + window - ms(5);
        let (changed, again) = doc.ring_step(&w(ms(5)), clock, now);
        assert!(!changed && doc.ring_lit());
        let again = again.expect("il risveglio in anticipo ne arma un altro");
        assert!(again >= ms(5));
        // Al secondo risveglio la finestra è finita.
        let (changed, last) = doc.ring_step(&[], clock + again.as_secs_f64(), now + again);
        assert!(changed && !doc.ring_lit(), "spento a fine finestra");
        assert_eq!(last, None, "nessun altro risveglio");
    }

    /// Un poll qualsiasi a anello acceso (un frame) non arma un risveglio nuovo.
    #[test]
    fn one_wake_per_window() {
        let t0 = Instant::now();
        let window = live::activity::WRITING_WINDOW;
        let mut doc = empty_doc();
        let w = |d| vec![(PathBuf::from("/r/a.md"), d)];
        assert!(doc.ring_step(&w(window), 0.0, t0).1.is_some());
        let dt = Duration::from_millis(16);
        assert_eq!(doc.ring_step(&w(window - dt), 0.016, t0 + dt).1, None);
        // Una nuova scrittura allunga la finestra: un risveglio per la fine nuova.
        assert!(doc.ring_step(&w(window), 0.032, t0 + 2 * dt).1.is_some());
    }

    /// Due file che scrivono: ognuno tiene l'orologio a cui si è acceso, il risveglio è per il
    /// primo che finisce, e quando finisce si spegne solo il suo anello.
    #[test]
    fn each_writing_file_has_its_own_ring() {
        let t0 = Instant::now();
        let window = live::activity::WRITING_WINDOW;
        let ms = |n| Duration::from_millis(n);
        let (a, b) = (PathBuf::from("/r/a.md"), PathBuf::from("/r/b.md"));
        let mut doc = empty_doc();
        doc.ring_step(&[(a.clone(), window)], 1.0, t0);
        let (changed, wake) = doc.ring_step(&[(a.clone(), window - ms(2000)), (b.clone(), window)], 3.0, t0 + ms(2000));
        assert!(changed);
        assert_eq!(wake, None, "il primo che finisce è ancora `a`: il risveglio armato vale");
        assert_eq!((doc.ring_of(&a), doc.ring_of(&b)), (Some(1.0), Some(3.0)));
        let (changed, wake) = doc.ring_step(&[(b.clone(), ms(2000))], 11.0, t0 + window);
        assert!(changed && doc.ring_of(&a).is_none() && doc.ring_of(&b) == Some(3.0));
        assert_eq!(wake, Some(ms(2001)), "poi quello di `b`");
    }

    /// Come l'anello: il risveglio per il segno che scade arriva in anticipo sul muro, ne arma
    /// un altro, e il segno si toglie alla fine.
    #[test]
    fn an_early_wake_rearms_and_the_changed_mark_goes_off() {
        let t0 = Instant::now();
        let ms = |n| Duration::from_millis(n);
        let mut doc = empty_doc();
        let change = doc::document::Change { added: vec![3], changed: vec![4], ..Default::default() };
        let (changed, wake) = doc.update_marks(Some(&change), 100.0, t0);
        assert!(changed && doc.is_marked(3) && doc.is_marked(4));
        let wake = wake.expect("un risveglio alla scadenza");
        let clock = 100.0 + wake.as_secs_f64();
        let now = t0 + live::arriving::CHANGED_MARK - ms(5);
        let (changed, again) = doc.update_marks(None, clock, now);
        assert!(!changed && doc.changed.is_marked(3, now));
        let again = again.expect("il risveglio in anticipo ne arma un altro");
        let (changed, last) = doc.update_marks(None, clock + again.as_secs_f64(), now + again);
        assert!(changed && !doc.changed.is_marked(3, now + again), "tolto alla scadenza");
        assert_eq!(last, None, "nessun altro risveglio");
    }

    /// Un poll qualsiasi con un segno acceso non arma un risveglio nuovo.
    #[test]
    fn one_wake_per_mark_expiry() {
        let t0 = Instant::now();
        let mut doc = empty_doc();
        let change = doc::document::Change { added: vec![1], ..Default::default() };
        assert!(doc.update_marks(Some(&change), 0.0, t0).1.is_some());
        let dt = Duration::from_millis(16);
        assert_eq!(doc.update_marks(None, 0.016, t0 + dt).1, None);
    }

    /// Il dither: solo con l'ultimo blocco aperto e il file che sta scrivendo; si spegne a fine
    /// finestra o quando il blocco si chiude.
    #[test]
    fn dither_only_on_an_open_last_block_while_writing() {
        let mut doc = empty_doc();
        doc.document = Some(Document::from_source(std::path::Path::new("t.md"), "a\n\n```rs\nfn"));
        assert!(!doc.dither_step(false, 1.0) && doc.dither_since.is_none(), "non sta scrivendo");
        assert!(doc.dither_step(true, 1.0) && doc.dither_since == Some(1.0));
        assert!(!doc.dither_step(true, 2.0) && doc.dither_since == Some(1.0), "resta acceso dalla prima volta");
        assert!(doc.dither_step(false, 3.0) && doc.dither_since.is_none(), "spento a fine finestra");
        doc.dither_step(true, 4.0);
        doc.document.as_mut().unwrap().update("a\n\n```rs\nfn\n```\n\n");
        assert!(doc.dither_step(true, 5.0) && doc.dither_since.is_none(), "blocco chiuso");
    }

    #[test]
    fn row_keys_change_after_open_paths() {
        let dir = std::env::temp_dir().join(format!("runemd-gen-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.md");
        std::fs::write(&f, "# a\n\nb\n").unwrap();
        let mut doc = Doc { document: None, style: ui::markdown_style::style(&ui::theme::Theme::for_mode(true), 15.5), scroll: None, generation: 0, live: live::Live::default(), library: index::Library::load(Vec::new(), None), sidebar: ui::sidebar::Sidebar::default(), rings: BTreeMap::new(), ring_wake: None, below: live::below::Below::default(), visible_last: 0, fire_since: None, fire_armed: None, window_active: true, changed: live::arriving::Changed::default(), mark_wake: None, dither_since: None };
        doc.open_paths(vec![f.clone()]);
        // Rune rilegge le chiavi solo se la versione cambia: anche lo stesso file invariato,
        // riaperto, deve cambiare versione e chiavi.
        let rows = |d: &Doc| {
            let doc = open_document(d);
            (ui::doc_view::rows_version(d.generation, doc), ui::doc_view::row_key(d.generation, 0, doc))
        };
        let before = rows(&doc);
        doc.open_paths(vec![f]);
        let after = rows(&doc);
        assert_ne!(before.0, after.0, "versione");
        assert_ne!(before.1, after.1, "chiavi");
    }
}
