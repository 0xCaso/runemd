//! La colonna del documento: un blocco top-level per riga di `virtual_rows` dentro uno scroll.
//!
//! Le righe hanno la chiave dell'id del blocco, non dell'indice: quando il file cambia,
//! Rune tiene ferma la riga in cima alla vista per chiave (`virtual_rows`: "inserting or
//! prepending rows keeps the reader's row in place"); se quel blocco sparisce, l'ancora passa al
//! primo blocco successivo che esiste ancora (`VirtualList::set_keys` → `first_visible`).

use std::borrow::Cow;
use std::collections::HashSet;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use rune::{Color, El, GrowthSpec, Key, KeyEvent, ScrollSpec, SelectedText, TextSelection, col, el};
use rune_kit::MarkdownStyle;

use crate::doc::document::{Block, Document};
use crate::doc::select_all::{End, Ends, SelectAll, SelectAllFlag};

use super::theme::{CHANGED_MARK_W, CHANGED_MARK_X, COLUMN, ROW_PAD_X, ROW_PAD_Y};

/// Altezza dello spazio sotto l'ultima riga: ci sta il dither "sta arrivando".
pub(super) const BOTTOM: f32 = 48.0;
/// Spazio sopra la prima riga (anche il sorgente parte da qui).
pub(super) const TOP: f32 = 24.0;

/// I segnali live dentro la colonna, che non spostano il testo.
pub struct Signals<S> {
    /// Se il blocco `id` ha il segno "cosa è cambiato" (a sinistra della riga, fuori dal flusso).
    pub marked: fn(&S, u64) -> bool,
    /// Colore del segno (`theme.mark`).
    pub mark: Color,
    /// La barra del dither "sta arrivando", se accesa: va nello spazio sotto l'ultima riga, che
    /// ha sempre la stessa altezza, quindi niente sopra si sposta.
    pub tail: Option<El>,
    /// I segni di Cerca del blocco, la cui riga ha la chiave data (`ui::find`).
    pub hits: fn(&S, &Block, &str) -> super::find::Marks,
    /// Dove sono la vista e la pagina del documento `id` (ognuna dal suo `on_layout`): servono a
    /// sapere se lo scroll è in fondo. Vero se la vista deve rigirare.
    pub edge: fn(&mut S, u64, Edge) -> bool,
}

/// Un rettangolo della vista del documento, in punti della finestra.
#[derive(Clone, Copy, Debug)]
pub enum Edge {
    /// La vista che scorre (`doc`): cambia solo con la finestra.
    Viewport([f32; 4]),
    /// La pagina dentro la vista: si muove a ogni frame di scroll.
    Page([f32; 4]),
}

/// Chiave della riga `i`: id del documento + id del blocco. Gli id dei blocchi ripartono da 1 a
/// ogni `Document`, l'id del documento no ed è unico nell'app: senza, una scheda o un file
/// riaperto erediterebbe le altezze misurate di un altro.
pub fn row_key(i: usize, doc: &Document) -> String {
    match doc.blocks.get(i) {
        Some(b) => format!("{:x}-{:x}", doc.id, b.id),
        // Le closure di `virtual_rows` girano con lo stato di adesso: se il documento è cambiato
        // dopo l'ultima vista, una chiave unica qualsiasi finché la vista non rigira.
        None => format!("{:x}-i{i:x}", doc.id),
    }
}

/// Ciò che dice a Rune se le chiavi delle righe sono cambiate: un altro documento (`id`, anche
/// se è lo stesso file invariato), un update (`revision`), un altro contenuto a parità di
/// revisione (`file_hash`).
pub fn rows_version(doc: &Document) -> (u64, u64, u64) {
    (doc.id, doc.revision, doc.file_hash)
}

/// La vista scorrevole del documento `doc_of(s)`. Le closure di `virtual_rows` non possono
/// tenere prestiti: a layout ricevono lo stato della finestra e ritrovano il documento con lo
/// stesso `doc_of`, così numero di righe, versione, chiavi e contenuto vengono tutti dallo stesso
/// documento.
///
/// Ogni documento ha un elemento suo (chiave `tab-<id>` attorno allo scroll `doc`): una scheda
/// che torna davanti parte da uno scroll nuovo, non da quello della scheda di prima.
///
/// `on_rows` riceve l'id del documento e gli indici delle righe visibili (`on_visible_rows` di
/// Rune: `start..end`, righe anche solo in parte nella vista) dopo il primo layout e a ogni loro
/// cambio; ritorna vero se ha cambiato stato che la vista legge.
pub fn view<S: 'static>(s: &S, doc_of: fn(&S) -> &Document, style: &MarkdownStyle, on_rows: fn(&mut S, u64, Range<usize>) -> bool, signals: Signals<S>) -> El {
    let doc = doc_of(s);
    let generation = doc.id;
    let style = style.clone();
    let blocks: Arc<[(u64, Arc<str>)]> = doc.blocks.iter().map(|b| (b.id, b.source.clone())).collect();
    let file = File { path: doc.path.clone(), hash: doc.file_hash };
    let select_all = doc.select_all.clone();
    let flag = select_all.clone();
    let order = blocks.clone();
    let rows = col().key("rows").w_full().max_w(COLUMN).self_center().selection_group(true)
        .growth(GrowthSpec::default())
        .selection_copy(move |sel| copy_source(sel, generation, &file, &select_all, &blocks))
        .on_selection_change(move |_: &mut S, ch, _| selection_changed(&flag, ch, generation, &order))
        .virtual_rows(
            doc.blocks.len(),
            60.0,
            move |s: &S, i| Cow::Owned(row_key(i, doc_of(s))),
            move |s: &S, i| match doc_of(s).blocks.get(i) {
                Some(b) => {
                    let hits = (signals.hits)(s, b, &row_key(i, doc_of(s)));
                    block_row(&b.source, &style, (signals.marked)(s, b.id).then_some(signals.mark), hits)
                }
                None => el(),
            },
        )
        .virtual_rows_version(rows_version(doc))
        .on_visible_rows(move |s: &mut S, rows| on_rows(s, generation, rows));
    let mut bottom = col().w_full().h(BOTTOM);
    if let Some(bar) = signals.tail {
        bottom = bottom.child(col().key("arriving").w_full().max_w(COLUMN).self_center().px(ROW_PAD_X).child(bar));
    }
    let edge = signals.edge;
    // Gli `on_layout` dicono solo dove sono vista e pagina: la vista rigira solo se cambia ciò
    // che mostra (`live_only` altrimenti), non a ogni frame di scroll.
    let page = col().w_full().child(el().h(TOP)).child(rows).child(bottom).on_layout_tx(move |s: &mut S, r, tx: &mut rune::Tx| {
        if !edge(s, generation, Edge::Page(r)) {
            tx.live_only();
        }
    });
    let scroll = col().key("doc").w_full().h_full().scroll(ScrollSpec { stick_bottom: false }).child(page).on_layout_tx(move |s: &mut S, r, tx: &mut rune::Tx| {
        if !edge(s, generation, Edge::Viewport(r)) {
            tx.live_only();
        }
    });
    el().key(format!("tab-{generation:x}")).w_full().grow(1.0).min_h(0.0).child(scroll)
}

