//! La colonna del documento: un blocco top-level per riga di `virtual_rows` dentro uno scroll.
//!
//! Le righe hanno la chiave dell'id del blocco (task 5), non dell'indice: quando il file cambia,
//! Rune tiene ferma la riga in cima alla vista per chiave (`virtual_rows`: "inserting or
//! prepending rows keeps the reader's row in place"); se quel blocco sparisce, l'ancora passa al
//! primo blocco successivo che esiste ancora (`VirtualList::set_keys` → `first_visible`).

use std::borrow::Cow;
use std::collections::HashSet;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};

use rune::{El, GrowthSpec, Key, KeyEvent, ScrollSpec, SelectedText, TextSelection, col, el};
use rune_kit::MarkdownStyle;

use crate::doc::document::Document;
use crate::doc::ids;

use super::theme::COLUMN;

/// Chiave della riga `i`: generazione del file aperto + id del blocco. Gli id ripartono da 1 a
/// ogni `Document`, la generazione no: senza, un file nuovo erediterebbe le altezze misurate
/// del precedente.
pub fn row_key(generation: u64, i: usize, doc: &Document) -> String {
    format!("{generation:x}-{:x}", doc.blocks[i].id)
}

/// Ciò che dice a Rune se le chiavi delle righe sono cambiate: un altro file aperto
/// (`generation`, anche se è lo stesso file invariato), un update (`revision`), un altro
/// contenuto a parità di revisione (`file_hash`).
pub fn rows_version(generation: u64, doc: &Document) -> (u64, u64, u64) {
    (generation, doc.revision, doc.file_hash)
}

/// La vista scorrevole del documento `doc_of(s)`, con le chiavi di `generation`. Le closure di
/// `virtual_rows` non possono tenere prestiti: a layout ricevono lo stato della finestra e
/// ritrovano il documento con lo stesso `doc_of`, così numero di righe, versione, chiavi e
/// contenuto vengono tutti dallo stesso documento.
pub fn view<S: 'static>(s: &S, doc_of: fn(&S) -> &Document, generation: u64, style: &MarkdownStyle) -> El {
    let doc = doc_of(s);
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
            move |s: &S, i| Cow::Owned(row_key(generation, i, doc_of(s))),
            move |s: &S, i| block_row(&doc_of(s).blocks[i].source, &style),
        )
        .virtual_rows_version(rows_version(generation, doc));
    let page = col().w_full().child(el().h(24.0)).child(rows).child(el().h(48.0));
    col().key("doc").w_full().h_full().scroll(ScrollSpec { stick_bottom: false }).child(page)
}

/// Se la selezione corrente viene da un Cmd+A (`Document::select_all`, condiviso con le closure
/// della vista).
#[derive(Debug, Default)]
pub struct SelectAllFlag(Mutex<SelectAll>);

#[derive(Debug, Default, Clone, PartialEq, Eq)]
enum SelectAll {
    #[default]
    No,
    /// Cmd+A premuto, la selezione che ne segue non è ancora arrivata.
    Pending,
    /// La selezione fatta dal Cmd+A, con le righe ai suoi capi.
    All(Ends),
}

/// I capi di una selezione: per la prima e l'ultima riga toccate, l'id del blocco e i byte
/// selezionati del suo testo; e se la selezione è arrivata troncata al tetto di copia di Rune
/// (allora l'ultima riga non è la fine).
#[derive(Debug, Clone, PartialEq, Eq)]
struct Ends {
    first: End,
    last: End,
    capped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct End {
    row: u64,
    range: Range<usize>,
    whole: bool,
}

impl End {
    /// Lo stesso capo di `was`: stessa riga, e il testo intero o selezionato come allora (un
    /// ricaricamento può allungare il blocco sotto una selezione che non è cambiata).
    fn holds(&self, was: &End) -> bool {
        self.row == was.row && (self.whole || self.range == was.range)
    }
}

/// I tetti della selezione ricostruita fuori vista (rune `MAX_VIRTUAL_COPY_BYTES` e
/// `MAX_VIRTUAL_COPY_ROWS`, `pub(crate)` in Rune): oltre, `TextSelection` si ferma prima della
/// fine. Il tetto delle righe conta anche le righe senza testo (`---`).
const COPY_CAP_BYTES: usize = 4 * 1024 * 1024;
const COPY_CAP_ROWS: usize = 10_000;
/// Il testo troncato può fermarsi poco prima del tetto (righe vuote e prefisso tra due pezzi).
const COPY_CAP_SLACK: usize = 1024;

impl SelectAllFlag {
    fn get(&self) -> SelectAll {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).clone()
    }

