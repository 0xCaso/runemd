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

mod doc;
mod fx;
mod live;
mod open;
mod probe;
mod ui;
mod watch;

use std::path::PathBuf;
use std::time::{Duration, Instant};

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
    /// L'orologio di Rune (`poll`) a cui si è acceso l'anello "sta scrivendo" del file aperto;
    /// `None` quando è spento.
    ring_since: Option<f64>,
    /// Il risveglio armato per spegnere l'anello: uno per finestra, non uno a frame.
    ring_wake: Option<RingWake>,
}

/// Un risveglio per la fine della finestra "sta scrivendo".
#[derive(Clone, Copy, Debug, PartialEq)]
struct RingWake {
    /// La fine della finestra (orologio del muro, come `Activity`).
    end: Instant,
    /// Quando Rune lo fa scattare, nel suo orologio (`poll` lo riceve come `clock`).
    at: f64,
}

impl Doc {
    /// L'anello "sta scrivendo" è acceso: il compositor scorre le istanze solo allora.
    pub fn ring_lit(&self) -> bool {
        self.ring_since.is_some()
    }

    /// Accende o spegne l'anello secondo `Live` a `now`; vero se è cambiato. Ritorna anche il
    /// risveglio da chiedere: a fine finestra `poll` lo spegne anche se nessun frame è passato
    /// in mezzo (con "riduci movimento" non ne passano).
    fn update_ring(&mut self, clock: f64, now: Instant) -> (bool, Option<Duration>) {
        self.ring_step(self.live.writing_left(now), clock, now)
    }