/// L'etichetta del fuoco: quanti blocchi nuovi ci sono sotto la vista (testo approvato, spec).
pub fn fire_label(n: usize) -> String {
    if n == 1 { "1 paragrafo nuovo più in basso".to_owned() } else { format!("{n} paragrafi nuovi più in basso") }
}

/// I tetti della selezione ricostruita fuori vista (rune `MAX_VIRTUAL_COPY_BYTES` e
/// `MAX_VIRTUAL_COPY_ROWS`, `pub(crate)` in Rune): oltre, `TextSelection` si ferma prima della
/// fine. Il tetto delle righe conta anche le righe senza testo (`---`).
const COPY_CAP_BYTES: usize = 4 * 1024 * 1024;
const COPY_CAP_ROWS: usize = 10_000;
/// Il testo troncato può fermarsi poco prima del tetto (righe vuote e prefisso tra due pezzi).
const COPY_CAP_SLACK: usize = 1024;

/// Da chiamare con ogni tasto che l'app riceve (`AppSpec::on_key`, prima della selezione di
/// Rune): un Cmd+A segna che la selezione che ne segue è "tutto il file".
///
/// Vale solo il tasto che Rune prende per sé: Cmd+A semplice (come `selection_key`: niente Shift,
/// Option, né Control con Logo) e non ripetuto.
pub fn note_key(doc: &Document, key: &KeyEvent, repeat: bool) {
    let m = key.mods;
    let plain = m.cmd && !m.shift && !m.alt && !(m.ctrl && m.logo);
    if matches!(key.key, Key::Command('a')) && plain && !repeat && doc.select_all.get() == SelectAll::No {
        doc.select_all.set(SelectAll::Pending);
    }
}

/// I capi di `sel`; `None` se nessun testo selezionato sta in una riga del documento.
fn ends(sel: &TextSelection, generation: u64, blocks: &[(u64, Arc<str>)]) -> Option<Ends> {
    let texts: Vec<_> = sel.texts().iter().filter(|t| !t.range().is_empty()).collect();
    let end = |t: &SelectedText| Some(End { row: row_id(t.path(), generation)?, range: t.range(), whole: t.is_whole() });
    let (first, last) = (end(texts.first()?)?, end(texts.last()?)?);
    let index = |row: u64| blocks.iter().position(|(id, _)| *id == row);
    let rows = match (index(first.row), index(last.row)) {
        (Some(a), Some(b)) => b.saturating_sub(a) + 1,
        _ => 0,
    };
    let capped = sel.text().len() + COPY_CAP_SLACK >= COPY_CAP_BYTES || rows >= COPY_CAP_ROWS;
    Some(Ends { first, last, capped })
}

/// Ogni cambio di selezione. Un Cmd+A in attesa scade al primo cambio che non è di Rune: una
/// selezione fatta col puntatore (`dragging`) o svuotata. Dopo un Cmd+A la prima selezione vale
/// "tutto" e se ne ricordano i capi. Rune poi la completa con le righe che ricostruisce, senza
/// `started` (oltre il tetto di copia la lunghezza oscilla col layout): resta "tutto" finché i
/// capi tengono ([`End::holds`]). Se la selezione è troncata l'ultima riga vista non è la fine e
/// non si confronta. Un clic, uno svuotamento, una selezione nuova o con un capo spostato
/// (Shift+clic) la tolgono.
///
/// Caso residuo: su una selezione troncata (oltre 4 MiB o 10 000 righe) un Shift+clic che
/// accorcia la fine senza scendere sotto il tetto non si vede, e Cmd+C copia ancora tutto il
/// file. Lo fissa `a_shift_trim_after_cmd_a_over_the_copy_cap_still_copies_the_file`.
fn selection_changed(flag: &SelectAllFlag, change: &rune::SelectionChange, generation: u64, blocks: &[(u64, Arc<str>)]) {
    let now = change.selection().filter(|t| !t.text().is_empty()).and_then(|t| ends(t, generation, blocks));
    let next = match (flag.get(), now) {
        _ if change.cleared() || change.dragging() => SelectAll::No,
        (SelectAll::Pending, Some(ends)) => SelectAll::All(ends),
        (SelectAll::All(was), Some(now))
            if !change.started() && now.first.holds(&was.first) && ((was.capped && now.capped) || now.last.holds(&was.last)) =>
        {
            SelectAll::All(was)
        }
        _ => SelectAll::No,
    };
    flag.set(next);
}

/// Ciò che copiano Cmd+C e il menu Copia: il Markdown sorgente dei blocchi toccati dalla
/// selezione, uniti da una riga vuota, non il testo disegnato. Le righe fuori vista non hanno
/// testo in layout, ma Rune le ricostruisce per la selezione e `sel.texts()` le contiene (fino a
/// 10 000 righe e 4 MiB per copia: oltre, la selezione arriva troncata e senza segno).
///
/// Per questo "tutto il file" non si deduce dalla selezione: lo dice il Cmd+A (`select_all`). Allora
/// si copia il file letto dal disco, che conserva le righe vuote in più tra i blocchi.
fn copy_source(sel: &TextSelection, generation: u64, file: &File, select_all: &SelectAllFlag, blocks: &[(u64, Arc<str>)]) -> Option<String> {
    if matches!(select_all.get(), SelectAll::All(_)) && !blocks.is_empty() {
        return file.read().or_else(|| Some(join(blocks.iter().map(|(_, s)| &**s))));
    }
    let picked: HashSet<u64> = sel.texts().iter().filter(|t| !t.range().is_empty()).filter_map(|t| row_id(t.path(), generation)).collect();
    let index: Vec<usize> = blocks.iter().enumerate().filter(|(_, (id, _))| picked.contains(id)).map(|(i, _)| i).collect();
    let (&first, &last) = (index.first()?, index.last()?);
    Some(join(blocks[first..=last].iter().map(|(_, s)| &**s)))
}

