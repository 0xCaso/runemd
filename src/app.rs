//! Lo stato dell'app: le schede dei file aperti e ciò che hanno in comune.
//!
//! Ogni [`Tab`] ha il suo documento, la sua rilettura (`live::Follow`), i suoi segnali (fuoco,
//! segno dei blocchi cambiati, dither) e dove era la vista. [`App`] tiene ciò che non dipende da
//! una scheda: watcher, "sta scrivendo" e scritture dell'app (`live::Live`), indice, sidebar,
//! anelli. Una scheda in secondo piano continua a ricaricarsi e a contare i blocchi nuovi sotto
//! la sua vista; il fuoco si vede quando torna davanti.

use std::collections::BTreeMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use rune::Tx;
use rune_kit::MarkdownStyle;

use crate::doc::document::{Change, Document};
use crate::live::arriving::Changed;
use crate::live::below::Below;
use crate::live::{Follow, Live};
use crate::{index, probe, ui};

/// Il sorgente in modifica (task 20, che lo sposta in `edit` con il suo `TextEdit`).
#[derive(Debug, Default)]
pub struct SourceEdit;

/// Come una scheda mostra il file.
#[derive(Debug, Default)]
#[allow(dead_code)] // `Source` lo costruisce Cmd+E (task 20)
pub enum Mode {
    #[default]
    Read,
    Source(SourceEdit),
}

/// Un file aperto.
pub struct Tab {
    pub doc: Document,
    #[allow(dead_code)] // letto da Cmd+E (task 20)
    pub mode: Mode,
    /// Il fuoco: i blocchi nuovi o cambiati sotto la vista di questa scheda.
    pub below: Below,
    /// I blocchi con il segno "cosa è cambiato".
    pub changed: Changed,
    /// La rilettura del file.
    pub(crate) follow: Follow,
    /// L'id dell'ultimo blocco visibile, dall'ultimo `on_visible_rows`; 0 prima del primo.
    pub(crate) visible_last: u64,
    /// L'id del primo blocco visibile: quando la scheda torna davanti la vista riparte da lì.
    pub(crate) visible_first: u64,
    /// L'orologio di Rune a cui si è acceso il fuoco; `None` quando è spento.
    pub(crate) fire_since: Option<f64>,
    /// L'orologio della ricarica che ha portato blocchi sotto la vista, finché Rune non riporta
    /// le righe visibili dopo il suo layout: solo allora si sa se sono davvero fuori vista.
    pub(crate) fire_armed: Option<f64>,
    /// Il risveglio armato per togliere il prossimo segno che scade.
    mark_wake: Option<Wake>,
    /// L'orologio di Rune a cui si è acceso il dither "sta arrivando"; `None` quando è spento.
    pub(crate) dither_since: Option<f64>,
}

impl Tab {
    fn new(doc: Document, follow: Follow) -> Tab {
        Tab { doc, mode: Mode::Read, below: Below::default(), changed: Changed::default(), follow, visible_last: 0, visible_first: 0, fire_since: None, fire_armed: None, mark_wake: None, dither_since: None }
    }

    /// Il file come lo riporta FSEvents (canonico): la chiave degli anelli.
    pub fn path(&self) -> &Path {
        self.follow.path()
    }

    /// Il nome della scheda.
    pub fn title(&self) -> String {
        self.doc.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }

    /// Se il blocco `id` ha il segno "cosa è cambiato" adesso.
    pub fn is_marked(&self, id: u64) -> bool {
        self.changed.is_marked(id, Instant::now())
    }

