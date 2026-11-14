//! La sidebar (DESIGN.md, "Layout" e "Riga della sidebar"): campo "Apri file ⌘P", "Recenti" e
//! "Repo del file".
//!
//! Lo stato (`Sidebar`) si ricalcola solo quando cambia l'indice, il file aperto o il branch:
//! la vista legge righe già pronte, senza toccare il disco. L'albero del repo viene dall'indice
//! (solo cartelle con Markdown, spec "Sidebar"), non da una lettura del disco.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use rune::{Anim, Color, Curve, Drag, El, Easing, GrowthSpec, Phase, ScrollSpec, TextStyle, Tx, col, el, icon, row, text};
use rune_kit::{FileNode, FileTree, TreeModel, glyphs};

use super::quick_open::{self, QuickOpen};
use super::relative_time;
use super::theme::{
    CHEVRON, EASE_OUT, FIELD_H, HAIRLINE, HOVER_FADE, ICON, PRESS_SCALE,
    PRESS_TIME, QUALIFIER_GAP, R_ROW, ROW, ROW_GAP, ROW_PAD, SECTION_PAD_BOTTOM, SECTION_PAD_TOP, SIDEBAR_BOTTOM,
    SIDEBAR_PAD, SIDEBAR_TOP, TREE_INDENT, TREE_ROW, TREE_ROW_PAD, Theme, UI_SMALL, UI_SMALL_WEIGHT, UI_STRONG_WEIGHT, UI_TEXT, UI_WEIGHT,
};
use crate::fx::{self, Mark};
use crate::index::Index;
use crate::repo::{self, Repo};

/// Righe di Recenti al massimo (spec, "Sidebar": 20, ipotesi).
pub const RECENT_MAX: usize = 20;
/// Recenti prende al massimo questa parte dell'altezza della sidebar e poi scorre.
pub const RECENT_SHARE: f32 = 0.4;
/// Lo spazio fra le righe di Recenti, con o senza anelli (`recent-gap`).
pub const RECENT_GAP: f32 = 3.0;
/// Sopra e sotto le due righe della riga del repo (nome e branch).
const REPO_PAD_Y: f32 = 4.0;

/// La linea guida del livello 0 sta sotto il centro dell'icona del repo; ogni livello ne aggiunge
/// una `TREE_INDENT` più a destra, sotto la freccia della cartella sopra.
const GUIDE_X: f32 = ROW_PAD + ICON / 2.0;
/// Le righe di livello `d` partono da qui (+ `d * TREE_INDENT`): la freccia della cartella cade
/// sulla linea guida del livello sotto.
const TREE_X: f32 = GUIDE_X + TREE_INDENT - TREE_ROW_PAD - CHEVRON / 2.0;
/// Spazio dentro lo scroll dell'albero per l'anello, che sta fuori dalla riga di `RING_W`.
const RING_ROOM: f32 = fx::ring::RING_W + HAIRLINE;

/// Una riga di Recenti.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentRow {
    pub path: PathBuf,
    pub name: String,
    /// Il repo (o la cartella) del file, solo quando un'altra riga ha lo stesso nome: i file di
    /// `.worktrees/` e `.repos/` restano in Recenti e si distinguono così (decisione Waggle
    /// c_453c875b, raccomandazione applicata).
    pub qualifier: Option<String>,
    pub mtime: SystemTime,
}

/// Il qualificatore di ogni riga con un nome che ritorna: il repo (o worktree) se i doppi stanno
/// in repo diversi e quel repo ne ha uno solo, altrimenti la cartella. `repo_name` dà il nome
/// del repo di un percorso (dalla cache della sidebar: niente disco per un percorso già visto).
fn qualify(rows: &mut [RecentRow], mut repo_name: impl FnMut(&Path) -> String) {
    let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        by_name.entry(r.name.as_str()).or_default().push(i);
    }
    let doubles: Vec<Vec<usize>> = by_name.into_values().filter(|v| v.len() > 1).collect();
    for group in doubles {
        let repos: Vec<String> = group.iter().map(|&i| repo_name(&rows[i].path)).collect();
        let distinct = repos.iter().collect::<HashSet<_>>().len() > 1;
        for (k, &i) in group.iter().enumerate() {
            let alone = repos.iter().filter(|r| **r == repos[k]).count() == 1;
            rows[i].qualifier = Some(if distinct && alone { repos[k].clone() } else { parent_name(&rows[i].path) });
        }
    }
}

fn file_name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn parent_name(path: &Path) -> String {
    path.parent().map(file_name).unwrap_or_default()
}

/// Lo stato della sidebar.
#[derive(Default)]
pub struct Sidebar {
    pub recent: Vec<RecentRow>,
    /// Il tempo relativo di ogni riga di Recenti, calcolato in `tick` (la vista non legge l'orologio).
    labels: Vec<String>,
    /// Il risveglio armato per il prossimo cambio di etichetta: l'istante del cambio e quando
    /// Rune lo fa scattare (orologio di `poll`).
    label_wake: Option<(SystemTime, f64)>,
    /// Le candidate di Recenti dell'ultimo ricalcolo (i `2n` percorsi più recenti, in ordine),
    /// e fra queste quelle che non sono più file: il disco si guarda solo per una candidata nuova.
    candidates: Vec<PathBuf>,
    missing: HashSet<PathBuf>,
    /// Il nome del repo di ogni percorso già visto fra i doppi (`repo::of`, una volta per percorso).
    repo_names: HashMap<PathBuf, String>,
    /// Il file aperto (canonico, come i percorsi dell'indice).
    open: Option<PathBuf>,
    pub repo: Option<Repo>,
    /// La radice ha un `.git`: senza, l'albero mostra solo i file della cartella del file, non
    /// tutto ciò che sta sotto (un `~/Desktop/nota.md` non porta tutto il Desktop).
    in_repo: bool,
    pub tree: TreeModel<FileNode<PathBuf>>,
    /// I file da cui è costruito l'albero, per non ricostruirlo se non cambiano.
    tree_files: Vec<PathBuf>,
    /// Le cartelle aperte, per tenerle aperte quando l'albero si ricostruisce.
    open_dirs: HashSet<String>,
    /// Cresce a ogni cambio delle righe dell'albero: Rune rilegge le chiavi.
    pub tree_version: u64,
    /// La riga premuta adesso (la sua chiave): scala a 0,98 finché il puntatore non si alza.
    pressed: Option<String>,
    /// macOS nega la lettura di una cartella osservata (Desktop): la sidebar lo dice.
    pub no_access: bool,
    /// Cmd+P: il campo e i risultati; le righe dei risultati hanno il qualificatore dei nomi doppi.
    pub quick: QuickOpen,
    pub quick_rows: Vec<RecentRow>,
    /// Cmd+N: il campo "Nuovo file" al posto di quello di Cmd+P.
    pub new_file: super::new_file::NewFile,
    /// La riga dell'albero da portare in vista (il file appena aperto); la prende `poll`.
    reveal: Option<String>,
}

