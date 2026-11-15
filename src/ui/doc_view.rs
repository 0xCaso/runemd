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

use rune::{Color, El, Family, GrowthSpec, Key, KeyEvent, ScrollSpec, SelectedText, TextSelection, TextStyle, col, el};
use rune_kit::MarkdownStyle;

use crate::doc::document::{Block, Document};
use crate::doc::select_all::{End, Ends, SelectAll, SelectAllFlag};

use super::theme::{CHANGED_MARK_W, CHANGED_MARK_X, R_ROW, ROW_PAD_X, ROW_PAD_Y, UI_SMALL, UI_WEIGHT};

/// Altezza dello spazio sotto l'ultima riga (il fuoco a riposo ci sta sotto).
pub(crate) const BOTTOM: f32 = 48.0;
/// Spazio sopra la prima riga (anche il sorgente parte da qui).
pub(super) const TOP: f32 = 24.0;

/// I segnali live dentro la colonna, che non spostano il testo.
pub struct Signals<S> {
    /// Se il blocco `id` ha il segno "cosa è cambiato" (a sinistra della riga, fuori dal flusso).
    pub marked: fn(&S, u64) -> bool,
    /// Colore del segno (`theme.mark`).
    pub mark: Color,
    /// Il caret "sta scrivendo", se acceso: nella riga del blocco modificato, fuori dal flusso.
    pub caret: Option<Caret>,
    /// I segni di Cerca del blocco, la cui riga ha la chiave data (`ui::find`).
    pub hits: fn(&S, &Block, &str) -> super::find::Marks,
    /// Dove sono la vista e la pagina del documento `id` (ognuna dal suo `on_layout`): servono a
    /// sapere se lo scroll è in fondo. Vero se la vista deve rigirare.
    pub edge: fn(&mut S, u64, Edge) -> bool,
    /// I colori del blocco del frontmatter YAML: testo (`fg2`) e riempimento.
    pub meta: Meta,
    /// I font del documento, per misurare dove vanno a capo le parole lunghe (`doc::breaks`).
    pub fonts: super::markdown_style::Fonts,
    /// Un clic su un link del documento (destinazione): i percorsi `runemd-open:` aprono il file.
    pub open: fn(&mut S, &str, &mut rune::Tx),
    /// La colonna del testo è larga almeno [`TWO_COLUMNS`]: i blocchi a etichetta vanno su due
    /// colonne (`label_columns`).
    pub columns: bool,
}

/// Da quanto è larga la colonna del testo (senza i margini delle righe) i blocchi a etichetta
/// vanno su due colonne (vincolo 4 del record dei registri, mockup t-mix).
pub const TWO_COLUMNS: f32 = 480.0;

/// Il caret "sta scrivendo" (record 2026-11-15-caret-al-punto-modificato): il carattere della
/// riga a cui si appoggia ha un'evidenziazione trasparente con la chiave [`caret_key`], la riga
/// la sua chiave di sempre ([`caret_row_key`]); dai loro rect dell'ultimo layout (`Cx::layout`, che lo scroll non
/// sposta) la vista sa dove metterlo, in `place`. Prima del primo layout non si vede.
pub struct Caret {
    pub at: crate::live::caret::At,
    /// `[x, y, h]` nella riga, in pt ([`caret_place`]).
    pub place: Option<[f32; 3]>,
    /// L'orologio a cui si è acceso, e il tema e "riduci movimento" del suo slot.
    pub since: f64,
    pub dark: bool,
    pub reduced: bool,
}

/// La chiave dell'evidenziazione del carattere del caret nel documento `doc`.
pub fn caret_key(doc: u64) -> String {
    format!("caret-{doc:x}")
}

/// La chiave dell'evidenziazione dell'ultimo carattere del blocco del caret (`At::end`) nel
/// documento `doc`: se il carattere del caret non ha un rect, il caret va lì.
pub fn caret_end_key(doc: u64) -> String {
    format!("caret-end-{doc:x}")
}

/// La chiave della riga del blocco `block` nel documento `doc` (quella di [`row_key`]).
pub fn caret_row_key(doc: u64, block: u64) -> String {
    format!("{doc:x}-{block:x}")
}

/// Spazio fra il carattere e il caret, pt (mockup: `margin-left: 3px` su una barra di 2).
const CARET_GAP: f32 = 2.0;
/// Altezza del caret rispetto alla riga di testo del carattere (mockup: 1,1 em su una riga di
/// circa 1,5 em).
const CARET_LINE: f32 = 0.74;

/// Dove va il caret nella riga, `[x, y, h]` in pt, dal rect `mark` del carattere e `row` della
/// riga (stessi punti): a destra del carattere, o a sinistra con `Side::Before`; alto quanto la
/// lettera, al centro della riga di testo.
pub fn caret_place(mark: [f32; 4], row: [f32; 4], side: crate::live::caret::Side) -> [f32; 3] {
    let x = match side {
        crate::live::caret::Side::After => mark[0] + mark[2] + CARET_GAP,
        crate::live::caret::Side::Before => mark[0] - CARET_GAP - crate::fx::caret::WIDTH,
    } - row[0];
    let h = (mark[3] * CARET_LINE).round();
    [x, mark[1] - row[1] + (mark[3] - h) * 0.5, h]
}

/// Dove va il caret dai rect dell'ultimo layout: del suo carattere `mark`, dell'ultimo carattere
/// del blocco `end` e della riga `row`. Senza `mark` (un carattere che non si vede) va dopo `end`,
/// alla fine del blocco cambiato (record 2026-11-15-caret-al-punto-modificato, vincolo 2).
pub fn caret_point(mark: Option<[f32; 4]>, end: Option<[f32; 4]>, row: Option<[f32; 4]>, side: crate::live::caret::Side) -> Option<[f32; 3]> {
    let row = row?;
    match mark {
        Some(m) => Some(caret_place(m, row, side)),
        None => end.map(|e| caret_place(e, row, crate::live::caret::Side::After)),
    }
}

/// Il blocco del frontmatter YAML in lettura (DESIGN.md, "Frontmatter"): monospaziato, piccolo,
/// testo `text` su `fill`, come un blocco di codice ma più quieto; e i colori dei callout.
#[derive(Clone, Copy, Debug)]
pub struct Meta {
    pub text: Color,
    pub fill: Color,
    /// I callout (`doc::callout`): etichetta (`fg`), icona (`fg2`) e linea della citazione.
    pub label: Color,
    pub icon: Color,
    pub rule: Color,
    /// La linea sopra le note a piè di pagina (`line`).
    pub line: Color,
    /// Il colore dei titoli 4, 5 e 6 (`body` come i titoli 1–3, `fg2`, `fg3`).
    pub headings: [Color; 3],
    /// I percorsi (`doc::paths`): nome del file (`fg`), cartelle, `:riga` e puntini (`fg3`), linea
    /// in hover (`fg2`) e fondo in hover (`sel`).
    pub path: PathColors,
}

