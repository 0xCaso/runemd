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

use rune::{TextEdit, Tx};
use rune_kit::MarkdownStyle;

use crate::doc::document::{Change, Document};
use crate::edit::SourceEdit;
use crate::live::arriving::Changed;
use crate::live::below::Below;
use crate::live::{Follow, Live};
use crate::settings::{Settings, ThemePref};
use crate::{edit, index, probe, ui};

/// Come una scheda mostra il file: la lettura, o il sorgente in modifica (Cmd+E). Ogni scheda ha
/// il suo.
#[derive(Debug, Default)]
pub enum Mode {
    #[default]
    Read,
    /// In una scatola: il `TextEdit` è grande e la lettura è il caso comune.
    Source(Box<SourceEdit>),
}

/// Un file aperto.
pub struct Tab {
    pub doc: Document,
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
    /// La posizione di `visible_first` all'ultimo rapporto: dopo una ricarica può non valere più
    /// (allora si cerca l'id).
    visible_start: usize,
    /// La posizione di `visible_last` all'ultimo rapporto.
    visible_end: usize,
    /// I titoli dell'indice "In questo file", ricalcolati solo quando il documento cambia.
    pub(crate) outline: ui::outline::Cache,
    /// Dove sono la vista che scorre e la pagina (`doc_view::Edge`), e se lo scroll è in fondo.
    viewport: [f32; 4],
    page: [f32; 4],
    pub(crate) at_bottom: bool,
    /// Il titolo dell'indice cliccato (id del blocco) e il primo blocco visibile dopo il salto:
    /// resta evidenziato finché si vede e non si scorre a mano, anche se non arriva in cima.
    picked: Option<(u64, Option<usize>)>,
    /// Cmd+E non è entrato perché il file non è UTF-8: la lettura mostra la riga che lo dice
    /// (`ui::source::NOT_UTF8_TEXT`) finché un Cmd+E non entra.
    pub(crate) not_utf8: bool,
    /// La scheda di anteprima (come VS Code): aperta con un clic singolo nella sidebar, il nome in
    /// corsivo, sostituita dal clic singolo dopo. Al massimo una. Diventa fissa con un doppio clic
    /// (sulla riga o sulla scheda), una modifica nel sorgente o Cmd+S (`App::pin`).
    pub(crate) preview: bool,
}

impl Tab {
    pub(crate) fn new(doc: Document, follow: Follow) -> Tab {
        Tab { doc, mode: Mode::Read, below: Below::default(), changed: Changed::default(), follow, visible_last: 0, visible_first: 0, fire_since: None, fire_armed: None, mark_wake: None, dither_since: None, visible_start: 0, visible_end: 0, outline: ui::outline::Cache::default(), viewport: [0.0; 4], page: [0.0; 4], at_bottom: false, picked: None, not_utf8: false, preview: false }
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
    /// (etichetta, fuoco o, con l'indice visibile `outline`, il titolo evidenziato). Il fuoco si
    /// accende qui, al primo rapporto dopo la ricarica che l'ha armato (con l'orologio di quella
    /// ricarica), e si spegne quando il conto arriva a 0.
    pub fn visible_rows(&mut self, rows: Range<usize>, outline: bool) -> bool {
        let doc = &self.doc;
        // Rune riporta gli indici dell'ultimo layout: con una ricarica in mezzo possono andare
        // oltre i blocchi di adesso, e allora si aspetta il prossimo rapporto.
        let Some(last) = rows.end.checked_sub(1).filter(|&i| i < doc.blocks.len() && !rows.is_empty()) else { return false };
        // L'indice evidenzia un altro titolo: la vista deve rigirare. Senza indice non si calcola.
        let heading = if outline { self.active_heading() } else { None };
        self.visible_last = self.doc.blocks[last].id;
        self.visible_first = self.doc.blocks[rows.start].id;
        self.visible_start = rows.start;
        self.visible_end = last;
        // Il primo rapporto dopo un clic nell'indice fissa dove è arrivato il salto; uno diverso
        // è uno scroll a mano.
        self.picked = match self.picked {
            Some((id, None)) => Some((id, Some(rows.start))),
            Some((id, Some(at))) if at == rows.start => Some((id, Some(at))),
            _ => None,
        };
        let heading_moved = outline && self.active_heading() != heading;
        let before = self.below.count();
        let order: Vec<u64> = self.doc.blocks.iter().map(|b| b.id).collect();
        self.below.on_scroll(last, &order);
        let was = self.fire_since;
        if self.below.count() == 0 {
            self.fire_since = None;
        } else if self.fire_since.is_none() {
            self.fire_since = self.fire_armed;
        }
        self.fire_armed = None;
        heading_moved || self.below.count() != before || self.fire_since != was
    }

    /// La vista o la pagina del documento hanno un nuovo rettangolo. Lo scroll è "in fondo" se la
    /// pagina è scesa sotto il bordo alto (il file non sta tutto nella vista) e finisce al bordo
    /// basso della vista. Vero se, con l'indice visibile, cambia il titolo evidenziato.
    pub fn edge(&mut self, edge: ui::doc_view::Edge, outline: bool) -> bool {
        match edge {
            ui::doc_view::Edge::Viewport(r) => self.viewport = r,
            ui::doc_view::Edge::Page(r) => self.page = r,
        }
        let ([_, vy, _, vh], [_, py, _, ph]) = (self.viewport, self.page);
        let at_bottom = vh > 0.0 && py < vy - 0.5 && py + ph <= vy + vh + 0.5;
        if at_bottom == self.at_bottom {
            return false;
        }
        let heading = if outline { self.active_heading() } else { None };
        self.at_bottom = at_bottom;
        outline && self.active_heading() != heading
    }

    /// Un clic sul titolo `block` dell'indice: resta evidenziato mentre si vede (`picked`).
    pub fn pick_heading(&mut self, block: u64) {
        self.picked = Some((block, None));
    }

    /// La posizione del blocco `id`, cercata prima dove l'ultimo rapporto l'aveva messa.
    fn index_of(&self, id: u64, guess: usize) -> Option<usize> {
        let blocks = &self.doc.blocks;
        match blocks.get(guess) {
            Some(b) if b.id == id => Some(guess),
            _ => blocks.iter().position(|b| b.id == id),
        }
    }

    /// Il titolo dell'indice evidenziato. In ordine: quello cliccato, se si vede e non si è
    /// scorso a mano; con lo scroll in fondo l'ultimo titolo che si vede (le sezioni corte in
    /// fondo non arrivano mai in cima); altrimenti quello della sezione del primo blocco
    /// visibile. Segue gli id dei blocchi, così vale anche dopo una ricarica, prima del nuovo
    /// rapporto di Rune.
    pub fn active_heading(&self) -> Option<usize> {
        if self.visible_first == 0 {
            return None;
        }
        let items = self.outline.get(&self.doc);
        let start = self.index_of(self.visible_first, self.visible_start)?;
        let end = self.index_of(self.visible_last, self.visible_end).unwrap_or(start);
        if let Some((id, _)) = self.picked
            && let Some(k) = items.iter().position(|h| h.id == id)
            && (start..=end).contains(&items[k].index)
        {
            return Some(k);
        }
        ui::outline::active(&items, if self.at_bottom { end } else { start })
    }
}

/// `RUNEMD_GLASS` letto: un'opacità del vetro in percento (`60` → 0,60), fra 0 e 100; altro o
/// assente, `None` (il vetro di `Theme`).
fn glass_env(v: Option<&str>) -> Option<f32> {
    v?.trim().parse::<f32>().ok().filter(|p| (0.0..=100.0).contains(p)).map(|p| p / 100.0)
}

/// Un passo del trascinamento del bordo di `pane`: `d.data` tiene la larghezza alla pressione,
/// `delta` è lo spostamento che la allarga. Vero quando la larghezza va salvata (lasciato).
fn drag_pane(pane: &mut crate::panes::Pane, limits: crate::panes::Limits, d: &mut rune::Drag, delta: f32, room: f32) -> bool {
    match d.phase {
        rune::Phase::Down => d.data = pane.width,
        rune::Phase::Move => pane.width = pane.drag(d.data + delta, limits, room),
        rune::Phase::Up if d.cancelled() => pane.width = d.data,
        rune::Phase::Up => {
            pane.learn(pane.width, limits);
            return true;
        }
    }
    false
}

/// Porta il blocco `i` della scheda `tab` in cima alla vista del documento, senza planata. La
/// riga può non essere montata: `scroll_center_in_instant` la monta, poi `scroll_key_to` la mette
/// in cima rimisurando dopo il layout; Rune fa le due richieste nello stesso frame, in
/// quest'ordine (`App::restore_view`, `App::go_to_heading`).
fn scroll_block_to_top(tab: &Tab, i: usize, tx: &mut Tx) {
    let key = ui::doc_view::row_key(i, &tab.doc);
    tx.scroll_center_in_instant("doc", key.clone());
    tx.scroll_key_to(key, 0.0);
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

/// Gli stili del documento per il tema chiaro e lo scuro, col font e la dimensione di `s`.
fn styles_for(s: &Settings) -> [MarkdownStyle; 2] {
    let typeface = crate::fonts::Typeface::of(&s.font, s.size);
    [false, true].map(|dark| ui::markdown_style::style_with(&ui::theme::Theme::for_mode(dark), typeface))
}

/// Le risposte alla riga "Salvare le modifiche a …?" (Cmd+W con modifiche non salvate).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAnswer {
    /// "Salva" (anche Invio): salva e chiude.
    Save,
    /// "Non salvare": chiude e butta le modifiche.
    Discard,
    /// "Annulla" (anche Esc): la riga sparisce, la scheda resta com'era.
    Cancel,
}

pub struct App {
    pub tabs: Vec<Tab>,
    /// L'indice della scheda davanti; vale solo se `tabs` non è vuoto.
    pub active: usize,
    /// Lo stile del documento nel tema chiaro e nello scuro (indice `dark as usize`), dal font e
    /// dalla dimensione delle impostazioni: la vista sceglie col tema di adesso, senza rifarlo.
    styles: [MarkdownStyle; 2],
    /// Le impostazioni (Cmd+,) e dove si salvano.
    pub(crate) settings: Settings,
    prefs: rune::prefs::Prefs,
    /// Il pannello delle impostazioni.
    pub(crate) settings_panel: ui::settings::Panel,
    /// `RUNEMD_THEME` (`light` o `dark`), letto una volta all'avvio: vince sul tema scelto.
    theme_env: Option<bool>,
    /// `RUNEMD_REDUCE_MOTION` (`1` o `0`), letto una volta all'avvio: vince sulla preferenza di
    /// sistema "riduci movimento".
    pub(crate) motion_env: Option<bool>,
    /// `RUNEMD_GLASS` (opacità del vetro in percento, `glass_env`), letto una volta all'avvio:
    /// solo per le foto, il default resta quello di `Theme`.
    pub(crate) glass_env: Option<f32>,
    /// "Riduci movimento" come l'ha vista l'ultima vista (`poll` non ha `Cx`): il fuoco a riposo
    /// chiede i suoi risvegli solo se si muove.
    pub(crate) reduced_seen: std::cell::Cell<bool>,
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
    /// La finestra è larga almeno `OUTLINE_MIN_WINDOW`: l'indice dei titoli si vede.
    pub(crate) wide: bool,
    /// La larghezza della finestra (`on_layout` della radice): lo spazio dei pannelli trascinati.
    pub(crate) window_w: f32,
    /// Il campo che `source_edit` dà quando la sua scheda non è più nel sorgente.
    spare_edit: TextEdit,
    /// Cerca nel file (Cmd+F): una barra per finestra, sulla scheda davanti.
    pub(crate) find: ui::find::Find,
    /// La cartella delle copie di recupero (`edit::recovery`); `None` nei test che non la danno,
    /// così nessun test scrive in quella vera.
    pub(crate) recovery: Option<PathBuf>,
}

impl App {
    pub fn new(library: index::Library, settings: Settings, prefs: rune::prefs::Prefs) -> App {
        App {
            tabs: Vec::new(),
            active: 0,
            styles: styles_for(&settings),
            settings,
            prefs,
            settings_panel: ui::settings::Panel::default(),
            theme_env: match std::env::var("RUNEMD_THEME").as_deref() {
                Ok("light") => Some(false),
                Ok("dark") => Some(true),
                _ => None,
            },
            reduced_seen: std::cell::Cell::new(false),
            motion_env: crate::fx::reduce_motion_env(std::env::var("RUNEMD_REDUCE_MOTION").ok().as_deref()),
            glass_env: glass_env(std::env::var("RUNEMD_GLASS").ok().as_deref()),
            scroll: None,
            live: Live::default(),
            rings: BTreeMap::new(),
            ring_wake: None,
            library,
            sidebar: ui::sidebar::Sidebar::default(),
            window_active: true,
            wide: false,
            window_w: 0.0,
            spare_edit: TextEdit::default(),
            find: ui::find::Find::default(),
            recovery: edit::recovery::default_dir(),
        }
    }