    /// I blocchi nuovi o cambiati da una ricarica prendono il segno; i segni scaduti a `now` si
    /// tolgono. Vero se la vista cambia. Ritorna anche il risveglio per il prossimo che scade:
    /// senza, con "riduci movimento" o a finestra ferma nessun frame lo toglierebbe.
    pub fn update_marks(&mut self, change: Option<&Change>, clock: f64, now: Instant) -> (bool, Option<Duration>) {
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
    pub fn dither_step(&mut self, writing: bool, clock: f64) -> bool {
        let lit = writing && self.doc.last_open && !self.doc.blocks.is_empty();
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
    ///
    /// In secondo piano vale lo stesso: il conto parte dall'ultima vista della scheda, e un fuoco
    /// armato si accende al primo rapporto di righe quando la scheda torna davanti.
    pub fn note_reload(&mut self, change: &Change, clock: f64) -> bool {
        let order: Vec<u64> = self.doc.blocks.iter().map(|b| b.id).collect();
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
    pub fn visible_rows(&mut self, rows: Range<usize>) -> bool {
        let doc = &self.doc;
        // Rune riporta gli indici dell'ultimo layout: con una ricarica in mezzo possono andare
        // oltre i blocchi di adesso, e allora si aspetta il prossimo rapporto.
        let Some(last) = rows.end.checked_sub(1).filter(|&i| i < doc.blocks.len() && !rows.is_empty()) else { return false };
        self.visible_last = doc.blocks[last].id;
        self.visible_first = doc.blocks[rows.start].id;
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
}

/// Un risveglio per la fine di uno stato a tempo ("sta scrivendo", un segno che scade).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Wake {
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
pub(crate) fn rearm(wake: &mut Option<Wake>, left: Option<Duration>, clock: f64, now: Instant) -> Option<Duration> {
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

pub struct App {
    pub tabs: Vec<Tab>,
    /// L'indice della scheda davanti; vale solo se `tabs` non è vuoto.
    pub active: usize,
    pub(crate) style: MarkdownStyle,
    /// I colori di rune-kit per la barra delle schede, dai token (`ui::tabs::kit`).
    pub(crate) kit: rune_kit::Kit,
    pub(crate) scroll: Option<probe::ScrollRun>,
    /// Watcher, "sta scrivendo" e scritture dell'app, per tutte le schede.
    pub(crate) live: Live,
    /// I file che "stanno scrivendo" e l'orologio di Rune (`poll`) a cui si è acceso il loro
    /// anello: lo mostrano la loro scheda e le loro righe in Recenti e nell'albero.
    rings: BTreeMap<PathBuf, f64>,
    /// Il risveglio armato per spegnere il primo anello che scade: uno per fine, non uno a frame.
    ring_wake: Option<Wake>,
    /// L'indice dei Markdown nelle cartelle osservate (Recenti, Cmd+P).
    pub(crate) library: index::Library,
    /// Recenti e il repo del file davanti.
    pub(crate) sidebar: ui::sidebar::Sidebar,
    /// La finestra è attiva (`on_window_focus`): senza, il fuoco resta fermo.
    pub(crate) window_active: bool,
    /// La riga da rimettere in cima alla vista al prossimo `poll` (`restore_view`).
    restore: Option<String>,
}

impl App {
    pub fn new(library: index::Library, dark: bool) -> App {
        let theme = ui::theme::Theme::for_mode(dark);
        App {
            tabs: Vec::new(),
            active: 0,
            style: ui::markdown_style::style(&theme, ui::theme::BODY_TEXT),
            kit: ui::tabs::kit(&theme),
            scroll: None,
            live: Live::default(),
            rings: BTreeMap::new(),
            ring_wake: None,
            library,
            sidebar: ui::sidebar::Sidebar::default(),
            window_active: true,
            restore: None,
        }
    }

    /// La scheda davanti.
    pub fn tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub fn tab_mut(&mut self) -> Option<&mut Tab> {
        self.tabs.get_mut(self.active)
    }

    /// Una scheda per `doc`, davanti (i test della vista, senza disco).
    #[cfg(test)]
    pub fn push_tab(&mut self, doc: Document) {
        let follow = self.live.follow(&doc.path.clone());
        self.tabs.push(Tab::new(doc, follow));
        self.active = self.tabs.len() - 1;
    }

    /// Il documento della scheda davanti; la vista lo chiama solo quando c'è una scheda.
    pub fn active_document(&self) -> &Document {
        &self.tab().expect("vista del documento senza schede").doc
    }

    /// La scheda del documento `id` (le closure della vista ritrovano così la loro scheda).
    fn tab_of(&mut self, id: u64) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.doc.id == id)
    }

    /// Apre i percorsi ricevuti (Finder, riga di comando, sidebar): una scheda per percorso,
    /// davanti l'ultimo. Un file già aperto viene portato davanti, non duplicato né riletto.
    pub fn open_paths(&mut self, paths: Vec<PathBuf>) {
        if !paths.is_empty() && std::env::var_os("RUNEMD_LOG_OPEN").is_some() {
            eprintln!("runemd: open_paths {paths:?}");
        }
        let before = self.tab().map(|t| t.doc.id);
        for path in &paths {
            let canonical = path.canonicalize().unwrap_or_else(|_| path.to_owned());
            if let Some(i) = self.tabs.iter().position(|t| t.path() == canonical || t.doc.path == *path) {
                self.active = i;
                continue;
            }
            match Document::load(path) {
                Ok(doc) => {
                    let follow = self.live.follow(path);
                    self.tabs.push(Tab::new(doc, follow));
                    self.active = self.tabs.len() - 1;
                }
                Err(e) => eprintln!("runemd: {}: {e}", path.display()),
            }
        }
        if self.tab().map(|t| t.doc.id) != before {
            self.fronted();
        }
    }

    /// Chiude la scheda davanti; davanti va quella che stava alla sua destra, o l'ultima. Le
    /// radici guardate solo per quel file escono dal watcher.
    pub fn close_active(&mut self) {
        self.close(self.active);
    }

    /// Chiude la scheda `i` (Cmd+W per quella davanti, il bottone × per le altre).
    pub fn close(&mut self, i: usize) {
        if i >= self.tabs.len() {
            return;
        }
        let tab = self.tabs.remove(i);
        self.live.unfollow(tab.follow);
        let front = self.active == i;
        if self.active > i || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
        if front && !self.tabs.is_empty() {
            self.fronted();
        }
    }

    /// Porta davanti la scheda `i`; vero se è cambiata.
    pub fn select(&mut self, i: usize) -> bool {
        if i >= self.tabs.len() || i == self.active {
            return false;
        }
        self.active = i;
        self.fronted();
        true
    }

    /// La scheda dopo (`step` 1) o prima (-1) di quella davanti, in giro (Ctrl+Tab).
    pub fn cycle(&mut self, step: isize) -> bool {
        let n = self.tabs.len() as isize;
        n > 1 && self.select((self.active as isize + step).rem_euclid(n) as usize)
    }

    /// Un'altra scheda è davanti: la sidebar mostra il suo repo.
    fn fronted(&mut self) {
        if let Some(tab) = self.tabs.get(self.active) {
            self.sidebar.set_open(&tab.doc.path, &self.library.index);
        }
    }

    /// Dopo un cambio della scheda davanti: la vista riparte dal primo blocco che la scheda
    /// mostrava, in cima, o dall'inizio se non era mai stata davanti. La vista della scheda è un
    /// elemento nuovo (`ui::doc_view::view`), che parte da 0 con le altezze stimate.
    ///
    /// In due passi, perché `Tx::scroll_key_to` vuole una riga già montata: qui la si porta al
    /// centro della vista (Rune la monta), al `poll` seguente la si mette in cima
    /// ([`App::take_restore`]).
    pub fn restore_view(&mut self, tx: &mut Tx) {
        self.restore = None;
        let Some(tab) = self.tab() else { return };
        match tab.doc.blocks.iter().position(|b| b.id == tab.visible_first) {
            Some(i) if i > 0 => {
                let key = ui::doc_view::row_key(i, &tab.doc);
                tx.scroll_center_in_instant("doc", key.clone());
                tx.wake_after(Duration::ZERO);
                self.restore = Some(key);
            }
            _ => tx.scroll_to_instant("doc", 0.0, 0.0),
        }
    }

    /// Il secondo passo di [`App::restore_view`], da `poll`.
    pub fn take_restore(&mut self, tx: &mut Tx) {
        if let Some(key) = self.restore.take() {
            tx.scroll_key_to(key, 0.0);
        }
    }

    /// Un clic su una riga della sidebar: il file va in una scheda (o si porta davanti la sua).
    pub fn open_from_sidebar(&mut self, path: PathBuf, tx: &mut Tx) {
        let before = self.tab().map(|t| t.doc.id);
        self.open_paths(vec![path]);
        if self.tab().map(|t| t.doc.id) != before {
            self.restore_view(tx);
        }
    }

    /// La ricerca di Cmd+P è cambiata.
    pub fn requery_sidebar(&mut self) {
        self.sidebar.requery(&self.library.index);
    }

    /// Le righe visibili riportate dalla vista del documento `id`: valgono per la sua scheda,
    /// anche se nel frattempo un'altra è venuta davanti.
    pub fn visible_rows(&mut self, id: u64, rows: Range<usize>) -> bool {
        self.tab_of(id).is_some_and(|t| t.visible_rows(rows))
    }

    /// Se il blocco `id` della scheda davanti ha il segno "cosa è cambiato".
    pub fn is_marked(&self, id: u64) -> bool {
        self.tab().is_some_and(|t| t.is_marked(id))
    }

    /// Un anello "sta scrivendo" è acceso: il compositor scorre le istanze solo allora.
    pub fn ring_lit(&self) -> bool {
        !self.rings.is_empty()
    }

    /// Un effetto per slot è acceso (anello, fuoco o dither della scheda davanti): il compositor
    /// scorre le istanze solo allora.
    pub fn effects_lit(&self) -> bool {
        self.ring_lit() || self.tab().is_some_and(|t| t.fire_since.is_some() || t.dither_since.is_some())
    }

    /// L'orologio a cui si è acceso l'anello di `path`, se "sta scrivendo" (scheda e sidebar).
    pub fn ring_of(&self, path: &Path) -> Option<f64> {
        self.rings.get(path).copied()
    }

    /// Accende o spegne gli anelli secondo `Live` a `now`; vero se è cambiato. Ritorna anche il
    /// risveglio da chiedere: a fine finestra `poll` li spegne anche se nessun frame è passato
    /// in mezzo (con "riduci movimento" non ne passano).
    pub fn update_ring(&mut self, clock: f64, now: Instant) -> (bool, Option<Duration>) {
        self.ring_step(&self.live.writing(now), clock, now)
    }

    /// `update_ring` con i file che stanno scrivendo e fra quanto smettono (a `now`) già
    /// calcolati. Un file già acceso tiene il suo orologio; il risveglio, per il primo che
    /// scade, viene da [`rearm`].
    pub(crate) fn ring_step(&mut self, writing: &[(PathBuf, Duration)], clock: f64, now: Instant) -> (bool, Option<Duration>) {
        let wake = rearm(&mut self.ring_wake, writing.iter().map(|(_, d)| *d).min(), clock, now);
        let rings: BTreeMap<PathBuf, f64> = writing.iter().map(|(p, _)| (p.clone(), self.rings.get(p).copied().unwrap_or(clock))).collect();
        let changed = rings != self.rings;
        self.rings = rings;
        (changed, wake)
    }

    /// Le ricariche di tutte le schede, a ogni `poll`: svuota il watcher, rilegge i file
    /// cambiati e aggiorna fuoco, segni e dither di ogni scheda. Ritorna se la vista cambia e i
    /// risvegli da chiedere.
    pub fn poll_tabs(&mut self, clock: f64, now: Instant) -> (bool, Vec<Duration>) {
        let open: Vec<&Path> = self.tabs.iter().map(Tab::path).collect();
        self.live.drain(&open, now);
        let (mut changed, mut wakes) = (false, Vec::new());
        for tab in &mut self.tabs {
            let (reloaded, recheck) = self.live.poll(&mut tab.follow, &mut tab.doc, now);
            wakes.extend(recheck);
            if let Some(r) = &reloaded {
                probe::reloaded(r);
                changed = true;
                changed |= tab.note_reload(&r.change, clock);
            }
            // Il segno dei blocchi cambiati: ogni ricarica applicata viene da fuori (le scritture
            // dell'app si saltano in `reload::commit`). Un risveglio per segno che scade.
            let (marks, wake) = tab.update_marks(reloaded.as_ref().map(|r| &r.change), clock, now);
            wakes.extend(wake);
            let writing = self.live.writing_left(tab.follow.path(), now).is_some();
            changed |= marks | tab.dither_step(writing, clock);
        }
        (changed, wakes)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::live;

    use super::*;

    pub(crate) fn empty_app() -> App {
        App::new(index::Library::load(Vec::new(), None), true)
    }

    fn files(names: &[&str]) -> (tempfile::TempDir, Vec<PathBuf>) {
        let dir = tempfile::tempdir().unwrap();
        let paths = names.iter().map(|n| {
            let p = dir.path().join(n);
            fs::write(&p, format!("# {n}\n")).unwrap();
            p
        }).collect();
        (dir, paths)
    }

    #[test]
    fn opening_the_same_file_twice_keeps_one_tab() {
        let (_dir, p) = files(&["a.md"]);
        let mut app = empty_app();
        app.open_paths(vec![p[0].clone()]);
        let id = app.tabs[0].doc.id;
        app.open_paths(vec![p[0].clone()]);
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.tabs[0].doc.id, id, "non riletto");
    }

    #[test]
    fn opening_two_files_makes_two_tabs_with_the_last_in_front() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.tab().unwrap().doc.path, p[1]);
        // Riaperto, `a` torna davanti senza una scheda nuova.
        app.open_paths(vec![p[0].clone()]);
        assert_eq!((app.tabs.len(), app.active), (2, 0));
    }

    #[test]
    fn closing_the_last_tab_leaves_no_tabs() {
        let (_dir, p) = files(&["a.md"]);
        let mut app = empty_app();
        app.open_paths(p);
        app.close_active();
        assert!(app.tabs.is_empty() && app.tab().is_none());
        app.close_active();
        assert!(app.tabs.is_empty());
        assert!(app.live.open_roots().is_empty(), "radice rilasciata");
    }

    /// Chiusa la scheda davanti, davanti va quella alla sua destra; chiusa una a sinistra, la
    /// scheda davanti resta la stessa.
    #[test]
    fn closing_keeps_a_sensible_tab_in_front() {
        let (_dir, p) = files(&["a.md", "b.md", "c.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        app.select(1);
        app.close_active();
        assert_eq!(app.tab().unwrap().doc.path, p[2]);
        app.close(0);
        assert_eq!(app.tab().unwrap().doc.path, p[2]);
        app.close_active();
        assert!(app.tabs.is_empty());
    }

    #[test]
    fn ctrl_tab_cycles() {
        let (_dir, p) = files(&["a.md", "b.md", "c.md"]);
        let mut app = empty_app();
        app.open_paths(p);
        assert!(app.cycle(1));
        assert_eq!(app.active, 0, "dopo l'ultima, la prima");
        assert!(app.cycle(-1));
        assert_eq!(app.active, 2);
    }

    /// Le chiavi delle righe sono diverse fra due schede, anche sullo stesso contenuto e dopo
    /// chiudere e riaprire lo stesso file: Rune non passa altezze misurate da una all'altra.
    #[test]
    fn row_keys_differ_across_tabs_and_reopens() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        let key = |app: &App, i: usize| ui::doc_view::row_key(0, &app.tabs[i].doc);
        assert_ne!(key(&app, 0), key(&app, 1));
        let before = key(&app, 1);
        app.close_active();
        app.open_paths(vec![p[1].clone()]);
        assert_ne!(before, key(&app, 1), "riaperto: chiavi nuove");
    }

    /// Una scheda in secondo piano che riceve testo nuovo non accende il fuoco della scheda
    /// davanti; il suo conto resta, e il fuoco si accende quando torna davanti e la vista
    /// riporta le righe.
    #[test]
    fn a_background_reload_does_not_light_the_front_tab() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(p);
        // `a` era davanti e mostrava il suo primo blocco.
        app.select(0);
        app.tabs[0].visible_rows(0..1);
        app.select(1);
        let change = app.tabs[0].doc.update("# a.md\n\nuno\n\ndue\n");
        assert!(!app.tabs[0].note_reload(&change, 3.0));
        assert_eq!(app.tabs[0].below.count(), 2, "conta in secondo piano");
        assert!(app.tab().unwrap().fire_since.is_none(), "la scheda davanti resta spenta");
        assert!(!app.effects_lit());
        app.select(0);
        assert!(app.visible_rows(app.tabs[0].doc.id, 0..1));
        assert_eq!(app.tab().unwrap().fire_since, Some(3.0));
    }

    /// Le righe visibili riportate dalla vista di una scheda valgono per quella scheda.
    #[test]
    fn visible_rows_go_to_the_tab_that_reported_them() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(p);
        let a = app.tabs[0].doc.id;
        app.visible_rows(a, 0..1);
        assert_eq!(app.tabs[0].visible_last, app.tabs[0].doc.blocks[0].id);
        assert_eq!(app.tabs[1].visible_last, 0);
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
        let mut app = empty_app();
        let w = |d| vec![(PathBuf::from("/r/a.md"), d)];
        let (changed, wake) = app.ring_step(&w(window), 100.0, t0);
        assert!(changed && app.ring_lit());
        let wake = wake.expect("un risveglio alla fine");
        let clock = 100.0 + wake.as_secs_f64();
        let now = t0 + window - ms(5);
        let (changed, again) = app.ring_step(&w(ms(5)), clock, now);
        assert!(!changed && app.ring_lit());
        let again = again.expect("il risveglio in anticipo ne arma un altro");
        assert!(again >= ms(5));
        let (changed, last) = app.ring_step(&[], clock + again.as_secs_f64(), now + again);
        assert!(changed && !app.ring_lit(), "spento a fine finestra");
        assert_eq!(last, None, "nessun altro risveglio");
    }