/// I colori dei percorsi nel testo (record 2026-11-16-percorsi-nome-e-link, mockup p-mix).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathColors {
    pub name: Color,
    pub dim: Color,
    pub hover_line: Color,
    pub hover_bg: Color,
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
pub fn view<S: 'static>(s: &S, doc_of: fn(&S) -> &Document, style: &MarkdownStyle, column: f32, on_rows: fn(&mut S, u64, Range<usize>) -> bool, signals: Signals<S>) -> El {
    let doc = doc_of(s);
    let generation = doc.id;
    let notes = doc.notes.block().map(|b| footnotes(&b, (signals.hits)(s, &b, crate::doc::footnotes::KEY), style, signals.fonts, signals.meta, column));
    let style = style.clone();
    let blocks: Arc<[(u64, Arc<str>)]> = doc.blocks.iter().map(|b| (b.id, b.source.clone())).collect();
    let file = File { path: doc.path.clone(), hash: doc.file_hash };
    let select_all = doc.select_all.clone();
    let flag = select_all.clone();
    let order = blocks.clone();
    let places = Arc::new(crate::doc::paths::Places::of(&doc.path));
    // La colonna è larga al massimo `column` ("Larghezza del testo"; "Intera" segue il pannello).
    let rows = super::theme::column(col().key("rows").w_full(), column).self_center().selection_group(true)
        .growth(GrowthSpec::default())
        .selection_copy(move |sel| copy_source(sel, generation, &file, &select_all, &blocks))
        .on_selection_change(move |_: &mut S, ch, _| selection_changed(&flag, ch, generation, &order))
        .virtual_rows(
            doc.blocks.len(),
            60.0,
            move |s: &S, i| Cow::Owned(row_key(i, doc_of(s))),
            move |s: &S, i| match doc_of(s).blocks.get(i) {
                Some(b) if i == 0 && crate::doc::frontmatter::is_block(&b.source) => frontmatter_row(&b.source, signals.meta),
                // Le definizioni delle note si disegnano in fondo (`footnotes`).
                Some(b) if crate::doc::footnotes::definitions(&b.source).is_some() => el().w_full(),
                Some(b) => {
                    let doc = doc_of(s);
                    let mut hits = (signals.hits)(s, b, &row_key(i, doc));
                    let caret = signals.caret.as_ref().filter(|c| c.at.block == b.id);
                    if let Some(c) = caret {
                        let clear = rune_kit::markdown::Highlight::new().bg(Color::TRANSPARENT).ring(Color::TRANSPARENT, 1.0);
                        hits.push((vec![c.at.anchor.clone()], clear.clone().key(caret_key(doc.id))));
                        hits.push((vec![c.at.end.clone()], clear.key(caret_end_key(doc.id))));
                    }
                    let progress = doc.tasks.get(&b.id).copied();
                    let row = block_row(&b.source, &style, &doc.notes, &places, progress, signals.columns, signals.open, signals.fonts, signals.meta, (signals.marked)(s, b.id).then_some(signals.mark), hits);
                    match caret {
                        Some(c) => caret_row(row, c),
                        None => row,
                    }
                }
                None => el(),
            },
        )
        .virtual_rows_version(rows_version(doc))
        .on_visible_rows(move |s: &mut S, rows| on_rows(s, generation, rows));
    let bottom = col().w_full().h(BOTTOM);
    let edge = signals.edge;
    // Gli `on_layout` dicono solo dove sono vista e pagina: la vista rigira solo se cambia ciò
    // che mostra (`live_only` altrimenti), non a ogni frame di scroll.
    let page = col().w_full().child(el().h(TOP)).child(rows).children(notes).child(bottom).on_layout_tx(move |s: &mut S, r, tx: &mut rune::Tx| {
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

/// Il frontmatter YAML: il testo fra le due righe `---`, così com'è, in un blocco di metadati
/// (`UI_SMALL` mono in `meta.text` su `meta.fill`, raggio `R_ROW`). Selezionabile come il resto:
/// la copia dà il sorgente del blocco, `---` compresi.
fn frontmatter_row(source: &str, meta: Meta) -> El {
    let body = source.split_once('\n').map_or("", |(_, rest)| rest);
    let body = body.rsplit_once('\n').map_or("", |(yaml, _)| yaml);
    let text = rune::text(body.to_owned()).font(TextStyle::new(UI_SMALL, UI_WEIGHT).family(Family::Mono)).color(meta.text).wrap().w_full().selectable(true);
    let block = col().key("frontmatter").w_full().fill(meta.fill).radius(R_ROW).px(FRONTMATTER_PAD_X).py(FRONTMATTER_PAD_Y).child(text);
    col().w_full().px(ROW_PAD_X).py(ROW_PAD_Y).child(block)
}

/// Il margine interno del blocco del frontmatter.
const FRONTMATTER_PAD_X: f32 = 12.0;
const FRONTMATTER_PAD_Y: f32 = 10.0;

/// Una riga del documento, con i segni di Cerca `hits`; con `mark` ha il segno "cosa è cambiato"
/// nel margine sinistro, fuori dal flusso (il testo non si sposta), alto quanto il testo del blocco.
#[allow(clippy::too_many_arguments)]
fn block_row<S: 'static>(source: &str, style: &MarkdownStyle, notes: &crate::doc::footnotes::Notes, places: &crate::doc::paths::Places, progress: Option<(u32, u32)>, columns: bool, open: fn(&mut S, &str, &mut rune::Tx), fonts: super::markdown_style::Fonts, meta: Meta, mark: Option<Color>, hits: super::find::Marks) -> El {
    // Un paragrafo a etichetta (registri, piani) tiene i suoi a capo, etichette in grassetto
    // (`doc::labels`); con la colonna larga, etichette e testo su due colonne. Gli altri
    // paragrafi restano Markdown standard.
    let body = match crate::doc::labels::block(source) {
        Some(lines) if columns => label_columns(source, &lines, hits, fonts, |src, hits| block_body(src, style, notes, places, None, open, fonts, meta, hits)),
        Some(lines) => {
            let e = crate::doc::labels::one_column(source, &lines);
            block_body(&e.text, style, notes, places, None, open, fonts, meta, remap(hits, |r| e.map(r)))
        }
        None => block_body(source, style, notes, places, progress, open, fonts, meta, hits),
    };
    // Le righe virtuali vogliono padding interno, non gap tra righe.
    let row = col().w_full().px(ROW_PAD_X).py(ROW_PAD_Y).child(body);
    match mark {
        Some(c) => row.child(el().key("changed").pin(Some(CHANGED_MARK_X), Some(ROW_PAD_Y), None, Some(ROW_PAD_Y)).w(CHANGED_MARK_W).fill(c)),
        None => row,
    }
}

/// I segni di Cerca `hits` con ogni intervallo passato per `f`.
fn remap(hits: super::find::Marks, f: impl Fn(&Range<usize>) -> Range<usize>) -> super::find::Marks {
    hits.into_iter().map(|(ranges, hl)| (ranges.iter().map(&f).collect(), hl)).collect()
}

/// I segni di Cerca dentro `r` del sorgente, contati da `r.start`.
fn clip(hits: &super::find::Marks, r: &Range<usize>) -> super::find::Marks {
    hits.iter()
        .filter_map(|(ranges, hl)| {
            let inside: Vec<Range<usize>> = ranges.iter().filter(|x| x.start < r.end && r.start < x.end || (x.is_empty() && r.contains(&x.start))).map(|x| x.start.max(r.start) - r.start..x.end.min(r.end) - r.start).collect();
            (!inside.is_empty()).then(|| (inside, hl.clone()))
        })
        .collect()
}

/// Un blocco a etichetta su due colonne (vincolo 4 del record dei registri, mockup t-mix larga):
/// a sinistra le etichette in grassetto senza i `:`, larghe quanto la più larga e al massimo
/// 10 em, a destra il testo, allineati riga per riga; una riga senza etichetta sta a destra.
fn label_columns(source: &str, lines: &[crate::doc::labels::Line], hits: super::find::Marks, fonts: super::markdown_style::Fonts, body: impl Fn(&str, super::find::Marks) -> El) -> El {
    let bold = TextStyle { weight: LABEL_WEIGHT, ..fonts.body };
    let widest = lines.iter().filter_map(|l| l.name()).map(|r| rune::text::advance(&source[r], bold)).fold(0.0, f32::max);
    let width = widest.min(LABEL_MAX_EM * fonts.body.size).ceil();
    col().w_full().children(lines.iter().map(|l| {
        let label = match l.name() {
            Some(r) => {
                let e = crate::doc::footnotes::Edited::new(&source[r.clone()], vec![(0..0, "**".to_owned()), (r.len()..r.len(), "**".to_owned())]);
                let hits = remap(clip(&hits, &r), |x| e.map(x));
                body(&e.text, hits)
            }
            None => el(),
        };
        let rest = l.rest(source);
        rune::row().w_full().gap(LABEL_GAP).child(el().w(width).child(label)).child(el().grow(1.0).min_w(0.0).child(body(&source[rest.clone()], clip(&hits, &rest))))
    }))
}

/// Peso delle etichette nella colonna (quello del grassetto del documento, `strong_weight`).
const LABEL_WEIGHT: f32 = 620.0;
/// La colonna delle etichette è larga al massimo 10 em (vincolo 4).
const LABEL_MAX_EM: f32 = 10.0;
/// Fra etichetta e testo.
const LABEL_GAP: f32 = 16.0;

/// Il Markdown di un blocco (o di un suo pezzo) come si disegna, con i segni di Cerca `hits`.
#[allow(clippy::too_many_arguments)]
fn block_body<S: 'static>(source: &str, style: &MarkdownStyle, notes: &crate::doc::footnotes::Notes, places: &crate::doc::paths::Places, progress: Option<(u32, u32)>, open: fn(&mut S, &str, &mut rune::Tx), fonts: super::markdown_style::Fonts, meta: Meta, hits: super::find::Marks) -> El {
    // Un titolo `Task N`: il numero fuori dal Markdown, in `fg3`, e l'avanzamento a destra.
    let task = crate::doc::labels::task_head(source);
    let untasked = task.as_ref().map(|t| crate::doc::footnotes::Edited::new(source, vec![(t.hashes.end + 1..t.prefix.end, String::new())]));
    let (source, hits) = match &untasked {
        Some(e) => (e.text.as_str(), hits.into_iter().map(|(ranges, hl)| (ranges.iter().map(|r| e.map(r)).collect(), hl)).collect()),
        None => (source, hits),
    };
    // Un callout (`> [!NOTE]`) si disegna senza la riga del segno, con l'etichetta sopra.
    let callout = crate::doc::callout::of(source);
    let (source, hits) = match &callout {
        Some(c) => (c.text.as_str(), hits.into_iter().map(|(ranges, hl)| (ranges.iter().map(|r| c.map(r)).collect(), hl)).collect()),
        None => (source, hits),
    };
    // Le note a piè di pagina: il numero in apice al posto di `[^1]`.
    let numbered = notes.shown(source);
    let (source, hits) = match &numbered {
        Some(e) => (e.text.as_str(), hits.into_iter().map(|(ranges, hl)| (ranges.iter().map(|r| e.map(r)).collect(), hl)).collect()),
        None => (source, hits),
    };
    // I percorsi: link in monospaziato con il nome del file in grassetto (`doc::paths`); il nome
    // prende il suo colore da un segno senza fondo, come quelli di Cerca.
    let linked = crate::doc::paths::shown(source, places);
    let (source, hits) = match &linked {
        Some(sh) => {
            let e = &sh.edited;
            let mut hits: super::find::Marks = hits.into_iter().map(|(ranges, hl)| (ranges.iter().map(|r| e.map(r)).collect(), hl)).collect();
            let paint = |c| rune_kit::markdown::Highlight::new().bg(Color::TRANSPARENT).ring(Color::TRANSPARENT, 1.0).fg(c);
            let marks = sh.names.iter().map(|r| (vec![r.clone()], paint(meta.path.name))).chain(sh.dims.iter().map(|r| (vec![r.clone()], paint(meta.path.dim))));
            hits.splice(0..0, marks);
            (e.text.as_str(), hits)
        }
        None => (source, hits),
    };
    // Le parole lunghe vanno a capo dopo `/ - _ .`, e ovunque solo se più larghe di una riga: il
    // testo disegnato ha degli U+200B in più, i segni di Cerca si spostano con loro.
    let (shown, hits) = match crate::doc::breaks::soft_breaks(source, line_limit, |t, kind| rune::text::advance(t, font_of(&fonts, kind))) {
        Some(b) => {
            let hits = hits.into_iter().map(|(ranges, hl)| (ranges.iter().map(|r| b.map(r)).collect(), hl)).collect();
            (b.text, hits)
        }
        None => (source.to_owned(), hits),
    };
    // `selectable` fa di ogni blocco un selection_group a sé, che fermerebbe un trascinamento
    // al bordo del blocco: si toglie il flag e vale il gruppo unico sulle righe.
    let level = heading_level(source);
    let style = match level {
        Some(n @ 4..=6) => Cow::Owned(style.clone().heading_font(4, fonts.headings[n as usize - 1]).muted(meta.headings[n as usize - 4])),
        _ => Cow::Borrowed(style),
    };
    // I percorsi che non esistono sono codice in linea: senza pastiglia se è il solo del blocco.
    let style = match &linked {
        Some(sh) if sh.bare => Cow::Owned(style.into_owned().inline_code_bg(Color::TRANSPARENT).inline_code_pad(0.0)),
        _ => style,
    };
    let md = rune_kit::Markdown::new(shown).highlights(hits).selectable(true);
    let md = if linked.is_some() { path_links(md, &meta, fonts) } else { md };
    // Ogni link si apre (`App::open_link`): percorsi, URL e `#titolo`.
    let md = md.on_link_tx(move |s: &mut S, url, tx| open(s, url, tx));
    let body = md.render(&style).selection_group(false);
    let body = match callout {
        Some(c) => col().w_full().child(callout_head(c.kind, &meta)).child(body),
        None => body,
    };
    match task {
        Some(t) => task_heading(body, &t, progress, fonts, &meta),
        None => body,
    }
}