    /// Un'app con le impostazioni di default, in memoria (i test).
    #[cfg(test)]
    pub fn bare(library: index::Library) -> App {
        let mut app = App::new(library, Settings::default(), rune::prefs::Prefs::memory());
        app.recovery = None;
        app
    }

    /// Lo stile del documento nel tema `dark`.
    pub fn style(&self, dark: bool) -> &MarkdownStyle {
        &self.styles[usize::from(dark)]
    }

    /// Uno stile fisso per tutti e due i temi (i test della vista).
    #[cfg(test)]
    pub fn set_style(&mut self, style: MarkdownStyle) {
        self.styles = [style.clone(), style];
    }

    /// Il tema di adesso, dato quello del sistema: `RUNEMD_THEME` (`light` o `dark`, per le foto
    /// e le prove) vince sulle impostazioni, che vincono sul sistema.
    pub fn dark(&self, system_dark: bool) -> bool {
        self.theme_env.unwrap_or_else(|| self.settings.theme.dark(system_dark))
    }

    /// "Riduci movimento" di adesso, data la preferenza di sistema: `RUNEMD_REDUCE_MOTION` vince.
    pub fn reduced_motion(&self, system: bool) -> bool {
        crate::fx::reduce_motion(self.motion_env, system)
    }

    /// Salva le impostazioni; un errore si dice sul terminale e il valore resta per questa sessione.
    fn save_settings(&mut self) {
        if let Err(e) = self.settings.save(&mut self.prefs) {
            eprintln!("runemd: impostazioni non salvate: {e}");
        }
    }

    /// Il font del documento (corpo e titoli): il documento si aggiorna subito.
    pub fn set_font(&mut self, font: &str) {
        self.settings_panel.clear_notice();
        if self.settings.font == font {
            return;
        }
        self.settings.font = font.to_owned();
        self.styles = styles_for(&self.settings);
        self.save_settings();
    }

    /// La dimensione del corpo un passo più piccola (−1) o più grande (+1).
    pub fn step_size(&mut self, step: i32) {
        self.settings_panel.clear_notice();
        let size = self.settings.step_size(step);
        if size == self.settings.size {
            return;
        }
        self.settings.size = size;
        self.styles = styles_for(&self.settings);
        self.save_settings();
    }

    pub fn set_theme(&mut self, theme: ThemePref) {
        self.settings_panel.clear_notice();
        if self.settings.theme != theme {
            self.settings.theme = theme;
            self.save_settings();
        }
    }

    /// La larghezza della colonna del testo (uno di `settings::COLUMNS`). Le altezze delle righe
    /// cambiano: il blocco che stava in cima alla vista torna in cima (`scroll_block_to_top`).
    pub fn set_column(&mut self, column: f32, tx: &mut Tx) {
        self.settings_panel.clear_notice();
        if self.settings.column == column || !crate::settings::COLUMNS.contains(&column) {
            return;
        }
        self.settings.column = column;
        self.save_settings();
        if let Some(tab) = self.tab().filter(|t| matches!(t.mode, Mode::Read))
            && let Some(i) = tab.doc.blocks.iter().position(|b| b.id == tab.visible_first).filter(|&i| i > 0)
        {
            scroll_block_to_top(tab, i, tx);
        }
    }

    /// Accende o spegne il fuoco in fondo al pannello; l'etichetta del testo nuovo resta.
    pub fn toggle_fire(&mut self) {
        self.settings_panel.clear_notice();
        self.settings.fire = !self.settings.fire;
        self.save_settings();
    }

    /// Una cartella osservata in più, da subito: la guarda il watcher, l'indice la scandisce e
    /// Recenti si aggiorna quando arriva la scansione. Una cartella sotto una già osservata non
    /// serve e non si aggiunge; una sopra prende il posto di quelle che contiene. Vero se è cambiato.
    pub fn add_root(&mut self, root: &Path) -> bool {
        self.settings_panel.clear_notice();
        let root = index::canonical(root);
        if !root.is_dir() || self.library.roots().iter().any(|r| root.starts_with(r)) {
            return false;
        }
        let inside: Vec<PathBuf> = self.library.roots().iter().filter(|r| r.starts_with(&root)).cloned().collect();
        for r in inside {
            self.remove_root(&r);
        }
        self.settings.add_root(&root);
        self.library.add_root(root.clone());
        self.live.add_tree(&root);
        self.save_settings();
        true
    }

    /// "Aggiungi cartella…" con la cartella `root` scelta nel dialogo: se non si aggiunge (già
    /// osservata, o sotto una osservata) il pannello dice "Già osservata" fino a `now + 3 s`.
    /// Vero se si è aggiunta.
    pub fn pick_root(&mut self, root: &Path, now: Instant) -> bool {
        let added = self.add_root(root);
        if !added {
            self.settings_panel.notice_already(now);
        }
        added
    }

    /// Una cartella osservata in meno, da subito: esce dal watcher e dall'indice, e le sue righe
    /// escono da Recenti. Vero se c'era.
    pub fn remove_root(&mut self, root: &Path) -> bool {
        self.settings_panel.clear_notice();
        let canonical = index::canonical(root);
        let known = self.settings.remove_root(root) | self.settings.remove_root(&canonical);
        let dropped = self.library.remove_root(&canonical);
        if !known && !dropped {
            return false;
        }
        self.live.remove_tree(&canonical);
        self.sidebar.refresh(&self.library.index, &[], std::time::SystemTime::now());
        self.save_settings();
        true
    }

