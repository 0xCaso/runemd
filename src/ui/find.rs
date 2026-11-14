//! Cerca nel file (Cmd+F): un campo nella barra delle schede cerca nel testo disegnato dei
//! blocchi della scheda davanti (`rune_kit::markdown::find`, senza maiuscole). Invio va al
//! risultato dopo, Maiusc+Invio a quello prima, in giro; Esc chiude e toglie i segni.
//!
//! La ricerca gira su un thread a parte: `markdown::find` costa ~5 ms per MB anche senza
//! risultati (su `big.md`, 1 MB, p95 8,7 ms sul thread della UI: oltre la soglia). Il tasto manda
//! il lavoro al thread e torna subito; i risultati arrivano con il `Waker` e `poll` li applica
//! (`App::poll_find`). Un lavoro superato da uno più nuovo si ferma a metà.
//!
//! Il risultato attivo ha la chiave `<riga>/find-current`: `Tx::scroll_center_in_instant` monta la
//! riga di `virtual_rows` anche se è fuori dalla finestra montata e poi centra il segno nella riga.
//!
//! Una sola barra per finestra: cambiata la scheda, la ricerca rigira sul file nuovo e riparte dal
//! primo risultato. Dopo una ricarica i risultati si ricalcolano e quello attivo resta il più
//! vicino possibile al blocco dov'era, senza spostare la vista.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use rune::{El, Key, KeyEvent, TextEdit, Tx, Waker, icon, row, text, text_input};
use rune_kit::glyphs;
use rune_kit::markdown::{Case, Highlight, find};

use super::sidebar::{small_font, ui_font};
use super::theme::{FIELD_H, FIND_W, HAIRLINE, ICON, R_FIELD, ROW_GAP, ROW_PAD, Theme};
use crate::app::App;
use crate::doc::document::Document;

/// Testi della barra (approvati).
pub const PLACEHOLDER: &str = "Cerca nel file";
pub const NO_RESULTS: &str = "Nessun risultato";

/// I segni di Cerca di un blocco, come li prende `Markdown::highlights`.
pub type Marks = Vec<(Vec<Range<usize>>, Highlight)>;

/// La chiave del segno attivo dentro la sua riga.
const CURRENT: &str = "find-current";

/// Un risultato: il blocco (id, hash del sorgente, posizione nel documento) e i byte del suo
/// sorgente che copre.
#[derive(Clone, Debug, PartialEq)]
struct Hit {
    block: u64,
    hash: u64,
    at: usize,
    ranges: Vec<Range<usize>>,
}

/// Da dove vengono i risultati: documento (id, revisione, hash del file) e ricerca.
type Source = (u64, u64, u64, String);

fn source_of(doc: &Document, query: &str) -> Source {
    (doc.id, doc.revision, doc.file_hash, query.to_owned())
}

/// Quale risultato diventa attivo quando arrivano i risultati.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goal {
    /// Il primo dal blocco in posizione `n` in giù, in giro (una ricerca nuova; 0 per un'altra
    /// scheda). La vista lo porta al centro.
    From(usize),
    /// Il più vicino a quello di prima (una ricarica). La vista non si sposta.
    Keep,
}

/// Un lavoro per il thread della ricerca.
struct Job {
    generation: u64,
    query: String,
    blocks: Vec<(u64, u64, Arc<str>)>,
}

/// I risultati di un lavoro.
struct Done {
    generation: u64,
    hits: Vec<Hit>,
}

/// Il thread della ricerca. Il parse dei blocchi resta nella cache di rune-kit di quel thread
/// (`doc_cached`, per sorgente), quindi i tasti dopo il primo non riparsano.
struct Worker {
    jobs: Sender<Job>,
    done: Receiver<Done>,
    latest: Arc<AtomicU64>,
}

impl Worker {
    fn spawn(waker: Arc<Mutex<Option<Waker>>>) -> Worker {
        let (jobs, inbox) = channel::<Job>();
        let (outbox, done) = channel();
        let latest = Arc::new(AtomicU64::new(0));
        let current = latest.clone();
        std::thread::Builder::new()
            .name("runemd-find".into())
            .spawn(move || {
                while let Ok(mut job) = inbox.recv() {
                    while let Ok(next) = inbox.try_recv() {
                        job = next;
                    }
                    let Some(hits) = search(&job, &current) else {
                        continue;
                    };
                    if outbox
                        .send(Done {
                            generation: job.generation,
                            hits,
                        })
                        .is_err()
                    {
                        return;
                    }
                    let w = waker.lock().unwrap().clone();
                    if let Some(w) = w {
                        w();
                    }
                }
            })
            .expect("thread della ricerca");
        Worker { jobs, done, latest }
    }
}

/// Cerca `job.query` in ogni blocco, in ordine. `None` se nel frattempo è arrivato un lavoro più
/// nuovo (si guarda ogni 64 blocchi).
fn search(job: &Job, latest: &AtomicU64) -> Option<Vec<Hit>> {
    let mut hits = Vec::new();
    for (at, (id, hash, src)) in job.blocks.iter().enumerate() {
        if at % 64 == 0 && latest.load(Ordering::Relaxed) != job.generation {
            return None;
        }
        hits.extend(
            find(src, &job.query, Case::Insensitive)
                .into_iter()
                .map(|ranges| Hit {
                    block: *id,
                    hash: *hash,
                    at,
                    ranges,
                }),
        );
    }
    Some(hits)
}