/// Un titolo `Task N` (mockup t-sezioni): il numero nel font del titolo in `fg3` a sinistra, il
/// titolo, e a destra "2 di 4" dalle caselle del task, piccolo in `fg3`, centrato sulla prima riga.
fn task_heading(body: El, t: &crate::doc::labels::TaskHead, progress: Option<(u32, u32)>, fonts: super::markdown_style::Fonts, meta: &Meta) -> El {
    let font = fonts.headings[t.level as usize - 1];
    let number = rune::text(t.number.clone()).font(font).color(meta.path.dim).selectable(false);
    let small = super::sidebar::small_font();
    let done = progress.map(|(d, n)| rune::text(format!("{d} di {n}")).font(small).color(meta.path.dim).selectable(false).pt(((font.line_height - small.line_height) / 2.0).max(0.0)));
    rune::row().w_full().gap(TASK_GAP).child(number).child(el().grow(1.0).min_w(0.0).child(body)).children(done)
}

/// Fra numero, titolo e avanzamento di un titolo `Task N`.
const TASK_GAP: f32 = 10.0;

/// L'aspetto dei link dei percorsi a file che esistono (mockup p-mix): monospaziato a 13,5 in
/// `fg3`, sottolineato in `fg3` (il kit non ha la linea a puntini) e in hover fondo `sel`, linea
/// `fg2` e "Apri in RuneMD". Quelli che non esistono non sono link (`doc::paths::shown`).
fn path_links(md: rune_kit::Markdown, meta: &Meta, fonts: super::markdown_style::Fonts) -> rune_kit::Markdown {
    use crate::doc::paths::OPEN;
    let c = meta.path;
    let size = super::theme::CODE_TEXT * fonts.body.size / super::theme::BODY_TEXT;
    let span = rune::text::Span { mono: true, size: Some(size), color: Some(c.dim), ..Default::default() };
    md.link_style(move |url| {
        if url.starts_with(OPEN) {
            Some((rune::text::Span { underline: Some(c.dim), ..span.clone() }, rune::text::LinkPaint::new().bg(c.hover_bg).underline(c.hover_line)))
        } else {
            None
        }
    })
    .link_help(|url| url.starts_with(OPEN).then(|| OPEN_HELP.to_owned()))
}