    /// Cmd+,: apre o chiude il pannello delle impostazioni.
    pub fn toggle_settings(&mut self) {
        self.settings_panel.toggle();
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
                // Aperto da Finder o dal terminale: la scheda resta.
                self.tabs[i].preview = false;
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

    /// Un clic singolo su un file nella sidebar (Recenti, albero, risultati di Cmd+P): se è già
    /// aperto si porta davanti la sua scheda; se no va nella scheda di anteprima, che prende il
    /// posto di quella di prima (nella stessa posizione) o, se non c'è, si aggiunge in fondo.
    /// Vero se è venuta davanti un'altra scheda.
    pub fn open_preview(&mut self, path: PathBuf) -> bool {
        let before = self.tab().map(|t| t.doc.id);
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        if let Some(i) = self.tabs.iter().position(|t| t.path() == canonical || t.doc.path == path) {
            self.active = i;
        } else {
            let doc = match Document::load(&path) {
                Ok(doc) => doc,
                Err(e) => {
                    eprintln!("runemd: {}: {e}", path.display());
                    return false;
                }
            };
            let follow = self.live.follow(&path);
            let mut tab = Tab::new(doc, follow);
            tab.preview = true;
            // Una scheda di anteprima con modifiche non salvate non si butta: resta (`pin`).
            let old = self.tabs.iter().position(|t| t.preview && !matches!(&t.mode, Mode::Source(s) if s.dirty));
            match old {
                Some(i) => {
                    let old = std::mem::replace(&mut self.tabs[i], tab);
                    self.live.unfollow(old.follow);
                    self.active = i;
                }
                None => {
                    for t in &mut self.tabs {
                        t.preview = false;
                    }
                    self.tabs.push(tab);
                    self.active = self.tabs.len() - 1;
                }
            }
        }
        let fronted = self.tab().map(|t| t.doc.id) != before;
        if fronted {
            self.fronted();
        }
        fronted
    }

    /// La scheda `id` diventa fissa: il prossimo clic singolo non la sostituisce.
    pub fn pin(&mut self, id: u64) {
        if let Some(t) = self.tab_of(id) {
            t.preview = false;
        }
    }

    /// Fissa la scheda del file `path`, se è aperto (doppio clic su una riga della sidebar).
    pub fn pin_path(&mut self, path: &Path) {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_owned());
        if let Some(t) = self.tabs.iter_mut().find(|t| t.path() == canonical || t.doc.path == path) {
            t.preview = false;
        }
    }

    /// Chiude la scheda davanti; davanti va quella che stava alla sua destra, o l'ultima. Le
    /// radici guardate solo per quel file escono dal watcher. Vero se si è chiusa.
    pub fn close_active(&mut self) -> bool {
        self.close(self.active)
    }

    /// Chiude la scheda `i` (Cmd+W per quella davanti, il clic centrale per le altre). Una scheda
    /// nel sorgente con modifiche non salvate non si chiude e non scrive: viene davanti con la
    /// riga "Salvare le modifiche a …?" ([`SourceEdit::asking`]), a cui risponde
    /// [`App::answer_close`]. Con la riga già aperta resta com'è. Vero se si è chiusa.
    pub fn close(&mut self, i: usize) -> bool {
        let Some(tab) = self.tabs.get_mut(i) else { return false };
        if let Mode::Source(s) = &mut tab.mode
            && s.dirty
        {
            s.asking = true;
            self.select(i);
            return false;
        }
        self.remove(i);
        true
    }

    /// La risposta alla riga "Salvare le modifiche a …?" della scheda `id`. "Salva" chiude solo se
    /// il salvataggio scrive; se no la riga diventa quella del conflitto o dell'errore. "Non
    /// salvare" chiude senza scrivere; "Annulla" toglie la riga e lascia tutto com'era. Una scheda
    /// che non chiede più (un bottone di un frame vecchio) resta com'è.
    pub fn answer_close(&mut self, id: u64, answer: CloseAnswer) {
        let Some(i) = self.tabs.iter().position(|t| t.doc.id == id) else { return };
        let tab = &mut self.tabs[i];
        let Mode::Source(s) = &mut tab.mode else { return };
        if !s.asking {
            return;
        }
        match answer {
            CloseAnswer::Save => {
                if edit::save(tab, &mut self.live.own).is_ok() {
                    self.remove(i);
                }
            }
            CloseAnswer::Discard => self.remove(i),
            CloseAnswer::Cancel => s.asking = false,
        }
    }

    /// [`App::answer_close`] da un bottone o un tasto: se viene davanti un'altra scheda, la sua
    /// vista riparte da dove era.
    pub fn answer_close_tx(&mut self, id: u64, answer: CloseAnswer, tx: &mut Tx) {
        let before = self.tab().map(|t| t.doc.id);
        self.answer_close(id, answer);
        if self.tab().map(|t| t.doc.id) != before {
            self.restore_view(tx);
        }
    }

    /// La scheda `id` mostra la riga "Salvare le modifiche a …?".
    pub fn asking(&self, id: u64) -> bool {
        self.tabs.iter().any(|t| t.doc.id == id && matches!(&t.mode, Mode::Source(s) if s.asking))
    }

    /// Toglie la scheda `i`, senza salvare; davanti va quella che stava alla sua destra, o
    /// l'ultima. Le radici guardate solo per quel file escono dal watcher.
    fn remove(&mut self, i: usize) {
        let tab = self.tabs.remove(i);
        self.live.unfollow(tab.follow);
        let front = self.active == i;
        if self.active > i || self.active >= self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
        if self.tabs.is_empty() {
            self.sidebar.clear_open();
            self.find.close();
        } else if front {
            self.fronted();
        }
    }

    /// La finestra si chiude o l'app esce (`on_close`, `on_quit`): ogni scheda nel sorgente con
    /// modifiche le salva. Rune non lascia annullare la chiusura: se il salvataggio non scrive (un
    /// conflitto o un errore) il testo va in una copia di recupero ([`edit::recovery`]) e il file
    /// su disco resta com'è. La scheda non ha più modifiche da salvare: una seconda chiamata (Cmd+Q
    /// fa `on_quit` e poi `on_close`) non scrive una seconda copia.
    pub fn save_all(&mut self) {
        let stamp = edit::recovery::stamp_now();
        for tab in &mut self.tabs {
            if !matches!(&tab.mode, Mode::Source(s) if s.dirty) {
                continue;
            }
            let why = match edit::save(tab, &mut self.live.own) {
                Ok(()) => continue,
                Err(edit::SaveError::Conflict) => "il file è cambiato su disco".to_owned(),
                Err(edit::SaveError::Io(e)) => format!("salvataggio non riuscito ({e})"),
                Err(edit::SaveError::NotUtf8) => "il file non è UTF-8".to_owned(),
            };
            let path = tab.doc.path.display().to_string();
            match self.recovery.as_deref().map(|dir| edit::recover(tab, dir, &stamp)) {
                Some(Ok(copy)) => eprintln!("runemd: {path}: {why}; modifiche in {}", copy.display()),
                Some(Err(e)) => eprintln!("runemd: {path}: {why}; copia di recupero non scritta: {e}"),
                None => eprintln!("runemd: {path}: {why}; modifiche non salvate"),
            }
        }
    }