impl Sidebar {
    /// Il file aperto è `path`: repo, branch e albero aperto fino al file. Il disco si legge qui
    /// (`repo::of`), una volta per file aperto.
    pub fn set_open(&mut self, path: &Path, index: &Index) {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_owned());
        let repo = repo::of(&path);
        if self.repo.as_ref().map(|r| &r.root) != Some(&repo.root) {
            self.tree_files.clear();
            self.open_dirs.clear();
            self.in_repo = repo.root.join(".git").exists();
        }
        self.repo = Some(repo);
        self.open = Some(path);
        self.refresh_tree(index);
        if let Some(id) = self.open_id() {
            self.tree.expand_to(&id);
            self.tree.select(&id);
            self.reveal = Some(id.clone());
            let mut dir = Path::new(&id).parent();
            while let Some(d) = dir.filter(|d| !d.as_os_str().is_empty()) {
                self.open_dirs.insert(d.to_string_lossy().into_owned());
                dir = d.parent();
            }
            self.tree_version += 1;
        }
    }

    /// La riga dell'albero del file appena aperto, da portare in vista una volta
    /// (`Tx::scroll_into_view_in("tree", …)`: lo scroll si muove solo se serve).
    pub fn take_reveal(&mut self) -> Option<String> {
        self.reveal.take()
    }

    /// Nessun file aperto (l'ultima scheda chiusa): "Repo del file" sparisce con il suo albero e
    /// la riga scelta, e il branch non si aggiorna più (la cartella di HEAD può non essere più
    /// guardata, `live::Live::unfollow`).
    pub fn clear_open(&mut self) {
        self.open = None;
        self.repo = None;
        self.in_repo = false;
        self.tree = TreeModel::default();
        self.tree_files.clear();
        self.open_dirs.clear();
        self.tree_version += 1;
    }

    /// L'id nell'albero del file aperto (il percorso relativo alla radice del repo).
    fn open_id(&self) -> Option<String> {
        let root = &self.repo.as_ref()?.root;
        let rel = self.open.as_ref()?.strip_prefix(root).ok()?;
        Some(rel.to_string_lossy().into_owned())
    }

    /// L'indice è cambiato. `touched` sono i percorsi (file o cartelle) che il cambio riguarda:
    /// l'albero si ricontrolla solo se uno di essi sta sopra o sotto la radice del repo. Vero se
    /// la vista cambia.
    pub fn refresh(&mut self, index: &Index, touched: &[PathBuf], now: SystemTime) -> bool {
        let mut changed = self.refresh_recent(index);
        changed |= self.tick(now);
        if self.quick.active() {
            // L'indice è cambiato, non la ricerca: la riga scelta resta quella.
            let before = (self.quick.hits.clone(), self.quick.selected);
            self.rank_quick(index, true);
            changed |= before != (self.quick.hits.clone(), self.quick.selected);
        }
        let near = |root: &Path| touched.iter().any(|p| p.starts_with(root) || root.starts_with(p));
        if self.repo.as_ref().is_some_and(|r| near(&r.root)) {
            changed |= self.refresh_tree(index);
        }
        changed
    }

    /// La ricerca di Cmd+P è cambiata, o l'indice: ricalcola i risultati. I nomi doppi prendono il
    /// qualificatore di Recenti (`qualify`).
    pub fn requery(&mut self, index: &Index) {
        self.rank_quick(index, false);
    }

    /// `keep`: la ricerca è la stessa (l'indice è cambiato), la riga scelta resta per percorso.
    fn rank_quick(&mut self, index: &Index, keep: bool) {
        if keep {
            self.quick.refresh(index.names());
        } else {
            self.quick.requery(index.names());
        }
        let mut rows: Vec<RecentRow> = self.quick.hits.iter().map(|p| RecentRow { name: file_name(p), path: p.clone(), qualifier: None, mtime: SystemTime::UNIX_EPOCH }).collect();
        let names = &mut self.repo_names;
        qualify(&mut rows, |p| names.entry(p.to_owned()).or_insert_with(|| repo::of(p).name).clone());
        self.quick_rows = rows;
    }

    /// Il campo ha perso il fuoco (un clic fuori): lista e ricerca si chiudono, così Esc e frecce
    /// non restano morti su una lista vecchia. Non durante un clic su una riga: il fuoco lascia il
    /// campo alla pressione, e il clic parte al rilascio. Vero se la lista si è chiusa.
    pub fn blur_close(&mut self) -> bool {
        if self.new_file.blur_close() {
            return true;
        }
        if self.quick.active() && !self.quick.edit.focused && self.pressed.is_none() {
            self.quick.close();
            self.quick_rows.clear();
            return true;
        }
        false
    }

    /// Ricalcola Recenti: le `RECENT_MAX` voci più recenti che sono ancora file. Il disco si
    /// guarda solo quando entra una candidata nuova (un `is_file`), il repo dei nomi doppi una
    /// volta per percorso; un cambio di solo mtime non tocca il disco. Vero se le righe cambiano.
    pub fn refresh_recent(&mut self, index: &Index) -> bool {
        let top = index.recent(RECENT_MAX * 2);
        if !top.iter().map(|e| &e.path).eq(self.candidates.iter()) {
            let old: HashSet<&PathBuf> = self.candidates.iter().collect();
            let missing: HashSet<PathBuf> = top
                .iter()
                .filter(|e| if old.contains(&e.path) { self.missing.contains(&e.path) } else { !e.path.is_file() })
                .map(|e| e.path.clone())
                .collect();
            self.missing = missing;
            self.candidates = top.iter().map(|e| e.path.clone()).collect();
        }
        let mut rows: Vec<RecentRow> = top
            .iter()
            .filter(|e| !self.missing.contains(&e.path))
            .take(RECENT_MAX)
            .map(|e| RecentRow { name: file_name(&e.path), path: e.path.clone(), qualifier: None, mtime: e.mtime })
            .collect();
        let names = &mut self.repo_names;
        qualify(&mut rows, |p| names.entry(p.to_owned()).or_insert_with(|| repo::of(p).name).clone());
        let changed = rows != self.recent;
        self.recent = rows;
        changed
    }

    /// Ricostruisce l'albero se i file sotto la radice del repo sono cambiati (non per un mtime).
    /// Il confronto scorre la parte dell'indice sotto la radice senza copiarla; si copia solo
    /// quando l'albero va ricostruito.
    fn refresh_tree(&mut self, index: &Index) -> bool {
        let Some(root) = self.repo.as_ref().map(|r| r.root.clone()) else { return false };
        let under = index.under(&root);
        let in_repo = self.in_repo;
        let shown = move |p: &&Path| in_repo || p.parent() == Some(root.as_path());
        // Il file aperto c'è sempre, anche fuori dalle cartelle osservate o prima della scansione.
        let extra = self.open.as_deref().filter(|o| under.binary_search_by(|e| e.path.as_path().cmp(o)).is_err());
        let files = || under.iter().map(|e| e.path.as_path()).filter(shown.clone()).chain(extra);
        if !self.tree.is_empty() && files().eq(self.tree_files.iter().map(PathBuf::as_path)) {
            return false;
        }
        let files: Vec<PathBuf> = files().map(Path::to_path_buf).collect();
        let root = &self.repo.as_ref().expect("repo").root;
        let rel = |p: &PathBuf| p.strip_prefix(root).ok().map(|r| (r.to_string_lossy().into_owned(), p.clone()));
        let mut tree = FileTree::new().compact_folders(false).key("repo-tree").build(files.iter().filter_map(rel));
        for id in &self.open_dirs {
            tree.set_expanded(id, true);
        }
        if let Some(id) = self.open_id() {
            tree.select(&id);
        }
        self.tree = tree;
        self.tree_files = files;
        self.tree_version += 1;
        true
    }

    /// Un gesto sulla riga `key`: giù la preme, su la rilascia (anche annullato o fuori).
    pub fn press(&mut self, key: &str, phase: Phase) {
        match phase {
            Phase::Down => self.pressed = Some(key.to_owned()),
            Phase::Up if self.pressed.as_deref() == Some(key) => self.pressed = None,
            _ => {}
        }
    }

    fn is_pressed(&self, key: &str) -> bool {
        self.pressed.as_deref() == Some(key)
    }

    /// `.git/HEAD` di `root` è cambiato (checkout, anche in un worktree: `Live` guarda la
    /// cartella di `repo::head_path`): si rilegge il branch. Vero se è cambiato.
    pub fn git_head(&mut self, root: &Path) -> bool {
        let Some(repo) = self.repo.as_mut().filter(|r| r.root == root) else { return false };
        let branch = repo::branch(root);
        std::mem::replace(&mut repo.branch, branch.clone()) != branch
    }

    /// Apre o chiude la cartella `id` dell'albero.
    pub fn toggle(&mut self, id: &str) {
        self.tree.toggle(id);
        if self.tree.is_expanded(id) {
            self.open_dirs.insert(id.to_owned());
        } else {
            self.open_dirs.remove(id);
        }
        self.tree_version += 1;
    }

    /// Ricalcola i tempi relativi a `now`. Vero se un'etichetta è cambiata.
    pub fn tick(&mut self, now: SystemTime) -> bool {
        let labels: Vec<String> = self.recent.iter().map(|r| relative_time::format(now.duration_since(r.mtime).unwrap_or_default())).collect();
        let changed = labels != self.labels;
        self.labels = labels;
        changed
    }

    /// Il risveglio da chiedere per il prossimo cambio di etichetta, come `rearm` in `main.rs`:
    /// uno per cambio, e un altro se quello armato è scattato prima del cambio sul muro (Rune
    /// misura da `clock`, l'inizio del frame). Un poll qualsiasi in mezzo non arma niente.
    pub fn label_wake(&mut self, now: SystemTime, clock: f64) -> Option<Duration> {
        let next = self.recent.iter().map(|r| relative_time::next_change(r.mtime, now)).min();
        let due = self.label_wake.is_some_and(|(_, at)| clock >= at);
        let Some(next) = next else {
            self.label_wake = None;
            return None;
        };
        if !due && self.label_wake.map(|(end, _)| end) == Some(next) {
            return None;
        }
        let left = next.duration_since(now).unwrap_or_default() + Duration::from_millis(1);
        self.label_wake = Some((next, clock + left.as_secs_f64()));
        Some(left)
    }
}

