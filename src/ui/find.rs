//! Find in file (Cmd+F): a field in the tab bar searches the drawn text of the
//! blocks of the front tab (`rune_kit::markdown::find`, case-insensitive). Return goes to the
//! next result, Shift+Return to the previous one, wrapping around; Esc closes and clears the marks.
//!
//! The search runs on its own thread: `markdown::find` costs ~5 ms per MB even with no
//! results (on `big.md`, 1 MB, p95 8.7 ms on the UI thread: over the threshold). The key sends
//! the work to the thread and returns at once; the results arrive with the `Waker` and `poll` applies them
//! (`App::poll_find`). A job overtaken by a newer one stops halfway.
//!
//! The current result has the key `<row>/find-current`: `Tx::scroll_center_in_instant` mounts the
//! `virtual_rows` row even if it is outside the mounted window, then centres the mark in the row.
//!
//! One bar per window: when the tab changes, the search runs again on the new file and starts from the
//! first result. After a reload the results are recomputed and the current one stays as
//! close as possible to the block it was in, without moving the view.

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

/// Bar texts (approved).
pub const PLACEHOLDER: &str = "Find in file";
pub const NO_RESULTS: &str = "No results";

/// The Find marks of a block, as `Markdown::highlights` takes them.
pub type Marks = Vec<(Vec<Range<usize>>, Highlight)>;

/// The key of the current mark inside its row.
const CURRENT: &str = "find-current";

/// A result: the block (id, source hash, position in the document) and the bytes of its
/// source that it covers.
#[derive(Clone, Debug, PartialEq)]
struct Hit {
    block: u64,
    hash: u64,
    at: usize,
    ranges: Vec<Range<usize>>,
}

/// Where the results come from: document (id, revision, file hash) and query.
type Source = (u64, u64, u64, String);

fn source_of(doc: &Document, query: &str) -> Source {
    (doc.id, doc.revision, doc.file_hash, query.to_owned())
}

/// Which result becomes current when the results arrive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Goal {
    /// The first one from the block at position `n` down, wrapping around (a new search; 0 for another
    /// tab). The view brings it to the centre.
    From(usize),
    /// The closest to the previous one (a reload). The view does not move.
    Keep,
}

/// A job for the search thread.
struct Job {
    generation: u64,
    query: String,
    /// Position in the document, id, hash and source of each block to search.
    blocks: Vec<(usize, u64, u64, Arc<str>)>,
}

/// The results of a job.
struct Done {
    generation: u64,
    hits: Vec<Hit>,
}

/// The search thread. The block parse stays in that thread's rune-kit cache
/// (`doc_cached`, per source), so the keys after the first one do not parse again.
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
            .expect("search thread");
        Worker { jobs, done, latest }
    }
}

/// Searches `job.query` in each block, in order. `None` if a newer job arrived in the
/// meantime (checked every 64 blocks).
fn search(job: &Job, latest: &AtomicU64) -> Option<Vec<Hit>> {
    let mut hits = Vec::new();
    for (n, &(at, id, hash, ref src)) in job.blocks.iter().enumerate() {
        if n % 64 == 0 && latest.load(Ordering::Relaxed) != job.generation {
            return None;
        }
        hits.extend(
            find(src, &job.query, Case::Insensitive)
                .into_iter()
                .map(|ranges| Hit {
                    block: id,
                    hash,
                    at,
                    ranges,
                }),
        );
    }
    Some(hits)
}

/// The bar state.
#[derive(Default)]
pub struct Find {
    pub edit: TextEdit,
    pub open: bool,
    hits: Vec<Hit>,
    /// For each block with results: the hash of the searched source and its entries in `hits`.
    by_block: HashMap<u64, (u64, Range<usize>)>,
    /// The index of the current result in `hits`.
    current: usize,
    /// Where the current results come from.
    shown: Option<Source>,
    /// The running job: generation, where from, and which result will become current.
    asked: Option<(u64, Source, Goal)>,
    generation: u64,
    /// Return and Shift+Return that arrived while a job was running: they apply to the results
    /// that arrive.
    queued: isize,
    worker: Option<Worker>,
    waker: Arc<Mutex<Option<Waker>>>,
}