    /// All'avvio: le copie di recupero scritte all'ultima uscita si aprono in una scheda.
    pub fn open_recovered(&mut self) {
        let Some(dir) = self.recovery.as_deref() else { return };
        let copies = edit::recovery::take_pending(dir);
        self.open_paths(copies);
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

    /// Dopo un cambio della scheda davanti, nello stesso handler: la vista riparte dal primo
    /// blocco che la scheda mostrava, in cima, o dall'inizio se non era mai stata davanti; il suo
    /// fuoco, se acceso o armato, risale da adesso (`tx.clock()`), non dalla ricarica di allora.
    ///
    /// La vista della scheda è un elemento nuovo (`ui::doc_view::view`), che parte da 0 con le
    /// altezze stimate, e `Tx::scroll_key_to` vuole una riga montata. Rune fa le due richieste
    /// nello stesso frame, in quest'ordine: prima le rivelazioni (`scroll_center_in_instant`
    /// monta la riga), poi gli scroll (`scroll_key_to` la mette in cima, rimisurando dopo il
    /// layout). Il primo frame dopo il cambio è già quello giusto.
    pub fn restore_view(&mut self, tx: &mut Tx) {
        let clock = tx.clock();
        if let Some(tab) = self.tab_mut() {
            if tab.fire_since.is_some() {
                tab.fire_since = Some(clock);
            }
            if tab.fire_armed.is_some() {
                tab.fire_armed = Some(clock);
            }
        }
        // Con Cerca aperta la ricerca rigira sul file davanti; la vista va al primo risultato
        // quando arriva (`poll_find`).
        self.sync_find();
        let Some(tab) = self.tab_mut() else { return };
        match tab.doc.blocks.iter().position(|b| b.id == tab.visible_first) {
            Some(i) if i > 0 => scroll_block_to_top(tab, i, tx),
            _ => tx.scroll_to_instant("doc", 0.0, 0.0),
        }
    }

    /// Un clic su un titolo dell'indice: il suo blocco va in cima alla vista, senza planata (come
    /// `restore_view`: la riga può non essere montata e la sua altezza è solo stimata). Solo se
    /// la scheda `doc` è ancora davanti e in lettura.
    pub fn go_to_heading(&mut self, doc: u64, block: u64, tx: &mut Tx) {
        let Some(tab) = self.tab_mut().filter(|t| t.doc.id == doc && matches!(t.mode, Mode::Read)) else { return };
        let Some(i) = tab.doc.blocks.iter().position(|b| b.id == block) else { return };
        tab.pick_heading(block);
        scroll_block_to_top(tab, i, tx);
    }

    /// Un clic su una riga della sidebar: il file va nella scheda di anteprima (o si porta
    /// davanti la sua, `open_preview`).
    pub fn open_from_sidebar(&mut self, path: PathBuf, tx: &mut Tx) {
        if self.open_preview(path) {
            self.restore_view(tx);
        }
    }

    /// Un doppio clic su una riga della sidebar: il file si apre (il primo clic l'ha già aperto)
    /// e la sua scheda diventa fissa.
    pub fn pin_from_sidebar(&mut self, path: PathBuf, tx: &mut Tx) {
        self.open_from_sidebar(path.clone(), tx);
        self.pin_path(&path);
    }

    /// La cartella di Cmd+N: quella del file davanti, senza schede la prima cartella osservata.
    pub fn new_file_dir(&self) -> Option<PathBuf> {
        let beside = self.tab().and_then(|t| t.doc.path.parent()).filter(|d| !d.as_os_str().is_empty());
        beside.map(Path::to_path_buf).or_else(|| self.library.roots().first().cloned())
    }

    /// Cmd+N, Invio: crea `name` (vuoto, mai sovrascritto) e lo porta davanti già nel sorgente; un
    /// nome che esiste apre quel file (o la sua scheda), anch'esso nel sorgente. `None` se non si è
    /// creato né aperto niente (nome non valido, nessuna cartella, errore): il campo resta aperto.
    /// `Some(true)` se è venuta davanti un'altra scheda.
    pub fn create_file(&mut self, name: &str) -> Option<bool> {
        edit::new_file::normalize(name)?;
        let dir = self.new_file_dir()?;
        let path = match edit::new_file::create(&dir, name) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("runemd: {}: {e}", dir.display());
                return None;
            }
        };
        let before = self.tab().map(|t| t.doc.id);
        self.open_paths(vec![path.clone()]);
        let tab = self.tabs.get_mut(self.active).filter(|t| t.doc.path == path || t.path() == path.canonicalize().unwrap_or_else(|_| path.clone()))?;
        if matches!(tab.mode, Mode::Read) {
            if edit::editable(tab.follow.path()) {
                tab.mode = Mode::Source(Box::new(edit::enter(&tab.doc)));
            } else {
                tab.not_utf8 = true;
            }
        }
        Some(self.tab().map(|t| t.doc.id) != before)
    }

    /// Cmd+N, Invio dal campo: come `create_file`, e se è venuta davanti un'altra scheda ne
    /// ripristina la vista. Vero se il campo si chiude.
    pub fn create_from_sidebar(&mut self, name: String, tx: &mut Tx) -> bool {
        match self.create_file(&name) {
            Some(fronted) => {
                if fronted {
                    self.restore_view(tx);
                }
                true
            }
            None => false,
        }
    }

    /// La ricerca di Cmd+P è cambiata. Un solo campo aperto alla volta: Cerca si chiude.
    pub fn requery_sidebar(&mut self) {
        self.find.close();
        self.sidebar.requery(&self.library.index);
    }

    /// Cmd+F: la barra di Cerca si apre col fuoco, e i campi di Cmd+P e Cmd+N si chiudono. Solo
    /// in lettura: nel sorgente (Cmd+E) e senza schede non fa niente. Vero se si è aperta.
    pub fn open_find(&mut self) -> bool {
        if !self.tab().is_some_and(|t| matches!(t.mode, Mode::Read)) {
            return false;
        }
        self.sidebar.quick.close();
        self.sidebar.quick_rows.clear();
        self.sidebar.new_file.close();
        self.find.show();
        true
    }

    /// La ricerca di Cerca è cambiata: si chiedono i risultati, e l'attivo sarà il primo dal
    /// primo blocco visibile in giù (la vista lo porta al centro quando arrivano).
    pub fn find_changed(&mut self) {
        let Some(tab) = self.tabs.get(self.active).filter(|t| matches!(t.mode, Mode::Read)) else { return };
        let from = tab.doc.blocks.iter().position(|b| b.id == tab.visible_first).unwrap_or(0);
        self.find.ask(&tab.doc, ui::find::Goal::From(from));
    }

    /// Invio (`1`) e Maiusc+Invio (`-1`) in Cerca: il risultato dopo o prima, in giro, al centro
    /// della vista dal primo frame dopo il tasto. Con i risultati ancora sul thread il passo
    /// aspetta che arrivino (`poll_find`).
    pub fn find_step(&mut self, delta: isize, tx: &mut Tx) {
        if self.find.enter(delta) {
            self.reveal_find(tx);
        }
    }

    /// Porta al centro della vista il risultato attivo, anche in un blocco non montato, senza
    /// planata. Vero se c'era un risultato.
    fn reveal_find(&self, tx: &mut Tx) -> bool {
        let Some(key) = self.tab().and_then(|t| self.find.reveal_key(&t.doc)) else { return false };
        tx.scroll_center_in_instant("doc", key);
        true
    }

    /// Come `reveal_find`, ma la vista si sposta solo quanto basta, e niente se il risultato si
    /// vede già: mentre si scrive la ricerca il testo non salta a ogni lettera.
    fn show_find(&self, tx: &mut Tx) -> bool {
        let Some(key) = self.tab().and_then(|t| self.find.reveal_key(&t.doc)) else { return false };
        tx.scroll_into_view_in("doc", key);
        true
    }

    /// Tiene Cerca al passo con la scheda davanti: un'altra scheda rigira la ricerca dal primo
    /// risultato; una ricarica la rigira tenendo l'attivo vicino a dov'era. Nel sorgente o senza
    /// schede la barra si chiude.
    pub fn sync_find(&mut self) {
        if !self.find.open {
            return;
        }
        let Some(tab) = self.tabs.get(self.active).filter(|t| matches!(t.mode, Mode::Read)) else {
            self.find.close();
            return;
        };
        if let Some(goal) = self.find.stale(&tab.doc) {
            self.find.ask(&tab.doc, goal);
        }
    }

    /// Da `poll`: segue la scheda davanti (`sync_find`) e applica i risultati arrivati dal thread
    /// della ricerca; per una ricerca nuova o un'altra scheda la vista mostra l'attivo. Vero se la
    /// vista cambia.
    pub fn poll_find(&mut self, tx: &mut Tx) -> bool {
        self.sync_find();
        let Some(tab) = self.tabs.get(self.active) else { return false };
        match self.find.poll(&tab.doc) {
            Some(reveal) => {
                match reveal {
                    ui::find::Reveal::Show => {
                        self.show_find(tx);
                    }
                    ui::find::Reveal::Center => {
                        self.reveal_find(tx);
                    }
                    ui::find::Reveal::No => {}
                }
                true
            }
            None => false,
        }
    }

    /// Le righe visibili riportate dalla vista del documento `id`: valgono per la sua scheda,
    /// anche se nel frattempo un'altra è venuta davanti.
    pub fn visible_rows(&mut self, id: u64, rows: Range<usize>) -> bool {
        let wide = self.wide;
        self.tab_of(id).is_some_and(|t| {
            let outline = wide && matches!(t.mode, Mode::Read);
            t.visible_rows(rows, outline)
        })
    }

    /// La vista o la pagina del documento `id` si sono spostate (`doc_view::Edge`).
    pub fn doc_edge(&mut self, id: u64, edge: ui::doc_view::Edge) -> bool {
        let wide = self.wide;
        self.tab_of(id).is_some_and(|t| {
            let outline = wide && matches!(t.mode, Mode::Read);
            t.edge(edge, outline)
        })
    }

    /// La finestra è larga `w` (`on_layout` della radice): l'indice dei titoli si vede se è
    /// almeno `outline_min_window`. La vista non rigira per questo: decide da `Cx::layout_with`.
    pub fn set_window_width(&mut self, w: f32) {
        self.window_w = w;
        self.wide = w >= self.outline_min_window();
    }

    /// La finestra più stretta con l'indice in vista, con le larghezze di adesso dei pannelli.
    pub fn outline_min_window(&self) -> f32 {
        ui::theme::outline_min_window(self.settings.sidebar.width, self.settings.outline.width)
    }