/// Come la vista raggiunge lo stato dell'app e cosa fa un clic.
pub struct Hooks<S> {
    pub get: fn(&S) -> &Sidebar,
    pub get_mut: fn(&mut S) -> &mut Sidebar,
    /// Apre il file nel lettore.
    pub open: fn(&mut S, PathBuf, &mut Tx),
    /// L'orologio di Rune a cui si è acceso l'anello "sta scrivendo" del file; `None` se spento.
    pub ring: fn(&S, &Path) -> Option<f64>,
    /// La ricerca di Cmd+P è cambiata: ricalcola i risultati dall'indice.
    pub query: fn(&mut S),
    /// Cmd+N, Invio: crea il file e lo apre nel sorgente. Vero se il campo si chiude.
    pub create: fn(&mut S, String, &mut Tx) -> bool,
}

impl<S> Clone for Hooks<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S> Copy for Hooks<S> {}

/// Testo UI (13/400) e UI piccola (12/500, cifre tabellari) di DESIGN.md.
pub(super) fn ui_font() -> TextStyle {
    TextStyle::new(UI_TEXT, UI_WEIGHT)
}

pub(super) fn small_font() -> TextStyle {
    TextStyle::new(UI_SMALL, UI_SMALL_WEIGHT).features("'tnum' 1")
}

/// La sidebar. `hover`: il puntatore è fine e può passare sopra (hover `sel` solo allora).
pub fn view<S: 'static>(s: &S, hooks: Hooks<S>, width: f32, theme: &Theme, reduced: bool, hover: bool) -> El {
    let sb = (hooks.get)(s);
    let motion = Motion { reduced, hover };
    let mut side = col().key("sidebar").w(width).h_full().pt(SIDEBAR_TOP).px(SIDEBAR_PAD).pb(SIDEBAR_BOTTOM).child(if sb.new_file.open { super::new_file::field(sb, hooks, theme) } else { quick_open::field(sb, hooks, theme) });
    if sb.no_access {
        side = side.child(no_access(sb, hooks, theme, motion));
    }
    // La ricerca di Cmd+P prende il posto di Recenti e dell'albero.
    if sb.quick.active() {
        return side.child(quick_open::view(sb, hooks, theme, motion));
    }
    if !sb.recent.is_empty() {
        // Al massimo `RECENT_SHARE` dell'altezza della sidebar, poi scorre (con la barra del
        // sistema, a scomparsa): "Repo del file" resta in vista anche con molti Recenti.
        // Lo scroll taglia ciò che esce dai suoi bordi: dentro c'è `RING_ROOM` per gli anelli, e
        // il margine negativo tiene le righe dove erano.
        let cap = rune::memo::layout_with("sidebar", |r| (r[3] * RECENT_SHARE).floor());
        let rows = col().w_full().gap(RECENT_GAP).padding(RING_ROOM).children(sb.recent.iter().enumerate().map(|(i, r)| {
            let label = sb.labels.get(i).cloned().unwrap_or_default();
            recent_row(s, hooks, i, r, label, theme, motion)
        }));
        let mut list = col().key("recent").mx(-RING_ROOM).my(-RING_ROOM).shrink().min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(rows);
        if let Some(cap) = cap {
            list = list.max_h(cap);
        }
        side = side.child(section("Recenti", theme)).child(list);
    }
    if let Some(repo) = &sb.repo {
        side = side.child(section("Repo del file", theme)).child(repo_row(repo, theme)).child(tree(sb, hooks, theme, motion));
    }
    side
}

/// Dove macOS tiene il permesso "File e cartelle".
const PRIVACY_URL: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_FilesAndFolders";

/// L'avviso sotto il campo: il testo in `fg`, e sotto una riga premibile in [`row_meta`] che apre le
/// Impostazioni di Sistema. Solo token di DESIGN.md: niente accento, niente pillola.
fn no_access<S: 'static>(sb: &Sidebar, hooks: Hooks<S>, theme: &Theme, motion: Motion) -> El {
    let link = row()
        .w_full()
        .h(ROW)
        .px(ROW_PAD)
        .center_y()
        .child(text("Apri Impostazioni di Sistema").font(small_font()).color(row_meta(theme)).truncate().shrink().min_w(0.0))
        .on_press_tx(|_: &mut S, _: &mut Tx| {
            if let Err(e) = std::process::Command::new("open").arg(PRIVACY_URL).spawn() {
                eprintln!("runemd: Impostazioni di Sistema non aperte: {e}");
            }
        });
    let link = pressable(link, "no-access-link".into(), sb, hooks, theme, motion, true).key("no-access-link");
    col()
        .key("no-access")
        .w_full()
        .child(text("Nessun accesso a Desktop").font(ui_font()).color(theme.fg).px(ROW_PAD).pt(SECTION_PAD_BOTTOM).pb(SECTION_PAD_BOTTOM))
        .child(link)
}