/// Lo stato della barra.
#[derive(Default)]
pub struct Find {
    pub edit: TextEdit,
    pub open: bool,
    hits: Vec<Hit>,
    /// Per ogni blocco con risultati: l'hash del sorgente cercato e i suoi in `hits`.
    by_block: HashMap<u64, (u64, Range<usize>)>,
    /// L'indice del risultato attivo in `hits`.
    current: usize,
    /// Da dove vengono i risultati di adesso.
    shown: Option<Source>,
    /// Il lavoro in corso: generazione, da dove, e quale risultato diventerà attivo.
    asked: Option<(u64, Source, Goal)>,
    generation: u64,
    /// Invio e Maiusc+Invio arrivati mentre un lavoro era in corso: si applicano ai risultati
    /// che arrivano.
    queued: isize,
    worker: Option<Worker>,
    waker: Arc<Mutex<Option<Waker>>>,
}

/// Come la vista segue il risultato attivo dopo `Find::poll`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reveal {
    /// Una ricarica: la vista non si sposta.
    No,
    /// Una ricerca nuova o un'altra scheda: la vista si sposta solo quanto basta.
    Show,
    /// Un Invio arrivato prima dei risultati: l'attivo al centro, come per ogni Invio.
    Center,
}

impl Find {
    /// Il `Waker` di `AppSpec::setup`: sveglia `poll` quando arrivano i risultati.
    pub fn set_waker(&mut self, waker: Waker) {
        *self.waker.lock().unwrap() = Some(waker);
    }

    /// Cmd+F: la barra si apre col fuoco; se era già aperta tiene la ricerca.
    pub fn show(&mut self) {
        self.open = true;
        self.edit.focused = true;
    }

    /// Esc: la barra si chiude, la ricerca si svuota e i segni spariscono. Il thread resta.
    pub fn close(&mut self) {
        self.edit = TextEdit::default();
        self.open = false;
        self.hits.clear();
        self.by_block.clear();
        self.current = 0;
        self.shown = None;
        self.asked = None;
        self.queued = 0;
        // Il lavoro in corso, se c'è, si ferma.
        self.generation += 1;
        if let Some(w) = &self.worker {
            w.latest.store(self.generation, Ordering::Relaxed);
        }
    }

    /// Chiede i risultati della ricerca di adesso in `doc`. Una ricerca vuota si applica subito.
    pub fn ask(&mut self, doc: &Document, goal: Goal) {
        let source = source_of(doc, &self.edit.text);
        // I risultati di un altro documento non valgono qui (gli id dei blocchi ripartono da 1 in
        // ogni documento): niente contatore, segni né Invio finché non arrivano quelli nuovi.
        if self.shown.as_ref().is_some_and(|s| s.0 != doc.id) {
            self.hits.clear();
            self.by_block.clear();
            self.current = 0;
            self.shown = None;
            self.queued = 0;
        }
        if self.edit.text.is_empty() {
            self.asked = None;
            self.apply(Vec::new(), source, goal, doc);
            return;
        }
        self.generation += 1;
        let waker = self.waker.clone();
        let worker = self.worker.get_or_insert_with(|| Worker::spawn(waker));
        worker.latest.store(self.generation, Ordering::Relaxed);
        let blocks = doc
            .blocks
            .iter()
            .map(|b| (b.id, b.hash, b.source.clone()))
            .collect();
        if worker
            .jobs
            .send(Job {
                generation: self.generation,
                query: self.edit.text.clone(),
                blocks,
            })
            .is_err()
        {
            self.worker = None;
            return;
        }
        self.asked = Some((self.generation, source, goal));
    }

    /// Un lavoro è in corso.
    #[cfg(test)]
    pub fn pending(&self) -> bool {
        self.asked.is_some()
    }

    /// Cosa chiedere perché i risultati seguano `doc`: niente se quelli mostrati o chiesti
    /// vengono già da lì; il primo risultato per un altro documento; il più vicino per lo stesso
    /// documento cambiato.
    pub fn stale(&self, doc: &Document) -> Option<Goal> {
        let target = source_of(doc, &self.edit.text);
        // Una ricarica mentre una ricerca nuova è sul thread: si richiede con lo stesso scopo.
        if let Some((_, s, goal)) = &self.asked
            && *s != target
            && s.0 == doc.id
            && s.3 == target.3
        {
            return Some(*goal);
        }
        let last = self
            .asked
            .as_ref()
            .map(|(_, s, _)| s)
            .or(self.shown.as_ref());
        match last {
            Some(s) if *s == target => None,
            Some(s) if s.0 == doc.id => Some(Goal::Keep),
            _ => Some(Goal::From(0)),
        }
    }

    /// Prende i risultati arrivati per l'ultimo lavoro e li applica su `doc` (il documento di
    /// adesso). `Some(reveal)` se sono cambiati: `reveal` se la vista va portata all'attivo
    /// (una ricerca nuova o un'altra scheda, non una ricarica).
    pub fn poll(&mut self, doc: &Document) -> Option<Reveal> {
        let done = self
            .worker
            .as_ref()
            .and_then(|w| w.done.try_iter().last())?;
        let (generation, source, goal) = self.asked.clone()?;
        if done.generation != generation {
            return None;
        }
        self.asked = None;
        self.apply(done.hits, source, goal, doc);
        if self.hits.is_empty() {
            self.queued = 0;
            return Some(Reveal::No);
        }
        if self.queued != 0 {
            let queued = std::mem::take(&mut self.queued);
            self.step(queued);
            return Some(Reveal::Center);
        }
        Some(if matches!(goal, Goal::From(_)) {
            Reveal::Show
        } else {
            Reveal::No
        })
    }