    /// Il bordo destro della sidebar trascinato (`panes`): segue il puntatore e scatta alle
    /// larghezze ricordate; lasciato, la larghezza si ricorda e si salva; Esc la rimette com'era.
    /// Con l'indice possibile (finestra larga) si allarga finché l'indice resta, se no finché il
    /// pannello resta largo almeno `DOC_MIN`.
    pub fn drag_sidebar(&mut self, d: &mut rune::Drag) {
        let fixed = if self.wide { self.outline_min_window() - self.settings.sidebar.width } else { ui::theme::FRAME + ui::theme::DOC_MIN };
        let room = self.window_w - fixed;
        if drag_pane(&mut self.settings.sidebar, crate::panes::SIDEBAR, d, d.dx, room) {
            self.save_settings();
        }
    }

    /// Il bordo sinistro dell'indice trascinato: come `drag_sidebar`, verso sinistra si allarga;
    /// mai tanto che l'indice si nasconda.
    pub fn drag_outline(&mut self, d: &mut rune::Drag) {
        let room = self.window_w - (self.outline_min_window() - self.settings.outline.width);
        if drag_pane(&mut self.settings.outline, crate::panes::OUTLINE, d, -d.dx, room) {
            self.save_settings();
        }
    }

    /// Doppio clic sul bordo: la larghezza di default.
    pub fn reset_pane(&mut self, outline: bool) {
        let (pane, limits) = if outline { (&mut self.settings.outline, crate::panes::OUTLINE) } else { (&mut self.settings.sidebar, crate::panes::SIDEBAR) };
        pane.width = limits.default;
        self.save_settings();
    }