/// Un'etichetta di sezione: 12/500 in `fg3` (`section-pad`).
fn section(label: &'static str, theme: &Theme) -> El {
    text(label).key(label).font(small_font()).color(theme.fg3).pt(SECTION_PAD_TOP).px(ROW_PAD).pb(SECTION_PAD_BOTTOM)
}

/// Come si muove una riga (DESIGN.md, `motion`); con "riduci movimento" tutto è immediato.
#[derive(Clone, Copy)]
pub(super) struct Motion {
    pub(super) reduced: bool,
    /// Il puntatore è fine e può passare sopra: hover `sel` solo allora.
    pub(super) hover: bool,
}

impl Motion {
    /// L'hover `sel` entra ed esce in 120 ms (`ease`).
    fn hover_fade(self) -> Anim {
        if self.reduced { Anim::INSTANT } else { Curve::tween(HOVER_FADE, Easing::EASE).into() }
    }

    /// La scala della riga premuta va e torna in 160 ms ease-out.
    fn press(self) -> Anim {
        if self.reduced { Anim::INSTANT } else { Curve::tween(PRESS_TIME, EASE_OUT).into() }
    }
}

/// Fa di `e` (la riga con chiave `key`) una riga premibile: hover `sel` con dissolvenza, solo con
/// puntatore fine (`hover_ok`), e scala 0,98 in 160 ms finché è premuta. Rune non ha una curva per
/// `press_scale` (scende subito e torna a molla): la pressione si segue con `on_drag` (giù/su) e
/// la scala con `scale` + `scale_motion`. Il clic resta di `on_press` (rilascio dentro la riga).
pub(super) fn pressable<S: 'static>(e: El, key: String, sb: &Sidebar, hooks: Hooks<S>, theme: &Theme, motion: Motion, hover_ok: bool) -> El {
    let down = sb.is_pressed(&key);
    let mut e = e
        .radius(R_ROW)
        .scale(if down { PRESS_SCALE } else { 1.0 })
        .scale_motion(motion.press())
        .on_drag(move |s: &mut S, d: &mut Drag, _: &mut Tx| (hooks.get_mut)(s).press(&key, d.phase));
    if motion.hover && hover_ok {
        e = e.hover(theme.sel).hover_motion(motion.hover_fade(), motion.hover_fade());
    }
    e
}

/// L'ospite dell'anello "sta scrivendo" attorno a una riga: sempre presente, così la struttura
/// non cambia quando si accende.
fn ring_host(row: El, mark: Mark, since: Option<f64>, reduced: bool, dark: bool) -> El {
    let host = el().w_full().radius(R_ROW).child(row);
    match since {
        Some(t) => fx::ring::slot(host, mark, t, reduced, dark),
        None => host,
    }
}

/// Il colore del testo piccolo dentro una riga premibile (tempo di Recenti, nome doppio, link):
/// l'hover `sel` sul vetro porterebbe `fg3` sotto 4,5:1 (DESIGN.md, "Contrasto").
pub fn row_meta(theme: &Theme) -> Color {
    theme.fg2
}

/// Il nome del file e, se un'altra riga ha lo stesso nome, il repo o la cartella in
/// [`row_meta`]: la riga prende `sel` in hover o scelta.
pub(super) fn name_cell(name: &str, qualifier: Option<&str>, theme: &Theme) -> El {
    let mut cell = row().gap(QUALIFIER_GAP).center_y().grow(1.0).shrink().min_w(0.0).child(text(name.to_owned()).font(ui_font()).color(theme.fg).truncate_middle().shrink().min_w(0.0));
    if let Some(q) = qualifier {
        cell = cell.child(text(q.to_owned()).font(small_font()).color(row_meta(theme)).truncate().shrink().min_w(0.0));
    }
    cell
}

#[allow(clippy::too_many_arguments)]
fn recent_row<S: 'static>(s: &S, hooks: Hooks<S>, i: usize, r: &RecentRow, label: String, theme: &Theme, motion: Motion) -> El {
    let name = name_cell(&r.name, r.qualifier.as_deref(), theme);
    let path = r.path.clone();
    let e = row()
        .w_full()
        .h(ROW)
        .px(ROW_PAD)
        .gap(ROW_GAP)
        .center_y()
        .child(name)
        .child(text(label).font(small_font()).color(row_meta(theme)))
        .on_press_tx(move |s: &mut S, tx: &mut Tx| (hooks.open)(s, path.clone(), tx));
    let since = (hooks.ring)(s, &r.path);
    let key = format!("recent-{}", r.path.display());
    let e = pressable(e, key.clone(), (hooks.get)(s), hooks, theme, motion, true);
    ring_host(e, Mark::RecentRing(i as u32), since, motion.reduced, theme.dark).key(key)
}

/// La riga del repo: icona cartella e nome 600; sotto il nome il branch, più piccolo (12) e a
/// peso normale, in `fg3` (la riga non ha hover, e `fg3` sul vetro sta sopra 4,5:1). Alta
/// almeno `FIELD_H`. Non sta al livello dei suoi file.
fn repo_row(repo: &Repo, theme: &Theme) -> El {
    let name = row()
        .gap(ROW_GAP)
        .center_y()
        .child(icon(&glyphs::FOLDER, ICON).color(theme.fg3))
        .child(text(repo.name.clone()).key("repo-name").font(TextStyle::new(UI_TEXT, UI_STRONG_WEIGHT)).color(theme.fg).truncate().shrink().min_w(0.0));
    let mut e = col().key("repo").w_full().min_h(FIELD_H).px(ROW_PAD).py(REPO_PAD_Y).justify_center().child(name);
    if let Some(b) = &repo.branch {
        e = e.child(text(b.clone()).key("repo-branch").font(TextStyle::new(UI_SMALL, UI_WEIGHT).features("'tnum' 1")).color(theme.fg3).truncate_middle().shrink().min_w(0.0).ml(ICON + ROW_GAP));
    }
    e
}

/// L'albero del repo: righe da 26 pt, rientro 16, linee guida, cartelle aperte fino al file.
fn tree<S: 'static>(sb: &Sidebar, hooks: Hooks<S>, theme: &Theme, motion: Motion) -> El {
    let theme = *theme;
    let rows = col()
        .key("tree-rows")
        .w_full()
        .growth(GrowthSpec::default())
        .virtual_rows(
            sb.tree.visible_len(),
            TREE_ROW,
            move |s: &S, i| Cow::Owned((hooks.get)(s).tree.visible_id(i).unwrap_or_default().to_owned()),
            move |s: &S, i| tree_row(s, hooks, i, &theme, motion),
        )
        .virtual_rows_version(sb.tree_version);
    col().key("tree").pt(RING_ROOM).w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(rows)
}