    /// Un poll qualsiasi a anello acceso (un frame) non arma un risveglio nuovo.
    #[test]
    fn one_wake_per_window() {
        let t0 = Instant::now();
        let window = live::activity::WRITING_WINDOW;
        let mut app = empty_app();
        let w = |d| vec![(PathBuf::from("/r/a.md"), d)];
        assert!(app.ring_step(&w(window), 0.0, t0).1.is_some());
        let dt = Duration::from_millis(16);
        assert_eq!(app.ring_step(&w(window - dt), 0.016, t0 + dt).1, None);
        assert!(app.ring_step(&w(window), 0.032, t0 + 2 * dt).1.is_some());
    }

    /// Due file che scrivono: ognuno tiene l'orologio a cui si è acceso, il risveglio è per il
    /// primo che finisce, e quando finisce si spegne solo il suo anello.
    #[test]
    fn each_writing_file_has_its_own_ring() {
        let t0 = Instant::now();
        let window = live::activity::WRITING_WINDOW;
        let ms = |n| Duration::from_millis(n);
        let (a, b) = (PathBuf::from("/r/a.md"), PathBuf::from("/r/b.md"));
        let mut app = empty_app();
        app.ring_step(&[(a.clone(), window)], 1.0, t0);
        let (changed, wake) = app.ring_step(&[(a.clone(), window - ms(2000)), (b.clone(), window)], 3.0, t0 + ms(2000));
        assert!(changed);
        assert_eq!(wake, None, "il primo che finisce è ancora `a`: il risveglio armato vale");
        assert_eq!((app.ring_of(&a), app.ring_of(&b)), (Some(1.0), Some(3.0)));
        let (changed, wake) = app.ring_step(&[(b.clone(), ms(2000))], 11.0, t0 + window);
        assert!(changed && app.ring_of(&a).is_none() && app.ring_of(&b) == Some(3.0));
        assert_eq!(wake, Some(ms(2001)), "poi quello di `b`");
    }