    fn set(&self, next: SelectAll) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = next;
    }
}

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
        let text = String::from_utf8_lossy(&std::fs::read(&self.path).ok()?).into_owned();
        (ids::hash_str(&text) == self.hash).then_some(text)
    }
}

/// L'id del blocco nel percorso di un testo: il segmento `<generazione>-<id>` della sua riga.
fn row_id(path: &str, generation: u64) -> Option<u64> {
    let prefix = format!("{generation:x}-");
    path.split('/').find_map(|seg| u64::from_str_radix(seg.strip_prefix(&prefix)?, 16).ok())
}

fn block_row(source: &str, style: &MarkdownStyle) -> El {
    // `selectable` fa di ogni blocco un selection_group a sé, che fermerebbe un trascinamento
    // al bordo del blocco: si toglie il flag e vale il gruppo unico sulle righe.
    let body = rune_kit::Markdown::new(source).selectable(true).render(style).selection_group(false);
    // Le righe virtuali vogliono padding interno, non gap tra righe.
    col().w_full().px(28.0).py(7.0).child(body)
}

#[cfg(test)]
mod tests {
    //! L'ancora di lettura su un runtime Rune headless: stessa vista dell'app (`crate::view`),
    //! `Document::update` come lo chiamerà il ricaricamento.

    use std::path::Path;

    use rune::{Env, Runtime};
    use rune_kit::MarkdownStyle;