/// Il suggerimento sui percorsi che si aprono (mockup p-mix).
pub const OPEN_HELP: &str = "Apri in RuneMD";

/// Il livello di un blocco che è un titolo ATX (`#` … `######`). Il kit porta i livelli dal 4 in giù
/// a uno solo, in `muted`: per il loro blocco la vista gli dà il font e il colore del livello vero.
fn heading_level(source: &str) -> Option<u8> {
    match crate::doc::breaks::Kind::of(source) {
        crate::doc::breaks::Kind::Heading(n) if !source.contains('\n') => Some(n),
        _ => None,
    }
}

/// Le note a piè di pagina in fondo al file: una linea in `line`, poi le note numerate a 13,5 in
/// `fg2`, nella colonna del testo, con i segni di Cerca.
fn footnotes(notes: &crate::doc::document::Block, hits: super::find::Marks, style: &MarkdownStyle, fonts: super::markdown_style::Fonts, meta: Meta, column: f32) -> El {
    let size = super::theme::CODE_TEXT * fonts.body.size / super::theme::BODY_TEXT;
    let font = TextStyle { size, line_height: size * super::theme::BODY_LEADING, ..fonts.body };
    let body = rune_kit::Markdown::new(notes.source.to_string()).highlights(hits).selectable(true).render(&style.clone().body_font(font).text(meta.text));
    let block = col().w_full().px(ROW_PAD_X).py(ROW_PAD_Y).gap(FOOTNOTES_GAP).child(el().w_full().h(super::theme::HAIRLINE).fill(meta.line)).child(body);
    super::theme::column(col().key(crate::doc::footnotes::KEY).w_full(), column).self_center().child(block)
}

/// Fra la linea e le note.
const FOOTNOTES_GAP: f32 = 12.0;

/// L'etichetta di un callout: icona 14 in `fg2` e parola 13/600 in `fg`, sulla linea della
/// citazione (2 pt, 12 prima del testo, come quella del kit), senza colori (DESIGN.md).
fn callout_head(kind: crate::doc::callout::Kind, meta: &Meta) -> El {
    let label = rune::text(kind.label()).font(TextStyle::new(UI_SMALL, 600.0)).color(meta.label);
    let head = rune::row().gap(CALLOUT_GAP).center_y().pl(QUOTE_PAD).pb(CALLOUT_GAP).child(rune::icon(callout_icon(kind), super::theme::ICON).color(meta.icon)).child(label);
    rune::row().key("callout").w_full().child(el().w(QUOTE_RULE).h_full().radius(QUOTE_RULE / 2.0).fill(meta.rule)).child(head)
}

/// La linea della citazione del kit e lo spazio prima del testo (`QuoteLook` di Rune Kit).
const QUOTE_RULE: f32 = 2.0;
const QUOTE_PAD: f32 = 12.0;
/// Fra icona ed etichetta, e sotto l'etichetta.
const CALLOUT_GAP: f32 = 6.0;

/// L'icona di un callout, disegnata come quelle del kit (viewBox 16, tratto 1, giunti tondi).
fn callout_icon(kind: crate::doc::callout::Kind) -> &'static rune::Icon {
    use crate::doc::callout::Kind;
    match kind {
        Kind::Note => &NOTE_ICON,
        Kind::Tip => &TIP_ICON,
        Kind::Important => &IMPORTANT_ICON,
        Kind::Warning => &WARNING_ICON,
        Kind::Caution => &CAUTION_ICON,
    }
}

/// Nota: una "i" in un cerchio.
static NOTE_ICON: rune::Icon = rune::Icon {
    name: "runemd-note",
    view: 16.0,
    body: r#"<circle cx="8" cy="8" r="6" stroke="currentColor"/><path d="M8 7.2V11" stroke="currentColor" stroke-linecap="round"/><circle cx="8" cy="5.1" r="0.6" fill="currentColor"/>"#,
};
/// Suggerimento: una lampadina.
static TIP_ICON: rune::Icon = rune::Icon {
    name: "runemd-tip",
    view: 16.0,
    body: r#"<path d="M6 11.5C6 10 4 9 4 6.5a4 4 0 0 1 8 0C12 9 10 10 10 11.5Z" stroke="currentColor" stroke-linejoin="round"/><path d="M6.3 13.5H9.7" stroke="currentColor" stroke-linecap="round"/>"#,
};
/// Importante: un fumetto con "!".
static IMPORTANT_ICON: rune::Icon = rune::Icon {
    name: "runemd-important",
    view: 16.0,
    body: r#"<path d="M3.2 2.5H12.8a1.2 1.2 0 0 1 1.2 1.2V10a1.2 1.2 0 0 1-1.2 1.2H7L4.5 13.5V11.2H3.2A1.2 1.2 0 0 1 2 10V3.7a1.2 1.2 0 0 1 1.2-1.2Z" stroke="currentColor" stroke-linejoin="round"/><path d="M8 4.8V7.6" stroke="currentColor" stroke-linecap="round"/><circle cx="8" cy="9.3" r="0.6" fill="currentColor"/>"#,
};
/// Attenzione: un triangolo con "!".
static WARNING_ICON: rune::Icon = rune::Icon {
    name: "runemd-warning",
    view: 16.0,
    body: r#"<path d="M7.13 2.5a1 1 0 0 1 1.74 0l5.5 9.75a1 1 0 0 1-.87 1.5H2.5a1 1 0 0 1-.87-1.5Z" stroke="currentColor" stroke-linejoin="round"/><path d="M8 6.2V9" stroke="currentColor" stroke-linecap="round"/><circle cx="8" cy="11.1" r="0.6" fill="currentColor"/>"#,
};
/// Cautela: un ottagono con "!".
static CAUTION_ICON: rune::Icon = rune::Icon {
    name: "runemd-caution",
    view: 16.0,
    body: r#"<path d="M5.5 2H10.5L14 5.5V10.5L10.5 14H5.5L2 10.5V5.5Z" stroke="currentColor" stroke-linejoin="round"/><path d="M8 5V8.4" stroke="currentColor" stroke-linecap="round"/><circle cx="8" cy="10.6" r="0.6" fill="currentColor"/>"#,
};

/// La riga del blocco con il caret: il caret fissato al punto, fuori dal flusso, così il testo
/// non si sposta.
fn caret_row(row: El, c: &Caret) -> El {
    match c.place {
        Some([x, y, h]) => {
            let bar = el().key("caret").pin(Some(x), Some(y), None, None).w(crate::fx::caret::WIDTH).h(h);
            row.child(crate::fx::caret::slot(bar, c.since, c.dark, c.reduced))
        }
        None => row,
    }
}