    /// `update_ring` con `left` (fra quanto finisce la finestra, a `now`) già calcolato.
    ///
    /// Si arma un risveglio quando la fine della finestra cambia (una scrittura nuova) e quando
    /// quello armato è scaduto (nell'orologio di Rune) con l'anello ancora acceso: Rune misura
    /// `wake_after` da `clock`, l'inizio del frame, e la fine da `now`, più tardi; il risveglio
    /// può arrivare un po' prima della fine sul muro. Un poll a metà finestra non arma niente.
    fn ring_step(&mut self, left: Option<Duration>, clock: f64, now: Instant) -> (bool, Option<Duration>) {
        let end = left.map(|d| now + d);
        let due = self.ring_wake.is_some_and(|w| clock >= w.at);
        let wake = match left {
            Some(d) if due || end != self.ring_wake.map(|w| w.end) => Some(d + Duration::from_millis(1)),
            _ => None,
        };
        self.ring_wake = match (end, wake) {
            (Some(end), Some(d)) => Some(RingWake { end, at: clock + d.as_secs_f64() }),
            (Some(_), None) => self.ring_wake,
            (None, _) => None,
        };
        let lit = left.is_some();
        if lit == self.ring_lit() {
            return (false, wake);
        }
        self.ring_since = lit.then_some(clock);
        (true, wake)
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
                self.document = Some(document);
                self.generation += 1;
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
    let mut doc = Doc { document: None, style: ui::markdown_style::style(&ui::theme::Theme::for_mode(dark_mode()), 15.5), scroll: probe::ScrollRun::from_env(), generation: 0, live: live::Live::default(), ring_since: None, ring_wake: None };
    doc.open_paths(paths);
    doc
}

fn setup(s: &mut Doc, waker: Waker) {
    open::queue::set_waker(waker.clone());
    s.live.start(waker);
}

/// Il documento aperto; `view` chiama la vista del documento solo quando c'è.
fn open_document(s: &Doc) -> &Document {
    s.document.as_ref().expect("vista del documento senza documento")
}

fn view(s: &Doc, cx: &mut Cx<Doc>) -> El {
    let body = if s.document.is_some() { ui::doc_view::view(s, open_document, s.generation, &s.style) } else { el() };
    let theme = ui::theme::Theme::for_mode(dark_mode());
    // Vetro di sistema attorno, pannello pieno a `FRAME` dai bordi con il documento.
    let panel = el().w_full().h_full().fill(theme.panel).radius(ui::theme::R_PANEL).clip_rounded(ui::theme::R_PANEL).child(body);
    // L'anello "sta scrivendo" sta in uno slot attorno al pannello, sempre presente perché la
    // struttura (e lo scroll del documento) non cambi quando si accende.
    // TODO(task 16/19): ospite temporaneo. L'anello va sulla scheda del file (task 19), sulla sua
    // riga in Recenti e nell'albero (task 16), con `fx::ring::slot` e `Mark::TabRing`,
    // `RecentRing`, `TreeRing`; il pannello del lettore poi non ne ha più.
    let host = el().w_full().h_full().radius(ui::theme::R_PANEL).child(panel);
    let host = match s.ring_since {
        Some(since) => fx::ring::slot(host, fx::Mark::TabRing(0), since, cx.is_reduced_motion()),
        None => host,
    };
    el().w_full().h_full().system_material(rune::SystemMaterial::Glass).fill(theme.glass).padding(ui::theme::FRAME).child(host)
}

/// Ogni tasto non preso da un controllo, prima della selezione di Rune: il Cmd+A va segnato.
fn on_key(s: &mut Doc, key: &rune::KeyEvent, tx: &mut Tx) -> bool {
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
    // L'anello si spegne con un solo risveglio a fine finestra, non con frame continui.
    let (ring, wake) = s.update_ring(clock, Instant::now());
    if let Some(d) = wake {
        tx.wake_after(d);
    }
    let changed = opened || reloaded.is_some() || ring;
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
        Doc { document: None, style: ui::markdown_style::style(&ui::theme::Theme::for_mode(true), 15.5), scroll: None, generation: 0, live: live::Live::default(), ring_since: None, ring_wake: None }
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
        // Ricarica applicata a t0, al clock 100 s; il frame finisce 5 ms dopo l'inizio.
        let (changed, wake) = doc.ring_step(Some(window), 100.0, t0);
        assert!(changed && doc.ring_lit());
        let wake = wake.expect("un risveglio alla fine");
        // Nessun frame in mezzo: solo il risveglio, che arriva 5 ms prima della fine sul muro.
        let clock = 100.0 + wake.as_secs_f64();
        let now = t0 + window - ms(5);
        let (changed, again) = doc.ring_step(Some(ms(5)), clock, now);
        assert!(!changed && doc.ring_lit());
        let again = again.expect("il risveglio in anticipo ne arma un altro");
        assert!(again >= ms(5));
        // Al secondo risveglio la finestra è finita.
        let (changed, last) = doc.ring_step(None, clock + again.as_secs_f64(), now + again);
        assert!(changed && !doc.ring_lit(), "spento a fine finestra");
        assert_eq!(last, None, "nessun altro risveglio");
    }

    /// Un poll qualsiasi a anello acceso (un frame) non arma un risveglio nuovo.
    #[test]
    fn one_wake_per_window() {
        let t0 = Instant::now();
        let window = live::activity::WRITING_WINDOW;
        let mut doc = empty_doc();
        assert!(doc.ring_step(Some(window), 0.0, t0).1.is_some());
        let dt = Duration::from_millis(16);
        assert_eq!(doc.ring_step(Some(window - dt), 0.016, t0 + dt).1, None);
        // Una nuova scrittura allunga la finestra: un risveglio per la fine nuova.
        assert!(doc.ring_step(Some(window), 0.032, t0 + 2 * dt).1.is_some());
    }

    #[test]
    fn row_keys_change_after_open_paths() {
        let dir = std::env::temp_dir().join(format!("runemd-gen-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.md");
        std::fs::write(&f, "# a\n\nb\n").unwrap();
        let mut doc = Doc { document: None, style: ui::markdown_style::style(&ui::theme::Theme::for_mode(true), 15.5), scroll: None, generation: 0, live: live::Live::default(), ring_since: None, ring_wake: None };
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
