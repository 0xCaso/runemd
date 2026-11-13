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
mod probe;

use std::borrow::Cow;
use std::time::Duration;

use rune::{App, AppSpec, Cx, El, GrowthSpec, ScrollSpec, Tx, col, el, oklcha};
use rune_kit::MarkdownStyle;

const COLUMN: f32 = 760.0;

pub struct Doc {
    blocks: Vec<String>,
    style: MarkdownStyle,
    scroll: Option<probe::ScrollRun>,
}

fn init() -> Doc {
    let path = rune::extra_args().first().cloned().or_else(|| std::env::var("MDSPIKE_FILE").ok())
        .expect("uso: runemd FILE.md");
    let source = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    Doc { blocks: doc::blocks::split(&source), style: MarkdownStyle::neutral(true), scroll: probe::ScrollRun::from_env() }
}

fn block_row(s: &Doc, i: usize) -> El {
    // `selectable` fa di ogni blocco un selection_group a sé, che fermerebbe un trascinamento
    // al bordo del blocco: si toglie il flag e vale il gruppo unico sulle righe (`view`).
    let body = rune_kit::Markdown::new(s.blocks[i].as_str()).selectable(true).render(&s.style).selection_group(false);
    // Le righe virtuali vogliono padding interno, non gap tra righe.
    col().w_full().px(28.0).py(7.0).child(body)
}

fn view(s: &Doc, _: &mut Cx<Doc>) -> El {
    let rows = col().key("rows").w_full().max_w(COLUMN).self_center().selection_group(true)
        .growth(GrowthSpec::default())
        .virtual_rows(s.blocks.len(), 60.0, |_: &Doc, i| Cow::Owned(format!("b{i}")), block_row);
    let page = col().w_full().child(el().h(24.0)).child(rows).child(el().h(48.0));
    let doc = col().key("doc").w_full().h_full().scroll(ScrollSpec { stick_bottom: false }).child(page);
    el().w_full().h_full().fill(oklcha(0.17, 0.01, 265.0, 1.0)).child(doc)
}

fn poll(s: &mut Doc, clock: f64, tx: &mut Tx) -> bool {
    let Some(run) = s.scroll.as_mut() else { return false };
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
    rune::prewarm_gpu();
    probe::mark_start();
    // Lo slot mono di default è Inter: il codice va in SF Mono.
    let spec = AppSpec { poll: Some(poll), families: ("Inter", "ui-monospace, Menlo"), ..AppSpec::new("runemd", (900.0, 1000.0), init, view) };
    App::new(spec).compositor(fx::FxCompositor::default()).on_frame_encoded(probe::on_frame).run();
}