    /// I segni di Cerca del blocco `block` della scheda davanti, nella riga `row_key`.
    pub fn find_hits(&self, block: &crate::doc::document::Block, row_key: &str) -> ui::find::Marks {
        self.find.highlights(block.id, block.hash, row_key)
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
    /// scorre le istanze solo allora. Il sorgente non mostra fuoco né dither.
    pub fn effects_lit(&self) -> bool {
        let fire = self.settings.fire;
        self.ring_lit() || self.tab().is_some_and(|t| matches!(t.mode, Mode::Read) && (fire || t.dither_since.is_some()))
    }

    /// Il fuoco della scheda davanti è a riposo e si muove: senza display link, `poll` chiede un
    /// risveglio ogni `fx::fire::REST_FRAME` per ridisegnarlo. Fermo (finestra non attiva, "riduci
    /// movimento") o acceso (lo muove il display link) no.
    pub fn fire_rests(&self) -> bool {
        self.settings.fire && self.window_active && !self.reduced_seen.get() && self.tab().is_some_and(|t| matches!(t.mode, Mode::Read) && t.fire_since.is_none())
    }

    /// Cmd+E sulla scheda davanti: entra nel sorgente o, se c'è già, salva se ci sono modifiche e
    /// torna alla lettura, dal blocco dove la lettura era rimasta. Se il salvataggio non scrive (il
    /// file è cambiato su disco, o un errore) resta nel sorgente con la riga. Vero se c'era una
    /// scheda.
    pub fn toggle_source(&mut self, tx: &mut Tx) -> bool {
        let Some(tab) = self.tabs.get_mut(self.active) else { return false };
        match tab.mode {
            Mode::Read => {
                if !edit::editable(tab.follow.path()) {
                    tab.not_utf8 = true;
                    return true;
                }
                tab.not_utf8 = false;
                tab.mode = Mode::Source(Box::new(edit::enter(&tab.doc)));
                // Cerca vale solo in lettura.
                self.find.close();
            }
            Mode::Source(_) => {
                if edit::leave(tab, &mut self.live.own).is_ok() {
                    self.restore_view(tx);
                }
            }
        }
        true
    }

    /// Cmd+S: salva il sorgente della scheda davanti (se non scrive, la riga lo dice). Vero se la
    /// scheda era nel sorgente.
    pub fn save_source(&mut self) -> bool {
        let Some(tab) = self.tabs.get_mut(self.active).filter(|t| matches!(t.mode, Mode::Source(_))) else { return false };
        tab.preview = false;
        let _ = edit::save(tab, &mut self.live.own);
        true
    }

    /// "Ricarica" nella riga del conflitto della scheda `id`: scarta le modifiche e riparte dal disco.
    pub fn reload_source(&mut self, id: u64) {
        if let Some(tab) = self.tab_of(id) {
            edit::reload(tab);
        }
    }

    /// "Tieni il mio" nella riga del conflitto della scheda `id`: scrive il testo del sorgente.
    pub fn keep_mine(&mut self, id: u64) {
        if let Some(i) = self.tabs.iter().position(|t| t.doc.id == id) {
            let _ = edit::keep_mine(&mut self.tabs[i], &mut self.live.own);
        }
    }

    /// Il campo del pezzo `piece` del sorgente della scheda `id` (`on_edit` della vista). Se la
    /// scheda non c'è più, è tornata alla lettura o il pezzo si è unito a un altro, un campo di
    /// scarto: il tasto si perde, nessun altro testo cambia.
    pub fn source_edit(&mut self, id: u64, piece: u64) -> &mut TextEdit {
        let src = self.tabs.iter_mut().find(|t| t.doc.id == id).and_then(|t| match &mut t.mode {
            Mode::Source(s) => s.pieces.edit_mut(piece),
            Mode::Read => None,
        });
        match src {
            Some(edit) => edit,
            None => &mut self.spare_edit,
        }
    }

    /// Un tasto ai bordi del pezzo `piece` del sorgente della scheda `id` (`on_input_key`): vero
    /// se l'ha preso (unito due pezzi o passato al vicino). Un'unione toglie un `\n`: il testo è
    /// cambiato. Rune fa seguire il cursore allo scroll solo dopo un tasto che passa dal campo:
    /// un tasto vuoto al campo che ha adesso il fuoco lo chiede senza cambiare niente.
    pub fn source_edge(&mut self, id: u64, piece: u64, edge: edit::blocks::Edge, first: bool, last: bool, tx: &mut Tx) -> bool {
        let Some(Mode::Source(s)) = self.tab_of(id).map(|t| &mut t.mode) else { return false };
        let took = s.pieces.edge(piece, edge, first, last);
        let joined = took && matches!(edge, edit::blocks::Edge::JoinBack | edit::blocks::Edge::JoinForward);
        if joined {
            s.dirty = true;
        }
        if took {
            tx.send_key(rune::KeyEvent { key: rune::Key::Text(String::new()), mods: rune::Mods::default() });
        }
        if joined {
            self.pin(id);
        }
        took
    }

    /// Il testo del sorgente della scheda `id` è cambiato.
    pub fn source_changed(&mut self, id: u64) {
        if let Some(t) = self.tab_of(id)
            && let Mode::Source(s) = &mut t.mode
        {
            s.dirty = true;
            // Una modifica fissa la scheda di anteprima.
            t.preview = false;
        }
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
        for (i, tab) in self.tabs.iter_mut().enumerate() {
            // Nel sorgente la vista non si ricarica da sola: gli eventi di questo file si perdono
            // e `edit::leave` lo fa rileggere. Una scrittura di fuori (il disco non ha più il
            // contenuto da cui parte la modifica, né quello del nostro ultimo salvataggio) accende
            // comunque "sta scrivendo", così si vede prima del conflitto al salvataggio.
            if let Mode::Source(src) = &tab.mode {
                if self.live.changed(tab.follow.path()) && edit::changed_on_disk(tab.follow.path(), src.base_hash) {
                    self.live.touch_open(tab.follow.path(), now);
                }
                continue;
            }
            let (reloaded, recheck) = self.live.poll(&mut tab.follow, &mut tab.doc, now);
            wakes.extend(recheck);
            if let Some(r) = &reloaded {
                // La sonda di latenza misura fino al frame che mostra la ricarica: solo la scheda
                // davanti ne ha uno.
                if i == self.active {
                    probe::reloaded(r);
                }
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

    #[test]
    fn glass_env_reads_a_percentage() {
        assert_eq!(glass_env(Some("60")), Some(0.6));
        assert_eq!(glass_env(Some(" 40 ")), Some(0.4));
        assert_eq!(glass_env(Some("140")), None);
        assert_eq!(glass_env(Some("vetro")), None);
        assert_eq!(glass_env(None), None);
    }

    pub(crate) fn empty_app() -> App {
        App::bare(index::Library::load(Vec::new(), None))
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

    fn app_with_root(root: &Path) -> App {
        App::bare(index::Library::load(vec![root.to_path_buf()], None))
    }

    /// "Riduci movimento": la preferenza di sistema, salvo `RUNEMD_REDUCE_MOTION`.
    #[test]
    fn reduced_motion_follows_the_system_unless_the_env_says_otherwise() {
        let mut app = empty_app();
        app.motion_env = None;
        assert!(app.reduced_motion(true) && !app.reduced_motion(false));
        app.motion_env = Some(true);
        assert!(app.reduced_motion(false), "RUNEMD_REDUCE_MOTION=1");
        app.motion_env = Some(false);
        assert!(!app.reduced_motion(true), "RUNEMD_REDUCE_MOTION=0");
    }

    #[test]
    fn new_file_without_tabs_goes_to_the_first_root_in_source() {
        let root = tempfile::tempdir().unwrap();
        let mut app = app_with_root(root.path());
        assert_eq!(app.create_file("prova"), Some(true));
        let path = root.path().join("prova.md");
        assert_eq!(fs::read_to_string(&path).unwrap(), "");
        assert_eq!(app.tabs.len(), 1);
        assert_eq!(app.tabs[0].title(), "prova.md");
        assert!(matches!(app.tabs[0].mode, Mode::Source(_)));
    }

    #[test]
    fn new_file_goes_beside_the_open_file() {
        let (dir, p) = files(&["a.md"]);
        let root = tempfile::tempdir().unwrap();
        let mut app = app_with_root(root.path());
        app.open_paths(vec![p[0].clone()]);
        assert_eq!(app.create_file("b.md"), Some(true));
        assert!(dir.path().join("b.md").exists() && !root.path().join("b.md").exists());
        assert_eq!(app.tabs.len(), 2);
        assert!(matches!(app.tabs[0].mode, Mode::Read), "l'altra scheda non cambia");
        assert!(matches!(app.tabs[1].mode, Mode::Source(_)));
    }

    #[test]
    fn an_existing_name_opens_that_file_in_source_without_touching_it() {
        let (dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(vec![p[0].clone(), p[1].clone()]);
        // Già aperto in una scheda dietro: si porta davanti, nel sorgente, senza una scheda nuova.
        assert_eq!(app.create_file("a"), Some(true));
        assert_eq!(app.tabs.len(), 2);
        assert_eq!(app.active, 0);
        assert!(matches!(app.tabs[0].mode, Mode::Source(_)));
        assert_eq!(fs::read_to_string(dir.path().join("a.md")).unwrap(), "# a.md\n");
        // Non aperto: una scheda nuova, nel sorgente.
        fs::write(dir.path().join("c.md"), "# c\n").unwrap();
        assert_eq!(app.create_file("c.md"), Some(true));
        assert_eq!(app.tabs.len(), 3);
        assert!(matches!(app.tabs[2].mode, Mode::Source(_)));
        assert_eq!(fs::read_to_string(dir.path().join("c.md")).unwrap(), "# c\n");
    }

    #[test]
    fn invalid_names_create_and_open_nothing() {
        let root = tempfile::tempdir().unwrap();
        let mut app = app_with_root(root.path());
        for bad in ["", "  ", "a/b", ".x"] {
            assert_eq!(app.create_file(bad), None, "{bad:?}");
        }
        assert!(app.tabs.is_empty() && fs::read_dir(root.path()).unwrap().count() == 0);
        // Senza cartella osservata né scheda non c'è dove creare.
        assert_eq!(empty_app().create_file("prova"), None);
    }

    #[test]
    fn creating_a_file_lights_no_ring() {
        let root = tempfile::tempdir().unwrap();
        let mut app = app_with_root(root.path());
        app.create_file("prova");
        assert!(!app.ring_lit() && !app.effects_lit());
        assert!(app.live.writing(Instant::now()).is_empty(), "niente \"sta scrivendo\"");
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

    /// Chiusa l'ultima scheda la sidebar non mostra più il repo del file chiuso.
    #[test]
    fn closing_the_last_tab_clears_the_sidebar() {
        let (_dir, p) = files(&["a.md"]);
        let mut app = empty_app();
        app.open_paths(p);
        assert!(app.sidebar.repo.is_some());
        app.close_active();
        assert!(app.sidebar.repo.is_none());
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
        app.tabs[0].visible_rows(0..1, false);
        app.select(1);
        let change = app.tabs[0].doc.update("# a.md\n\nuno\n\ndue\n");
        assert!(!app.tabs[0].note_reload(&change, 3.0));
        assert_eq!(app.tabs[0].below.count(), 2, "conta in secondo piano");
        assert!(app.tab().unwrap().fire_since.is_none(), "la scheda davanti resta spenta");
        assert!(app.fire_rests(), "la scheda davanti resta a riposo");
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

    /// Il salvataggio del sorgente torna alla lettura con il testo salvato: la rilettura del
    /// watcher è una scrittura dell'app, quindi niente segno dei blocchi cambiati, fuoco, dither
    /// o "sta scrivendo".
    #[test]
    fn saving_the_source_lights_nothing() {
        let (_dir, p) = files(&["a.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        app.tabs[0].visible_rows(0..1, false);
        let tab = &mut app.tabs[0];
        tab.mode = Mode::Source(Box::new(edit::enter(&tab.doc)));
        let mine = "# a.md\n\nuno\n\ndue\n\n```rs\nfn";
        if let Mode::Source(s) = &mut tab.mode {
            s.set_text(mine);
            s.dirty = true;
        }
        assert!(edit::leave(tab, &mut app.live.own).is_ok());
        assert_eq!(fs::read_to_string(&p[0]).unwrap(), mine);
        let hash = crate::doc::ids::hash_str(mine);
        assert!(app.live.own.is_own(&app.tabs[0].doc.path, hash));
        // La rilettura chiesta da `leave` gira su un thread: si fa `poll` finché la consuma.
        let t0 = Instant::now();
        while app.live.own.is_own(&app.tabs[0].doc.path, hash) {
            assert!(t0.elapsed() < Duration::from_secs(5), "rilettura mai arrivata");
            let (changed, _) = app.poll_tabs(1.0, Instant::now());
            assert!(!changed, "la vista non cambia per la nostra scrittura");
            std::thread::sleep(Duration::from_millis(5));
        }
        let tab = &app.tabs[0];
        assert_eq!((tab.doc.file_hash, tab.doc.blocks.len()), (hash, 4), "la lettura mostra il testo salvato");
        assert!(tab.doc.blocks.iter().all(|b| !tab.is_marked(b.id)), "nessun segno");
        assert_eq!((tab.fire_since, tab.fire_armed, tab.below.count()), (None, None, 0), "nessun fuoco");
        assert_eq!(tab.dither_since, None, "nessun dither");
        assert!(app.live.writing_left(tab.path(), Instant::now()).is_none() && !app.ring_lit(), "nessun anello");
    }

    /// "Tieni il mio" dopo un conflitto: come un salvataggio, la rilettura del watcher è una
    /// scrittura dell'app; tornati alla lettura niente segni, fuoco, dither o anello.
    #[test]
    fn keeping_mine_lights_nothing() {
        let (_dir, p) = files(&["a.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        app.tabs[0].visible_rows(0..1, false);
        let mine = "# a.md\n\nuno\n\ndue\n";
        edit_tab(&mut app, 0, mine);
        fs::write(&p[0], "# a.md\n\ndell'agente\n").unwrap();
        assert!(app.save_source(), "Cmd+S sulla scheda davanti, nel sorgente");
        let id = app.tabs[0].doc.id;
        assert!(matches!(&app.tabs[0].mode, Mode::Source(s) if s.notice == Some(edit::Notice::Conflict)));
        app.keep_mine(id);
        assert_eq!(fs::read_to_string(&p[0]).unwrap(), mine);
        let hash = crate::doc::ids::hash_str(mine);
        assert!(edit::leave(&mut app.tabs[0], &mut app.live.own).is_ok());
        let t0 = Instant::now();
        while app.live.own.is_own(&app.tabs[0].doc.path, hash) {
            assert!(t0.elapsed() < Duration::from_secs(5), "rilettura mai arrivata");
            let (changed, _) = app.poll_tabs(1.0, Instant::now());
            assert!(!changed, "la vista non cambia per la nostra scrittura");
            std::thread::sleep(Duration::from_millis(5));
        }
        let tab = &app.tabs[0];
        assert_eq!((tab.doc.file_hash, tab.doc.blocks.len()), (hash, 3), "la lettura mostra il mio testo");
        assert!(tab.doc.blocks.iter().all(|b| !tab.is_marked(b.id)), "nessun segno");
        assert_eq!((tab.fire_since, tab.fire_armed, tab.below.count()), (None, None, 0), "nessun fuoco");
        assert_eq!(tab.dither_since, None, "nessun dither");
        assert!(app.live.writing_left(tab.path(), Instant::now()).is_none() && !app.ring_lit(), "nessun anello");
    }

    /// Mette la scheda `i` nel sorgente con il testo `text` modificato.
    fn edit_tab(app: &mut App, i: usize, text: &str) {
        let tab = &mut app.tabs[i];
        let mut src = edit::enter(&tab.doc);
        src.set_text(text);
        src.dirty = true;
        tab.mode = Mode::Source(Box::new(src));
    }

    fn asking(app: &App, i: usize) -> bool {
        matches!(&app.tabs[i].mode, Mode::Source(s) if s.asking)
    }

    /// Un mtime nel passato: una scrittura lo porterebbe ad adesso.
    fn age(path: &Path) -> std::time::SystemTime {
        let old = std::time::SystemTime::now() - Duration::from_secs(3600);
        fs::File::options().write(true).open(path).unwrap().set_modified(old).unwrap();
        old
    }

    /// Senza modifiche (anche nel sorgente) la scheda si chiude subito, senza chiedere.
    #[test]
    fn closing_without_edits_closes_at_once() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        app.tabs[0].mode = Mode::Source(Box::new(edit::enter(&app.tabs[0].doc)));
        assert!(app.close(0), "chiusa");
        assert!(app.close_active(), "chiusa");
        assert!(app.tabs.is_empty());
    }

    /// Con modifiche non salvate Cmd+W (o il clic centrale) non chiude e non scrive: la scheda
    /// viene davanti con la domanda. Un secondo Cmd+W la lascia com'è.
    #[test]
    fn closing_a_tab_with_edits_asks() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        let old = age(&p[0]);
        edit_tab(&mut app, 0, "# a.md\n\nmio\n");
        assert_eq!(app.active, 1);
        assert!(!app.close(0), "non chiusa");
        assert_eq!((app.tabs.len(), app.active), (2, 0), "aperta e davanti");
        assert!(asking(&app, 0), "chiede");
        assert!(!app.close_active(), "il secondo Cmd+W non chiude");
        assert!(asking(&app, 0), "chiede ancora");
        assert_eq!(fs::metadata(&p[0]).unwrap().modified().unwrap(), old, "niente scritto");
    }

    /// "Salva": scrive e chiude.
    #[test]
    fn save_answer_writes_and_closes() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        edit_tab(&mut app, 0, "# a.md\n\nmio\n");
        app.close(0);
        let id = app.tabs[0].doc.id;
        app.answer_close(id, CloseAnswer::Save);
        assert_eq!(fs::read_to_string(&p[0]).unwrap(), "# a.md\n\nmio\n");
        assert_eq!(app.tabs.len(), 1, "chiusa");
        assert_eq!(app.tabs[0].doc.path, p[1]);
    }

    /// "Salva" con il file cambiato su disco: niente si scrive, la scheda resta con la riga del
    /// conflitto al posto della domanda.
    #[test]
    fn save_answer_with_a_conflict_keeps_the_tab() {
        let (_dir, p) = files(&["a.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        edit_tab(&mut app, 0, "# a.md\n\nmio\n");
        app.close(0);
        fs::write(&p[0], "# a.md\n\ndell'agente\n").unwrap();
        let id = app.tabs[0].doc.id;
        app.answer_close(id, CloseAnswer::Save);
        assert_eq!(app.tabs.len(), 1, "non chiusa");
        assert!(matches!(&app.tabs[0].mode, Mode::Source(s) if !s.asking && s.notice == Some(edit::Notice::Conflict) && s.dirty && s.text() == "# a.md\n\nmio\n"));
        assert_eq!(fs::read_to_string(&p[0]).unwrap(), "# a.md\n\ndell'agente\n");
    }

    /// "Salva" con un errore di scrittura: la scheda resta con "Salvataggio non riuscito".
    #[test]
    fn save_answer_with_a_write_error_keeps_the_tab() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, p) = files(&["a.md"]);
        if edit::tests::as_root(dir.path()) {
            return;
        }
        let mut app = empty_app();
        app.open_paths(p.clone());
        edit_tab(&mut app, 0, "# a.md\n\nmio\n");
        app.close(0);
        let id = app.tabs[0].doc.id;
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
        app.answer_close(id, CloseAnswer::Save);
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(app.tabs.len(), 1, "non chiusa");
        assert!(matches!(&app.tabs[0].mode, Mode::Source(s) if !s.asking && s.notice == Some(edit::Notice::SaveFailed) && s.dirty));
    }

    /// "Non salvare": chiude e butta le modifiche; il file su disco non cambia.
    #[test]
    fn discard_answer_closes_without_writing() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        let before = fs::read(&p[0]).unwrap();
        let old = age(&p[0]);
        edit_tab(&mut app, 0, "# a.md\n\nmio\n");
        app.close(0);
        let id = app.tabs[0].doc.id;
        app.answer_close(id, CloseAnswer::Discard);
        assert_eq!(app.tabs.len(), 1, "chiusa");
        assert_eq!(fs::read(&p[0]).unwrap(), before, "stessi byte");
        assert_eq!(fs::metadata(&p[0]).unwrap().modified().unwrap(), old, "niente scritto");
    }

    /// "Annulla" (ed Esc): la domanda sparisce, la scheda resta con modifiche e cursore.
    #[test]
    fn cancel_answer_keeps_edits_and_caret() {
        let (_dir, p) = files(&["a.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        edit_tab(&mut app, 0, "# a.md\n\nmio\n");
        if let Mode::Source(s) = &mut app.tabs[0].mode {
            s.pieces.set_caret(9);
        }
        app.close(0);
        let id = app.tabs[0].doc.id;
        app.answer_close(id, CloseAnswer::Cancel);
        assert_eq!(app.tabs.len(), 1);
        assert!(matches!(&app.tabs[0].mode, Mode::Source(s) if !s.asking && s.dirty && s.text() == "# a.md\n\nmio\n" && s.pieces.caret() == 9));
        assert_eq!(fs::read_to_string(&p[0]).unwrap(), "# a.md\n");
    }

    /// Una risposta per una scheda che non chiede più (il bottone di un frame vecchio) non fa niente.
    #[test]
    fn a_stale_answer_does_nothing() {
        let (_dir, p) = files(&["a.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        edit_tab(&mut app, 0, "# a.md\n\nmio\n");
        let id = app.tabs[0].doc.id;
        app.answer_close(id, CloseAnswer::Discard);
        assert_eq!(app.tabs.len(), 1, "non chiedeva: resta");
    }

    /// Cmd+S con la domanda aperta: salva e la domanda sparisce (non c'è più niente da salvare).
    #[test]
    fn saving_while_asking_ends_the_ask() {
        let (_dir, p) = files(&["a.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        edit_tab(&mut app, 0, "# a.md\n\nmio\n");
        app.close(0);
        assert!(app.save_source());
        assert!(!asking(&app, 0));
        assert_eq!(app.tabs.len(), 1);
    }

    /// All'uscita dall'app (`on_close`, `on_quit`) le modifiche di ogni scheda si salvano.
    #[test]
    fn quitting_saves_every_tab() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        edit_tab(&mut app, 0, "# a.md\n\nuno\n");
        edit_tab(&mut app, 1, "# b.md\n\ndue\n");
        app.save_all();
        assert_eq!(fs::read_to_string(&p[0]).unwrap(), "# a.md\n\nuno\n");
        assert_eq!(fs::read_to_string(&p[1]).unwrap(), "# b.md\n\ndue\n");
    }

    /// Le copie di recupero in `dir` (i file `.md`, senza la lista delle copie da aprire).
    fn copies(dir: &Path) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = fs::read_dir(dir).map(|r| r.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.extension().is_some_and(|e| e == "md")).collect()).unwrap_or_default();
        v.sort();
        v
    }

    /// Uscire con un conflitto non perde le modifiche. Il testo va in una copia di recupero
    /// (una sola, anche se `on_quit` e `on_close` arrivano tutti e due), il file su disco resta
    /// quello dell'agente, e al prossimo avvio la copia si apre in una scheda (una volta sola).
    #[test]
    fn quitting_with_a_conflict_keeps_a_recovery_copy() {
        let (_dir, p) = files(&["a.md"]);
        let support = tempfile::tempdir().unwrap();
        let rec = support.path().join("Recuperati");
        let mut app = empty_app();
        app.recovery = Some(rec.clone());
        app.open_paths(p.clone());
        edit_tab(&mut app, 0, "# a.md\n\nmio\n");
        fs::write(&p[0], "# a.md\n\ndell'agente\n").unwrap();
        app.save_all();
        app.save_all();
        assert_eq!(fs::read_to_string(&p[0]).unwrap(), "# a.md\n\ndell'agente\n", "il file resta com'è");
        let found = copies(&rec);
        assert_eq!(found.len(), 1, "una sola copia: {found:?}");
        assert_eq!(fs::read_to_string(&found[0]).unwrap(), "# a.md\n\nmio\n", "il testo esatto");
        let name = found[0].file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("a-") && name.len() == "a-AAAAMMGG-HHMMSS.md".len(), "{name}");
        // Il prossimo avvio: la copia si apre in una scheda e resta su disco.
        let mut next = empty_app();
        next.recovery = Some(rec.clone());
        next.open_recovered();
        assert_eq!(next.tabs.len(), 1);
        assert_eq!(next.tabs[0].title(), name);
        assert!(found[0].exists(), "la copia resta finché l'utente la cancella");
        // L'avvio dopo non la riapre.
        let mut later = empty_app();
        later.recovery = Some(rec);
        later.open_recovered();
        assert!(later.tabs.is_empty());
    }

    /// Anche un errore di scrittura all'uscita lascia una copia; il file resta com'era.
    #[test]
    fn quitting_with_a_write_error_keeps_a_recovery_copy() {
        use std::os::unix::fs::PermissionsExt;
        let (dir, p) = files(&["a.md"]);
        if edit::tests::as_root(dir.path()) {
            return;
        }
        let support = tempfile::tempdir().unwrap();
        let rec = support.path().join("Recuperati");
        let mut app = empty_app();
        app.recovery = Some(rec.clone());
        app.open_paths(p.clone());
        edit_tab(&mut app, 0, "# a.md\n\nmio\n");
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
        app.save_all();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(fs::read_to_string(&p[0]).unwrap(), "# a.md\n");
        let found = copies(&rec);
        assert_eq!(found.len(), 1);
        assert_eq!(fs::read_to_string(&found[0]).unwrap(), "# a.md\n\nmio\n");
    }

    /// Un agente che scrive il file in modifica (Cmd+E) accende "sta scrivendo" come in
    /// lettura, prima che il salvataggio trovi il conflitto; il nostro salvataggio no.
    #[test]
    fn an_external_write_during_source_lights_the_ring_and_our_save_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().canonicalize().unwrap().join("a.md");
        fs::write(&p, "# a\n").unwrap();
        let mut app = empty_app();
        app.open_paths(vec![p.clone()]);
        app.live.start(std::sync::Arc::new(|| {}), Vec::new());
        edit_tab(&mut app, 0, "# a\n\nmio\n");
        std::thread::sleep(Duration::from_millis(300));
        // Il nostro salvataggio: l'evento arriva, ma l'anello resta spento.
        assert!(app.save_source());
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_millis(1500) {
            app.poll_tabs(1.0, Instant::now());
            assert!(app.live.writing_left(&p, Instant::now()).is_none(), "la nostra scrittura non accende l'anello");
            std::thread::sleep(Duration::from_millis(16));
        }
        // L'agente scrive: l'anello si accende, la scheda resta nel sorgente con le modifiche.
        fs::write(&p, "# a\n\ndell'agente\n").unwrap();
        let t0 = Instant::now();
        while app.live.writing_left(&p, Instant::now()).is_none() {
            assert!(t0.elapsed() < Duration::from_secs(5), "anello mai acceso");
            app.poll_tabs(1.0, Instant::now());
            std::thread::sleep(Duration::from_millis(16));
        }
        assert!(matches!(&app.tabs[0].mode, Mode::Source(s) if s.text() == "# a\n\nmio\n"));
    }

    /// Da capo a fondo con il watcher vero: la cartella del file aperto sparisce (un `git
    /// checkout` senza `docs/`) e ricompare con il file cambiato; la scheda si ricarica.
    #[test]
    fn a_tab_whose_folder_is_deleted_and_recreated_reloads() {
        let dir = tempfile::tempdir().unwrap();
        let docs = dir.path().canonicalize().unwrap().join("docs");
        fs::create_dir(&docs).unwrap();
        let p = docs.join("a.md");
        fs::write(&p, "# a\n").unwrap();
        let mut app = empty_app();
        app.open_paths(vec![p.clone()]);
        app.live.start(std::sync::Arc::new(|| {}), Vec::new());
        std::thread::sleep(Duration::from_millis(300));
        fs::remove_dir_all(&docs).unwrap();
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_millis(1500) {
            app.poll_tabs(1.0, Instant::now());
            std::thread::sleep(Duration::from_millis(16));
        }
        fs::create_dir(&docs).unwrap();
        fs::write(&p, "# a\n\ntornato\n").unwrap();
        let t0 = Instant::now();
        while app.tabs[0].doc.blocks.len() != 2 {
            assert!(t0.elapsed() < Duration::from_secs(6), "la scheda non si è più ricaricata");
            app.poll_tabs(1.0, Instant::now());
            std::thread::sleep(Duration::from_millis(16));
        }
    }

    /// Nel sorgente la scheda non si ricarica; tornata alla lettura senza modifiche, rilegge e
    /// mostra la scrittura arrivata nel frattempo. Ogni scheda ha la sua modalità.
    #[test]
    fn the_source_suspends_reload_and_leaving_rereads() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(p.clone());
        let tab = &mut app.tabs[0];
        tab.mode = Mode::Source(Box::new(edit::enter(&tab.doc)));
        assert!(matches!(app.tabs[1].mode, Mode::Read), "l'altra scheda resta in lettura");
        fs::write(&p[0], "# a.md\n\ndell'agente\n").unwrap();
        app.tabs[0].follow.reread();
        for _ in 0..20 {
            app.poll_tabs(1.0, Instant::now());
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(app.tabs[0].doc.blocks.len(), 1, "sospesa nel sorgente");
        assert!(edit::leave(&mut app.tabs[0], &mut app.live.own).is_ok());
        let t0 = Instant::now();
        while app.tabs[0].doc.blocks.len() != 2 {
            assert!(t0.elapsed() < Duration::from_secs(5), "rilettura mai arrivata");
            app.poll_tabs(1.0, Instant::now());
            std::thread::sleep(Duration::from_millis(5));
        }
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

    // La scheda di anteprima (come VS Code): un clic singolo nella sidebar apre il file in una sola
    // scheda che il clic dopo sostituisce.

    fn previews(app: &App) -> Vec<bool> {
        app.tabs.iter().map(|t| t.preview).collect()
    }

    #[test]
    fn a_single_click_opens_one_preview_tab_that_the_next_replaces() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        assert!(app.open_preview(p[0].clone()));
        assert_eq!(previews(&app), [true]);
        assert!(app.open_preview(p[1].clone()));
        assert_eq!(app.tabs.len(), 1, "sostituita, non aggiunta");
        assert_eq!(app.tab().unwrap().doc.path, p[1]);
        assert_eq!(previews(&app), [true]);
        assert_eq!(app.live.open_roots().len(), 1, "la rilettura di a.md è rilasciata");
    }

    #[test]
    fn the_preview_is_replaced_in_place_and_pinned_tabs_stay() {
        let (_dir, p) = files(&["a.md", "b.md", "c.md", "d.md"]);
        let mut app = empty_app();
        app.open_paths(vec![p[0].clone()]);
        app.open_preview(p[1].clone());
        app.open_paths(vec![p[2].clone()]);
        assert_eq!(previews(&app), [false, true, false], "Finder e terminale aprono fisse");
        app.open_preview(p[3].clone());
        let names: Vec<String> = app.tabs.iter().map(Tab::title).collect();
        assert_eq!(names, ["a.md", "d.md", "c.md"]);
        assert_eq!((app.active, previews(&app)), (1, vec![false, true, false]));
    }

    #[test]
    fn a_click_on_an_open_file_only_switches_to_its_tab() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_paths(vec![p[0].clone()]);
        app.open_preview(p[1].clone());
        let ids: Vec<u64> = app.tabs.iter().map(|t| t.doc.id).collect();
        assert!(app.open_preview(p[0].clone()));
        assert_eq!(app.active, 0);
        assert_eq!(app.tabs.iter().map(|t| t.doc.id).collect::<Vec<_>>(), ids, "niente riletto né sostituito");
        assert_eq!(previews(&app), [false, true]);
        assert!(!app.open_preview(p[0].clone()), "già davanti");
    }

    #[test]
    fn pinning_keeps_the_tab_when_the_next_preview_opens() {
        let (_dir, p) = files(&["a.md", "b.md", "c.md"]);
        let mut app = empty_app();
        app.open_preview(p[0].clone());
        app.pin_path(&p[0]);
        assert_eq!(previews(&app), [false]);
        app.open_preview(p[1].clone());
        assert_eq!(previews(&app), [false, true]);
        let id = app.tabs[1].doc.id;
        app.pin(id);
        app.open_preview(p[2].clone());
        assert_eq!(previews(&app), [false, false, true]);
    }

    #[test]
    fn opening_from_finder_pins_a_preview_tab() {
        let (_dir, p) = files(&["a.md"]);
        let mut app = empty_app();
        app.open_preview(p[0].clone());
        app.open_paths(vec![p[0].clone()]);
        assert_eq!(previews(&app), [false]);
    }

    #[test]
    fn editing_or_saving_pins_the_preview_tab() {
        let (_dir, p) = files(&["a.md", "b.md"]);
        let mut app = empty_app();
        app.open_preview(p[0].clone());
        let tab = app.tab_mut().unwrap();
        tab.mode = Mode::Source(Box::new(edit::enter(&tab.doc)));
        assert_eq!(previews(&app), [true], "Cmd+E senza modifiche resta anteprima");
        let id = app.tabs[0].doc.id;
        app.source_changed(id);
        assert_eq!(previews(&app), [false], "una modifica la fissa");
        app.open_preview(p[1].clone());
        assert_eq!(previews(&app), [false, true]);
        let tab = app.tab_mut().unwrap();
        tab.mode = Mode::Source(Box::new(edit::enter(&tab.doc)));
        assert!(app.save_source());
        assert_eq!(previews(&app), [false, false], "Cmd+S la fissa");
    }

    #[test]
    fn closing_the_preview_tab_leaves_no_preview() {
        let (_dir, p) = files(&["a.md", "b.md", "c.md"]);
        let mut app = empty_app();
        app.open_paths(vec![p[0].clone()]);
        app.open_preview(p[1].clone());
        app.close_active();
        app.open_preview(p[2].clone());
        assert_eq!(previews(&app), [false, true]);
        assert_eq!(app.tabs.len(), 2);
    }
}