/// La riga più stretta che ha un blocco di tipo `kind` con la colonna più stretta (`COLUMN`
/// meno i margini): un titolo la prende tutta, un paragrafo ne lascia per due livelli di lista,
/// una cella per tre colonne. Oltre, una parola si spezza ovunque (`doc::breaks`).
fn line_limit(kind: crate::doc::breaks::Kind) -> f32 {
    use crate::doc::breaks::Kind;
    let line = super::theme::COLUMN - 2.0 * ROW_PAD_X;
    match kind {
        Kind::Heading(_) => line,
        Kind::Body => line - 64.0,
        Kind::Cell => line / 3.0,
    }
}

/// Il font di un blocco di tipo `kind`.
fn font_of(fonts: &super::markdown_style::Fonts, kind: crate::doc::breaks::Kind) -> TextStyle {
    use crate::doc::breaks::Kind;
    match kind {
        Kind::Heading(n @ 1..=6) => fonts.headings[n as usize - 1],
        Kind::Heading(_) | Kind::Body => fonts.body,
        Kind::Cell => fonts.table,
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
        assert!(!fire_drawn(&rt), "etichetta spenta");
        assert!(rt.elements().iter().any(|e| e.key.as_deref() == Some("fire") && !e.exiting), "il fuoco resta, a riposo");
    }

    /// Il fuoco acceso dal testo nuovo: l'etichetta "N paragrafi nuovi più in basso". Lo slot
    /// delle fiamme c'è sempre (a riposo, basso).
    fn fire_drawn(rt: &Runtime<Doc>) -> bool {
        rt.elements().iter().any(|e| e.key.as_deref() == Some("fire-text") && !e.exiting)
    }

    /// Sempre acceso: in lettura lo slot del fuoco c'è anche senza testo nuovo, senza etichetta;
    /// nel sorgente (Cmd+E) e con il fuoco spento dalle impostazioni no.
    #[test]
    fn the_fire_rests_at_the_panel_bottom_without_new_text() {
        let mut rt = open(&join(&sample()));
        let slot = |rt: &Runtime<Doc>| rt.elements().iter().any(|e| e.key.as_deref() == Some("fire") && !e.exiting);
        assert!(slot(&rt), "slot del fuoco a riposo");
        assert!(!fire_drawn(&rt), "nessuna etichetta a riposo");
        assert!(rt.state.effects_lit(), "il compositor scorre le istanze");
        rt.update(|s, _| s.settings.fire = false);
        settle(&mut rt);
        assert!(!slot(&rt) && !fire_drawn(&rt), "fuoco spento: niente slot né etichetta");
    }

    /// A riposo, con la finestra attiva e senza "riduci movimento", le fiamme chiedono un
    /// risveglio ogni `fire::REST_FRAME`; ferme no.
    #[test]
    fn the_resting_fire_is_paced_by_wakes_only_while_it_moves() {
        let mut rt = open(&join(&sample()));
        assert!(rt.state.fire_rests());
        rt.update(|s, _| s.window_active = false);
        assert!(!rt.state.fire_rests(), "finestra non attiva: ferme");
        rt.update(|s, _| {
            s.window_active = true;
            s.motion_env = Some(true);
        });
        settle(&mut rt);
        assert!(!rt.state.fire_rests(), "riduci movimento: ferme");
        rt.update(|s, _| {
            s.motion_env = Some(false);
            s.tabs[0].fire_since = Some(0.0);
        });
        settle(&mut rt);
        assert!(!rt.state.fire_rests(), "acceso: lo muove il display link");
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

    /// Caret e segno accesi su una modifica a metà file (record 2026-11-15-caret-al-punto-modificato,
    /// vincoli 1–3): le righe restano dove erano, niente barra in fondo, il caret di 2 pt sta nella
    /// riga cambiata subito dopo la parola cambiata, e il segno sta nel margine sinistro.
    #[test]
    fn caret_and_changed_mark_do_not_move_the_text() {
        let mut rt = open("# Titolo\n\nIl gatto dorme sul divano tutto il giorno.\n\nFine.\n");
        let before = row_rects(&rt);
        assert_eq!(before.len(), 3);
        assert!(keyed(&rt, "caret").is_empty() && keyed(&rt, "changed").is_empty());
        rt.update(|s, _| {
            let tab = s.tab_mut().unwrap();
            let old = tab.doc.blocks.clone();
            let change = tab.doc.update("# Titolo\n\nIl cane dorme sul divano tutto il giorno.\n\nFine.\n");
            let at = crate::live::caret::locate(&old, &tab.doc.blocks, &change);
            tab.changed.mark(&change.changed, std::time::Instant::now());
            tab.caret_step(at.as_ref(), true, 0.0);
        });
        settle(&mut rt);
        assert_eq!(row_rects(&rt).iter().map(|r| r.1).collect::<Vec<_>>(), before.iter().map(|r| r.1).collect::<Vec<_>>(), "niente si sposta");
        assert!(keyed(&rt, "arriving").is_empty(), "niente barra in fondo");
        let caret = keyed(&rt, "caret");
        assert_eq!(caret.len(), 1, "un caret");
        let (c, row) = (caret[0], before[1].1);
        assert_eq!(c[2], crate::fx::caret::WIDTH);
        assert!(c[1] > row[1] && c[1] + c[3] < row[1] + row[3], "nella riga cambiata: {c:?} {row:?}");
        // Dopo "Il cane": a destra del carattere evidenziato, a sinistra di metà riga.
        let mark = keyed(&rt, &super::caret_key(rt.state.active_document().id));
        assert!(!mark.is_empty(), "il carattere del caret ha un rect");
        assert!((c[0] - (mark[0][0] + mark[0][2] + super::CARET_GAP)).abs() < 0.5, "subito dopo la parola: {c:?} {mark:?}");
        assert!(c[0] < row[0] + row[2] * 0.4, "dopo «Il cane», non in fondo: {c:?}");
        let mark = keyed(&rt, "changed");
        assert_eq!(mark.len(), 1, "un solo blocco segnato");
        assert_eq!(mark[0][2], crate::ui::theme::CHANGED_MARK_W);
        assert!(mark[0][0] > row[0] && mark[0][0] + mark[0][2] < row[0] + crate::ui::theme::ROW_PAD_X, "nel margine sinistro");
    }

    /// Il carattere del caret senza rect (record 2026-11-15-caret-al-punto-modificato, vincolo 2):
    /// qui la modifica cade nell'URL di un link, che non si vede. Il caret va alla fine del blocco
    /// cambiato, dopo l'ultima parola, invece di sparire.
    #[test]
    fn without_the_character_rect_the_caret_goes_to_the_end_of_the_block() {
        let text = |u: &str| format!("# Titolo\n\nVedi [la pagina lunga con molte parole che va a capo di sicuro su due righe nella colonna del documento](http://example.com/{u}) e poi testo che continua per un po' fino alla fine.\n\nFine.\n");
        let mut rt = open(&text("a"));
        rt.update(|s, _| {
            let tab = s.tab_mut().unwrap();
            let old = tab.doc.blocks.clone();
            let change = tab.doc.update(&text("b"));
            let at = crate::live::caret::locate(&old, &tab.doc.blocks, &change);
            tab.caret_step(at.as_ref(), true, 0.0);
        });
        settle(&mut rt);
        let id = rt.state.active_document().id;
        assert!(keyed(&rt, &super::caret_key(id)).is_empty(), "il carattere nell'URL non ha rect");
        let caret = keyed(&rt, "caret");
        assert_eq!(caret.len(), 1, "il caret si vede lo stesso");
        let end = keyed(&rt, &super::caret_end_key(id));
        assert_eq!(end.len(), 1, "l'ultima parola del blocco ha un rect");
        let (c, e) = (caret[0], end[0]);
        assert!((c[0] - (e[0] + e[2] + super::CARET_GAP)).abs() < 0.5, "subito dopo «fine.»: {c:?} {e:?}");
        let row = row_rects(&rt)[1].1;
        assert!(c[1] > row[1] + row[3] * 0.5, "sull'ultima riga del blocco: {c:?} {row:?}");
    }

    /// Il frontmatter YAML in lettura: un blocco di metadati (chiave `frontmatter`) col testo così
    /// com'è, in `fg2`; senza chiusura resta Markdown.
    #[test]
    fn frontmatter_reads_as_one_muted_metadata_block() {
        let rt = open("---\nname: RuneMD\ndescription: Lettore\n---\n\n# Titolo\n\nTesto.\n");
        let els = rt.elements();
        let meta: Vec<_> = els.iter().filter(|e| e.key.as_deref() == Some("frontmatter") && !e.exiting).collect();
        assert_eq!(meta.len(), 1, "un blocco di metadati");
        let theme = crate::ui::theme::Theme::for_mode(true);
        let text = els.iter().find(|e| e.text.as_deref() == Some("name: RuneMD\ndescription: Lettore")).expect("il YAML così com'è, senza i ---");
        assert_eq!(text.text_color, Some(theme.fg2));
        let rt = open("---\nname: RuneMD\n\nTesto.\n");
        assert!(!rt.elements().iter().any(|e| e.key.as_deref() == Some("frontmatter")), "senza chiusura: Markdown");
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
            // Dopo il nome, la × (sempre il suo posto): 4 + 16 + 6.
            let tail = 4.0 + crate::ui::tabs::CLOSE + 6.0;
            assert!((tab[0] + tab[2] - label[0] - label[2] - tail).abs() <= 0.5, "e la × in fondo: {tab:?} {label:?}");
        }
        assert!(b[2] > a[2] + 100.0, "larghe quanto il nome: {a:?} {b:?}");
        assert_eq!(a[3], crate::ui::theme::ROW);
    }

    /// La × per chiudere: sulla scheda davanti sempre, sulle altre solo in hover; il posto è
    /// riservato, così il nome non si sposta. Etichetta "Chiudi <nome>"; un clic chiude come Cmd+W.
    #[test]
    fn the_close_button_shows_on_the_front_and_hovered_tab_and_closes() {
        let mut rt = open("# a\n");
        rt.update(|s, _| s.push_tab(Document::from_source(Path::new("b.md"), "# b\n")));
        settle(&mut rt);
        let ids: Vec<u64> = rt.state.tabs.iter().map(|t| t.doc.id).collect();
        let key = |i: usize, part: &str| format!("t-{:x}{part}", ids[i]);
        let el = |rt: &Runtime<Doc>, k: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(k) && !e.exiting).unwrap_or_else(|| panic!("{k}"));
        // b.md è davanti (l'ultima aperta), t.md dietro.
        assert_eq!(rt.state.active, 1);
        let close = el(&rt, &key(1, "-close"));
        assert_eq!((close.role, close.label.as_deref()), ("button", Some("Chiudi b.md")));
        assert!(close.rect[2] >= 16.0 && close.rect[3] >= 16.0, "area di 16 pt: {:?}", close.rect);
        assert!(el(&rt, &key(1, "-close")).opacity > 0.99, "davanti: visibile");
        assert!(el(&rt, &key(0, "-close")).opacity < 0.01, "dietro: nascosta");
        let label = el(&rt, &key(0, "-label")).rect;
        let tab = el(&rt, &key(0, "")).rect;
        rt.pointer_move(tab[0] + 8.0, tab[1] + tab[3] / 2.0);
        settle(&mut rt);
        assert!(el(&rt, &key(0, "-close")).opacity > 0.99, "in hover: visibile");
        assert_eq!(el(&rt, &key(0, "-label")).rect, label, "il nome non si sposta");
        let c = el(&rt, &key(0, "-close")).rect;
        assert!(c[0] + c[2] <= tab[0] + tab[2] + 0.5 && c[0] > label[0] + label[2] - 0.5, "in fondo alla scheda");
        let (x, y) = (c[0] + c[2] / 2.0, c[1] + c[3] / 2.0);
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        settle(&mut rt);
        assert_eq!(rt.state.tabs.len(), 1, "chiusa");
        assert_eq!(rt.state.tab().unwrap().doc.id, ids[1], "resta b.md");
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
        open_sized(source, dark, VIEWPORT)
    }

    /// [`open_themed`] in una finestra di `size`.
    fn open_sized(source: &str, dark: bool, size: (f32, f32)) -> Runtime<Doc> {
        let mut state = Doc::bare(crate::index::Library::load(Vec::new(), None));
        state.set_style(crate::ui::markdown_style::style(&crate::ui::theme::Theme::for_mode(dark), crate::ui::theme::BODY_TEXT));
        state.push_tab(Document::from_source(Path::new("t.md"), source));
        let mut rt = Runtime::new(state, crate::view, size, Env::new(dark, 2.0));
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

    /// I callout di GitHub (`> [!NOTE]` …) hanno l'etichetta in parole e non mostrano il segno.
    #[test]
    fn callouts_show_a_label_not_the_marker() {
        let src = "> [!NOTE]\n> Una nota.\n\n> [!TIP]\n> Un suggerimento.\n\n> [!IMPORTANT]\n> Importante.\n\n> [!warning]\n> Un avviso.\n\n> [!CAUTION]\n> Un rischio.\n";
        for dark in [true, false] {
            let rt = open_themed(src, dark);
            let texts: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).collect();
            assert!(!texts.iter().any(|t| t.contains("[!")), "il segno si vede: {texts:?}");
            for label in ["Nota", "Suggerimento", "Importante", "Attenzione", "Cautela"] {
                assert!(texts.iter().any(|t| t == label), "manca {label}: {texts:?}");
            }
            assert!(texts.iter().any(|t| t.contains("Un avviso.")), "{texts:?}");
        }
    }

    /// Le note a piè di pagina: il numero in alto nel testo, le note in fondo al file sotto una
    /// linea, non dove sono scritte.
    #[test]
    fn footnotes_show_a_number_and_go_to_the_end() {
        let src = "Una nota[^1] qui.\n\n[^1]: Il testo della nota.\n\nDopo la nota.\n";
        let rt = open_themed(src, true);
        let els = rt.elements();
        let texts: Vec<String> = els.iter().filter_map(|e| e.text.as_deref()).map(|t| t.replace(['\u{2066}', '\u{2069}'], "")).collect();
        assert!(!texts.iter().any(|t| t.contains("[^")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("Una nota¹ qui.")), "{texts:?}");
        let y = |needle: &str| els.iter().find(|e| e.text.as_deref().is_some_and(|t| t.replace(['\u{2066}', '\u{2069}'], "").contains(needle))).map(|e| e.rect[1]).unwrap_or_else(|| panic!("manca {needle}: {texts:?}"));
        assert!(y("Il testo della nota.") > y("Dopo la nota."), "la nota va in fondo");
        assert!(els.iter().any(|e| e.key.as_deref() == Some("footnotes")));
    }

    /// Titoli 3–6: 17 / 15,5 / 14 / 13, il colore scende (`body`, `body`, `fg2`, `fg3`); 4–6 non sono
    /// uguali al 3.
    #[test]
    fn headings_three_to_six_step_down() {
        let t = crate::ui::theme::Theme::for_mode(true);
        let rt = open_themed("### T3\n\n#### T4\n\n##### T5\n\n###### T6\n", true);
        let els = rt.elements();
        let h = |n: &str| els.iter().find(|e| e.text.as_deref() == Some(n)).map(|e| (e.rect[3], e.text_color)).unwrap_or_else(|| panic!("manca {n}"));
        let [h3, h4, h5, h6] = ["T3", "T4", "T5", "T6"].map(h);
        assert!(h3.0 > h4.0 && h4.0 > h5.0 && h5.0 > h6.0, "altezze {h3:?} {h4:?} {h5:?} {h6:?}");
        assert_eq!([h3.1, h4.1, h5.1, h6.1], [Some(t.body), Some(t.body), Some(t.fg2), Some(t.fg3)]);
    }

    /// I percorsi (record 2026-11-16-percorsi-nome-e-link): senza backtick, in monospaziato; chi
    /// esiste è un link che apre il file alla riga; chi non esiste no; nel codice niente.
    #[test]
    fn paths_show_as_links_that_open_the_file_at_the_line() {
        let t = std::env::temp_dir().join(format!("runemd-paths-view-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        std::fs::create_dir_all(t.join("src/live")).unwrap();
        std::fs::create_dir_all(t.join(".git")).unwrap();
        let target: String = (1..=40).map(|n| format!("Paragrafo {n}.\n\n")).collect();
        std::fs::write(t.join("src/live/below.rs"), &target).unwrap();
        let src = "Vedi src/live/below.rs:31, `src/live/below.rs` e src/old/gone.rs.\n\n```\ncat src/live/below.rs\n```\n";
        let doc_path = t.join("doc.md");
        std::fs::write(&doc_path, src).unwrap();
        let mut state = Doc::bare(crate::index::Library::load(Vec::new(), None));
        state.set_style(crate::ui::markdown_style::style(&crate::ui::theme::Theme::for_mode(true), crate::ui::theme::BODY_TEXT));
        state.open_paths(vec![doc_path.clone()]);
        let mut rt = Runtime::new(state, crate::view, VIEWPORT, Env::new(true, 2.0));
        rt.frame(0.0);
        settle(&mut rt);
        let texts: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).map(|x| x.replace(['\u{200B}', '\u{2066}', '\u{2069}', '\u{202F}'], "")).collect();
        assert!(texts.iter().any(|x| x.contains("Vedi src/live/below.rs:31, src/live/below.rs e src/old/gone.rs.")), "{texts:?}");
        assert!(texts.iter().any(|x| x.contains("cat src/live/below.rs")), "il codice resta: {texts:?}");
        let url = crate::doc::paths::open_target(&t.join("src/live/below.rs").canonicalize().unwrap(), Some(31));
        rt.update(|s, tx| s.open_link(&url, tx));
        settle(&mut rt);
        let tab = rt.state.tab().unwrap();
        assert!(tab.doc.path.ends_with("src/live/below.rs"), "{:?}", tab.doc.path);
        let texts: Vec<String> = rt.elements().into_iter().filter(|e| e.visible[3] > 0.0).filter_map(|e| e.text).collect();
        assert!(texts.iter().any(|x| x == "Paragrafo 16."), "la riga 31 è in vista: {texts:?}");
        let _ = std::fs::remove_dir_all(&t);
    }

    /// Un percorso che non esiste non è un link (vincolo 3 del record dei percorsi): niente mano,
    /// niente clic; resta in monospaziato con il nome in evidenza.
    #[test]
    fn a_missing_path_is_not_a_link() {
        for src in ["Vedi src/old/gone.rs:3 qui.\n", "Vedi src/old/gone.rs e `cargo test`.\n"] {
            let rt = open_themed(src, true);
            let els = rt.elements();
            let links: Vec<_> = els.iter().filter(|e| e.role == "link").map(|e| e.key.clone()).collect();
            assert!(links.is_empty(), "{src:?}: {links:?}");
            let texts: Vec<String> = els.iter().filter_map(|e| e.text.clone()).map(|x| x.replace(['\u{200B}', '\u{2066}', '\u{2069}', '\u{202F}'], "")).collect();
            assert!(texts.iter().any(|x| x.contains("Vedi src/old/gone.rs")), "{texts:?}");
        }
    }

    /// Il registro (fixtures/registro.md, dall'inizio di .superpowers/sdd/plan/progress.md) si
    /// legge a righe: ogni riga del file è una riga (vincolo 2 del record degli a capo).
    #[test]
    fn a_register_reads_as_lines() {
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/registro.md")).unwrap();
        let rt = open_sized(&source, true, NARROW);
        let texts: Vec<String> = rt.elements().into_iter().filter_map(|e| e.text).map(|x| x.replace('\u{200B}', "")).collect();
        let reg = texts.iter().find(|x| x.starts_with("Task 1: BASE root")).unwrap_or_else(|| panic!("{texts:?}"));
        assert!(reg.contains("Implementer a382e1858b685d390.\nRuling: keychain"), "{reg:?}");
        assert!(reg.contains("review clean)\nTask 2: BASE 0095282."), "{reg:?}");
        let preamble = texts.iter().find(|x| x.starts_with("Spec: docs/spec.md")).unwrap();
        assert!(preamble.contains("push).\nRuling: lavoro"), "due righe, tutte e due a etichetta: {preamble:?}");
    }

    /// Una finestra in cui la colonna del testo è più stretta di [`super::TWO_COLUMNS`].
    const NARROW: (f32, f32) = (640.0, 1000.0);

    /// Con la colonna larga il registro va su due colonne (vincolo 4 del record dei registri):
    /// etichette senza `:` a sinistra, tutte allo stesso x e larghe al massimo 10 em, testo a
    /// destra allineato; una riga senza etichetta sta nella colonna del testo. Stretta, una colonna.
    #[test]
    fn a_wide_register_shows_two_columns() {
        let source = "Task 2: BASE 0095282. Implementer abd1.\nRuling: commit del task 2.\nmisurato di nuovo dopo lo sblocco dello schermo: 118 fps.\nTask 2: minor (deferred): TreeRing.\nTask 2: complete (commits 0095282..f04c761)\n";
        let rt = open_themed(source, false);
        let els = rt.elements();
        let rows = els.iter().find(|e| e.key.as_deref() == Some("rows")).unwrap().rect;
        assert!(rows[2] - 2.0 * crate::ui::theme::ROW_PAD_X >= super::TWO_COLUMNS, "colonna {rows:?}");
        let at = |s: &str| els.iter().find(|e| e.text.as_deref().map(|t| t.replace(['\u{2066}', '\u{2069}'], "")).as_deref() == Some(s)).unwrap_or_else(|| panic!("manca {s:?}: {:?}", els.iter().filter_map(|e| e.text.clone()).collect::<Vec<_>>())).rect;
        let labels = ["Task 2", "Ruling", "Task 2: minor (deferred)"].map(at);
        let texts = ["BASE 0095282. Implementer abd1.", "commit del task 2.", "misurato di nuovo dopo lo sblocco dello schermo: 118 fps.", "TreeRing."].map(at);
        assert!(labels.iter().all(|l| l[0] == labels[0][0]), "etichette allo stesso x: {labels:?}");
        assert!(texts.iter().all(|t| t[0] == texts[0][0] && t[0] > labels[0][0] + 40.0), "testo allineato a destra: {texts:?}");
        let em = crate::ui::markdown_style::fonts(crate::fonts::Typeface::of("Inter", crate::ui::theme::BODY_TEXT)).body.size;
        assert!(texts[0][0] - labels[0][0] <= 10.0 * em + super::LABEL_GAP + 1.0, "al massimo 10 em");
        assert!((labels[0][1] - texts[0][1]).abs() < 1.0 && (labels[1][1] - texts[1][1]).abs() < 1.0, "righe allineate");
        let narrow = open_sized(source, false, NARROW);
        let one: Vec<String> = narrow.elements().into_iter().filter_map(|e| e.text).filter(|t| t.starts_with("Task 2: BASE")).collect();
        assert_eq!(one.len(), 1, "una colonna: {one:?}");
        assert!(one[0].contains("abd1.\nRuling: commit"));
    }

    /// DESIGN.md (paragraphs andati a capo a mano, nessun registro) esce identico a com'era senza
    /// i blocchi a etichetta: nessun blocco lo è, e la vista ha gli stessi testi negli stessi posti
    /// (vincolo 5 del record degli a capo).
    #[test]
    fn design_md_renders_as_before() {
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/DESIGN.md")).unwrap();
        let d = Document::from_source(Path::new("DESIGN.md"), &source);
        let labeled: Vec<&str> = d.blocks.iter().map(|b| &*b.source).filter(|b| crate::doc::labels::block(b).is_some() || crate::doc::labels::task_head(b).is_some()).collect();
        assert!(labeled.is_empty(), "{labeled:?}");
        let layout = |off: bool| {
            crate::doc::labels::OFF.with(|o| o.set(off));
            let mut rt = open_themed(&source, false);
            let mut seen = Vec::new();
            for step in 0..6 {
                rt.update(|_, tx| tx.scroll_to_instant("doc", 0.0, step as f32 * 2400.0));
                settle(&mut rt);
                seen.extend(rt.elements().into_iter().filter(|e| e.text.is_some()).map(|e| (e.text, e.rect.map(|v| (v * 2.0).round() as i32))));
            }
            crate::doc::labels::OFF.with(|o| o.set(false));
            seen
        };
        let before = layout(true);
        assert!(before.len() > 100, "testi: {}", before.len());
        assert_eq!(layout(false), before);
    }

    /// Un titolo `Task N` (vincolo 3 del record dei registri): il numero in `fg3`, il titolo senza
    /// `Task N:`, e a destra l'avanzamento dalle caselle del task.
    #[test]
    fn task_headings_show_number_and_progress() {
        for dark in [true, false] {
            let t = crate::ui::theme::Theme::for_mode(dark);
            let rt = open_themed("### Task 12: Fuoco\n\n- [x] uno\n- [x] due\n- [ ] tre\n- [ ] quattro\n\n### Task 13: Dither\n\nTesto.\n", dark);
            let els = rt.elements();
            let find = |s: &str| els.iter().find(|e| e.text.as_deref() == Some(s)).unwrap_or_else(|| panic!("manca {s:?}")).clone();
            let (number, title, done) = (find("12"), find("Fuoco"), find("2 di 4"));
            assert_eq!(number.text_color, Some(t.fg3));
            assert_eq!(done.text_color, Some(t.fg3));
            assert!(number.rect[0] < title.rect[0] && title.rect[0] < done.rect[0], "{number:?} {title:?} {done:?}");
            assert!(find("Dither").rect[1] > done.rect[1]);
            assert!(!els.iter().any(|e| e.text.as_deref().is_some_and(|x| x.contains("Task 1") || x.contains("di 0"))), "senza caselle niente avanzamento");
        }
    }

    /// Il link `key` della vista: un clic al suo centro.
    fn click_link(rt: &mut Runtime<Doc>, key: &str) {
        let r = rt.elements().into_iter().find(|e| e.role == "link" && e.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("manca il link {key}")).rect;
        let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.pointer_move(x, y);
        rt.frame(1.0 / 120.0);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        settle(rt);
    }

    /// In un paragrafo con un percorso, e fuori, gli altri link funzionano: un URL si apre nel
    /// browser, `#titolo` porta al titolo.
    #[test]
    fn links_beside_a_path_still_open() {
        let filler: String = (1..=40).map(|n| format!("Paragrafo {n}.\n\n")).collect();
        for with_path in [true, false] {
            let path = if with_path { " e src/old/gone.rs" } else { "" };
            let src = format!("Vedi [il sito](https://example.com/a){path} o [la fine](#la-fine).\n\n{filler}## La fine\n\nUltimo.\n");
            let mut rt = open_themed(&src, true);
            click_link(&mut rt, "https://example.com/a");
            assert_eq!(rt.take_url_requests(), ["https://example.com/a"], "con percorso: {with_path}");
            click_link(&mut rt, "#la-fine");
            let shown: Vec<String> = rt.elements().into_iter().filter(|e| e.visible[3] > 0.0).filter_map(|e| e.text).collect();
            assert!(shown.iter().any(|x| x == "La fine"), "con percorso: {with_path}: {shown:?}");
            assert!(!shown.iter().any(|x| x == "Paragrafo 1."), "il titolo in cima, con percorso: {with_path}");
        }
    }

    /// Le caselle delle liste di compiti sono quelle delle Impostazioni: spenta vuota (bordo `fg3`),
    /// accesa piena in `fg2`.
    #[test]
    fn task_checkboxes_look_like_the_settings_one() {
        for dark in [true, false] {
            let t = crate::ui::theme::Theme::for_mode(dark);
            let rt = open_themed("- [ ] da fare\n- [x] fatto\n", dark);
            let els = rt.elements();
            let control = |label: &str| {
                let row = els.iter().position(|e| e.role == "checkbox" && e.label.as_deref() == Some(label)).unwrap_or_else(|| panic!("manca {label}"));
                els.iter().find(|e| e.parent == Some(row) && e.key.as_deref() == Some("control")).unwrap().clone()
            };
            let (off, on) = (control("da fare"), control("fatto"));
            assert_eq!(on.fill, t.fg2, "accesa, dark={dark}");
            assert_eq!(off.fill.a, 0.0, "spenta vuota, dark={dark}");
            assert_eq!(on.rect[2], crate::ui::theme::ICON);
        }
    }

    /// Le parole lunghe (fixtures/long-title.md: titolo-percorso, paragrafo, hash senza
    /// separatori, lista, celle) vanno a capo: nessuna riga esce dalla colonna. Ogni testo è
    /// dentro le righe e la sua parte più larga che non si spezza ci sta.
    #[test]
    fn long_words_never_overflow_the_column() {
        let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/long-title.md")).unwrap();
        let mut rt = open_themed(&source, true);
        let els = rt.elements();
        let rows = els.iter().find(|e| e.key.as_deref() == Some("rows")).unwrap().rect;
        let right = rows[0] + rows[2] - crate::ui::theme::ROW_PAD_X;
        let fonts = crate::ui::markdown_style::fonts(crate::fonts::Typeface::of("Inter", crate::ui::theme::BODY_TEXT));
        let mut seen = 0;
        for e in els.iter().filter(|e| e.text.as_deref().is_some_and(|t| !t.is_empty())) {
            let t = e.text.as_deref().unwrap();
            let mut up = e.parent;
            let mut inside = false;
            while let Some(i) = up {
                inside |= els[i].key.as_deref() == Some("rows");
                up = els[i].parent;
            }
            if !inside {
                continue;
            }
            seen += 1;
            assert!(e.rect[0] + e.rect[2] <= right + 0.5, "{t:?} esce: {:?} oltre {right}", e.rect);
            let font = match (e.role, e.level) {
                ("heading", Some(n @ 1..=6)) => fonts.headings[n as usize - 1],
                _ => fonts.body,
            };
            let widest = rune::text::measure(t, &[], font, Some(0.0)).0;
            assert!(e.rect[0] + widest <= right + 0.5, "{t:?}: la parte più larga ({widest} pt) esce da {right}");
        }
        assert!(seen >= 6, "testi controllati: {seen}");
        let _ = &mut rt;
    }
}