    use super::row_key;
    use crate::Doc;
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
        let state = Doc { document: Some(document), style: MarkdownStyle::neutral(true), scroll: None, generation: 1, live: crate::live::Live::default() };
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
        let doc = s.document.as_ref().unwrap();
        let (key, y) = els.iter()
            .filter(|e| !e.exiting && e.rect[1] + e.rect[3] > top + 0.5)
            .filter_map(|e| { let k = e.key.as_deref()?; e.path.ends_with(&format!("rows/{k}")).then_some((k, e.rect[1])) })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .expect("nessuna riga visibile");
        let i = (0..doc.blocks.len()).find(|&i| row_key(s.generation, i, doc) == key).expect("chiave senza blocco");
        (doc.blocks[i].id, y - top)
    }

    fn source(rt: &Runtime<Doc>) -> Vec<String> {
        rt.state.document.as_ref().unwrap().blocks.iter().map(|b| b.source.to_string()).collect()
    }

    fn index_of(rt: &Runtime<Doc>, id: u64) -> usize {
        rt.state.document.as_ref().unwrap().blocks.iter().position(|b| b.id == id).unwrap()
    }

    fn height(rt: &Runtime<Doc>, id: u64) -> f32 {
        let key = row_key(rt.state.generation, index_of(rt, id), rt.state.document.as_ref().unwrap());
        rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key.as_str())).unwrap().rect[3]
    }

    fn edit(rt: &mut Runtime<Doc>, blocks: &[String]) {
        let src = join(blocks);
        rt.update(|s, _| { s.document.as_mut().unwrap().update(&src); });
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
        assert_eq!(rt.state.document.as_ref().unwrap().blocks[i].id, before.0, "modifica sul posto: stesso id");
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
        let next = rt.state.document.as_ref().unwrap().blocks[i + 1].id;
        let rows = rt.elements();
        let top = rows.iter().find(|e| e.key.as_deref() == Some("doc")).unwrap().rect[1];
        let next_key = row_key(rt.state.generation, i + 1, rt.state.document.as_ref().unwrap());
        let next_y = rows.iter().find(|e| e.key.as_deref() == Some(next_key.as_str())).unwrap().rect[1] - top;
        blocks.splice(i..=i, [format!("Diviso A {}", paragraph(5)), format!("Diviso B {}", paragraph(6))]);
        edit(&mut rt, &blocks);
        let doc = rt.state.document.as_ref().unwrap();
        assert!(doc.blocks.iter().all(|b| b.id != before.0), "il blocco in cima non c'è più");
        let j = index_of(&rt, next);
        let key = row_key(rt.state.generation, j, doc);
        let els = rt.elements();
        let top = els.iter().find(|e| e.key.as_deref() == Some("doc")).unwrap().rect[1];
        let y = els.iter().find(|e| e.key.as_deref() == Some(key.as_str())).unwrap().rect[1] - top;
        eprintln!("blocco sparito: seguente prima {next_y:.2}, dopo {y:.2}");
        assert!((y - next_y).abs() <= 1.0, "il blocco seguente si sposta: {next_y} → {y}");
    }

    #[test]
    fn an_empty_file_opens() {
        let mut rt = open("");
        assert!(rt.state.document.as_ref().unwrap().blocks.is_empty());
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
        assert_eq!(rt.state.document.as_ref().unwrap().blocks.len(), 1);
        scroll_to(&mut rt, 20000.0);
        let before = anchor(&rt);
        assert!(before.1 < -1000.0, "dentro il blocco: {before:?}");
        let blocks: Vec<String> = (0..5).map(|n| format!("Nuovo {n}: {}", paragraph(n))).chain([big.trim_end().to_owned()]).collect();
        edit(&mut rt, &blocks);
        assert_still("500 KB, inserisci in testa", before, anchor(&rt));
    }

    // --- selezione e copia (task 7) ---

    use rune::{Key, KeyEvent, Mods};

    fn cmd(rt: &mut Runtime<Doc>, c: char) {
        rt.key(KeyEvent { key: Key::Command(c), mods: Mods { cmd: true, logo: true, ..Mods::default() } });
        rt.frame(1.0 / 60.0);
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
        let blocks = rt.state.document.as_ref().unwrap().blocks.len();
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
        let doc = rt.state.document.as_ref().unwrap();
        let rect = |i: usize| {
            let key = row_key(rt.state.generation, i, doc);
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
            assert!(crate::live::reload::apply(s.document.as_mut().unwrap(), &after, &own).is_some());
        });
        settle(&mut rt);
        cmd(&mut rt, 'c');
        let copied = clipboard.text().map(|t| t.to_string()).unwrap_or_default();
        assert_eq!(copied.trim_end(), after.trim_end(), "il file nuovo, non il vecchio né la selezione");
    }

    /// Cmd+A, poi Shift+clic nel penultimo blocco (pressione e rilascio nello stesso frame,
    /// come un clic veloce: la selezione non arriva mai `dragging`): toglie meno dello 0,1 % del
    /// file, ma non è più "tutto": Cmd+C copia i blocchi fino al penultimo.
    #[test]
    fn a_shift_trim_after_cmd_a_copies_the_trimmed_blocks_not_the_file() {
        let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/big.md"));
        let src = std::fs::read_to_string(path).unwrap();
        let mut rt = open_doc(Document::load(path).unwrap());
        let blocks = source(&rt);
        let n = blocks.len();
        let clipboard = rune::clipboard::memory();
        rt.use_clipboard(&clipboard);
        let doc = rt.elements().into_iter().find(|e| e.key.as_deref() == Some("doc")).unwrap();
        let (x, y) = (doc.rect[0] + doc.rect[2] / 2.0, doc.rect[1] + 12.0);
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        cmd(&mut rt, 'a');
        scroll_to(&mut rt, 1.0e9);
        let key = row_key(rt.state.generation, n - 2, rt.state.document.as_ref().unwrap());
        let r = rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key.as_str())).expect("penultimo blocco in vista").rect;
        let (x, y) = (r[0] + 80.0, r[1] + r[3] / 2.0);
        let trimmed = blocks[n - 2].len() + blocks[n - 1].len();
        assert!(trimmed * 1000 < src.len(), "il taglio sta nella vecchia tolleranza: {trimmed} su {}", src.len());
        rt.physical_input(rune::PhysicalInput::Modifiers(Mods { shift: true, ..Mods::default() }));
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        rt.frame(1.0 / 60.0);
        rt.physical_input(rune::PhysicalInput::Modifiers(Mods::default()));
        cmd(&mut rt, 'c');
        let copied = clipboard.text().map(|t| t.to_string()).unwrap_or_default();
        assert_ne!(copied.trim_end(), src.trim_end(), "copiato tutto il file dopo il taglio");
        assert_eq!(copied, blocks[..=n - 2].join("\n\n"));
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
}