fn join<'a>(sources: impl Iterator<Item = &'a str>) -> String {
    sources.collect::<Vec<_>>().join("\n\n")
}

/// Il file aperto: si rilegge dal disco solo se è ancora quello da cui vengono i blocchi.
#[derive(Clone)]
struct File {
    path: PathBuf,
    hash: u64,
}

impl File {
    fn read(&self) -> Option<String> {
        let (text, hash, _) = crate::doc::disk::read_disk(&self.path).ok()?;
        (hash == self.hash).then_some(text)
    }
}

/// L'id del blocco nel percorso di un testo: il segmento `<id documento>-<id>` della sua riga.
fn row_id(path: &str, generation: u64) -> Option<u64> {
    let prefix = format!("{generation:x}-");
    path.split('/').find_map(|seg| u64::from_str_radix(seg.strip_prefix(&prefix)?, 16).ok())
}

/// Una riga del documento, con i segni di Cerca `hits`; con `mark` ha il segno "cosa è cambiato"
/// nel margine sinistro, fuori dal flusso (il testo non si sposta), alto quanto il testo del blocco.
fn block_row(source: &str, style: &MarkdownStyle, mark: Option<Color>, hits: super::find::Marks) -> El {
    // `selectable` fa di ogni blocco un selection_group a sé, che fermerebbe un trascinamento
    // al bordo del blocco: si toglie il flag e vale il gruppo unico sulle righe.
    let body = rune_kit::Markdown::new(source).highlights(hits).selectable(true).render(style).selection_group(false);
    // Le righe virtuali vogliono padding interno, non gap tra righe.
    let row = col().w_full().px(ROW_PAD_X).py(ROW_PAD_Y).child(body);
    match mark {
        Some(c) => row.child(el().key("changed").pin(Some(CHANGED_MARK_X), Some(ROW_PAD_Y), None, Some(ROW_PAD_Y)).w(CHANGED_MARK_W).fill(c)),
        None => row,
    }
}

#[cfg(test)]
mod tests {
    //! L'ancora di lettura su un runtime Rune headless: stessa vista dell'app (`crate::view`),
    //! `Document::update` come lo chiamerà il ricaricamento.

    use std::path::Path;

    use rune::{Env, Runtime};
    use rune_kit::MarkdownStyle;

    use super::row_key;
    use crate::app::App as Doc;
    use crate::doc::document::Document;

    const VIEWPORT: (f32, f32) = (900.0, 1000.0);

    fn paragraph(n: usize) -> String {
        let words = ["riga", "lettore", "blocco", "scroll", "ancora", "frame", "testo", "colonna"];
        (0..12 + (n * 7) % 60).map(|k| words[(n + k) % words.len()]).collect::<Vec<_>>().join(" ")
    }

    /// 120 blocchi di altezze diverse: titoli ogni 10, paragrafi lunghi da 12 a 71 parole.
    fn sample() -> Vec<String> {
        (0..120).map(|n| if n % 10 == 0 { format!("## Sezione {n}") } else { format!("{n}. {}", paragraph(n)).replacen(". ", " ", 1) }).collect()
    }

    fn join(blocks: &[String]) -> String {
        blocks.join("\n\n") + "\n"
    }

    fn settle(rt: &mut Runtime<Doc>) {
        for _ in 0..4000 {
            if !rt.frame(1.0 / 120.0) { return; }
        }
        panic!("la vista non si ferma");
    }

    fn open(source: &str) -> Runtime<Doc> {
        open_doc(Document::from_source(Path::new("t.md"), source))
    }

    fn open_doc(document: Document) -> Runtime<Doc> {
        let mut state = Doc::bare(crate::index::Library::load(Vec::new(), None));
        state.set_style(MarkdownStyle::neutral(true));
        state.push_tab(document);
        let mut rt = Runtime::new(state, crate::view, VIEWPORT, Env::new(true, 2.0));
        rt.on_key = Some(crate::on_key);
        rt.frame(0.0);
        settle(&mut rt);
        rt
    }

    fn scroll_to(rt: &mut Runtime<Doc>, y: f32) {
        rt.update(|_, tx| tx.scroll_to_instant("doc", 0.0, y));
        settle(rt);
    }

    /// Il primo blocco visibile (id) e il suo offset dal bordo alto della vista, in pt.
    fn anchor(rt: &Runtime<Doc>) -> (u64, f32) {
        let els = rt.elements();
        let top = els.iter().find(|e| e.key.as_deref() == Some("doc")).expect("vista doc").rect[1];
        let s = &rt.state;
        let doc = s.active_document();
        let (key, y) = els.iter()
            .filter(|e| !e.exiting && e.rect[1] + e.rect[3] > top + 0.5)
            .filter_map(|e| { let k = e.key.as_deref()?; e.path.ends_with(&format!("rows/{k}")).then_some((k, e.rect[1])) })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .expect("nessuna riga visibile");
        let i = (0..doc.blocks.len()).find(|&i| row_key(i, doc) == key).expect("chiave senza blocco");
        (doc.blocks[i].id, y - top)
    }

    fn source(rt: &Runtime<Doc>) -> Vec<String> {
        rt.state.active_document().blocks.iter().map(|b| b.source.to_string()).collect()
    }

    fn index_of(rt: &Runtime<Doc>, id: u64) -> usize {
        rt.state.active_document().blocks.iter().position(|b| b.id == id).unwrap()
    }