    fn apply(&mut self, hits: Vec<Hit>, source: Source, goal: Goal, doc: &Document) {
        let old = std::mem::replace(&mut self.hits, hits);
        let was = self.current;
        self.by_block.clear();
        let mut start = 0;
        for i in 1..=self.hits.len() {
            if i == self.hits.len() || self.hits[i].block != self.hits[start].block {
                self.by_block
                    .insert(self.hits[start].block, (self.hits[start].hash, start..i));
                start = i;
            }
        }
        self.shown = Some(source);
        self.current = match goal {
            Goal::From(n) => {
                let i = self.hits.partition_point(|h| h.at < n);
                if i < self.hits.len() { i } else { 0 }
            }
            Goal::Keep => self.nearest(&old, was, doc),
        };
    }

    /// Dopo una ricarica: il risultato attivo resta nel suo blocco se ci ha ancora risultati
    /// (stesso numero d'ordine nel blocco, o l'ultimo), altrimenti va al primo dal posto del
    /// blocco in giù se il blocco c'è ancora, o dal primo blocco dopo che c'è ancora (fra quelli
    /// che avevano risultati).
    fn nearest(&self, old: &[Hit], was: usize, doc: &Document) -> usize {
        if old.is_empty() || self.hits.is_empty() {
            return 0;
        }
        let was = was.min(old.len() - 1);
        let cur = &old[was];
        let local = old[..was]
            .iter()
            .rev()
            .take_while(|h| h.block == cur.block)
            .count();
        if let Some((_, r)) = self.by_block.get(&cur.block) {
            return r.start + local.min(r.len() - 1);
        }
        let at: HashMap<u64, usize> = doc
            .blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (b.id, i))
            .collect();
        let target = match old[was..].iter().find_map(|h| at.get(&h.block).copied()) {
            Some(j) => self.hits.partition_point(|h| h.at < j),
            None => {
                let before = old[..was]
                    .iter()
                    .rev()
                    .find_map(|h| at.get(&h.block).copied())
                    .unwrap_or(0);
                self.hits.partition_point(|h| h.at <= before)
            }
        };
        target.min(self.hits.len() - 1)
    }

    /// Invio o Maiusc+Invio: il risultato dopo (`1`) o prima (`-1`), in giro. Vero se è
    /// cambiato adesso; con un lavoro in corso il passo aspetta i risultati.
    pub fn enter(&mut self, delta: isize) -> bool {
        if self.asked.is_some() {
            self.queued += delta;
            return false;
        }
        self.step(delta);
        !self.hits.is_empty()
    }

    /// Il risultato dopo (`1`) o prima (`-1`), in giro.
    pub fn step(&mut self, delta: isize) {
        if !self.hits.is_empty() {
            self.current =
                (self.current as isize + delta).rem_euclid(self.hits.len() as isize) as usize;
        }
    }

    pub fn count(&self) -> usize {
        self.hits.len()
    }

    /// L'indice del risultato attivo, se ce n'è uno.
    pub fn current(&self) -> Option<usize> {
        (self.current < self.hits.len()).then_some(self.current)
    }

    /// Il contatore: "3 di 12", "Nessun risultato", niente con la ricerca vuota o prima dei
    /// primi risultati di questa ricerca.
    pub fn label(&self) -> Option<String> {
        if self.edit.text.is_empty() || self.shown.is_none() {
            return None;
        }
        Some(match self.current() {
            Some(i) => format!("{} di {}", i + 1, self.count()),
            None => NO_RESULTS.to_owned(),
        })
    }

    /// I segni del blocco `id` (sorgente con hash `hash`) per `Markdown::highlights`: il
    /// risultato attivo con la chiave `<row_key>/find-current`, gli altri senza chiave. Un blocco
    /// cambiato dopo la ricerca non ne ha finché non arrivano i risultati nuovi.
    pub fn highlights(&self, id: u64, hash: u64, row_key: &str) -> Marks {
        let Some((_, r)) = self
            .by_block
            .get(&id)
            .filter(|(h, _)| self.open && *h == hash)
        else {
            return Vec::new();
        };
        r.clone()
            .map(|i| {
                let h = if i == self.current {
                    Highlight::current().key(current_key(row_key))
                } else {
                    Highlight::new()
                };
                (self.hits[i].ranges.clone(), h)
            })
            .collect()
    }

    /// La chiave da passare a `Tx::scroll_center_in_instant("doc", …)` per il risultato attivo.
    pub fn reveal_key(&self, doc: &Document) -> Option<String> {
        let h = self.hits.get(self.current)?;
        let i = match doc.blocks.get(h.at) {
            Some(b) if b.id == h.block => h.at,
            _ => doc.blocks.iter().position(|b| b.id == h.block)?,
        };
        Some(current_key(&super::doc_view::row_key(i, doc)))
    }
}

fn current_key(row_key: &str) -> String {
    format!("{row_key}/{CURRENT}")
}