    fn tab_of(source: &str) -> Tab {
        let mut live = Live::default();
        let path = Path::new("/r/t.md");
        Tab::new(Document::from_source(path, source), live.follow(path))
    }

    /// Come l'anello: il risveglio per il segno che scade arriva in anticipo sul muro, ne arma
    /// un altro, e il segno si toglie alla fine.
    #[test]
    fn an_early_wake_rearms_and_the_changed_mark_goes_off() {
        let t0 = Instant::now();
        let ms = |n| Duration::from_millis(n);
        let mut tab = tab_of("# t\n");
        let change = Change { added: vec![3], changed: vec![4], ..Default::default() };
        let (changed, wake) = tab.update_marks(Some(&change), 100.0, t0);
        assert!(changed && tab.is_marked(3) && tab.is_marked(4));
        let wake = wake.expect("un risveglio alla scadenza");
        let clock = 100.0 + wake.as_secs_f64();
        let now = t0 + live::arriving::CHANGED_MARK - ms(5);
        let (changed, again) = tab.update_marks(None, clock, now);
        assert!(!changed && tab.changed.is_marked(3, now));
        let again = again.expect("il risveglio in anticipo ne arma un altro");
        let (changed, last) = tab.update_marks(None, clock + again.as_secs_f64(), now + again);
        assert!(changed && !tab.changed.is_marked(3, now + again), "tolto alla scadenza");
        assert_eq!(last, None, "nessun altro risveglio");
    }