fn tree_row<S: 'static>(s: &S, hooks: Hooks<S>, i: usize, theme: &Theme, motion: Motion) -> El {
    let sb = (hooks.get)(s);
    let Some((node, r)) = sb.tree.row(i) else { return el() };
    let depth = r.depth as f32;
    // A destra lo spazio per l'anello: lo scroll dell'albero taglia ciò che esce dai suoi bordi.
    let mut outer = row().w_full().h(TREE_ROW).pr(RING_ROOM);
    // Una linea per livello, dal repo fino a quello della riga: insieme fanno linee continue.
    for level in 0..=r.depth {
        let x = GUIDE_X + level as f32 * TREE_INDENT;
        outer = outer.child(el().key(format!("guide{level}")).pin(Some(x), Some(0.0), None, Some(0.0)).w(HAIRLINE).fill(theme.line));
    }
    let chevron = match r.expanded {
        Some(open) => icon(&glyphs::CHEVRON_DOWN, CHEVRON).color(theme.fg3).icon_rotation(if open { 0.0 } else { -std::f32::consts::FRAC_PI_2 }),
        None => el(),
    };
    let color = if r.selected || !node.dir { theme.fg } else { theme.fg2 };
    let mut e = row()
        .w_full()
        .h(TREE_ROW)
        .px(TREE_ROW_PAD)
        .gap(ROW_GAP)
        .center_y()
        .child(el().size(CHEVRON, CHEVRON).center().child(chevron))
        .child(text(node.name.clone()).font(ui_font()).color(color).truncate_middle().shrink().min_w(0.0));
    if r.selected {
        e = e.fill(theme.sel);
    }
    let id = r.id.to_owned();
    let e = match node.data.clone() {
        Some(path) => e.on_press_tx(move |s: &mut S, tx: &mut Tx| (hooks.open)(s, path.clone(), tx)),
        None => e.on_press(move |s: &mut S| (hooks.get_mut)(s).toggle(&id)),
    };
    // La riga selezionata tiene il suo `sel`: niente hover sopra.
    let e = pressable(e, format!("tree-{}", r.id), sb, hooks, theme, motion, !r.selected);
    let since = node.data.as_deref().and_then(|p| (hooks.ring)(s, p));
    // L'ospite è largo quanto la riga: col rientro deve stringersi, o esce dall'albero (nome
    // tagliato senza "…", riempimento e anello senza il lato destro).
    outer.child(ring_host(e, Mark::TreeRing(i as u32), since, motion.reduced, theme.dark).grow(1.0).shrink().min_w(0.0).ml(TREE_X + depth * TREE_INDENT))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Entry;
    use std::fs;

    /// Recenti di un indice, da una sidebar nuova.
    fn recent_rows(index: &Index) -> Vec<RecentRow> {
        let mut sb = Sidebar::default();
        sb.refresh_recent(index);
        sb.recent
    }

    fn at(secs: u64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 + secs)
    }

    #[test]
    fn missing_paths_are_dropped() {
        let d = tempfile::tempdir().unwrap();
        let here = d.path().join("here.md");
        fs::write(&here, "x").unwrap();
        let gone = d.path().join("gone.md");
        let index = Index::from_entries(vec![Entry { path: here.clone(), mtime: at(1) }, Entry { path: gone, mtime: at(2) }]);
        let rows = recent_rows(&index);
        assert_eq!(rows.iter().map(|r| r.path.clone()).collect::<Vec<_>>(), vec![here]);
    }

    fn entry(path: &Path, secs: u64) -> Entry {
        Entry { path: path.to_owned(), mtime: at(secs) }
    }

    /// Un repo con `.git/HEAD` su `main` e i file `files` (relativi), tutti scritti.
    fn repo_with(files: &[&str]) -> (tempfile::TempDir, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap().join("proj");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::write(root.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        for f in files {
            let p = root.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "x").unwrap();
        }
        (d, root)
    }

    /// Un file lungo tre cartelle sotto il repo, nome lungo: ogni riga dell'albero (riempimento,
    /// raggio e anello) finisce dentro l'albero, il rientro non la spinge oltre il bordo destro.
    #[test]
    fn deep_tree_rows_stay_inside_the_tree() {
        let deep = "docs/architettura/sincronizzazione/2026-11-14-architettura-del-sistema-di-sincronizzazione-offline-v2.md";
        let (_d, root) = repo_with(&[deep, "README.md"]);
        let mut app = crate::App::bare(crate::index::Library::load(vec![root.clone()], None));
        app.open_paths(vec![root.join(deep)]);
        let mut rt = rune::Runtime::new(app, crate::view, (1280.0, 900.0), rune::Env::new(true, 2.0));
        rt.frame(0.0);
        for _ in 0..400 {
            if !rt.frame(1.0 / 120.0) { break; }
        }
        let els = rt.elements();
        let tree = els.iter().find(|e| e.key.as_deref() == Some("tree")).expect("albero").rect;
        let right = tree[0] + tree[2];
        let rows: Vec<_> = els.iter().filter(|e| !e.exiting && e.path.contains("tree-rows/") && e.rect[2] > 0.0).collect();
        assert!(rows.len() >= 5, "righe dell'albero: {}", rows.len());
        for e in rows {
            assert!(e.rect[0] + e.rect[2] <= right + 0.5, "{} esce dall'albero: {:?}, bordo destro {right}", e.path, e.rect);
        }
    }

    /// D1, D2, D3 e D5 (`docs/decisions/2026-11-14-sidebar-schede-nomi-lunghi.md`), con 60 file
    /// in una cartella e nomi lunghi: Recenti ≤ 40 % dell'altezza della sidebar e 3 pt fra le
    /// righe; l'albero scorre fino al file aperto; il branch sotto il nome del repo; la scheda
    /// larga al massimo 240.
    #[test]
    fn long_lists_and_names_keep_their_bounds() {
        let long = "2026-11-14-architettura-del-sistema-di-sincronizzazione-offline-v2-definitiva";
        let files: Vec<String> = (1..=60).map(|n| format!("docs/molti/{long}-{n:02}.md")).collect();
        let refs: Vec<&str> = files.iter().map(String::as_str).collect();
        let (_d, root) = repo_with(&refs);
        let open = root.join(&files[59]);
        let mut app = crate::App::bare(crate::index::Library::load(vec![root.clone()], None));
        app.library.index = Index::from_entries(files.iter().enumerate().map(|(i, f)| entry(&root.join(f), i as u64)).collect());
        app.open_paths(vec![open.clone()]);
        app.sidebar.refresh(&app.library.index, &[], SystemTime::now());
        let mut rt = rune::Runtime::new(app, crate::view, (1280.0, 800.0), rune::Env::new(true, 2.0));
        rt.poll = Some(crate::poll);
        rt.frame(0.0);
        for _ in 0..400 {
            if !rt.frame(1.0 / 120.0) {
                break;
            }
        }
        let els = rt.elements();
        let rect = |key: &str| els.iter().find(|e| e.key.as_deref() == Some(key) && !e.exiting).unwrap_or_else(|| panic!("{key} non c'è")).rect;
        let side = rect("sidebar");
        let recent = rect("recent");
        assert!(recent[3] <= side[3] * RECENT_SHARE + 0.5, "Recenti alto {} su {}", recent[3], side[3]);
        let mut rows: Vec<[f32; 4]> = els.iter().filter(|e| !e.exiting && e.key.as_deref().is_some_and(|k| k.starts_with("recent-"))).map(|e| e.rect).collect();
        rows.sort_by(|a, b| a[1].total_cmp(&b[1]));
        assert!(rows.len() >= 3);
        for w in rows.windows(2) {
            assert!((w[1][1] - (w[0][1] + w[0][3]) - RECENT_GAP).abs() < 0.5, "spazio fra le righe: {w:?}");
        }
        let (name, branch) = (rect("repo-name"), rect("repo-branch"));
        assert!(branch[1] >= name[1] + name[3] - 0.5, "branch sotto il nome: {name:?} {branch:?}");
        let tree = rect("tree");
        let id = files[59].clone();
        let row = rect(&id);
        assert!(row[1] >= tree[1] - 0.5 && row[1] + row[3] <= tree[1] + tree[3] + 0.5, "riga del file aperto fuori vista: {row:?} in {tree:?}");
        let tab = rect(&format!("t-{:x}", rt.state.tabs[0].doc.id));
        assert!(tab[2] <= crate::ui::theme::TAB_MAX_W + 0.5, "scheda larga {}", tab[2]);
    }

    /// Due righe con lo stesso nome: accanto al nome il repo (worktree), o la cartella se il
    /// repo è lo stesso. Un nome unico non ha niente accanto.
    #[test]
    fn doubles_show_their_repo_or_folder() {
        let (_d, root) = repo_with(&["PLAN.md", "docs/PLAN.md", "README.md", ".worktrees/feat/README.md"]);
        fs::write(root.join(".worktrees/feat/.git"), "gitdir: x\n").unwrap();
        let index = Index::from_entries(["PLAN.md", "docs/PLAN.md", "README.md", ".worktrees/feat/README.md"].iter().enumerate().map(|(i, f)| entry(&root.join(f), i as u64)).collect());
        let rows = recent_rows(&index);
        let q = |rel: &str| rows.iter().find(|r| r.path == root.join(rel)).unwrap().qualifier.clone();
        assert_eq!(q("README.md").as_deref(), Some("proj"));
        assert_eq!(q(".worktrees/feat/README.md").as_deref(), Some("feat"));
        assert_eq!(q("PLAN.md").as_deref(), Some("proj"), "stesso repo: la cartella");
        assert_eq!(q("docs/PLAN.md").as_deref(), Some("docs"));
        let index = Index::from_entries(vec![entry(&root.join("PLAN.md"), 1)]);
        assert_eq!(recent_rows(&index)[0].qualifier, None);
    }

    #[test]
    fn at_most_twenty_newest_first() {
        let d = tempfile::tempdir().unwrap();
        let entries: Vec<Entry> = (0..30).map(|i| {
            let p = d.path().join(format!("f{i}.md"));
            fs::write(&p, "x").unwrap();
            entry(&p, i)
        }).collect();
        let rows = recent_rows(&Index::from_entries(entries));
        assert_eq!(rows.len(), RECENT_MAX);
        assert_eq!(rows[0].name, "f29.md");
    }

    /// L'albero viene dall'indice, aperto fino al file aperto e con il file selezionato; un
    /// cambio di solo mtime non lo ricostruisce, un file nuovo sì, e le cartelle aperte restano.
    #[test]
    fn tree_opens_to_the_file_and_rebuilds_only_for_new_files() {
        let (_d, root) = repo_with(&["CLAUDE.md", "docs/spike/a.md", "docs/spike/b.md", "design/x.md"]);
        let files = ["CLAUDE.md", "docs/spike/a.md", "docs/spike/b.md", "design/x.md"];
        let index = Index::from_entries(files.iter().map(|f| entry(&root.join(f), 1)).collect());
        let mut sb = Sidebar::default();
        sb.set_open(&root.join("docs/spike/a.md"), &index);
        let repo = sb.repo.clone().unwrap();
        assert_eq!((repo.name.as_str(), repo.branch.as_deref()), ("proj", Some("main")));
        let shown: Vec<String> = (0..sb.tree.visible_len()).map(|i| sb.tree.visible_id(i).unwrap().to_owned()).collect();
        assert_eq!(shown, ["design", "docs", "docs/spike", "docs/spike/a.md", "docs/spike/b.md", "CLAUDE.md"]);
        assert!(sb.tree.is_selected("docs/spike/a.md"));
        sb.toggle("design");
        let v = sb.tree_version;
        let index = Index::from_entries(files.iter().map(|f| entry(&root.join(f), 2)).collect());
        assert!(!sb.refresh_tree(&index), "solo mtime");
        assert_eq!(sb.tree_version, v);
        let mut more: Vec<Entry> = files.iter().map(|f| entry(&root.join(f), 2)).collect();
        more.push(entry(&root.join("docs/new.md"), 3));
        assert!(sb.refresh_tree(&Index::from_entries(more)));
        assert!(sb.tree.is_expanded("design") && sb.tree.is_expanded("docs/spike"), "cartelle aperte tenute");
        assert!(sb.tree.contains("docs/new.md") && sb.tree.is_selected("docs/spike/a.md"));
    }

    /// Chiusa l'ultima scheda non c'è un file aperto: niente repo, niente albero, niente riga
    /// scelta, e un cambio di HEAD non tocca più niente.
    #[test]
    fn clear_open_drops_the_repo_and_the_selection() {
        let (_d, root) = repo_with(&["a.md"]);
        let mut sb = Sidebar::default();
        sb.set_open(&root.join("a.md"), &Index::default());
        assert!(sb.repo.is_some());
        sb.clear_open();
        assert!(sb.repo.is_none() && sb.open.is_none());
        assert!(sb.tree.is_empty());
        assert!(!sb.git_head(&root));
    }

    /// Un checkout nel repo del file aperto cambia il branch; in un altro repo no.
    #[test]
    fn git_head_rereads_the_branch() {
        let (_d, root) = repo_with(&["a.md"]);
        let mut sb = Sidebar::default();
        sb.set_open(&root.join("a.md"), &Index::default());
        fs::write(root.join(".git/HEAD"), "ref: refs/heads/feat/x\n").unwrap();
        assert!(!sb.git_head(Path::new("/altro")));
        assert!(sb.git_head(&root));
        assert_eq!(sb.repo.as_ref().unwrap().branch.as_deref(), Some("feat/x"));
        assert!(!sb.git_head(&root), "invariato");
    }

    /// Una cartella che diventa illeggibile a metà sessione: niente panic, le sue righe spariscono
    /// da Recenti, l'albero resta quello dell'indice.
    #[test]
    fn an_unreadable_folder_does_not_panic() {
        use std::os::unix::fs::PermissionsExt;
        /// Rimette i permessi anche se un'asserzione fallisce, così la tempdir si cancella.
        struct Restore(PathBuf);
        impl Drop for Restore {
            fn drop(&mut self) {
                let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
            }
        }
        let (_d, root) = repo_with(&["a.md", "secret/b.md"]);
        let index = Index::from_entries(vec![entry(&root.join("a.md"), 1), entry(&root.join("secret/b.md"), 2)]);
        let mut sb = Sidebar::default();
        sb.set_open(&root.join("secret/b.md"), &index);
        fs::set_permissions(root.join("secret"), fs::Permissions::from_mode(0o000)).unwrap();
        let restore = Restore(root.join("secret"));
        sb.refresh(&index, std::slice::from_ref(&root), at(10));
        sb.set_open(&root.join("secret/b.md"), &index);
        sb.git_head(&root);
        drop(restore);
        assert_eq!(sb.recent.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["a.md"]);
        assert!(sb.tree.contains("secret/b.md"));
    }

    /// Un risveglio per cambio di etichetta, non uno per poll; se scatta prima del cambio sul
    /// muro se ne arma un altro.
    #[test]
    fn one_label_wake_per_change() {
        let mut sb = Sidebar { recent: vec![RecentRow { path: PathBuf::from("/a.md"), name: "a.md".into(), qualifier: None, mtime: at(0) }], ..Sidebar::default() };
        assert!(sb.tick(at(30)));
        assert_eq!(sb.labels, ["ora"]);
        let wake = sb.label_wake(at(30), 100.0).expect("al minuto");
        assert_eq!(wake, Duration::from_secs(30) + Duration::from_millis(1));
        assert_eq!(sb.label_wake(at(31), 101.0), None, "un poll in mezzo");
        // Scatta in anticipo: il muro dice ancora 59 s.
        let early = 100.0 + wake.as_secs_f64();
        assert!(!sb.tick(at(59)));
        assert!(sb.label_wake(at(59), early).is_some(), "ne arma un altro");
        assert!(sb.tick(at(60)));
        assert_eq!(sb.labels, ["1 min"]);
    }

    /// Tre righe con lo stesso nome, due nello stesso repo: il repo non basta a distinguerle,
    /// quelle due prendono la cartella.
    #[test]
    fn doubles_in_the_same_repo_fall_back_to_the_folder() {
        let (_d, root) = repo_with(&["a/PLAN.md", "b/PLAN.md", ".worktrees/feat/PLAN.md"]);
        fs::write(root.join(".worktrees/feat/.git"), "gitdir: x\n").unwrap();
        let index = Index::from_entries(["a/PLAN.md", "b/PLAN.md", ".worktrees/feat/PLAN.md"].iter().map(|f| entry(&root.join(f), 1)).collect());
        let rows = recent_rows(&index);
        let q = |rel: &str| rows.iter().find(|r| r.path == root.join(rel)).unwrap().qualifier.clone().unwrap();
        assert_eq!((q("a/PLAN.md").as_str(), q("b/PLAN.md").as_str(), q(".worktrees/feat/PLAN.md").as_str()), ("a", "b", "feat"));
    }

    /// Un cambio di solo mtime (un agente che riscrive un file già in Recenti) non guarda il
    /// disco: né `is_file` né il repo dei doppi. Un file nuovo fra le candidate sì.
    #[test]
    fn an_mtime_only_change_does_not_touch_the_disk() {
        let (_d, root) = repo_with(&["README.md", "x/README.md", "b.md"]);
        let entries = |m: u64| vec![entry(&root.join("README.md"), m), entry(&root.join("x/README.md"), 2), entry(&root.join("b.md"), 3)];
        let mut sb = Sidebar::default();
        sb.refresh_recent(&Index::from_entries(entries(1)));
        // Da qui il disco dice altro: se si rileggesse, le righe cambierebbero.
        fs::remove_file(root.join("b.md")).unwrap();
        fs::remove_dir_all(root.join(".git")).unwrap();
        assert!(sb.refresh_recent(&Index::from_entries(entries(9))), "l'ordine cambia");
        assert_eq!(sb.recent.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["README.md", "b.md", "README.md"]);
        assert_eq!(sb.recent[0].qualifier.as_deref(), Some("proj"), "cartella dalla cache: niente rilettura");
        // Una candidata nuova si guarda sul disco.
        let mut more = entries(9);
        more.push(entry(&root.join("gone.md"), 10));
        sb.refresh_recent(&Index::from_entries(more));
        assert!(sb.recent.iter().all(|r| r.name != "gone.md"));
    }

    /// Un cambio fuori dalla radice del repo non ricontrolla l'albero; uno dentro sì.
    #[test]
    fn changes_outside_the_repo_skip_the_tree() {
        let (_d, root) = repo_with(&["a.md"]);
        let mut sb = Sidebar::default();
        sb.set_open(&root.join("a.md"), &Index::default());
        let v = sb.tree_version;
        let index = Index::from_entries(vec![entry(&root.join("a.md"), 1), entry(&root.join("new.md"), 2)]);
        sb.refresh(&index, &[PathBuf::from("/altrove/x.md")], at(5));
        assert_eq!(sb.tree_version, v);
        assert!(!sb.tree.contains("new.md"));
        sb.refresh(&index, &[root.join("new.md")], at(5));
        assert!(sb.tree.contains("new.md"));
        // Una scansione di una cartella sopra la radice la riguarda.
        let index = Index::from_entries(vec![entry(&root.join("a.md"), 1), entry(&root.join("new.md"), 2), entry(&root.join("z.md"), 2)]);
        sb.refresh(&index, &[root.parent().unwrap().to_owned()], at(5));
        assert!(sb.tree.contains("z.md"));
    }

    /// Fuori da un repo l'albero mostra solo i file della cartella del file, non le sottocartelle.
    #[test]
    fn outside_a_repo_the_tree_shows_only_the_folder() {
        let d = tempfile::tempdir().unwrap();
        let desk = d.path().canonicalize().unwrap().join("Desktop");
        for f in ["note.md", "other.md", "deep/x.md", "proj/docs/y.md"] {
            let p = desk.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "x").unwrap();
        }
        let index = Index::from_entries(["note.md", "other.md", "deep/x.md", "proj/docs/y.md"].iter().map(|f| entry(&desk.join(f), 1)).collect());
        let mut sb = Sidebar::default();
        sb.set_open(&desk.join("note.md"), &index);
        let shown: Vec<String> = (0..sb.tree.visible_len()).map(|i| sb.tree.visible_id(i).unwrap().to_owned()).collect();
        assert_eq!(shown, ["note.md", "other.md"]);
    }

    /// I risultati di Cmd+P: un nome doppio prende il repo o la cartella, come in Recenti.
    #[test]
    fn quick_results_qualify_doubles() {
        let index = Index::from_entries(vec![entry(Path::new("/r1/docs/spec.md"), 1), entry(Path::new("/r2/docs/spec.md"), 2), entry(Path::new("/r1/docs/other.md"), 3)]);
        let mut sb = Sidebar::default();
        sb.quick.edit.replace_text("spec");
        sb.requery(&index);
        let rows: Vec<(&str, Option<&str>)> = sb.quick_rows.iter().map(|r| (r.name.as_str(), r.qualifier.as_deref())).collect();
        assert_eq!(rows, [("spec.md", Some("docs")), ("spec.md", Some("docs"))]);
        sb.quick.edit.replace_text("oth");
        sb.requery(&index);
        assert_eq!(sb.quick_rows.len(), 1);
        assert_eq!(sb.quick_rows[0].qualifier, None);
    }

    /// Un clic fuori dal campo chiude la lista; un clic su una riga (premuta) no, finché non parte.
    #[test]
    fn blur_closes_the_list_except_during_a_press() {
        let index = Index::from_entries(vec![entry(Path::new("/r/a.md"), 1)]);
        let mut sb = Sidebar::default();
        sb.quick.edit.replace_text("a");
        sb.quick.edit.focused = true;
        sb.requery(&index);
        assert!(!sb.blur_close(), "col fuoco resta aperta");
        sb.quick.edit.focused = false;
        sb.press("quick-/r/a.md", Phase::Down);
        assert!(!sb.blur_close(), "durante la pressione di una riga resta aperta");
        sb.press("quick-/r/a.md", Phase::Up);
        assert!(sb.blur_close());
        assert!(!sb.quick.active() && sb.quick_rows.is_empty());
    }

    /// Una scrittura altrove (l'indice cambia) non sposta la riga scelta con le frecce.
    #[test]
    fn index_refresh_keeps_the_chosen_row() {
        let a = Index::from_entries(vec![entry(Path::new("/r/a.md"), 1), entry(Path::new("/r/b.md"), 1), entry(Path::new("/r/c.md"), 1)]);
        let mut sb = Sidebar::default();
        sb.quick.edit.replace_text("md");
        sb.requery(&a);
        sb.quick.step(2);
        let chosen = sb.quick.chosen().unwrap().to_path_buf();
        let b = Index::from_entries(vec![entry(Path::new("/r/0.md"), 1), entry(Path::new("/r/a.md"), 1), entry(Path::new("/r/b.md"), 1), entry(Path::new("/r/c.md"), 1), entry(Path::new("/other/z.md"), 9)]);
        sb.refresh(&b, &[], SystemTime::now());
        assert_eq!(sb.quick.chosen(), Some(chosen.as_path()));
        assert_eq!(sb.quick_rows[sb.quick.selected].path, chosen);
    }

    /// Il gesto: giù preme la riga, su la rilascia; il su di un'altra riga non la tocca.
    #[test]
    fn press_follows_the_gesture() {
        let mut sb = Sidebar::default();
        sb.press("recent-a", Phase::Down);
        assert!(sb.is_pressed("recent-a"));
        sb.press("recent-a", Phase::Move);
        sb.press("recent-b", Phase::Up);
        assert!(sb.is_pressed("recent-a"));
        sb.press("recent-a", Phase::Up);
        assert!(!sb.is_pressed("recent-a"));
    }

    /// Misura: il costo sul thread della UI di `Sidebar::refresh` per un
    /// cambio dell'indice, con l'indice vero delle cartelle osservate e `big.md` aperto.
    /// `cargo test --profile perf sidebar_refresh_cost -- --ignored --nocapture`. Ogni giro
    /// prepara (fuori misura) un indice con una scrittura e misura solo `refresh`.
    #[test]
    #[ignore]
    fn sidebar_refresh_cost() {
        use std::time::Instant;
        let roots = crate::index::roots_from(&crate::settings::default_roots());
        let scanned = crate::index::scan::scan(&roots);
        let big = roots[0].join("rune-md-spike/fixtures/big.md").canonicalize().expect("big.md");
        let base = Index::from_entries(scanned.clone());
        let mut sb = Sidebar::default();
        sb.refresh(&base, &[], SystemTime::now());
        sb.set_open(&big, &base);
        let repo = sb.repo.clone().unwrap().root;
        let in_recent: Vec<PathBuf> = sb.recent.iter().map(|r| r.path.clone()).collect();
        let in_repo: Vec<PathBuf> = base.under(&repo).iter().map(|e| e.path.clone()).collect();
        let elsewhere: Vec<PathBuf> = scanned.iter().map(|e| e.path.clone()).filter(|p| !p.starts_with(&repo)).step_by(97).collect();
        let now = SystemTime::now();
        let mut case = |name: &str, paths: &[PathBuf], new_file: bool| {
            let mut ms = Vec::new();
            for (k, p) in paths.iter().cycle().take(200).enumerate() {
                let mut entries = scanned.clone();
                let path = if new_file { repo.join(format!("bench-new-{k}.md")) } else { p.clone() };
                entries.retain(|e| e.path != path);
                entries.push(Entry { path: path.clone(), mtime: now + Duration::from_secs(k as u64 + 1) });
                let index = Index::from_entries(entries);
                let t = Instant::now();
                sb.refresh(&index, std::slice::from_ref(&path), now);
                ms.push(t.elapsed().as_secs_f64() * 1000.0);
            }
            ms.sort_by(f64::total_cmp);
            let pct = |q: f64| ms[((ms.len() - 1) as f64 * q).round() as usize];
            eprintln!("sidebar_refresh_cost {name}: n={} p50={:.3} ms p95={:.3} ms max={:.3} ms", ms.len(), pct(0.5), pct(0.95), pct(1.0));
        };
        eprintln!("sidebar_refresh_cost indice {} voci, repo {} ({} voci)", base.len(), repo.display(), in_repo.len());
        case("scrittura a un file già in Recenti", &in_recent, false);
        case("scrittura a un file del repo aperto", &in_repo, false);
        case("scrittura fuori dal repo aperto", &elsewhere, false);
        case("file nuovo nel repo aperto (albero ricostruito)", &in_repo, true);
    }

    /// Misura: il costo sul thread della UI di un tasto di Cmd+P, con
    /// l'indice vero (`RUNEMD_ROOTS` non impostata, scansione delle cartelle osservate). Ogni ricerca
    /// si digita lettera per lettera; ogni tasto chiama `requery` (rank + qualificatori).
    /// `cargo test --profile perf quick_open_cost -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn quick_open_cost() {
        use std::time::Instant;
        // Nessuna cache su questa macchina: si scandiscono le cartelle osservate, come fa l'app.
        let index = &Index::from_entries(crate::index::scan::scan(&crate::index::roots_from(
            &crate::settings::default_roots(),
        )));
        eprintln!("quick_open_cost indice {} voci", index.len());
        let mut sb = Sidebar::default();
        let mut ms = Vec::new();
        let mut rank_ms = Vec::new();
        let queries = ["runemd", "readme", "spec", "design", "plan md", "a", "e", "docs", "claude", "task 18", "zzzq", "index", "superpowers", "notes", "rune md spike"];
        for _ in 0..20 {
            for q in queries {
                let mut typed = String::new();
                for c in q.chars() {
                    typed.push(c);
                    sb.quick.edit.replace_text(typed.clone());
                    let t = Instant::now();
                    sb.requery(index);
                    ms.push(t.elapsed().as_secs_f64() * 1000.0);
                    let t = Instant::now();
                    let _ = crate::ui::quick_open::rank(&typed, index.names(), 50);
                    rank_ms.push(t.elapsed().as_secs_f64() * 1000.0);
                }
            }
        }
        ms.sort_by(f64::total_cmp);
        let pct = |q: f64| ms[((ms.len() - 1) as f64 * q).round() as usize];
        rank_ms.sort_by(f64::total_cmp);
        eprintln!("quick_open_cost solo rank: p50={:.3} ms p95={:.3} ms max={:.3} ms", rank_ms[rank_ms.len() / 2], rank_ms[(rank_ms.len() as f64 * 0.95) as usize], rank_ms[rank_ms.len() - 1]);
        eprintln!("quick_open_cost n={} p50={:.3} ms p95={:.3} ms max={:.3} ms", ms.len(), pct(0.5), pct(0.95), pct(1.0));
    }
}