/// Il campo nella barra delle schede: icona, ricerca e contatore in `fg3`.
pub fn field(s: &App, theme: &Theme) -> El {
    let input = text_input(&s.find.edit, ui_font())
        .single_line()
        .key("find-input")
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .color(theme.fg)
        .placeholder(PLACEHOLDER, theme.fg2)
        .caret(theme.fg, theme.sel)
        .label(PLACEHOLDER)
        .on_edit(|s: &mut App| &mut s.find.edit)
        .on_input_change(|s: &mut App, _: &mut Tx| s.find_changed())
        .on_input_key(on_key);
    let mut e = row()
        .key("find")
        .w(FIND_W)
        .h(FIELD_H)
        .px(ROW_PAD)
        .gap(ROW_GAP)
        .center_y()
        .radius(R_FIELD)
        .fill(theme.field)
        .border(HAIRLINE, theme.line)
        .child(icon(&glyphs::SEARCH, ICON).color(theme.fg2))
        .child(input);
    if let Some(label) = s.find.label() {
        e = e.child(
            text(label)
                .key("find-count")
                .font(small_font())
                .color(theme.fg3),
        );
    }
    e
}

/// I tasti del campo prima dell'editor: Invio, Maiusc+Invio, Esc. Vero se li prende.
fn on_key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if s.find.edit.preedit().is_some() {
        return false;
    }
    match &ev.key {
        Key::Escape => {
            if !tx.key_repeat() {
                s.find.close();
            }
            true
        }
        Key::Enter if !ev.mods.alt && !ev.mods.cmd && !ev.mods.ctrl => {
            s.find_step(if ev.mods.shift { -1 } else { 1 }, tx);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn fixture(name: &str) -> Document {
        Document::load(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .join(name),
        )
        .unwrap()
    }

    /// Aspetta i risultati del thread della ricerca e li applica su `doc`.
    fn wait(f: &mut Find, doc: &Document) {
        let t = std::time::Instant::now();
        while f.pending() {
            assert!(t.elapsed().as_secs() < 10, "risultati mai arrivati");
            f.poll(doc);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    fn search(f: &mut Find, doc: &Document, q: &str) {
        f.edit.replace_text(q);
        f.ask(doc, Goal::From(0));
        wait(f, doc);
    }

    fn searched(doc: &Document, q: &str) -> Find {
        let mut f = Find::default();
        f.show();
        search(&mut f, doc, q);
        f
    }

    /// Le occorrenze di `q` nel sorgente, senza maiuscole, come `grep -io q | wc -l`.
    fn grep_count(src: &str, q: &str) -> usize {
        src.to_lowercase().matches(&q.to_lowercase()).count()
    }

    /// "rune" su `normal.md` dà quanti risultati dà `grep -io rune fixtures/normal.md | wc -l`
    /// (0: il generatore non usa la parola). Lo stesso confronto con una parola che c'è,
    /// "albero": `grep` conta anche le destinazioni dei link (`](https://example.com/albero)`),
    /// che non sono testo disegnato e `markdown::find` salta; si tolgono dal conto.
    #[test]
    fn matches_across_blocks() {
        let src = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/normal.md"),
        )
        .unwrap();
        let doc = fixture("normal.md");
        assert_eq!(searched(&doc, "rune").count(), grep_count(&src, "rune"));
        let in_links = src
            .match_indices("](")
            .filter(|(i, _)| {
                src[i + 2..]
                    .split(')')
                    .next()
                    .is_some_and(|d| d.to_lowercase().contains("albero"))
            })
            .count();
        let found = searched(&doc, "albero");
        assert!(in_links > 0, "il fixture ha link verso /albero");
        assert_eq!(found.count(), grep_count(&src, "albero") - in_links);
        let blocks: std::collections::HashSet<u64> = found.hits.iter().map(|h| h.block).collect();
        assert!(
            blocks.len() > 10,
            "risultati in molti blocchi: {}",
            blocks.len()
        );
        assert_eq!(
            searched(&doc, "ALBERO").count(),
            found.count(),
            "senza maiuscole"
        );
    }

    /// Next e prev in giro, sul modello.
    #[test]
    fn step_wraps_both_ways() {
        let doc = Document::from_source(Path::new("t.md"), "uno zebra\n\ndue\n\nZebra e zebra\n");
        let mut f = searched(&doc, "zebra");
        assert_eq!((f.current(), f.count()), (Some(0), 3));
        assert_eq!(f.label().as_deref(), Some("1 di 3"));
        f.step(-1);
        assert_eq!(
            f.label().as_deref(),
            Some("3 di 3"),
            "prima del primo: l'ultimo"
        );
        f.step(1);
        assert_eq!(f.current(), Some(0), "dopo l'ultimo: il primo");
        search(&mut f, &doc, "giraffa");
        assert_eq!(f.label().as_deref(), Some(NO_RESULTS));
        f.step(1);
        assert_eq!(f.current(), None);
        search(&mut f, &doc, "");
        assert_eq!(f.label(), None, "ricerca vuota: nessun contatore");
    }

    /// Ricarica: il risultato attivo resta nel suo blocco (per id) anche se sopra arrivano
    /// blocchi con altri risultati; se il blocco sparisce va al primo dopo.
    #[test]
    fn refresh_keeps_the_current_block() {
        let mut doc = Document::from_source(
            Path::new("t.md"),
            "a zebra\n\nb\n\nc zebra zebra\n\nd zebra\n",
        );
        let mut f = searched(&doc, "zebra");
        f.step(2);
        assert_eq!(f.label().as_deref(), Some("3 di 4"));
        let block = f.hits[f.current].block;
        doc.update("nuovo zebra\n\naltro zebra\n\na zebra\n\nb\n\nc zebra zebra\n\nd zebra\n");
        f.ask(&doc, Goal::Keep);
        wait(&mut f, &doc);
        assert_eq!(f.hits[f.current].block, block, "stesso blocco");
        assert_eq!(
            f.label().as_deref(),
            Some("5 di 6"),
            "stesso numero d'ordine nel blocco"
        );
        doc.update("nuovo zebra\n\naltro zebra\n\na zebra\n\nb\n\nd zebra\n");
        f.ask(&doc, Goal::Keep);
        wait(&mut f, &doc);
        assert_eq!(
            f.label().as_deref(),
            Some("4 di 4"),
            "blocco sparito: il primo dopo"
        );
    }

    // --- sul runtime, con la vista dell'app ---

    use rune::{Env, KeyEvent, Mods, Runtime};

    use crate::app::Mode;

    fn settle(rt: &mut Runtime<App>) {
        for _ in 0..4000 {
            if !rt.frame(1.0 / 120.0) {
                return;
            }
        }
        panic!("la vista non si ferma");
    }

    /// 200 blocchi; "zebra" nei blocchi 3, 120 e 190 (due volte nel 190).
    fn long() -> String {
        (0..200)
            .map(|n| match n {
                3 | 120 => format!("Paragrafo {n} con una zebra in mezzo al testo che continua."),
                190 => format!("Paragrafo {n}: zebra e ancora Zebra."),
                _ if n % 10 == 0 => format!("## Sezione {n}"),
                _ => format!("Paragrafo {n} di prova, abbastanza lungo da andare a capo nella colonna del lettore, con altre parole."),
            })
            .collect::<Vec<_>>()
            .join("\n\n")
            + "\n"
    }

    fn open(sources: &[(&str, &str)]) -> Runtime<App> {
        let mut state = App::bare(crate::index::Library::load(Vec::new(), None));
        state.set_style(crate::ui::markdown_style::style(&Theme::for_mode(true), crate::ui::theme::BODY_TEXT));
        for (name, src) in sources {
            state.push_tab(Document::from_source(Path::new(name), src));
        }
        let mut rt = Runtime::new(state, crate::view, (900.0, 1000.0), Env::new(true, 2.0));
        rt.on_key = Some(crate::on_key);
        rt.frame(0.0);
        settle(&mut rt);
        rt
    }

    fn key(rt: &mut Runtime<App>, key: Key, mods: Mods) {
        rt.key(KeyEvent { key, mods });
        settle(rt);
    }

    fn cmd(rt: &mut Runtime<App>, c: char) {
        key(
            rt,
            Key::Command(c),
            Mods {
                cmd: true,
                logo: true,
                ..Mods::default()
            },
        );
    }

    /// Come `poll` dell'app: aspetta e applica i risultati del thread della ricerca.
    fn wait_find(rt: &mut Runtime<App>) {
        let t = std::time::Instant::now();
        loop {
            rt.update(|s, tx| {
                s.poll_find(tx);
            });
            if !rt.state.find.pending() {
                break;
            }
            assert!(t.elapsed().as_secs() < 10, "risultati mai arrivati");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        settle(rt);
    }

    fn type_text(rt: &mut Runtime<App>, t: &str) {
        for c in t.chars() {
            key(rt, Key::Text(c.to_string()), Mods::default());
            wait_find(rt);
        }
    }

    /// Invio (o Maiusc+Invio): il segno attivo è in vista al primo frame dopo il tasto.
    fn enter(rt: &mut Runtime<App>, shift: bool) {
        rt.key(KeyEvent {
            key: Key::Enter,
            mods: Mods {
                shift,
                ..Mods::default()
            },
        });
        rt.frame(1.0 / 120.0);
        assert!(current_in_view(rt), "segno attivo in vista al primo frame");
        settle(rt);
    }

    fn count_text(rt: &Runtime<App>) -> Option<String> {
        rt.elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("find-count"))
            .and_then(|e| e.text)
    }

    /// I segni di Cerca disegnati: (chiave, rect visibile).
    fn marks(rt: &Runtime<App>) -> Vec<(Option<String>, [f32; 4])> {
        rt.elements()
            .into_iter()
            .filter(|e| e.role == "mark" && !e.exiting)
            .map(|e| (e.key, e.rect))
            .collect()
    }

    /// Il segno attivo è disegnato dentro la vista del documento.
    fn current_in_view(rt: &Runtime<App>) -> bool {
        let els = rt.elements();
        let doc = els
            .iter()
            .find(|e| e.key.as_deref() == Some("doc"))
            .expect("vista doc")
            .rect;
        els.iter()
            .filter(|e| {
                e.role == "mark"
                    && e.key
                        .as_deref()
                        .is_some_and(|k| k.ends_with("/find-current"))
            })
            .any(|e| {
                let r = e.rect;
                r[1] >= doc[1] - 0.5 && r[1] + r[3] <= doc[1] + doc[3] + 0.5
            })
    }

    fn current_block(rt: &Runtime<App>) -> usize {
        let f = &rt.state.find;
        f.hits[f.current().expect("un risultato attivo")].at
    }

    /// Cmd+F, si scrive: il contatore dice "1 di 4" e il primo risultato è in vista; Invio
    /// porta al blocco 120, che non era montato, e lo centra; Maiusc+Invio torna indietro; in
    /// giro ai due capi.
    #[test]
    fn enter_and_shift_enter_walk_the_matches_and_reveal_unmounted_blocks() {
        let mut rt = open(&[("a.md", &long())]);
        cmd(&mut rt, 'f');
        assert!(
            rt.state.find.open && rt.state.find.edit.focused,
            "barra aperta col fuoco"
        );
        assert!(
            rt.elements()
                .iter()
                .any(|e| e.key.as_deref() == Some("find-input"))
        );
        type_text(&mut rt, "ZEBRA");
        assert_eq!(count_text(&rt).as_deref(), Some("1 di 4"));
        assert_eq!(current_block(&rt), 3);
        assert!(current_in_view(&rt));
        let doc = rt.state.active_document();
        let far = crate::ui::doc_view::row_key(120, doc);
        assert!(
            !rt.elements()
                .iter()
                .any(|e| e.key.as_deref() == Some(far.as_str())),
            "il blocco 120 non è montato"
        );
        enter(&mut rt, false);
        assert_eq!(
            (count_text(&rt).as_deref(), current_block(&rt)),
            (Some("2 di 4"), 120)
        );
        assert!(
            current_in_view(&rt),
            "il blocco 120 è montato e il segno è in vista"
        );
        enter(&mut rt, false);
        enter(&mut rt, false);
        assert_eq!(
            (count_text(&rt).as_deref(), current_block(&rt)),
            (Some("4 di 4"), 190)
        );
        assert!(current_in_view(&rt));
        enter(&mut rt, false);
        assert_eq!(
            (count_text(&rt).as_deref(), current_block(&rt)),
            (Some("1 di 4"), 3),
            "in giro all'inizio"
        );
        assert!(current_in_view(&rt));
        enter(&mut rt, true);
        assert_eq!(
            (count_text(&rt).as_deref(), current_block(&rt)),
            (Some("4 di 4"), 190),
            "Maiusc+Invio in giro alla fine"
        );
        assert!(current_in_view(&rt));
        enter(&mut rt, true);
        assert_eq!(count_text(&rt).as_deref(), Some("3 di 4"));
        // Uno solo dei segni disegnati è quello attivo.
        let drawn = marks(&rt);
        assert!(
            drawn.len() >= 2,
            "i due risultati del blocco 190: {drawn:?}"
        );
        assert_eq!(
            drawn
                .iter()
                .filter(|(k, _)| k.as_deref().is_some_and(|k| k.ends_with("/find-current")))
                .count(),
            1
        );
        type_text(&mut rt, "q");
        assert_eq!(count_text(&rt).as_deref(), Some(NO_RESULTS));
        assert!(marks(&rt).is_empty());
    }

    /// Esc chiude la barra e toglie i segni; i tasti tornano al documento (Cmd+A arriva a
    /// `on_key` e segna "tutto il file").
    #[test]
    fn escape_closes_clears_and_gives_keys_back_to_the_document() {
        let mut rt = open(&[("a.md", &long())]);
        cmd(&mut rt, 'f');
        type_text(&mut rt, "zebra");
        assert!(!marks(&rt).is_empty());
        key(&mut rt, Key::Escape, Mods::default());
        assert!(!rt.state.find.open && rt.state.find.edit.text.is_empty());
        assert!(
            !rt.elements()
                .iter()
                .any(|e| e.key.as_deref() == Some("find")),
            "barra chiusa"
        );
        assert!(marks(&rt).is_empty(), "segni tolti");
        cmd(&mut rt, 'a');
        assert_ne!(
            rt.state.active_document().select_all.get(),
            crate::doc::select_all::SelectAll::No,
            "Cmd+A arriva al documento"
        );
        cmd(&mut rt, 'f');
        assert!(
            rt.state.find.open && rt.state.find.edit.text.is_empty(),
            "si riapre vuota"
        );
    }

    /// Una ricarica mentre si cerca: i risultati si ricalcolano, l'attivo resta nel suo blocco
    /// e la vista non torna in cima.
    #[test]
    fn a_reload_keeps_the_current_match_and_the_view() {
        let mut rt = open(&[("a.md", &long())]);
        cmd(&mut rt, 'f');
        type_text(&mut rt, "zebra");
        enter(&mut rt, false);
        assert_eq!(current_block(&rt), 120);
        let id = rt.state.active_document().blocks[120].id;
        let top = |rt: &Runtime<App>| rt.state.tab().unwrap().visible_first;
        let before = top(&rt);
        let src = format!(
            "In testa una zebra nuova.\n\nE un'altra zebra.\n\n{}",
            long()
        );
        rt.update(|s, _| {
            s.tab_mut().unwrap().doc.update(&src);
        });
        wait_find(&mut rt);
        let f = &rt.state.find;
        assert_eq!(f.count(), 6);
        assert_eq!(f.hits[f.current().unwrap()].block, id, "stesso blocco");
        assert_eq!(count_text(&rt).as_deref(), Some("4 di 6"));
        assert_eq!(top(&rt), before, "la vista resta dov'era");
        assert!(current_in_view(&rt));
    }

    /// Una barra per finestra: cambiata la scheda la ricerca rigira sul file nuovo, dal primo
    /// risultato, e lo porta in vista.
    #[test]
    fn switching_tab_reruns_the_query_from_the_first_match() {
        let other = (0..150)
            .map(|n| {
                if n == 100 || n == 140 {
                    format!("Altra {n} zebra.")
                } else {
                    format!("Altra riga {n} del secondo file, abbastanza lunga da occupare spazio.")
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let mut rt = open(&[("a.md", &long()), ("b.md", &other)]);
        cmd(&mut rt, '1');
        cmd(&mut rt, 'f');
        type_text(&mut rt, "zebra");
        enter(&mut rt, false);
        enter(&mut rt, false);
        assert_eq!(count_text(&rt).as_deref(), Some("3 di 4"));
        cmd(&mut rt, '2');
        wait_find(&mut rt);
        assert_eq!(rt.state.active, 1);
        assert!(rt.state.find.open);
        assert_eq!(count_text(&rt).as_deref(), Some("1 di 2"));
        assert_eq!(current_block(&rt), 100);
        assert!(
            current_in_view(&rt),
            "il primo risultato del file nuovo è in vista"
        );
        cmd(&mut rt, '1');
        wait_find(&mut rt);
        assert_eq!(
            (count_text(&rt).as_deref(), current_block(&rt)),
            (Some("1 di 4"), 3),
            "torna al primo"
        );
        assert!(current_in_view(&rt));
    }

    /// Cmd+P, Cmd+N e Cmd+F si chiudono a vicenda; nel sorgente Cmd+F non fa niente.
    #[test]
    fn one_field_at_a_time_and_nothing_in_source() {
        let mut rt = open(&[("a.md", &long())]);
        cmd(&mut rt, 'f');
        type_text(&mut rt, "zebra");
        cmd(&mut rt, 'p');
        assert!(!rt.state.find.open && rt.state.sidebar.quick.edit.focused);
        type_text(&mut rt, "x");
        cmd(&mut rt, 'f');
        assert!(
            rt.state.find.open && !rt.state.sidebar.quick.active(),
            "Cmd+F chiude Cmd+P"
        );
        cmd(&mut rt, 'n');
        assert!(!rt.state.find.open && rt.state.sidebar.new_file.open);
        cmd(&mut rt, 'f');
        assert!(
            rt.state.find.open && !rt.state.sidebar.new_file.open,
            "Cmd+F chiude Cmd+N"
        );
        rt.update(|s, tx| {
            s.toggle_source(tx);
        });
        settle(&mut rt);
        assert!(matches!(rt.state.tab().unwrap().mode, Mode::Source(_)));
        assert!(!rt.state.find.open, "entrare nel sorgente chiude Cerca");
        assert!(!rt.state.open_find(), "nel sorgente Cmd+F non apre niente");
        rt.state.sync_find();
        assert!(!rt.state.find.open);
    }

    /// Scrive senza aspettare i risultati (il thread della ricerca risponde dopo).
    fn type_now(rt: &mut Runtime<App>, t: &str) {
        for c in t.chars() {
            key(rt, Key::Text(c.to_string()), Mods::default());
        }
    }

    /// Una ricarica mentre la ricerca appena scritta è ancora sul thread: i risultati partono
    /// comunque dal primo blocco visibile e l'attivo è in vista (non "vicino" ai risultati di
    /// prima).
    #[test]
    fn a_reload_during_a_new_query_still_starts_from_the_view() {
        let mut rt = open(&[("a.md", &long())]);
        let key110 = crate::ui::doc_view::row_key(110, rt.state.active_document());
        rt.update(|_, tx| {
            tx.scroll_center_in_instant("doc", key110.clone());
            tx.scroll_key_to(key110, 0.0);
        });
        settle(&mut rt);
        let first = rt.state.tab().unwrap().visible_first;
        let at = rt
            .state
            .active_document()
            .blocks
            .iter()
            .position(|b| b.id == first)
            .unwrap();
        assert!(
            (100..=120).contains(&at),
            "vista vicino al blocco 110: {at}"
        );
        cmd(&mut rt, 'f');
        type_now(&mut rt, "zebra");
        assert!(rt.state.find.pending());
        let src = format!("{}\n\nIn fondo, senza la parola.\n", long());
        rt.update(|s, _| {
            s.tab_mut().unwrap().doc.update(&src);
        });
        wait_find(&mut rt);
        assert_eq!(count_text(&rt).as_deref(), Some("2 di 4"));
        assert_eq!(current_block(&rt), 120);
        assert!(current_in_view(&rt));
    }

    /// Cambiata la scheda, finché non arrivano i risultati del file nuovo non resta niente di
    /// quelli dell'altro: né contatore, né segni, né un Invio che porta a un blocco a caso.
    #[test]
    fn after_a_tab_switch_the_old_results_are_gone_until_the_new_arrive() {
        let other = (0..40)
            .map(|n| {
                if n == 30 {
                    format!("Riga {n} zebra.")
                } else {
                    format!("Riga {n}.")
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let mut rt = open(&[("a.md", &long()), ("b.md", &other)]);
        cmd(&mut rt, '1');
        cmd(&mut rt, 'f');
        type_text(&mut rt, "zebra");
        assert_eq!(count_text(&rt).as_deref(), Some("1 di 4"));
        cmd(&mut rt, '2');
        assert!(rt.state.find.pending());
        assert_eq!(count_text(&rt), None, "nessun contatore dell'altro file");
        assert_eq!(rt.state.find.count(), 0);
        assert!(marks(&rt).is_empty());
        wait_find(&mut rt);
        assert_eq!(count_text(&rt).as_deref(), Some("1 di 1"));
    }

    /// Invio mentre i risultati sono sul thread: il passo si applica quando arrivano.
    #[test]
    fn enter_while_pending_steps_once_the_results_arrive() {
        let mut rt = open(&[("a.md", &long())]);
        cmd(&mut rt, 'f');
        type_now(&mut rt, "zebra");
        assert!(rt.state.find.pending());
        rt.key(KeyEvent {
            key: Key::Enter,
            mods: Mods::default(),
        });
        rt.frame(1.0 / 120.0);
        assert!(
            rt.state.find.pending(),
            "Invio arrivato prima dei risultati"
        );
        wait_find(&mut rt);
        assert_eq!(
            (count_text(&rt).as_deref(), current_block(&rt)),
            (Some("2 di 4"), 120)
        );
        assert!(current_in_view(&rt));
    }

    /// Esc ferma il lavoro in corso sul thread.
    #[test]
    fn close_stops_the_job_in_flight() {
        let doc = Document::from_source(Path::new("t.md"), "zebra\n");
        let mut f = Find::default();
        f.show();
        f.edit.replace_text("zebra");
        f.ask(&doc, Goal::From(0));
        let asked = f.generation;
        f.close();
        assert_ne!(
            f.worker.as_ref().unwrap().latest.load(Ordering::Relaxed),
            asked,
            "il lavoro chiesto non è più l'ultimo"
        );
        assert!(!f.pending());
    }

    /// Misura (report del task 24): il costo sul thread della UI di un tasto di Cerca su
    /// `fixtures/big.md`, con la vista dell'app su un runtime headless. Ogni ricerca si digita
    /// lettera per lettera. Per ogni tasto: il tasto e il suo frame (campo e richiesta al thread),
    /// e, quando arrivano i risultati, `poll_find` e il frame che li disegna (segni, rivelazione);
    /// a parte, quanto il thread della ricerca impiega a rispondere.
    /// `cargo test --profile perf find_cost -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn find_cost() {
        use std::time::Instant;
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/big.md");
        let queries = [
            "finestra",
            "la",
            "commit",
            "zzzq",
            "albero colonna",
            "e",
            "rendering",
            "the",
            "sistema",
            "x",
        ];
        let pct = |ms: &mut Vec<f64>, q: f64| {
            ms.sort_by(f64::total_cmp);
            ms[((ms.len() - 1) as f64 * q).round() as usize]
        };
        let mut state = App::bare(crate::index::Library::load(Vec::new(), None));
        state.set_style(crate::ui::markdown_style::style(&Theme::for_mode(true), crate::ui::theme::BODY_TEXT));
        state.push_tab(Document::load(&path).unwrap());
        let mut rt = Runtime::new(state, crate::view, (900.0, 1000.0), Env::new(true, 2.0));
        rt.on_key = Some(crate::on_key);
        rt.frame(0.0);
        settle(&mut rt);
        cmd(&mut rt, 'f');
        let (mut keyed, mut applied, mut worker, mut ui) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        let mut first = None;
        for round in 0..3 {
            for q in queries {
                for c in q.chars() {
                    let t = Instant::now();
                    rt.key(KeyEvent {
                        key: Key::Text(c.to_string()),
                        mods: Mods::default(),
                    });
                    rt.frame(1.0 / 120.0);
                    let k = t.elapsed().as_secs_f64() * 1000.0;
                    let asked = Instant::now();
                    let d = loop {
                        if let Ok(d) = rt.state.find.worker.as_ref().unwrap().done.try_recv() {
                            break d;
                        }
                        std::thread::sleep(std::time::Duration::from_micros(100));
                    };
                    let w = asked.elapsed().as_secs_f64() * 1000.0;
                    // Quello che fa `poll_find` quando i risultati arrivano, e il frame dopo.
                    let t = Instant::now();
                    let (generation, source, goal) = rt.state.find.asked.clone().unwrap();
                    assert_eq!(d.generation, generation);
                    rt.update(|s, tx| {
                        s.find.asked = None;
                        let doc = &s.tabs[s.active].doc;
                        s.find.apply(d.hits, source, goal, doc);
                        if let Some(key) = s.find.reveal_key(doc) {
                            tx.scroll_into_view_in("doc", key);
                        }
                    });
                    rt.frame(1.0 / 120.0);
                    let a = t.elapsed().as_secs_f64() * 1000.0;
                    settle(&mut rt);
                    if round == 0 && first.is_none() {
                        first = Some(w);
                        continue;
                    }
                    keyed.push(k);
                    applied.push(a);
                    worker.push(w);
                    ui.push(k.max(a));
                }
                for _ in 0..q.chars().count() {
                    rt.key(KeyEvent {
                        key: Key::Backspace,
                        mods: Mods::default(),
                    });
                    wait_find(&mut rt);
                }
            }
        }
        // Il riferimento: un salto della vista su big.md senza Cerca (lo stesso `scroll_center_in_instant`
        // su una riga lontana, non montata), come i salti che fanno i risultati lontani.
        let mut jump = Vec::new();
        for k in 0..60usize {
            let key = crate::ui::doc_view::row_key(
                (k * 977) % rt.state.active_document().blocks.len(),
                rt.state.active_document(),
            );
            let t = Instant::now();
            rt.update(|_, tx| tx.scroll_center_in_instant("doc", key));
            rt.frame(1.0 / 120.0);
            jump.push(t.elapsed().as_secs_f64() * 1000.0);
            settle(&mut rt);
        }
        eprintln!(
            "find_cost big.md {} blocchi; primo tasto, risposta del thread (parse dei blocchi) {:.2} ms",
            rt.state.active_document().blocks.len(),
            first.unwrap()
        );
        for (name, ms) in [
            ("tasto + frame", &mut keyed),
            ("risultati + frame", &mut applied),
            ("thread UI, il peggiore dei due", &mut ui),
            ("risposta del thread della ricerca", &mut worker),
            ("riferimento: salto della vista senza Cerca", &mut jump),
        ] {
            eprintln!(
                "find_cost {name}: n={} p50={:.3} ms p95={:.3} ms max={:.3} ms",
                ms.len(),
                pct(ms, 0.5),
                pct(ms, 0.95),
                pct(ms, 1.0)
            );
        }
    }
}
