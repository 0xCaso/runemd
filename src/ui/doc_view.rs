//! La colonna del documento: un blocco top-level per riga di `virtual_rows` dentro uno scroll.
//!
//! Le righe hanno la chiave dell'id del blocco (task 5), non dell'indice: quando il file cambia,
//! Rune tiene ferma la riga in cima alla vista per chiave (`virtual_rows`: "inserting or
//! prepending rows keeps the reader's row in place"); se quel blocco sparisce, l'ancora passa al
//! primo blocco successivo che esiste ancora (`VirtualList::set_keys` → `first_visible`).

use std::borrow::Cow;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use rune::{El, GrowthSpec, ScrollSpec, TextSelection, col, el};
use rune_kit::MarkdownStyle;

use crate::doc::document::Document;
use crate::doc::ids;

const COLUMN: f32 = 760.0;

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
    let rows = col().key("rows").w_full().max_w(COLUMN).self_center().selection_group(true)
        .growth(GrowthSpec::default())
        .selection_copy(move |sel| copy_source(sel, generation, &file, &blocks))
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

/// Ciò che copiano Cmd+C e il menu Copia: il Markdown sorgente dei blocchi toccati dalla
/// selezione, uniti da una riga vuota, non il testo disegnato. Le righe fuori vista non hanno
/// testo in layout, ma Rune le ricostruisce per la selezione e `sel.texts()` le contiene.
///
/// Tutto selezionato (Cmd+A) copia il file letto dal disco: i blocchi non conservano le righe
/// vuote in più tra uno e l'altro. Rune ricostruisce al più 10 000 righe e 4 MiB di testo per
/// copia: se la selezione parte dal primo blocco e ha toccato uno dei due massimi, è arrivata
/// troncata da un Cmd+A e vale come tutto selezionato.
fn copy_source(sel: &TextSelection, generation: u64, file: &File, blocks: &[(u64, Arc<str>)]) -> Option<String> {
    let picked: HashSet<u64> = sel.texts().iter().filter(|t| !t.range().is_empty()).filter_map(|t| row_id(t.path(), generation)).collect();
    let index: Vec<usize> = blocks.iter().enumerate().filter(|(_, (id, _))| picked.contains(id)).map(|(i, _)| i).collect();
    let (&first, &last) = (index.first()?, index.last()?);
    // Prova che Rune ha troncato: ha ricostruito il massimo di righe o di testo. Una selezione
    // corta, anche da un file enorme, non li raggiunge.
    let truncated = picked.len() >= COPY_ROWS || sel.text().len() >= COPY_LIMIT;
    if first == 0
        && (last + 1 == blocks.len() || truncated)
        && let Some(text) = file.read()
    {
        return Some(text);
    }
    let end = if first == 0 && truncated { blocks.len() } else { last + 1 };
    let sources: Vec<&str> = blocks[first..end].iter().map(|(_, s)| &**s).collect();
    Some(sources.join("\n\n"))
}

/// I limiti di Rune per una copia sulle righe virtuali (`MAX_VIRTUAL_COPY_*`, non esportati).
const COPY_LIMIT: usize = 4 * 1024 * 1024;
const COPY_ROWS: usize = 10_000;

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
        let document = Document::from_source(Path::new("t.md"), source);
        let state = Doc { document: Some(document), style: MarkdownStyle::neutral(true), scroll: None, generation: 1 };
        let mut rt = Runtime::new(state, crate::view, VIEWPORT, Env::new(true, 2.0));
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
        let state = Doc { document: Some(Document::load(path).unwrap()), style: MarkdownStyle::neutral(true), scroll: None, generation: 1 };
        let mut rt = Runtime::new(state, crate::view, VIEWPORT, Env::new(true, 2.0));
        rt.frame(0.0);
        settle(&mut rt);
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
        let state = Doc { document: Some(document), style: MarkdownStyle::neutral(true), scroll: None, generation: 1 };
        let mut rt = Runtime::new(state, crate::view, VIEWPORT, Env::new(true, 2.0));
        rt.frame(0.0);
        settle(&mut rt);
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
        let state = Doc { document: Some(Document::load(&path).unwrap()), style: MarkdownStyle::neutral(true), scroll: None, generation: 1 };
        let mut rt = Runtime::new(state, crate::view, VIEWPORT, Env::new(true, 2.0));
        rt.frame(0.0);
        settle(&mut rt);
        let blocks = source(&rt);
        let copied = drag_copy(&mut rt, 0, 1);
        std::fs::remove_dir_all(&dir).ok();
        assert_eq!(copied, blocks[0..=1].join("\n\n"));
    }
}
