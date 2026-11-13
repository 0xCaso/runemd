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

mod doc;
mod fx;
mod open;
mod probe;
mod ui;

use std::path::PathBuf;
use std::time::Duration;

use rune::{App, AppSpec, Cx, El, Tx, Waker, el, oklcha};
use rune_kit::MarkdownStyle;

use doc::document::Document;

pub struct Doc {
    /// `None` finché non si apre un file.
    document: Option<Document>,
    style: MarkdownStyle,
    scroll: Option<probe::ScrollRun>,
    /// Cresce a ogni documento aperto: le chiavi delle righe cambiano e Rune scarta le altezze misurate.
    generation: u64,
}

impl Doc {
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
                self.document = Some(document);
                self.generation += 1;
            }
            Err(e) => eprintln!("runemd: {}: {e}", path.display()),
        }
    }
}

fn init() -> Doc {
    let mut paths = open::cli_paths(rune::extra_args());
    if paths.is_empty() {
        paths.extend(std::env::var("MDSPIKE_FILE").ok().map(PathBuf::from));
    }
    let mut doc = Doc { document: None, style: MarkdownStyle::neutral(true), scroll: probe::ScrollRun::from_env(), generation: 0 };
    doc.open_paths(paths);
    doc
}

fn setup(_: &mut Doc, waker: Waker) {
    open::queue::set_waker(waker);
}

/// Il documento aperto; `view` chiama la vista del documento solo quando c'è.
fn open_document(s: &Doc) -> &Document {
    s.document.as_ref().expect("vista del documento senza documento")
}

fn view(s: &Doc, _: &mut Cx<Doc>) -> El {
    let body = if s.document.is_some() { ui::doc_view::view(s, open_document, s.generation, &s.style) } else { el() };
    el().w_full().h_full().fill(oklcha(0.17, 0.01, 265.0, 1.0)).child(body)
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
    let Some(run) = s.scroll.as_mut() else { return opened };
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
    false
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

    #[test]
    fn row_keys_change_after_open_paths() {
        let dir = std::env::temp_dir().join(format!("runemd-gen-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.md");
        std::fs::write(&f, "# a\n\nb\n").unwrap();
        let mut doc = Doc { document: None, style: MarkdownStyle::neutral(true), scroll: None, generation: 0 };
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