/// How the view follows the current result after `Find::poll`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reveal {
    /// A reload: the view does not move.
    No,
    /// A new search or another tab: the view moves only as much as needed.
    Show,
    /// A Return that arrived before the results: the current one in the centre, as for every Return.
    Center,
}

impl Find {
    /// The `Waker` from `AppSpec::setup`: wakes `poll` when the results arrive.
    pub fn set_waker(&mut self, waker: Waker) {
        *self.waker.lock().unwrap() = Some(waker);
    }

    /// Cmd+F: the bar opens with focus; if it was already open it keeps the query.
    pub fn show(&mut self) {
        self.open = true;
        self.edit.focused = true;
    }

    /// Esc: the bar closes, the query empties and the marks go away. The thread stays.
    pub fn close(&mut self) {
        self.edit = TextEdit::default();
        self.open = false;
        self.hits.clear();
        self.by_block.clear();
        self.current = 0;
        self.shown = None;
        self.asked = None;
        self.queued = 0;
        // The running job, if any, stops.
        self.generation += 1;
        if let Some(w) = &self.worker {
            w.latest.store(self.generation, Ordering::Relaxed);
        }
    }

    /// Asks for the results of the current query in `doc`. An empty query applies at once.
    pub fn ask(&mut self, doc: &Document, goal: Goal) {
        let source = source_of(doc, &self.edit.text);
        // The results of another document are not valid here (block ids start again from 1 in
        // each document): no counter, marks or Return until the new ones arrive.
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
        // Footnote definition blocks are not drawn: notes are searched where they
        // show, in the notes block at the end (`Notes::block`), after all the others.
        let blocks = doc
            .blocks
            .iter()
            .enumerate()
            .filter(|(_, b)| crate::doc::footnotes::definitions(&b.source).is_none())
            .map(|(at, b)| (at, b.id, b.hash, b.source.clone()))
            .chain(doc.notes.block().map(|b| (doc.blocks.len(), b.id, b.hash, b.source)))
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

    /// A job is running.
    #[cfg(test)]
    pub fn pending(&self) -> bool {
        self.asked.is_some()
    }

    /// What to ask so the results follow `doc`: nothing if the ones shown or asked for
    /// already come from there; the first result for another document; the closest for the same
    /// documento cambiato.
    pub fn stale(&self, doc: &Document) -> Option<Goal> {
        let target = source_of(doc, &self.edit.text);
        // A reload while a new search is on the thread: ask again with the same target.
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

    /// Takes the results that arrived for the last job and applies them to `doc` (the current
    /// document). `Some(reveal)` if they changed: `reveal` if the view must be brought to the current one
    /// (a new search or another tab, not a reload).
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

    /// After a reload: the current result stays in its block if it still has results there
    /// (same ordinal in the block, or the last one), otherwise it goes to the first one from the place of the
    /// block down if the block is still there, or from the first later block that is still there (among the ones
    /// that had results).
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

    /// Return or Shift+Return: the next result (`1`) or the previous one (`-1`), wrapping around. True if it
    /// changed now; with a running job the step waits for the results.
    pub fn enter(&mut self, delta: isize) -> bool {
        if self.asked.is_some() {
            self.queued += delta;
            return false;
        }
        self.step(delta);
        !self.hits.is_empty()
    }

    /// The next result (`1`) or the previous one (`-1`), wrapping around.
    pub fn step(&mut self, delta: isize) {
        if !self.hits.is_empty() {
            self.current =
                (self.current as isize + delta).rem_euclid(self.hits.len() as isize) as usize;
        }
    }

    pub fn count(&self) -> usize {
        self.hits.len()
    }

    /// The index of the current result, if there is one.
    pub fn current(&self) -> Option<usize> {
        (self.current < self.hits.len()).then_some(self.current)
    }

    /// The counter: "3 of 12", "No results", nothing with an empty query or before the
    /// first results of this query.
    pub fn label(&self) -> Option<String> {
        if self.edit.text.is_empty() || self.shown.is_none() {
            return None;
        }
        Some(match self.current() {
            Some(i) => format!("{} of {}", i + 1, self.count()),
            None => NO_RESULTS.to_owned(),
        })
    }

    /// The marks of block `id` (source with hash `hash`) for `Markdown::highlights`: the
    /// current result with the key `<row_key>/find-current`, the others with no key. A block
    /// changed after the search has none until the new results arrive.
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

    /// The key to pass to `Tx::scroll_center_in_instant("doc", …)` for the current result.
    pub fn reveal_key(&self, doc: &Document) -> Option<String> {
        let h = self.hits.get(self.current)?;
        if h.block == crate::doc::footnotes::BLOCK_ID {
            return Some(current_key(crate::doc::footnotes::KEY));
        }
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

/// The field in the tab bar: icon, query and counter in `fg3`.
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

/// The field keys before the editor: Return, Shift+Return, Esc. True if it takes them.
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

    /// Waits for the results of the search thread and applies them to `doc`.
    fn wait(f: &mut Find, doc: &Document) {
        let t = std::time::Instant::now();
        while f.pending() {
            assert!(t.elapsed().as_secs() < 10, "results never arrived");
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

    /// The occurrences of `q` in the source, case-insensitive, like `grep -io q | wc -l`.
    fn grep_count(src: &str, q: &str) -> usize {
        src.to_lowercase().matches(&q.to_lowercase()).count()
    }

    /// "rune" on `normal.md` gives as many results as `grep -io rune fixtures/normal.md | wc -l`
    /// (0: the generator does not use the word). The same comparison with a word that is there,
    /// "albero": `grep` also counts link destinations (`](https://example.com/albero)`),
    /// which are not drawn text and `markdown::find` skips; they are taken out of the count.
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
        assert!(in_links > 0, "the fixture has links to /albero");
        assert_eq!(found.count(), grep_count(&src, "albero") - in_links);
        let blocks: std::collections::HashSet<u64> = found.hits.iter().map(|h| h.block).collect();
        assert!(
            blocks.len() > 10,
            "results in many blocks: {}",
            blocks.len()
        );
        assert_eq!(
            searched(&doc, "ALBERO").count(),
            found.count(),
            "case-insensitive"
        );
    }

    /// Next and prev wrap around, on the model.
    #[test]
    fn step_wraps_both_ways() {
        let doc = Document::from_source(Path::new("t.md"), "one zebra\n\ntwo\n\nZebra and zebra\n");
        let mut f = searched(&doc, "zebra");
        assert_eq!((f.current(), f.count()), (Some(0), 3));
        assert_eq!(f.label().as_deref(), Some("1 of 3"));
        f.step(-1);
        assert_eq!(
            f.label().as_deref(),
            Some("3 of 3"),
            "before the first: the last"
        );
        f.step(1);
        assert_eq!(f.current(), Some(0), "after the last: the first");
        search(&mut f, &doc, "giraffa");
        assert_eq!(f.label().as_deref(), Some(NO_RESULTS));
        f.step(1);
        assert_eq!(f.current(), None);
        search(&mut f, &doc, "");
        assert_eq!(f.label(), None, "empty query: no counter");
    }

    /// Reload: the current result stays in its block (by id) even if blocks with other results
    /// arrive above it; if the block goes away it moves to the first one after.
    #[test]
    fn refresh_keeps_the_current_block() {
        let mut doc = Document::from_source(
            Path::new("t.md"),
            "a zebra\n\nb\n\nc zebra zebra\n\nd zebra\n",
        );
        let mut f = searched(&doc, "zebra");
        f.step(2);
        assert_eq!(f.label().as_deref(), Some("3 of 4"));
        let block = f.hits[f.current].block;
        doc.update("fresh zebra\n\nother zebra\n\na zebra\n\nb\n\nc zebra zebra\n\nd zebra\n");
        f.ask(&doc, Goal::Keep);
        wait(&mut f, &doc);
        assert_eq!(f.hits[f.current].block, block, "same block");
        assert_eq!(
            f.label().as_deref(),
            Some("5 of 6"),
            "same ordinal in the block"
        );
        doc.update("fresh zebra\n\nother zebra\n\na zebra\n\nb\n\nd zebra\n");
        f.ask(&doc, Goal::Keep);
        wait(&mut f, &doc);
        assert_eq!(
            f.label().as_deref(),
            Some("4 of 4"),
            "block gone: the first one after"
        );
    }

    // --- on the runtime, with the app view ---

    use rune::{Env, KeyEvent, Mods, Runtime};

    use crate::app::Mode;

    fn settle(rt: &mut Runtime<App>) {
        for _ in 0..4000 {
            if !rt.frame(1.0 / 120.0) {
                return;
            }
        }
        panic!("the view does not settle");
    }

    /// 200 blocks; "zebra" in blocks 3, 120 and 190 (twice in 190).
    fn long() -> String {
        (0..200)
            .map(|n| match n {
                3 | 120 => format!("Paragraph {n} with a zebra in the middle of the text that goes on."),
                190 => format!("Paragraph {n}: zebra and Zebra again."),
                _ if n % 10 == 0 => format!("## Sezione {n}"),
                _ => format!("Test paragraph {n}, long enough to wrap in the reader column, with more words."),
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

    /// Like the app `poll`: waits for and applies the results of the search thread.
    fn wait_find(rt: &mut Runtime<App>) {
        let t = std::time::Instant::now();
        loop {
            rt.update(|s, tx| {
                s.poll_find(tx);
            });
            if !rt.state.find.pending() {
                break;
            }
            assert!(t.elapsed().as_secs() < 10, "results never arrived");
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

    /// Return (or Shift+Return): the current mark is in view on the first frame after the key.
    fn enter(rt: &mut Runtime<App>, shift: bool) {
        rt.key(KeyEvent {
            key: Key::Enter,
            mods: Mods {
                shift,
                ..Mods::default()
            },
        });
        rt.frame(1.0 / 120.0);
        assert!(current_in_view(rt), "current mark in view on the first frame");
        settle(rt);
    }

    fn count_text(rt: &Runtime<App>) -> Option<String> {
        rt.elements()
            .into_iter()
            .find(|e| e.key.as_deref() == Some("find-count"))
            .and_then(|e| e.text)
    }

    /// The drawn Find marks: (key, visible rect).
    fn marks(rt: &Runtime<App>) -> Vec<(Option<String>, [f32; 4])> {
        rt.elements()
            .into_iter()
            .filter(|e| e.role == "mark" && !e.exiting)
            .map(|e| (e.key, e.rect))
            .collect()
    }

    /// The current mark is drawn inside the document view.
    fn current_in_view(rt: &Runtime<App>) -> bool {
        let els = rt.elements();
        let doc = els
            .iter()
            .find(|e| e.key.as_deref() == Some("doc"))
            .expect("doc view")
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
        f.hits[f.current().expect("a current result")].at
    }

    /// Cmd+F, typing: the counter says "1 of 4" and the first result is in view; Return
    /// goes to block 120, which was not mounted, and centres it; Shift+Return goes back; it wraps
    /// around at both ends.
    #[test]
    fn enter_and_shift_enter_walk_the_matches_and_reveal_unmounted_blocks() {
        let mut rt = open(&[("a.md", &long())]);
        cmd(&mut rt, 'f');
        assert!(
            rt.state.find.open && rt.state.find.edit.focused,
            "bar open with focus"
        );
        assert!(
            rt.elements()
                .iter()
                .any(|e| e.key.as_deref() == Some("find-input"))
        );
        type_text(&mut rt, "ZEBRA");
        assert_eq!(count_text(&rt).as_deref(), Some("1 of 4"));
        assert_eq!(current_block(&rt), 3);
        assert!(current_in_view(&rt));
        let doc = rt.state.active_document();
        let far = crate::ui::doc_view::row_key(120, doc);
        assert!(
            !rt.elements()
                .iter()
                .any(|e| e.key.as_deref() == Some(far.as_str())),
            "block 120 is not mounted"
        );
        enter(&mut rt, false);
        assert_eq!(
            (count_text(&rt).as_deref(), current_block(&rt)),
            (Some("2 of 4"), 120)
        );
        assert!(
            current_in_view(&rt),
            "block 120 is mounted and the mark is in view"
        );
        enter(&mut rt, false);
        enter(&mut rt, false);
        assert_eq!(
            (count_text(&rt).as_deref(), current_block(&rt)),
            (Some("4 of 4"), 190)
        );
        assert!(current_in_view(&rt));
        enter(&mut rt, false);
        assert_eq!(
            (count_text(&rt).as_deref(), current_block(&rt)),
            (Some("1 of 4"), 3),
            "wraps to the start"
        );
        assert!(current_in_view(&rt));
        enter(&mut rt, true);
        assert_eq!(
            (count_text(&rt).as_deref(), current_block(&rt)),
            (Some("4 of 4"), 190),
            "Shift+Return wraps to the end"
        );
        assert!(current_in_view(&rt));
        enter(&mut rt, true);
        assert_eq!(count_text(&rt).as_deref(), Some("3 of 4"));
        // Only one of the drawn marks is the current one.
        let drawn = marks(&rt);
        assert!(
            drawn.len() >= 2,
            "the two results of block 190: {drawn:?}"
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

    /// Esc closes the bar and clears the marks; keys go back to the document (Cmd+A reaches
    /// `on_key` and selects "the whole file").
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
            "Cmd+A reaches the document"
        );
        cmd(&mut rt, 'f');
        assert!(
            rt.state.find.open && rt.state.find.edit.text.is_empty(),
            "it reopens empty"
        );
    }

    /// A reload while searching: the results are recomputed, the current one stays in its block
    /// and the view does not go back to the top.
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
            "A new zebra at the top.\n\nAnd another zebra.\n\n{}",
            long()
        );
        rt.update(|s, _| {
            s.tab_mut().unwrap().doc.update(&src);
        });
        wait_find(&mut rt);
        let f = &rt.state.find;
        assert_eq!(f.count(), 6);
        assert_eq!(f.hits[f.current().unwrap()].block, id, "same block");
        assert_eq!(count_text(&rt).as_deref(), Some("4 of 6"));
        assert_eq!(top(&rt), before, "the view stays where it was");
        assert!(current_in_view(&rt));
    }

    /// One bar per window: when the tab changes the search runs again on the new file, from the first
    /// result, and brings it into view.
    #[test]
    fn switching_tab_reruns_the_query_from_the_first_match() {
        let other = (0..150)
            .map(|n| {
                if n == 100 || n == 140 {
                    format!("Other {n} zebra.")
                } else {
                    format!("Other line {n} of the second file, long enough to take up space.")
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
        assert_eq!(count_text(&rt).as_deref(), Some("3 of 4"));
        cmd(&mut rt, '2');
        wait_find(&mut rt);
        assert_eq!(rt.state.active, 1);
        assert!(rt.state.find.open);
        assert_eq!(count_text(&rt).as_deref(), Some("1 of 2"));
        assert_eq!(current_block(&rt), 100);
        assert!(
            current_in_view(&rt),
            "the first result of the new file is in view"
        );
        cmd(&mut rt, '1');
        wait_find(&mut rt);
        assert_eq!(
            (count_text(&rt).as_deref(), current_block(&rt)),
            (Some("1 of 4"), 3),
            "back to the first"
        );
        assert!(current_in_view(&rt));
    }

    /// Cmd+P, Cmd+N and Cmd+F close each other; in the source Cmd+F does nothing.
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
            "Cmd+F closes Cmd+P"
        );
        cmd(&mut rt, 'n');
        assert!(!rt.state.find.open && rt.state.sidebar.new_file.open);
        cmd(&mut rt, 'f');
        assert!(
            rt.state.find.open && !rt.state.sidebar.new_file.open,
            "Cmd+F closes Cmd+N"
        );
        rt.update(|s, tx| {
            s.toggle_source(tx);
        });
        settle(&mut rt);
        assert!(matches!(rt.state.tab().unwrap().mode, Mode::Source(_)));
        assert!(!rt.state.find.open, "entering the source closes Find");
        assert!(!rt.state.open_find(), "in the source Cmd+F opens nothing");
        rt.state.sync_find();
        assert!(!rt.state.find.open);
    }

    /// The text of a note is found where it shows, at the end of the file below the rule, not in the
    /// definition block (which is not drawn): the current mark is in the notes and in view.
    #[test]
    fn a_footnote_is_found_where_it_is_shown() {
        let body: Vec<String> = (0..60).map(|n| format!("Test paragraph {n}, long enough to wrap in the column, with more words.")).collect();
        let src = format!("A note[^1] here.\n\n[^1]: The zebra text.\n\n{}\n", body.join("\n\n"));
        let mut rt = open(&[("a.md", &src)]);
        cmd(&mut rt, 'f');
        type_text(&mut rt, "zebra");
        assert_eq!(count_text(&rt).as_deref(), Some("1 of 1"));
        enter(&mut rt, false);
        let current: Vec<_> = marks(&rt).into_iter().filter(|(k, _)| k.as_deref().is_some_and(|k| k.ends_with("/find-current"))).collect();
        assert_eq!(current.len(), 1, "{current:?}");
        assert_eq!(current[0].0.as_deref(), Some("footnotes/find-current"));
        let els = rt.elements();
        let notes = els.iter().find(|e| e.key.as_deref() == Some("footnotes")).expect("note in fondo").rect;
        let r = current[0].1;
        assert!(r[1] >= notes[1] && r[1] + r[3] <= notes[1] + notes[3] + 0.5, "segno {r:?} fuori dalle note {notes:?}");
    }

    /// Types without waiting for the results (the search thread answers later).
    fn type_now(rt: &mut Runtime<App>, t: &str) {
        for c in t.chars() {
            key(rt, Key::Text(c.to_string()), Mods::default());
        }
    }

    /// A reload while the query just typed is still on the thread: the results still start
    /// from the first visible block and the current one is in view (not "close" to the results of
    /// before).
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
            "view near block 110: {at}"
        );
        cmd(&mut rt, 'f');
        type_now(&mut rt, "zebra");
        assert!(rt.state.find.pending());
        let src = format!("{}\n\nAt the end, without the word.\n", long());
        rt.update(|s, _| {
            s.tab_mut().unwrap().doc.update(&src);
        });
        wait_find(&mut rt);
        assert_eq!(count_text(&rt).as_deref(), Some("2 of 4"));
        assert_eq!(current_block(&rt), 120);
        assert!(current_in_view(&rt));
    }

    /// After a tab change, until the results of the new file arrive nothing is left of
    /// the other one's: no counter, no marks, no Return that goes to a random block.
    #[test]
    fn after_a_tab_switch_the_old_results_are_gone_until_the_new_arrive() {
        let other = (0..40)
            .map(|n| {
                if n == 30 {
                    format!("Line {n} zebra.")
                } else {
                    format!("Line {n}.")
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let mut rt = open(&[("a.md", &long()), ("b.md", &other)]);
        cmd(&mut rt, '1');
        cmd(&mut rt, 'f');
        type_text(&mut rt, "zebra");
        assert_eq!(count_text(&rt).as_deref(), Some("1 of 4"));
        cmd(&mut rt, '2');
        assert!(rt.state.find.pending());
        assert_eq!(count_text(&rt), None, "no counter from the other file");
        assert_eq!(rt.state.find.count(), 0);
        assert!(marks(&rt).is_empty());
        wait_find(&mut rt);
        assert_eq!(count_text(&rt).as_deref(), Some("1 of 1"));
    }

    /// Return while the results are on the thread: the step applies when they arrive.
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
            "Return arrived before the results"
        );
        wait_find(&mut rt);
        assert_eq!(
            (count_text(&rt).as_deref(), current_block(&rt)),
            (Some("2 of 4"), 120)
        );
        assert!(current_in_view(&rt));
    }

    /// Esc stops the running job on the thread.
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
            "the requested job is no longer the last one"
        );
        assert!(!f.pending());
    }

    /// Measure: the UI thread cost of one Find key press on
    /// `fixtures/big.md`, with the app view on a headless runtime. Each query is typed
    /// letter by letter. For each key: the key and its frame (field and request to the thread),
    /// and, when the results arrive, `poll_find` and the frame that draws them (marks, reveal);
    /// separately, how long the search thread takes to answer.
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
                    // What `poll_find` does when the results arrive, and the frame after.
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
        // The reference: a view jump on big.md without Find (the same `scroll_center_in_instant`
        // on a far, unmounted row), like the jumps that far results make.
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
            "find_cost big.md {} blocks; first key, thread answer (block parse) {:.2} ms",
            rt.state.active_document().blocks.len(),
            first.unwrap()
        );
        for (name, ms) in [
            ("tasto + frame", &mut keyed),
            ("risultati + frame", &mut applied),
            ("UI thread, the worse of the two", &mut ui),
            ("search thread answer", &mut worker),
            ("reference: view jump without Find", &mut jump),
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