    /// Un poll qualsiasi con un segno acceso non arma un risveglio nuovo.
    #[test]
    fn one_wake_per_mark_expiry() {
        let t0 = Instant::now();
        let mut tab = tab_of("# t\n");
        let change = Change { added: vec![1], ..Default::default() };
        assert!(tab.update_marks(Some(&change), 0.0, t0).1.is_some());
        let dt = Duration::from_millis(16);
        assert_eq!(tab.update_marks(None, 0.016, t0 + dt).1, None);
    }

    /// Il dither: solo con l'ultimo blocco aperto e il file che sta scrivendo; si spegne a fine
    /// finestra o quando il blocco si chiude.
    #[test]
    fn dither_only_on_an_open_last_block_while_writing() {
        let mut tab = tab_of("a\n\n```rs\nfn");
        assert!(!tab.dither_step(false, 1.0) && tab.dither_since.is_none(), "non sta scrivendo");
        assert!(tab.dither_step(true, 1.0) && tab.dither_since == Some(1.0));
        assert!(!tab.dither_step(true, 2.0) && tab.dither_since == Some(1.0), "resta acceso dalla prima volta");
        assert!(tab.dither_step(false, 3.0) && tab.dither_since.is_none(), "spento a fine finestra");
        tab.dither_step(true, 4.0);
        tab.doc.update("a\n\n```rs\nfn\n```\n\n");
        assert!(tab.dither_step(true, 5.0) && tab.dither_since.is_none(), "blocco chiuso");
    }
}