    fn height(rt: &Runtime<Doc>, id: u64) -> f32 {
        let key = row_key(index_of(rt, id), rt.state.active_document());
        rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key.as_str())).unwrap().rect[3]
    }

    fn edit(rt: &mut Runtime<Doc>, blocks: &[String]) {
        let src = join(blocks);
        rt.update(|s, _| { s.tab_mut().unwrap().doc.update(&src); });
        settle(rt);
    }

    /// Apre il campione e scorre a metà, con il bordo alto a metà di un blocco.
    fn reading() -> (Runtime<Doc>, (u64, f32)) {
        let mut rt = open(&join(&sample()));
        scroll_to(&mut rt, 6000.0);
        let a = anchor(&rt);
        assert!(a.1 < -1.0, "il bordo alto taglia il primo blocco: {a:?}");
        (rt, a)
    }

    fn assert_still(case: &str, before: (u64, f32), after: (u64, f32)) {
        eprintln!("{case}: prima {before:?}, dopo {after:?}, scarto {:.2} pt", after.1 - before.1);
        assert_eq!(before.0, after.0, "{case}: il primo blocco visibile cambia");
        assert!((before.1 - after.1).abs() <= 1.0, "{case}: offset {} → {}", before.1, after.1);
    }

    #[test]
    fn insert_five_paragraphs_at_top_keeps_reading_position() {
        let (mut rt, before) = reading();
        let mut blocks: Vec<String> = (0..5).map(|n| format!("Nuovo {n}: {}", paragraph(n + 3))).collect();
        blocks.extend(source(&rt));
        edit(&mut rt, &blocks);
        assert_still("inserisci in testa", before, anchor(&rt));
    }

    #[test]
    fn lengthening_the_top_block_keeps_reading_position() {
        let (mut rt, before) = reading();
        let mut blocks = source(&rt);
        let i = index_of(&rt, before.0);
        blocks[i] = format!("{} {}", blocks[i], paragraph(40).repeat(3));
        let h0 = height(&rt, before.0);
        edit(&mut rt, &blocks);
        assert!(height(&rt, before.0) > h0 + 40.0, "il blocco si allunga: {h0} → {}", height(&rt, before.0));
        assert_eq!(rt.state.active_document().blocks[i].id, before.0, "modifica sul posto: stesso id");
        assert_still("allunga il blocco in cima", before, anchor(&rt));
    }

    #[test]
    fn lengthening_the_last_block_keeps_reading_position() {
        let (mut rt, before) = reading();
        let mut blocks = source(&rt);
        let last = blocks.len() - 1;
        blocks[last] = format!("{} {}", blocks[last], paragraph(41).repeat(4));
        edit(&mut rt, &blocks);
        assert_still("allunga l'ultimo blocco", before, anchor(&rt));
    }

    /// Il blocco in cima diventa due blocchi: id nuovi, l'ancora passa al blocco seguente.
    #[test]
    fn a_vanished_top_block_hands_the_anchor_to_the_next() {
        let (mut rt, before) = reading();
        let mut blocks = source(&rt);
        let i = index_of(&rt, before.0);
        let next = rt.state.active_document().blocks[i + 1].id;
        let rows = rt.elements();
        let top = rows.iter().find(|e| e.key.as_deref() == Some("doc")).unwrap().rect[1];
        let next_key = row_key(i + 1, rt.state.active_document());
        let next_y = rows.iter().find(|e| e.key.as_deref() == Some(next_key.as_str())).unwrap().rect[1] - top;
        blocks.splice(i..=i, [format!("Diviso A {}", paragraph(5)), format!("Diviso B {}", paragraph(6))]);
        edit(&mut rt, &blocks);
        let doc = rt.state.active_document();
        assert!(doc.blocks.iter().all(|b| b.id != before.0), "il blocco in cima non c'è più");
        let j = index_of(&rt, next);
        let key = row_key(j, doc);
        let els = rt.elements();
        let top = els.iter().find(|e| e.key.as_deref() == Some("doc")).unwrap().rect[1];
        let y = els.iter().find(|e| e.key.as_deref() == Some(key.as_str())).unwrap().rect[1] - top;
        eprintln!("blocco sparito: seguente prima {next_y:.2}, dopo {y:.2}");
        assert!((y - next_y).abs() <= 1.0, "il blocco seguente si sposta: {next_y} → {y}");
    }

    #[test]
    fn an_empty_file_opens() {
        let mut rt = open("");
        assert!(rt.state.active_document().blocks.is_empty());
        edit(&mut rt, &["# Ora c'è".to_owned()]);
        assert_eq!(anchor(&rt).1.round(), 24.0, "il primo blocco sotto il margine alto");
    }

    /// 500 KB senza righe vuote sono un blocco solo: il caso "inserisci in testa" tiene la riga
    /// dentro quel blocco.
    #[test]
    fn a_500_kb_file_without_blank_lines_keeps_reading_position() {
        let line = "Una riga lunga di testo senza righe vuote tra una e l'altra, lettore blocco scroll ancora.\n";
        let big = line.repeat(500 * 1024 / line.len());
        assert!(big.len() >= 500 * 1000 && !big.contains("\n\n"));
        let mut rt = open(&big);
        assert_eq!(rt.state.active_document().blocks.len(), 1);
        scroll_to(&mut rt, 20000.0);
        let before = anchor(&rt);
        assert!(before.1 < -1000.0, "dentro il blocco: {before:?}");
        let blocks: Vec<String> = (0..5).map(|n| format!("Nuovo {n}: {}", paragraph(n))).chain([big.trim_end().to_owned()]).collect();
        edit(&mut rt, &blocks);
        assert_still("500 KB, inserisci in testa", before, anchor(&rt));
    }

    /// Il fuoco sul runtime: 3 blocchi aggiunti in fondo mentre si legge in cima lo accendono
    /// (l'ultimo visibile viene da `on_visible_rows`), lo scroll in fondo lo spegne.
    #[test]
    fn appended_blocks_light_the_fire_until_scrolled_into_view() {
        let mut rt = open(&join(&sample()));
        assert!(rt.state.tab().unwrap().visible_last != 0, "la vista ha riportato l'ultima riga visibile");
        let mut blocks = sample();
        blocks.extend((0..3).map(|n| format!("Nuovo in fondo {n}: {}", paragraph(n))));
        let src = join(&blocks);
        rt.update(|s, _| {
            let change = s.tab_mut().unwrap().doc.update(&src);
            s.tab_mut().unwrap().note_reload(&change, 1.0);
        });
        settle(&mut rt);
        assert_eq!(rt.state.tab().unwrap().below.count(), 3);
        assert_eq!(rt.state.tab().unwrap().fire_since, Some(1.0));
        assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("fire")), "slot del fuoco");
        scroll_to(&mut rt, 1.0e9);
        assert_eq!(rt.state.tab().unwrap().below.count(), 0);
        assert_eq!(rt.state.tab().unwrap().fire_since, None);
        assert!(!rt.elements().iter().any(|e| e.key.as_deref() == Some("fire") && !e.exiting), "fuoco spento");
    }

    fn fire_drawn(rt: &Runtime<Doc>) -> bool {
        rt.elements().iter().any(|e| matches!(e.key.as_deref(), Some("fire" | "fire-label")) && !e.exiting)
    }

    /// Un documento corto, tutto in vista: un blocco aggiunto in fondo entra subito nella vista.
    /// Nessun frame deve disegnare il fuoco né l'etichetta, nemmeno quello subito dopo la
    /// ricarica (Rune riporta le righe visibili nuove solo dopo il layout).
    #[test]
    fn a_block_appended_in_view_never_draws_the_fire() {
        let blocks: Vec<String> = (0..3).map(|n| format!("Breve {n}")).collect();
        let mut rt = open(&join(&blocks));
        assert!(rt.state.tab().unwrap().visible_last != 0);
        let mut more = blocks.clone();
        more.push("Nuovo in fondo, già in vista".to_owned());
        let src = join(&more);
        rt.update(|s, _| {
            let change = s.tab_mut().unwrap().doc.update(&src);
            s.tab_mut().unwrap().note_reload(&change, 1.0);
        });
        for f in 0..30 {
            assert!(!fire_drawn(&rt), "fuoco disegnato al frame {f}");
            assert_eq!(rt.state.tab().unwrap().fire_since, None, "fuoco acceso al frame {f}");
            rt.frame(1.0 / 60.0);
        }
        assert_eq!(rt.state.tab().unwrap().below.count(), 0);
    }

    /// Un blocco cambiato sotto la vista (stessa chiave, niente righe nuove) accende il fuoco.
    #[test]
    fn a_block_changed_below_the_view_lights_the_fire() {
        let mut rt = open(&join(&sample()));
        let mut blocks = sample();
        let last = blocks.len() - 1;
        blocks[last] = format!("{} cambiato", blocks[last]);
        let src = join(&blocks);
        rt.update(|s, _| {
            let change = s.tab_mut().unwrap().doc.update(&src);
            assert_eq!(change.changed.len(), 1);
            s.tab_mut().unwrap().note_reload(&change, 1.0);
        });
        settle(&mut rt);
        assert_eq!(rt.state.tab().unwrap().below.count(), 1);
        assert_eq!(rt.state.tab().unwrap().fire_since, Some(1.0));
        assert!(fire_drawn(&rt));
    }

    /// I rect delle righe del documento (chiave → rect), in ordine.
    fn row_rects(rt: &Runtime<Doc>) -> Vec<(String, [f32; 4])> {
        let doc = rt.state.active_document();
        let keys: Vec<String> = (0..doc.blocks.len()).map(|i| row_key(i, doc)).collect();
        rt.elements().iter().filter(|e| !e.exiting).filter_map(|e| { let k = e.key.as_deref()?; keys.iter().any(|x| x == k).then(|| (k.to_owned(), e.rect)) }).collect()
    }

    fn keyed(rt: &Runtime<Doc>, key: &str) -> Vec<[f32; 4]> {
        rt.elements().iter().filter(|e| !e.exiting && e.key.as_deref() == Some(key)).map(|e| e.rect).collect()
    }

    /// Dither e segno accesi su un file che finisce con una fence aperta: le righe restano dove
    /// erano, la barra sta sotto l'ultima riga, alta una riga e larga come il testo, e il segno
    /// sta nel margine sinistro della riga segnata.
    #[test]
    fn dither_and_changed_mark_do_not_move_the_text() {
        let src = "# Titolo\n\nUn paragrafo.\n\n```rs\nfn main() {\n    let x = 1;";
        let mut rt = open(src);
        let before = row_rects(&rt);
        assert_eq!(before.len(), 3);
        assert!(keyed(&rt, "arriving").is_empty() && keyed(&rt, "changed").is_empty());
        let marked = rt.state.active_document().blocks[1].id;
        rt.update(|s, _| {
            s.tab_mut().unwrap().dither_since = Some(0.0);
            s.tab_mut().unwrap().changed.mark(&[marked], std::time::Instant::now());
        });
        settle(&mut rt);
        assert_eq!(row_rects(&rt), before, "niente sopra si sposta");
        let bar = keyed(&rt, "arriving");
        assert_eq!(bar.len(), 1, "la barra del dither");
        let last = before[2].1;
        assert!(bar[0][1] >= last[1] + last[3] - 0.5, "sotto l'ultima riga: {bar:?} {last:?}");
        let text_w = last[2] - 2.0 * crate::ui::theme::ROW_PAD_X;
        assert!((bar[0][2] - 2.0 * crate::ui::theme::ROW_PAD_X - text_w).abs() < 0.5, "larga come il testo");
        let mark = keyed(&rt, "changed");
        assert_eq!(mark.len(), 1, "un solo blocco segnato");
        let row = before[1].1;
        assert_eq!(mark[0][2], crate::ui::theme::CHANGED_MARK_W);
        assert!(mark[0][0] > row[0] && mark[0][0] + mark[0][2] < row[0] + crate::ui::theme::ROW_PAD_X, "nel margine sinistro");
        assert!(mark[0][1] > row[1] && mark[0][1] + mark[0][3] < row[1] + row[3], "dentro la riga");
    }

    #[test]
    fn fire_label() {
        assert_eq!(super::fire_label(1), "1 paragrafo nuovo più in basso");
        assert_eq!(super::fire_label(4), "4 paragrafi nuovi più in basso");
    }

    // --- selezione e copia ---

    use rune::{Key, KeyEvent, Mods};

    fn cmd(rt: &mut Runtime<Doc>, c: char) {
        rt.key(KeyEvent { key: Key::Command(c), mods: Mods { cmd: true, logo: true, ..Mods::default() } });
        rt.frame(1.0 / 60.0);
    }

    // --- schede ---

    /// Due schede sullo stesso campione: B scorre a metà, si passa ad A (Cmd+1) e si torna a B
    /// (Ctrl+Tab). A parte dall'inizio, B torna dove era: il blocco che stava in cima torna in
    /// cima (`App::restore_view`), senza la parte tagliata dal bordo.
    #[test]
    fn each_tab_keeps_its_place() {
        let mut rt = open(&join(&sample()));
        rt.update(|s, _| s.push_tab(Document::from_source(Path::new("b.md"), &join(&sample()))));
        settle(&mut rt);
        scroll_to(&mut rt, 6000.0);
        let b = anchor(&rt);
        cmd(&mut rt, '1');
        assert_eq!(rt.state.active, 0);
        assert_eq!(anchor(&rt).1.round(), 24.0, "A dall'inizio, dal primo frame");
        settle(&mut rt);
        rt.key(KeyEvent { key: Key::Tab, mods: Mods { ctrl: true, ..Mods::default() } });
        // Il primo frame dopo il cambio è già giusto, e così i seguenti: nessun salto.
        for f in 0..10 {
            rt.frame(1.0 / 60.0);
            assert_eq!(rt.state.active, 1, "Ctrl+Tab");
            let back = anchor(&rt);
            if f == 0 {
                eprintln!("scheda B: prima {b:?}, primo frame dopo {back:?}");
            }
            assert_eq!(back.0, b.0, "frame {f}: B riparte dal suo primo blocco visibile");
            assert!(back.1.abs() <= 1.0, "frame {f}: in cima alla vista: {back:?}");
        }
    }

    /// Il fuoco di una scheda in secondo piano risale (420 ms) quando la scheda torna davanti:
    /// il suo orologio riparte dal cambio, non dalla ricarica di allora.
    #[test]
    fn a_background_fire_rises_again_when_its_tab_comes_back() {
        let mut rt = open(&join(&sample()));
        rt.update(|s, _| s.push_tab(Document::from_source(Path::new("b.md"), "# b\n")));
        settle(&mut rt);
        rt.update(|s, _| s.tabs[0].fire_since = Some(0.0));
        for _ in 0..120 {
            rt.frame(1.0 / 60.0);
        }
        cmd(&mut rt, '1');
        let since = rt.state.tab().unwrap().fire_since.expect("fuoco acceso");
        assert!(since > 1.5, "orologio del cambio, non della ricarica: {since}");
    }

    /// Le schede sono larghe quanto il loro nome, con `ROW_PAD` ai lati (mockup `scheda`): un
    /// nome lungo non si taglia mentre la barra ha spazio.
    #[test]
    fn tabs_are_as_wide_as_their_names() {
        let mut rt = open("# a\n");
        rt.update(|s, _| s.push_tab(Document::from_source(Path::new("un-nome-di-file-molto-lungo.md"), "# b\n")));
        settle(&mut rt);
        let els = rt.elements();
        let rect = |key: &str| els.iter().find(|e| e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("{key}")).rect;
        let ids: Vec<u64> = rt.state.tabs.iter().map(|t| t.doc.id).collect();
        let (a, b) = (rect(&format!("t-{:x}", ids[0])), rect(&format!("t-{:x}", ids[1])));
        let (la, lb) = (rect(&format!("t-{:x}-label", ids[0])), rect(&format!("t-{:x}-label", ids[1])));
        let pad = crate::ui::theme::ROW_PAD;
        for (tab, label) in [(a, la), (b, lb)] {
            assert!((label[0] - tab[0] - pad).abs() <= 0.5, "testo a {pad} dal bordo: {tab:?} {label:?}");
            assert!((tab[0] + tab[2] - label[0] - label[2] - pad).abs() <= 0.5, "e a {pad} dalla fine: {tab:?} {label:?}");
        }
        assert!(b[2] > a[2] + 100.0, "larghe quanto il nome: {a:?} {b:?}");
        assert_eq!(a[3], crate::ui::theme::ROW);
    }

    /// La barra mostra una scheda per file, quella davanti con riempimento `sel`, e "⌘E
    /// sorgente"; Cmd+W chiude la scheda davanti, e senza schede lascia il tasto a Rune.
    #[test]
    fn the_tab_bar_shows_the_files_and_cmd_w_closes() {
        let mut rt = open("# a\n");
        rt.update(|s, _| s.push_tab(Document::from_source(Path::new("b.md"), "# b\n")));
        settle(&mut rt);
        let ids: Vec<String> = rt.state.tabs.iter().map(|t| format!("t-{:x}", t.doc.id)).collect();
        let els = rt.elements();
        for id in &ids {
            assert!(els.iter().any(|e| e.key.as_deref() == Some(id.as_str())), "scheda {id}");
        }
        assert!(els.iter().any(|e| e.key.as_deref() == Some("source-key")), "⌘E sorgente");
        assert!(els.iter().any(|e| e.text.as_deref() == Some(crate::ui::tabs::SOURCE_KEY)));
        cmd(&mut rt, 'w');
        settle(&mut rt);
        assert_eq!(rt.state.tabs.len(), 1);
        cmd(&mut rt, 'w');
        settle(&mut rt);
        assert!(rt.state.tabs.is_empty(), "nessuna scheda, nessun panic");
        let mut tx = rune::Tx::default();
        let key = KeyEvent { key: Key::Command('w'), mods: Mods { cmd: true, logo: true, ..Mods::default() } };
        assert!(!crate::on_key(&mut rt.state, &key, &mut tx), "Cmd+W senza schede resta a Rune");
    }

    /// Cmd+A poi Cmd+C su un runtime con clipboard di prova; ritorna il testo copiato.
    fn copy_all(rt: &mut Runtime<Doc>) -> String {
        let clipboard = rune::clipboard::memory();
        rt.use_clipboard(&clipboard);
        // un clic dentro il documento dà il focus alla selezione, come nell'app
        let doc = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("doc")).unwrap();
        let (x, y) = (doc.rect[0] + doc.rect[2] / 2.0, doc.rect[1] + 12.0);
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        cmd(rt, 'a');
        cmd(rt, 'c');
        clipboard.text().map(|t| t.to_string()).unwrap_or_default()
    }

    #[test]
    fn copy_all_gives_the_markdown_source() {
        let src = join(&sample());
        let mut rt = open(&src);
        // il campione è lungo molte schermate: la maggior parte delle righe non è in layout
        assert!(rt.elements().iter().filter(|e| e.path.contains("rows/")).count() < 120 * 4);
        let copied = copy_all(&mut rt);
        assert_eq!(copied.trim_end(), src.trim_end());
    }

    #[test]
    fn copy_all_after_scrolling_gives_the_markdown_source() {
        let src = join(&sample());
        let mut rt = open(&src);
        scroll_to(&mut rt, 6000.0);
        let copied = copy_all(&mut rt);
        assert_eq!(copied.trim_end(), src.trim_end());
    }

    #[test]
    fn copy_all_of_the_big_fixture_gives_the_file() {
        let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/big.md")).unwrap();
        let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/big.md"));
        let mut rt = open_doc(Document::load(path).unwrap());
        let blocks = rt.state.active_document().blocks.len();
        let copied = copy_all(&mut rt);
        eprintln!("big.md: {blocks} blocchi, {} byte copiati su {}", copied.len(), src.len());
        assert_eq!(copied.trim_end(), src.trim_end());
    }

    #[test]
    fn copy_all_of_a_file_over_4_mib_gives_the_file() {
        let dir = std::env::temp_dir().join(format!("runemd-copy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("huge.md");
        let src = join(&sample()).repeat(5 * 1024 * 1024 / join(&sample()).len() + 1);
        assert!(src.len() > 4 * 1024 * 1024);
        std::fs::write(&path, &src).unwrap();
        let document = Document::load(&path).unwrap();
        let mut rt = open_doc(document);
        let copied = copy_all(&mut rt);
        std::fs::remove_dir_all(&dir).ok();
        eprintln!("huge: {} byte copiati su {}", copied.len(), src.len());
        assert_eq!(copied.trim_end(), src.trim_end());
    }

    /// Trascina dal centro della riga `from` al centro della riga `to` e copia con Cmd+C.
    fn drag_copy(rt: &mut Runtime<Doc>, from: usize, to: usize) -> String {
        let clipboard = rune::clipboard::memory();
        rt.use_clipboard(&clipboard);
        let doc = rt.state.active_document();
        let rect = |i: usize| {
            let key = row_key(i, doc);
            rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key.as_str())).unwrap().rect
        };
        let (a, c) = (rect(from), rect(to));
        let start = (a[0] + 60.0, a[1] + a[3] / 2.0);
        let end = (c[0] + 120.0, c[1] + c[3] / 2.0);
        rt.pointer_move(start.0, start.1);
        rt.pointer_down(start.0, start.1);
        rt.frame(1.0 / 60.0);
        for k in 1..=10 {
            let t = k as f32 / 10.0;
            rt.pointer_move(start.0 + (end.0 - start.0) * t, start.1 + (end.1 - start.1) * t);
            rt.frame(1.0 / 60.0);
        }
        rt.pointer_up(end.0, end.1);
        rt.frame(1.0 / 60.0);
        cmd(rt, 'c');
        clipboard.text().map(|t| t.to_string()).unwrap_or_default()
    }

    #[test]
    fn copy_of_a_drag_over_three_blocks_gives_those_three() {
        let mut rt = open(&join(&sample()));
        let blocks = source(&rt);
        assert_eq!(drag_copy(&mut rt, 2, 4), blocks[2..=4].join("\n\n"));
    }

    #[test]
    fn copy_of_a_drag_from_block_0_on_a_file_over_4_mib_gives_two_blocks() {
        let dir = std::env::temp_dir().join(format!("runemd-drag-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("huge.md");
        let one = join(&sample());
        std::fs::write(&path, one.repeat(5 * 1024 * 1024 / one.len() + 1)).unwrap();
        let mut rt = open_doc(Document::load(&path).unwrap());
        let blocks = source(&rt);
        let copied = drag_copy(&mut rt, 0, 1);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(copied, blocks[0..=1].join("\n\n"));
    }

    #[test]
    fn copy_all_of_a_cjk_file_over_4_mib_with_textless_blocks_gives_the_file() {
        let dir = std::env::temp_dir().join(format!("runemd-cjk-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("cjk.md");
        let unit: String = (0..40).map(|n| format!("段落{n}は日本語のテキストです。読み取りとスクロールの確認に使います。\n\n---\n\n")).collect();
        let src = unit.repeat(5 * 1024 * 1024 / unit.len() + 1);
        std::fs::write(&path, &src).unwrap();
        let mut rt = open_doc(Document::load(&path).unwrap());
        let copied = copy_all(&mut rt);
        std::fs::remove_dir_all(&dir).ok();
        eprintln!("cjk: {} byte copiati su {}", copied.len(), src.len());
        assert_eq!(copied.trim_end(), src.trim_end());
    }

    /// Cmd+A, poi un agente riscrive il file (ricarica), poi Cmd+C: si copia il file nuovo.
    #[test]
    fn copy_after_cmd_a_and_a_reload_gives_the_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("live.md");
        let before = join(&sample());
        std::fs::write(&path, &before).unwrap();
        let mut rt = open_doc(Document::load(&path).unwrap());
        let clipboard = rune::clipboard::memory();
        rt.use_clipboard(&clipboard);
        let doc = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("doc")).unwrap();
        let (x, y) = (doc.rect[0] + doc.rect[2] / 2.0, doc.rect[1] + 12.0);
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        cmd(&mut rt, 'a');
        let after = format!("Nuovo in testa: {}\n\n{before}Nuovo in fondo: {}\n", paragraph(3), paragraph(4));
        std::fs::write(&path, &after).unwrap();
        rt.update(|s, _| {
            let own = crate::live::reload::OwnWrites::default();
            let doc = &mut s.tab_mut().unwrap().doc;
            let read = crate::live::reload::read(&path, &doc.snapshot()).unwrap();
            assert!(matches!(crate::live::reload::commit(doc, read, &own), crate::live::reload::Commit::Applied(_)));
        });
        settle(&mut rt);
        cmd(&mut rt, 'c');
        let copied = clipboard.text().map(|t| t.to_string()).unwrap_or_default();
        assert_eq!(copied.trim_end(), after.trim_end(), "il file nuovo, non il vecchio né la selezione");
    }

    /// Cmd+A, scroll in fondo, Shift+clic nel penultimo blocco (pressione e rilascio nello
    /// stesso frame, come un clic veloce: la selezione non arriva mai `dragging`), Cmd+C.
    fn cmd_a_then_shift_click_second_to_last(rt: &mut Runtime<Doc>) -> String {
        let n = rt.state.active_document().blocks.len();
        let clipboard = rune::clipboard::memory();
        rt.use_clipboard(&clipboard);
        let doc = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("doc")).unwrap();
        let (x, y) = (doc.rect[0] + doc.rect[2] / 2.0, doc.rect[1] + 12.0);
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        cmd(rt, 'a');
        scroll_to(rt, 1.0e9);
        let key = row_key(n - 2, rt.state.active_document());
        let r = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key.as_str())).expect("penultimo blocco in vista").rect;
        let (x, y) = (r[0] + 80.0, r[1] + r[3] / 2.0);
        rt.physical_input(rune::PhysicalInput::Modifiers(Mods { shift: true, ..Mods::default() }));
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        rt.physical_input(rune::PhysicalInput::Modifiers(Mods::default()));
        cmd(rt, 'c');
        clipboard.text().map(|t| t.to_string()).unwrap_or_default()
    }

    /// Il taglio toglie meno dello 0,1 % del file, ma non è più "tutto": Cmd+C copia i blocchi
    /// fino al penultimo.
    #[test]
    fn a_shift_trim_after_cmd_a_copies_the_trimmed_blocks_not_the_file() {
        let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/big.md"));
        let src = std::fs::read_to_string(path).unwrap();
        let mut rt = open_doc(Document::load(path).unwrap());
        let blocks = source(&rt);
        let n = blocks.len();
        let trimmed = blocks[n - 2].len() + blocks[n - 1].len();
        assert!(trimmed * 1000 < src.len(), "il taglio sta nella vecchia tolleranza: {trimmed} su {}", src.len());
        let copied = cmd_a_then_shift_click_second_to_last(&mut rt);
        assert_ne!(copied.trim_end(), src.trim_end(), "copiato tutto il file dopo il taglio");
        assert_eq!(copied, blocks[..=n - 2].join("\n\n"));
    }

    /// Caso residuo, fissato com'è: oltre il tetto di copia di Rune l'ultimo capo non si
    /// confronta, quindi un taglio in fondo dopo Cmd+A resta "tutto" e copia il file intero.
    #[test]
    fn a_shift_trim_after_cmd_a_over_the_copy_cap_still_copies_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("huge.md");
        let src = join(&sample()).repeat(5 * 1024 * 1024 / join(&sample()).len() + 1);
        std::fs::write(&path, &src).unwrap();
        let mut rt = open_doc(Document::load(&path).unwrap());
        let copied = cmd_a_then_shift_click_second_to_last(&mut rt);
        assert_eq!(copied.trim_end(), src.trim_end(), "comportamento noto: tutto il file");
    }

    #[test]
    fn a_drag_after_cmd_a_copies_the_dragged_blocks_not_the_file() {
        let mut rt = open(&join(&sample()));
        let blocks = source(&rt);
        cmd(&mut rt, 'a');
        assert_eq!(drag_copy(&mut rt, 2, 4), blocks[2..=4].join("\n\n"));
    }

    #[test]
    fn a_drag_after_a_cmd_a_that_selected_nothing_copies_the_dragged_blocks() {
        let mut rt = open(&join(&sample()));
        let blocks = source(&rt);
        // Cmd+Shift+A è un tasto dell'app: Rune non seleziona, la vista non deve restare in attesa.
        rt.key(KeyEvent { key: Key::Command('a'), mods: Mods { cmd: true, logo: true, shift: true, ..Mods::default() } });
        rt.frame(1.0 / 60.0);
        assert_eq!(drag_copy(&mut rt, 2, 3), blocks[2..=3].join("\n\n"));
    }

    /// Il lettore con lo stile vero dell'app (`markdown_style::style`), nel tema `dark`.
    fn open_themed(source: &str, dark: bool) -> Runtime<Doc> {
        let mut state = Doc::bare(crate::index::Library::load(Vec::new(), None));
        state.set_style(crate::ui::markdown_style::style(&crate::ui::theme::Theme::for_mode(dark), crate::ui::theme::BODY_TEXT));
        state.push_tab(Document::from_source(Path::new("t.md"), source));
        let mut rt = Runtime::new(state, crate::view, VIEWPORT, Env::new(dark, 2.0));
        rt.frame(0.0);
        settle(&mut rt);
        rt
    }

    fn figures(rt: &Runtime<Doc>) -> Vec<String> {
        rt.elements().into_iter().filter(|e| e.role == "figure").filter_map(|e| e.label).collect()
    }

    fn has_source_block(rt: &Runtime<Doc>) -> bool {
        rt.elements().iter().any(|e| e.key.as_deref() == Some("source"))
    }

    /// I blocchi ```mermaid di `fixtures/mermaid.md` si disegnano (un flowchart e un
    /// sequence, in tutti e due i temi); quello non valido resta un blocco di codice.
    #[test]
    fn mermaid_fences_draw_as_diagrams_and_an_unknown_one_stays_code() {
        let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/mermaid.md")).unwrap();
        for dark in [true, false] {
            let rt = open_themed(&src, dark);
            assert_eq!(figures(&rt), ["Flowchart", "Sequence diagram"], "dark={dark}");
            assert!(has_source_block(&rt), "il blocco non valido resta codice, dark={dark}");
        }
    }

    /// Sorgenti vuote, spezzate o enormi non vanno in panic e non bloccano la vista: o si
    /// disegnano, o restano il blocco di codice com'erano.
    #[test]
    fn broken_and_huge_mermaid_never_panic() {
        let huge_graph = format!("flowchart TD\n{}", (0..3000).map(|i| format!("  N{i} --> N{}\n", i + 1)).collect::<String>());
        let big_but_drawable = format!("flowchart TD\n{}", (0..70).map(|i| format!("  N{i} --> N{}\n  N{i} --> M{i}\n", i + 1)).collect::<String>());
        let sources = [
            String::new(),
            big_but_drawable,
            "graph".to_owned(),
            "flowchart TD\n  A -->".to_owned(),
            "sequenceDiagram\n  A->>".to_owned(),
            "pie\n  \"a\" : x".to_owned(),
            "flowchart LR\n  A[\n".to_owned(),
            "\u{0}\u{1f600}".to_owned(),
            huge_graph,
        ];
        for body in sources {
            let doc = format!("# t\n\n```mermaid\n{body}\n```\n\nfine\n");
            let t = std::time::Instant::now();
            let rt = open_themed(&doc, true);
            let drawn = !figures(&rt).is_empty();
            assert!(drawn || has_source_block(&rt), "né disegno né codice per {:?}", body.chars().take(30).collect::<String>());
            println!("mermaid {:>6} byte: {} in {:?}", body.len(), if drawn { "disegnato" } else { "codice" }, t.elapsed());
        }
    }

    /// Due schede con lo stesso diagramma non si pestano: ognuna ha il suo, e cambiando scheda
    /// il diagramma resta quello della scheda.
    #[test]
    fn the_same_diagram_in_two_tabs_draws_in_each() {
        let one = "```mermaid\nflowchart LR\n  A --> B\n```\n";
        let mut rt = open_themed(one, true);
        rt.update(|s, _| s.push_tab(Document::from_source(Path::new("b.md"), one)));
        settle(&mut rt);
        assert_eq!(figures(&rt), ["Flowchart"]);
        cmd(&mut rt, 'w');
        settle(&mut rt);
        assert_eq!(figures(&rt), ["Flowchart"]);
    }
}
