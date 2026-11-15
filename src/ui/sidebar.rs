//! La sidebar (DESIGN.md, "Layout" e "Riga della sidebar"): campo "Apri file ⌘P", "Recenti" su
//! due righe e, sotto una linea, le cartelle salvate, ognuna con il suo albero; la cartella del
//! file aperto si apre da sola, con "Salva" se non è salvata (record
//! `2026-11-15-cartelle-al-posto-albero`). In fondo "Aggiungi cartella".
//!
//! Lo stato (`Sidebar`) si ricalcola solo quando cambia l'indice, il file aperto, le salvate o il
//! branch: la vista legge righe già pronte, senza toccare il disco. Gli alberi vengono dall'indice
//! (solo cartelle con Markdown, spec "Sidebar"), non da una lettura del disco.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use rune::{Anim, Color, Curve, Drag, El, Easing, GrowthSpec, Icon, Phase, ScrollSpec, TextStyle, Tx, col, el, icon, row, text};
use rune_kit::{FileNode, FileTree, TreeModel, glyphs};

use super::quick_open::{self, QuickOpen};
use super::relative_time;
use super::theme::{
    CHEVRON, EASE_OUT, HAIRLINE, HOVER_FADE, ICON, PRESS_SCALE,
    PRESS_TIME, R_ROW, ROW, ROW_GAP, ROW_PAD, SECTION_PAD_BOTTOM, SECTION_PAD_TOP, SIDEBAR_BOTTOM,
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
/// Una riga di Recenti è alta così (`recent-row`): il nome sopra, cartella o repo e ora sotto
/// (record `2026-11-15-recenti-due-righe`, mockup A).
pub const RECENT_ROW: f32 = 44.0;
/// L'interlinea del nome (13 pt) e della seconda riga (12 pt), e lo spazio fra le due.
pub const RECENT_NAME_LH: f32 = 16.0;
pub const RECENT_META_LH: f32 = 15.0;
pub const RECENT_LINE_GAP: f32 = 2.0;
/// Fra l'icona del branch e il branch.
const BRANCH_GAP: f32 = 4.0;
/// Il branch ha l'icona più piccola delle altre (mockup A, 12 contro 14).
const BRANCH_ICON: f32 = 12.0;
/// La linea fra Recenti e il repo: 12 sopra, 8 sotto, `ROW_PAD` ai lati (mockup A, `.sep`).
const SEP_TOP: f32 = 12.0;
const SEP_BOTTOM: f32 = 8.0;

/// L'altezza massima di Recenti in una sidebar alta `sidebar_h`: il `RECENT_SHARE`, arrotondato
/// in giù al punto.
pub fn recent_cap(sidebar_h: f32) -> f32 {
    (sidebar_h * RECENT_SHARE).floor()
}

/// Il repo: un libro con il dorso a sinistra (mockup A), disegnato come le icone del kit
/// (`glyphs::FOLDER`: viewBox 16, tratto 1, giunti tondi).
pub static REPO_ICON: Icon = Icon {
    name: "runemd-repo",
    view: 16.0,
    body: r#"<path d="M3.5 12.45V3a1.2 1.2 0 0 1 1.2-1.2H12.5V11.3H4.7a1.15 1.15 0 0 0 0 2.3H12.5" stroke="currentColor" stroke-linejoin="round" stroke-linecap="round"/><path d="M6.5 4.8H10" stroke="currentColor" stroke-linecap="round"/>"#,
};

/// Il branch: due commit sulla linea e un ramo che ne esce (mockup A). Si disegna a 12 pt, quindi
/// il tratto è 1,2 nel viewBox 16 per avere lo spessore delle icone da 14 (0,9 pt).
pub static BRANCH_ICON_GLYPH: Icon = Icon {
    name: "runemd-branch",
    view: 16.0,
    body: r#"<circle cx="4.5" cy="3.5" r="1.75" stroke="currentColor" stroke-width="1.2"/><circle cx="4.5" cy="12.5" r="1.75" stroke="currentColor" stroke-width="1.2"/><circle cx="11.5" cy="5" r="1.75" stroke="currentColor" stroke-width="1.2"/><path d="M4.5 5.25V10.75M11.5 6.75C11.5 9.75 4.5 8.35 4.5 10.75" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#,
};

/// La linea guida del livello 0 sta sotto la freccia della radice; ogni livello ne aggiunge
/// una `TREE_INDENT` più a destra, sotto la freccia della cartella sopra.
const GUIDE_X: f32 = TREE_ROW_PAD + CHEVRON / 2.0;
/// "Salva" e "Togli": 6 pt ai lati, 1 sopra e sotto, raggio 4 (mockup "al posto", `.pin`).
const CHIP_PAD_X: f32 = 6.0;
const CHIP_PAD_Y: f32 = 1.0;
const CHIP_R: f32 = 4.0;
/// Il nome di una radice non si stringe e arriva fino a 120 pt (mockup "al posto", `.froot .rn`,
/// `flex:none`): si stringe il branch, a cui restano almeno 48 pt.
const ROOT_NAME_MAX: f32 = 120.0;
const ROOT_BRANCH_MIN: f32 = 48.0;

/// La larghezza massima del nome di una radice in una sidebar larga `width`: `ROOT_NAME_MAX`, o
/// meno se la riga non ci sta con freccia, icona e `ROOT_BRANCH_MIN` per il branch.
fn root_name_max(width: f32) -> f32 {
    let room = width - 2.0 * SIDEBAR_PAD - 2.0 * TREE_ROW_PAD - CHEVRON - ICON - 3.0 * ROW_GAP - ROOT_BRANCH_MIN;
    room.clamp(0.0, ROOT_NAME_MAX)
}
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
    /// Dove sta il file. Per un nome che ritorna, il repo o la cartella che lo distingue: i file
    /// di `.worktrees/` e `.repos/` restano in Recenti e si distinguono così (decisione Waggle
    /// c_453c875b). In Recenti c'è sempre, perché è la seconda riga: per un nome unico è il repo
    /// (la cartella fuori da un repo). In ⌘P è sempre "repo / cartella" (`place`).
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

/// Radice e nome del repo di un file (la sua cartella fuori da un repo).
fn repo_of(path: &Path) -> (PathBuf, String) {
    let r = repo::of(path);
    (r.root, r.name)
}

/// Dove sta un file in ⌘P: "repo / cartella" con la cartella relativa alla radice del repo, o
/// solo il repo per un file nella radice.
fn place(path: &Path, root: &Path, name: &str) -> String {
    match path.parent().and_then(|d| d.strip_prefix(root).ok()).filter(|rel| !rel.as_os_str().is_empty()) {
        Some(rel) => format!("{name} / {}", rel.to_string_lossy()),
        None => name.to_owned(),
    }
}

fn parent_name(path: &Path) -> String {
    path.parent().map(file_name).unwrap_or_default()
}

/// Una radice sotto Recenti: una cartella salvata, o quella del file aperto non salvata (con
/// "Salva"). Il suo albero ha solo le cartelle con un Markdown, anche più giù.
pub struct Folder {
    pub root: PathBuf,
    pub name: String,
    pub branch: Option<String>,
    /// La radice ha un `.git`: icona del repo e branch.
    pub git: bool,
    pub saved: bool,
    /// La radice è aperta: le sue righe si vedono.
    pub expanded: bool,
    pub tree: TreeModel<FileNode<PathBuf>>,
    /// I file da cui è costruito l'albero, per non ricostruirlo se non cambiano.
    files: Vec<PathBuf>,
    /// Le cartelle aperte, per tenerle aperte quando l'albero si ricostruisce.
    open_dirs: HashSet<String>,
}

impl Folder {
    /// Una radice nuova, chiusa. Il disco si legge qui (`.git`, branch), una volta per radice.
    fn new(root: PathBuf, saved: bool, repo: Option<&Repo>) -> Folder {
        let git = root.join(".git").exists();
        let (name, branch) = match repo.filter(|r| r.root == root) {
            Some(r) => (r.name.clone(), r.branch.clone()),
            None => (file_name(&root), if git { repo::branch(&root) } else { None }),
        };
        Folder { root, name, branch: branch.filter(|_| git), git, saved, expanded: false, tree: TreeModel::default(), files: Vec::new(), open_dirs: HashSet::new() }
    }

    /// Tutto ciò che sta sotto si vede per una cartella salvata o un repo; la cartella non salvata
    /// di un file fuori da un repo mostra solo i suoi file (un `~/Desktop/nota.md` non porta tutto
    /// il Desktop).
    fn recursive(&self) -> bool {
        self.saved || self.git
    }

    /// Ricostruisce l'albero se i suoi file sono cambiati (non per un mtime). Vero se è cambiato.
    fn refresh(&mut self, index: &Index, open: Option<&Path>) -> bool {
        let files = shown_files(index.under(&self.root), &self.root, self.recursive(), open);
        if !self.tree.is_empty() && files.iter().copied().eq(self.files.iter().map(PathBuf::as_path)) {
            return false;
        }
        let files: Vec<PathBuf> = files.into_iter().map(Path::to_path_buf).collect();
        let root = &self.root;
        let rel = |p: &PathBuf| p.strip_prefix(root).ok().map(|r| (r.to_string_lossy().into_owned(), p.clone()));
        let mut tree = FileTree::new().compact_folders(false).key("repo-tree").build(files.iter().filter_map(rel));
        for id in &self.open_dirs {
            tree.set_expanded(id, true);
        }
        if let Some(id) = open.and_then(|o| self.id_of(o)) {
            tree.select(&id);
        }
        self.tree = tree;
        self.files = files;
        true
    }

    /// L'id nell'albero di `path` (relativo alla radice), se ci sta sotto.
    fn id_of(&self, path: &Path) -> Option<String> {
        let rel = path.strip_prefix(&self.root).ok()?;
        Some(rel.to_string_lossy().into_owned())
    }

    /// Le righe che si vedono: la radice e, se aperta, il suo albero.
    fn rows(&self) -> usize {
        1 + if self.expanded { self.tree.visible_len() } else { 0 }
    }
}

/// Una cartella nascosta (`.worktrees`, `.claude`): fuori dall'albero.
fn hidden(name: &std::ffi::OsStr) -> bool {
    name.to_string_lossy().starts_with('.')
}

/// I file da mostrare sotto `root` (record `2026-11-15-cartelle-al-posto-albero`, vincolo 3):
/// l'indice ha solo Markdown, quindi le cartelle senza Markdown, anche vuote, non entrano; fuori
/// le cartelle nascoste e, se non `recursive`, le sottocartelle. Il file aperto `open` c'è sempre
/// se sta sotto `root`, anche in una cartella nascosta o prima della scansione.
fn shown_files<'a>(under: &'a [crate::index::Entry], root: &Path, recursive: bool, open: Option<&'a Path>) -> Vec<&'a Path> {
    let shown = |p: &Path| {
        let Some(dir) = p.strip_prefix(root).ok().and_then(Path::parent) else { return false };
        if recursive { !dir.iter().any(hidden) } else { dir.as_os_str().is_empty() }
    };
    let mut files: Vec<&Path> = under.iter().map(|e| e.path.as_path()).filter(|p| shown(p)).collect();
    if let Some(o) = open.filter(|o| o.starts_with(root) && o != &root)
        && let Err(at) = files.binary_search(&o)
    {
        files.insert(at, o);
    }
    files
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
    /// Radice e nome del repo di ogni percorso già visto (`repo::of`, una volta per percorso).
    repo_names: HashMap<PathBuf, (PathBuf, String)>,
    /// Il file aperto (canonico, come i percorsi dell'indice).
    open: Option<PathBuf>,
    /// Il repo del file aperto (o la sua cartella fuori da un repo), con il branch.
    pub repo: Option<Repo>,
    /// Le cartelle salvate (canoniche), nell'ordine di `Settings::saved`.
    saved: Vec<PathBuf>,
    /// Le radici sotto Recenti: le salvate e, in fondo, quella del file aperto se non sta in una
    /// salvata (record `2026-11-15-cartelle-al-posto-albero`).
    pub folders: Vec<Folder>,
    /// Cresce a ogni cambio delle righe sotto Recenti: Rune rilegge le chiavi.
    pub tree_version: u64,
    /// La radice salvata sotto il puntatore: la sua riga mostra "Togli" al posto del branch.
    hover_folder: Option<PathBuf>,
    /// La riga premuta adesso (la sua chiave): scala a 0,98 finché il puntatore non si alza.
    pressed: Option<String>,
    /// macOS nega la lettura di una cartella osservata (Desktop): la sidebar lo dice.
    pub no_access: bool,
    /// Cmd+P: il campo e i risultati; le righe dei risultati hanno il qualificatore dei nomi doppi.
    pub quick: QuickOpen,
    pub quick_rows: Vec<RecentRow>,
    /// L'ora di ogni riga di `quick_rows`, calcolata quando i risultati cambiano.
    pub quick_labels: Vec<String>,
    /// Cmd+N: il campo "Nuovo file" al posto di quello di Cmd+P.
    pub new_file: super::new_file::NewFile,
    /// La riga dell'albero da portare in vista (il file appena aperto); la prende `poll`.
    reveal: Option<String>,
}

impl Sidebar {
    /// Il file aperto è `path`: la sua radice (la salvata più profonda che lo contiene, o il suo
    /// repo con "Salva") si apre da sola fino al file (vincolo 2). Il disco si legge qui
    /// (`repo::of`), una volta per file aperto.
    pub fn set_open(&mut self, path: &Path, index: &Index) {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_owned());
        self.repo = Some(repo::of(&path));
        self.open = Some(path.clone());
        self.sync_folders(index);
        for f in &mut self.folders {
            f.tree.clear_selection();
        }
        let Some(k) = self.home() else { return };
        let f = &mut self.folders[k];
        f.expanded = true;
        if let Some(id) = f.id_of(&path) {
            f.tree.expand_to(&id);
            f.tree.select(&id);
            let mut dir = Path::new(&id).parent();
            while let Some(d) = dir.filter(|d| !d.as_os_str().is_empty()) {
                f.open_dirs.insert(d.to_string_lossy().into_owned());
                dir = d.parent();
            }
            self.reveal = Some(flat_id(k, &id));
        }
        self.tree_version += 1;
    }

    /// Le cartelle salvate sono queste (canoniche, in ordine): le radici si rifanno, tenendo lo
    /// stato di quelle che restano.
    pub fn set_saved(&mut self, saved: Vec<PathBuf>, index: &Index) {
        self.saved = saved;
        self.sync_folders(index);
        self.tree_version += 1;
    }

    /// La radice del file aperto: la salvata più profonda che lo contiene, o quella non salvata.
    pub fn home(&self) -> Option<usize> {
        let open = self.open.as_ref()?;
        let saved = self.folders.iter().enumerate().filter(|(_, f)| f.saved && open.starts_with(&f.root)).max_by_key(|(_, f)| f.root.components().count());
        saved.or_else(|| self.folders.iter().enumerate().find(|(_, f)| !f.saved)).map(|(k, _)| k)
    }

    /// L'albero della radice del file aperto (i test).
    #[cfg(test)]
    pub fn tree(&self) -> &TreeModel<FileNode<PathBuf>> {
        &self.folders[self.home().expect("radice del file aperto")].tree
    }

    /// Rifà la lista delle radici: le salvate, poi quella del file aperto se nessuna salvata lo
    /// contiene. Una radice che c'era tiene albero e cartelle aperte (anche passando da non
    /// salvata a salvata); gli alberi si ricontrollano tutti.
    fn sync_folders(&mut self, index: &Index) {
        let mut old: Vec<Folder> = std::mem::take(&mut self.folders);
        let mut take = |root: &Path, saved: bool, repo: Option<&Repo>| match old.iter().position(|f| f.root == root) {
            Some(i) => {
                let mut f = old.swap_remove(i);
                f.saved = saved;
                f
            }
            None => Folder::new(root.to_owned(), saved, repo),
        };
        let mut folders: Vec<Folder> = self.saved.iter().map(|r| take(r, true, self.repo.as_ref())).collect();
        if let (Some(open), Some(repo)) = (&self.open, &self.repo)
            && !self.saved.iter().any(|r| open.starts_with(r))
        {
            folders.push(take(&repo.root, false, Some(repo)));
        }
        let open = self.open.as_deref();
        for f in &mut folders {
            f.refresh(index, open);
        }
        self.folders = folders;
    }

    /// La riga dell'albero del file appena aperto, da portare in vista una volta
    /// (`Tx::scroll_into_view_in("tree", …)`: lo scroll si muove solo se serve).
    pub fn take_reveal(&mut self) -> Option<String> {
        self.reveal.take()
    }

    /// Nessun file aperto (l'ultima scheda chiusa): la radice non salvata sparisce, le salvate
    /// restano senza riga scelta, e il branch non si aggiorna più (la cartella di HEAD può non
    /// essere più guardata, `live::Live::unfollow`).
    pub fn clear_open(&mut self) {
        self.open = None;
        self.repo = None;
        self.folders.retain(|f| f.saved);
        for f in &mut self.folders {
            f.tree.clear_selection();
        }
        self.tree_version += 1;
    }

    /// Quante righe ci sono sotto Recenti: per ogni radice la sua riga e, se aperta, l'albero.
    pub fn flat_len(&self) -> usize {
        self.folders.iter().map(Folder::rows).sum()
    }

    /// La riga `i` sotto Recenti: la radice `k` e, per una riga dell'albero, la sua posizione
    /// fra le righe visibili.
    pub fn flat(&self, mut i: usize) -> Option<(usize, Option<usize>)> {
        for (k, f) in self.folders.iter().enumerate() {
            if i < f.rows() {
                return Some((k, i.checked_sub(1)));
            }
            i -= f.rows();
        }
        None
    }

    /// La chiave della riga `i` sotto Recenti (vedi [`flat_id`]).
    pub fn flat_key(&self, i: usize) -> String {
        match self.flat(i) {
            Some((k, None)) => flat_id(k, ""),
            Some((k, Some(r))) => flat_id(k, self.folders[k].tree.visible_id(r).unwrap_or_default()),
            None => String::new(),
        }
    }

    /// L'indice è cambiato. `touched` sono i percorsi (file o cartelle) che il cambio riguarda:
    /// un albero si ricontrolla solo se uno di essi sta sopra o sotto la sua radice. Vero se la
    /// vista cambia.
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
        let open = self.open.as_deref();
        let mut trees = false;
        for f in self.folders.iter_mut().filter(|f| near(&f.root)) {
            trees |= f.refresh(index, open);
        }
        if trees {
            self.tree_version += 1;
        }
        changed | trees
    }

    /// La ricerca di Cmd+P è cambiata, o l'indice: ricalcola i risultati. I nomi doppi prendono il
    /// qualificatore di Recenti (`qualify`).
    pub fn requery(&mut self, index: &Index) {
        self.rank_quick(index, false);
    }

    /// `keep`: la ricerca è la stessa (l'indice è cambiato), la riga scelta resta per percorso.
    /// Con la ricerca vuota i risultati sono Recenti. Ogni riga ha la seconda riga di Recenti:
    /// il repo (la cartella fuori da un repo), per i nomi doppi quello che li distingue.
    fn rank_quick(&mut self, index: &Index, keep: bool) {
        if self.quick.empty_query() {
            self.quick.set_hits(self.recent.iter().map(|r| r.path.clone()).collect(), keep);
        } else if keep {
            self.quick.refresh(index.names());
        } else {
            self.quick.requery(index.names());
        }
        // Ogni riga dice "repo / cartella" (mockup "compatto"): distingue già i nomi doppi.
        let names = &mut self.repo_names;
        let rows: Vec<RecentRow> = self
            .quick
            .hits
            .iter()
            .map(|p| {
                let (root, name) = names.entry(p.to_owned()).or_insert_with(|| repo_of(p));
                RecentRow { name: file_name(p), path: p.clone(), qualifier: Some(place(p, root, name)), mtime: index.mtime(p).unwrap_or(SystemTime::UNIX_EPOCH) }
            })
            .collect();
        let now = SystemTime::now();
        self.quick_labels = rows.iter().map(|r| if r.mtime == SystemTime::UNIX_EPOCH { String::new() } else { relative_time::format(now.duration_since(r.mtime).unwrap_or_default()) }).collect();
        self.quick_rows = rows;
    }

    /// Il campo di Cmd+N ha perso il fuoco: si chiude. Il pannello di Cmd+P è modale e si chiude
    /// da sé al clic fuori (`quick_open::panel`). Vero se il campo si è chiuso.
    pub fn blur_close(&mut self) -> bool {
        self.new_file.blur_close()
    }

    /// Ricalcola Recenti: le `RECENT_MAX` voci più recenti che sono ancora file. Il disco si
    /// guarda solo quando entra una candidata nuova (un `is_file`), il repo di ogni riga una
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
        let mut repo_name = |p: &Path| names.entry(p.to_owned()).or_insert_with(|| repo_of(p)).1.clone();
        qualify(&mut rows, &mut repo_name);
        // La seconda riga: un nome unico mostra il suo repo.
        for r in rows.iter_mut().filter(|r| r.qualifier.is_none()) {
            r.qualifier = Some(repo_name(&r.path));
        }
        let changed = rows != self.recent;
        self.recent = rows;
        changed
    }

    /// Ricontrolla tutti gli alberi (i test). Vero se uno è cambiato.
    #[cfg(test)]
    fn refresh_trees(&mut self, index: &Index) -> bool {
        let open = self.open.clone();
        let changed = self.folders.iter_mut().fold(false, |c, f| f.refresh(index, open.as_deref()) | c);
        if changed {
            self.tree_version += 1;
        }
        changed
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
    /// cartella di `repo::head_path`): si rilegge il branch del repo e della sua radice. Vero se
    /// è cambiato.
    pub fn git_head(&mut self, root: &Path) -> bool {
        let Some(repo) = self.repo.as_mut().filter(|r| r.root == root) else { return false };
        let branch = repo::branch(root);
        for f in self.folders.iter_mut().filter(|f| f.root == root && f.git) {
            f.branch = branch.clone();
        }
        std::mem::replace(&mut repo.branch, branch.clone()) != branch
    }

    /// Apre o chiude la cartella `id` dell'albero della radice `k`.
    pub fn toggle(&mut self, k: usize, id: &str) {
        let Some(f) = self.folders.get_mut(k) else { return };
        f.tree.toggle(id);
        if f.tree.is_expanded(id) {
            f.open_dirs.insert(id.to_owned());
        } else {
            f.open_dirs.remove(id);
        }
        self.tree_version += 1;
    }

    /// Apre o chiude la radice `k`.
    pub fn toggle_folder(&mut self, k: usize) {
        if let Some(f) = self.folders.get_mut(k) {
            f.expanded = !f.expanded;
            self.tree_version += 1;
        }
    }

    /// Il puntatore entra (`on`) o esce dalla riga della radice `root`. Vero se cambia.
    pub fn hover_folder(&mut self, root: &Path, on: bool) -> bool {
        let next = if on { Some(root.to_owned()) } else { self.hover_folder.clone().filter(|r| r != root) };
        let changed = next != self.hover_folder;
        self.hover_folder = next;
        changed
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

/// La chiave di una riga sotto Recenti: l'indice della radice e l'id nell'albero (vuoto per la
/// radice). Le radici salvate possono stare una dentro l'altra: il percorso da solo non basta.
pub fn flat_id(k: usize, id: &str) -> String {
    format!("{k}:{id}")
}

/// Come la vista raggiunge lo stato dell'app e cosa fa un clic.
pub struct Hooks<S> {
    pub get: fn(&S) -> &Sidebar,
    pub get_mut: fn(&mut S) -> &mut Sidebar,
    /// Clic singolo: apre il file nella scheda di anteprima.
    pub open: fn(&mut S, PathBuf, &mut Tx),
    /// Doppio clic: apre il file in una scheda fissa.
    pub pin: fn(&mut S, PathBuf, &mut Tx),
    /// L'orologio di Rune a cui si è acceso l'anello "sta scrivendo" del file; `None` se spento.
    pub ring: fn(&S, &Path) -> Option<f64>,
    /// La ricerca di Cmd+P è cambiata: ricalcola i risultati dall'indice.
    pub query: fn(&mut S),
    /// Cmd+N, Invio: crea il file e lo apre nel sorgente. Vero se il campo si chiude.
    pub create: fn(&mut S, String, &mut Tx) -> bool,
    /// "Salva" sulla radice del file aperto, o la cartella scelta da "Aggiungi cartella".
    pub save: fn(&mut S, PathBuf),
    /// "Togli" sulla riga di una cartella salvata.
    pub unsave: fn(&mut S, PathBuf),
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
    // Tutto ciò che sta nella sidebar sta sul vetro: i suoi grigi (`Theme::on_glass`).
    let glass = theme.on_glass();
    let theme = &glass;
    let mut side = col().key("sidebar").w(width).h_full().pt(SIDEBAR_TOP).px(SIDEBAR_PAD).pb(SIDEBAR_BOTTOM).child(if sb.new_file.open { super::new_file::field(sb, hooks, theme) } else { quick_open::launcher(hooks, theme) });
    if sb.no_access {
        side = side.child(no_access(sb, hooks, theme, motion));
    }
    if !sb.recent.is_empty() {
        // Al massimo `RECENT_SHARE` dell'altezza della sidebar, poi scorre (con la barra del
        // sistema, a scomparsa): il repo e l'albero restano in vista anche con molti Recenti.
        // Lo scroll taglia ciò che esce dai suoi bordi: dentro c'è `RING_ROOM` per gli anelli, e
        // il margine negativo tiene le righe dove erano.
        let cap = rune::memo::layout_with("sidebar", |r| recent_cap(r[3]));
        let rows = col().w_full().gap(RECENT_GAP).padding(RING_ROOM).children(sb.recent.iter().enumerate().map(|(i, r)| {
            let label = sb.labels.get(i).cloned().unwrap_or_default();
            recent_row(s, hooks, i, r, label, theme, motion)
        }));
        let mut list = col().key("recent").mx(-RING_ROOM).my(-RING_ROOM).shrink().min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(rows);
        if let Some(cap) = cap {
            list = list.max_h(cap);
        }
        side = side.child(section("Recent", theme)).child(list);
    }
    if !sb.folders.is_empty() {
        // Niente etichetta: le radici sono i titoli dei loro alberi, sotto una linea.
        let sep = el().key("repo-sep").h(HAIRLINE).mx(ROW_PAD).mt(SEP_TOP).mb(SEP_BOTTOM).fill(theme.line);
        side = side.child(sep).child(tree(sb, hooks, width, theme, motion));
    } else {
        side = side.child(el().grow(1.0).min_h(0.0));
    }
    side = side.child(add_folder(sb, hooks, theme, motion));
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
        .child(text("Open System Settings").font(small_font()).color(row_meta(theme)).truncate().shrink().min_w(0.0))
        .on_press_tx(|_: &mut S, _: &mut Tx| {
            if let Err(e) = std::process::Command::new("open").arg(PRIVACY_URL).spawn() {
                eprintln!("runemd: Impostazioni di Sistema non aperte: {e}");
            }
        });
    let link = pressable(link, "no-access-link".into(), sb, hooks, theme, motion, true).key("no-access-link");
    col()
        .key("no-access")
        .w_full()
        .child(text("No access to Desktop").font(ui_font()).color(theme.fg).px(ROW_PAD).pt(SECTION_PAD_BOTTOM).pb(SECTION_PAD_BOTTOM))
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

/// Una riga di Recenti su due righe (record `2026-11-15-recenti-due-righe`): sopra il nome, largo
/// quanto la riga e tagliato in mezzo solo se non ci sta; sotto la cartella o il repo (tagliati in
/// fondo) e l'ora a destra, in [`row_meta`] anche in hover.
#[allow(clippy::too_many_arguments)]
fn recent_row<S: 'static>(s: &S, hooks: Hooks<S>, i: usize, r: &RecentRow, label: String, theme: &Theme, motion: Motion) -> El {
    let name = text(r.name.clone()).font(ui_font().lh(RECENT_NAME_LH)).color(theme.fg).truncate_middle().w_full().min_w(0.0);
    let place = text(r.qualifier.clone().unwrap_or_default()).font(TextStyle::new(UI_SMALL, UI_WEIGHT).lh(RECENT_META_LH)).color(row_meta(theme)).truncate().grow(1.0).shrink().min_w(0.0);
    let time = text(label).font(small_font().lh(RECENT_META_LH)).color(row_meta(theme));
    let meta = row().w_full().gap(ROW_GAP).center_y().child(place).child(time);
    let path = r.path.clone();
    let e = col()
        .w_full()
        .h(RECENT_ROW)
        .px(ROW_PAD)
        .gap(RECENT_LINE_GAP)
        .justify_center()
        .child(name)
        .child(meta)
        .on_double_press_tx({
            let path = path.clone();
            move |s: &mut S, tx: &mut Tx| (hooks.pin)(s, path.clone(), tx)
        })
        .on_press_tx(move |s: &mut S, tx: &mut Tx| (hooks.open)(s, path.clone(), tx));
    let since = (hooks.ring)(s, &r.path);
    let key = format!("recent-{}", r.path.display());
    let e = pressable(e, key.clone(), (hooks.get)(s), hooks, theme, motion, true);
    ring_host(e, Mark::RecentRing(i as u32), since, motion.reduced, theme.dark).key(key)
}

/// Il "+" di "Aggiungi cartella", disegnato come le icone del kit (viewBox 16, tratto 1).
pub static PLUS_ICON: Icon = Icon {
    name: "runemd-plus",
    view: 16.0,
    body: r#"<path d="M8 3V13M3 8H13" stroke="currentColor" stroke-linecap="round"/>"#,
};

/// Un'azione piccola sulla riga di una radice ("Salva", "Togli"): 12/500 in [`row_meta`] dentro
/// un bordo `line` da mezzo punto, raggio 4 (mockup "al posto", `.pin`).
fn chip<S: 'static>(label: &'static str, key: String, root: PathBuf, act: fn(&mut S, PathBuf), theme: &Theme) -> El {
    text(label)
        .key(key)
        .font(small_font())
        .color(row_meta(theme))
        .px(CHIP_PAD_X)
        .py(CHIP_PAD_Y)
        .radius(CHIP_R)
        .border(HAIRLINE, theme.line)
        .on_press(move |s: &mut S| act(s, root.clone()))
}

/// La riga di una radice (mockup "al posto", `.froot`): freccia, icona del repo (o cartella) in
/// `fg3`, nome; a destra il branch in `fg3` (stretto per primo) e "Salva" se non è salvata; su
/// una salvata in hover "Togli" prende il posto del branch (vincoli 2 e 5). Il nome di una salvata è 13/600 in `fg`, quello della non salvata 13/400 in
/// `fg2`. Un clic apre o chiude la radice.
fn folder_row<S: 'static>(s: &S, hooks: Hooks<S>, k: usize, width: f32, theme: &Theme, motion: Motion) -> El {
    let sb = (hooks.get)(s);
    let Some(f) = sb.folders.get(k) else { return el() };
    let chevron = icon(&glyphs::CHEVRON_DOWN, CHEVRON).color(theme.fg3).icon_rotation(if f.expanded { 0.0 } else { -std::f32::consts::FRAC_PI_2 });
    let glyph = if f.git { &REPO_ICON } else { &glyphs::FOLDER };
    let (weight, color) = if f.saved { (UI_STRONG_WEIGHT, theme.fg) } else { (UI_WEIGHT, row_meta(theme)) };
    // Il nome non si stringe (fino a `root_name_max`): si stringe il branch.
    let name = text(f.name.clone()).key(format!("folder-name-{k}")).font(TextStyle::new(UI_TEXT, weight)).color(color).truncate().shrink().min_w(0.0);
    let name = el().max_w(root_name_max(width)).child(name);
    let mut e = row()
        .w_full()
        .h(TREE_ROW)
        .px(TREE_ROW_PAD)
        .gap(ROW_GAP)
        .center_y()
        .child(el().size(CHEVRON, CHEVRON).center().child(chevron))
        .child(icon(glyph, ICON).color(theme.fg3).key(format!("folder-icon-{k}")))
        .child(name)
        .child(el().grow(1.0).min_w(0.0));
    let hovered = motion.hover && sb.hover_folder.as_ref() == Some(&f.root);
    if f.saved && hovered {
        e = e.child(chip("Remove", format!("folder-remove-{k}"), f.root.clone(), hooks.unsave, theme));
    } else if let Some(b) = &f.branch {
        let line = row()
            .gap(BRANCH_GAP)
            .center_y()
            .shrink()
            .min_w(0.0)
            .child(icon(&BRANCH_ICON_GLYPH, BRANCH_ICON).color(theme.fg3))
            .child(text(b.clone()).key(format!("folder-branch-{k}")).font(TextStyle::new(UI_SMALL, UI_WEIGHT)).color(theme.fg3).truncate_middle().shrink().min_w(0.0));
        e = e.child(line);
    }
    if !f.saved {
        e = e.child(chip("Save", format!("folder-save-{k}"), f.root.clone(), hooks.save, theme));
    }
    let root = f.root.clone();
    let e = e
        .on_press(move |s: &mut S| (hooks.get_mut)(s).toggle_folder(k))
        .on_hover_change(move |s: &mut S, on: bool, _: &mut Tx| {
            (hooks.get_mut)(s).hover_folder(&root, on);
        });
    pressable(e, flat_id(k, ""), sb, hooks, theme, motion, true)
}

/// "Aggiungi cartella" in fondo alla sidebar (mockup "al posto"): il dialogo di macOS e la
/// cartella scelta si salva.
fn add_folder<S: 'static>(sb: &Sidebar, hooks: Hooks<S>, theme: &Theme, motion: Motion) -> El {
    let e = row()
        .w_full()
        .h(ROW)
        .px(ROW_PAD)
        .gap(ROW_GAP)
        .center_y()
        .child(icon(&PLUS_ICON, ICON).color(theme.fg3))
        .child(text("Add folder").font(ui_font()).color(row_meta(theme)).truncate().shrink().min_w(0.0))
        .on_press_tx(move |_: &mut S, tx: &mut Tx| {
            let directory = std::env::var_os("HOME").map(PathBuf::from);
            tx.open_folder(rune::FileDialogOptions { title: "Add folder".into(), directory, ..Default::default() }, move |s: &mut S, result, _: &mut Tx| match result {
                Ok(Some(path)) => (hooks.save)(s, path),
                Ok(None) => {}
                Err(e) => eprintln!("runemd: dialogo della cartella: {e}"),
            });
        });
    pressable(e, "add-folder".into(), sb, hooks, theme, motion, true).key("add-folder").mt(SEP_BOTTOM)
}

/// Le radici e i loro alberi: righe da 26 pt, rientro 16, linee guida, la radice del file aperto
/// aperta fino al file.
fn tree<S: 'static>(sb: &Sidebar, hooks: Hooks<S>, width: f32, theme: &Theme, motion: Motion) -> El {
    let theme = *theme;
    let rows = col()
        .key("tree-rows")
        .w_full()
        .growth(GrowthSpec::default())
        .virtual_rows(sb.flat_len(), TREE_ROW, move |s: &S, i| Cow::Owned((hooks.get)(s).flat_key(i)), move |s: &S, i| tree_row(s, hooks, i, width, &theme, motion))
        .virtual_rows_version(sb.tree_version);
    col().key("tree").pt(RING_ROOM).w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(rows)
}

fn tree_row<S: 'static>(s: &S, hooks: Hooks<S>, i: usize, width: f32, theme: &Theme, motion: Motion) -> El {
    let sb = (hooks.get)(s);
    let Some((k, at)) = sb.flat(i) else { return el() };
    let Some(at) = at else { return folder_row(s, hooks, k, width, theme, motion) };
    let Some((node, r)) = sb.folders[k].tree.row(at) else { return el() };
    let depth = r.depth as f32;
    // A destra lo spazio per l'anello: lo scroll dell'albero taglia ciò che esce dai suoi bordi.
    let mut outer = row().w_full().h(TREE_ROW).pr(RING_ROOM);
    // Una linea per livello, dalla radice fino a quello della riga: insieme fanno linee continue.
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
        Some(path) => {
            let pin = path.clone();
            e.on_double_press_tx(move |s: &mut S, tx: &mut Tx| (hooks.pin)(s, pin.clone(), tx)).on_press_tx(move |s: &mut S, tx: &mut Tx| (hooks.open)(s, path.clone(), tx))
        }
        None => e.on_press(move |s: &mut S| (hooks.get_mut)(s).toggle(k, &id)),
    };
    // La riga selezionata tiene il suo `sel`: niente hover sopra.
    let e = pressable(e, format!("tree-{}", flat_id(k, r.id)), sb, hooks, theme, motion, !r.selected);
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

    /// Quante righe di Recenti si vedono intere in `cap` (le righe e i `RECENT_GAP` fra loro).
    fn recent_rows_in(cap: f32) -> usize {
        ((cap + RECENT_GAP) / (RECENT_ROW + RECENT_GAP)).floor().max(0.0) as usize
    }

    /// Le misure della riga a due righe: nome (16) + 2 + seconda riga (15) stanno nei 44 con
    /// 5,5 sopra e sotto; nel 40 % della sidebar entrano circa 5 Recenti (record, vincolo 3)
    /// invece dei 7–8 da 28.
    #[test]
    fn recent_row_height_maths() {
        let pad = (RECENT_ROW - RECENT_NAME_LH - RECENT_LINE_GAP - RECENT_META_LH) / 2.0;
        assert_eq!(pad, 5.5);
        assert_eq!((recent_cap(616.0), recent_cap(800.0), recent_cap(1000.5)), (246.0, 320.0, 400.0));
        // La finestra del mockup (sidebar alta 616), una finestra da 800 e una bassa da 600.
        assert_eq!(recent_rows_in(recent_cap(616.0)), 5);
        assert_eq!(recent_rows_in(recent_cap(800.0)), 6);
        assert_eq!(recent_rows_in(recent_cap(600.0)), 5);
        assert_eq!(recent_rows_in(recent_cap(560.0)), 4);
        let old = |cap: f32| ((cap + RECENT_GAP) / (ROW + RECENT_GAP)).floor() as usize;
        assert_eq!(old(recent_cap(616.0)), 8, "prima: righe da 28");
        assert_eq!(recent_rows_in(0.0), 0);
    }

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
        for r in &rows {
            assert!((r[3] - RECENT_ROW).abs() < 0.5, "riga di Recenti alta {}", r[3]);
        }
        assert!(els.iter().all(|e| e.key.as_deref() != Some("Repo del file")), "niente etichetta \"Repo del file\"");
        let tree = rect("tree");
        let id = flat_id(0, &files[59]);
        let row = rect(&id);
        assert!(row[1] >= tree[1] - 0.5 && row[1] + row[3] <= tree[1] + tree[3] + 0.5, "riga del file aperto fuori vista: {row:?} in {tree:?}");
        let tab = rect(&format!("t-{:x}", rt.state.tabs[0].doc.id));
        assert!(tab[2] <= crate::ui::theme::TAB_MAX_W + 0.5, "scheda larga {}", tab[2]);
    }

    /// Due righe con lo stesso nome: sotto il nome il repo (worktree), o la cartella se il
    /// repo è lo stesso. Un nome unico ha sotto il suo repo.
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
        assert_eq!(recent_rows(&index)[0].qualifier.as_deref(), Some("proj"), "nome unico: il repo, seconda riga");
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
        let shown: Vec<String> = (0..sb.tree().visible_len()).map(|i| sb.tree().visible_id(i).unwrap().to_owned()).collect();
        assert_eq!(shown, ["design", "docs", "docs/spike", "docs/spike/a.md", "docs/spike/b.md", "CLAUDE.md"]);
        assert!(sb.tree().is_selected("docs/spike/a.md"));
        sb.toggle(0, "design");
        let v = sb.tree_version;
        let index = Index::from_entries(files.iter().map(|f| entry(&root.join(f), 2)).collect());
        assert!(!sb.refresh_trees(&index), "solo mtime");
        assert_eq!(sb.tree_version, v);
        let mut more: Vec<Entry> = files.iter().map(|f| entry(&root.join(f), 2)).collect();
        more.push(entry(&root.join("docs/new.md"), 3));
        assert!(sb.refresh_trees(&Index::from_entries(more)));
        assert!(sb.tree().is_expanded("design") && sb.tree().is_expanded("docs/spike"), "cartelle aperte tenute");
        assert!(sb.tree().contains("docs/new.md") && sb.tree().is_selected("docs/spike/a.md"));
    }

    /// Una radice con le sue righe aperte, tutte le cartelle aperte: gli id che si vedono.
    fn all_open(f: &mut Folder) -> Vec<String> {
        f.expanded = true;
        f.tree.expand_all();
        (0..f.tree.visible_len()).map(|i| f.tree.visible_id(i).unwrap().to_owned()).collect()
    }

    /// Vincolo 3 (`2026-11-15-cartelle-al-posto-albero`): solo le cartelle con un Markdown, anche
    /// più giù; fuori le vuote, quelle con altri file e le nascoste. Un file aperto in una
    /// nascosta resta raggiungibile.
    #[test]
    fn only_folders_with_markdown_show() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap().join("Note");
        for f in ["a/b/c/deep.md", "top.md", "solo-testo/x.txt", ".nascosta/h.md", "a/.bozze/z.md", "vuota/dentro/niente.txt"] {
            let p = root.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "x").unwrap();
        }
        fs::create_dir_all(root.join("vuota-del-tutto")).unwrap();
        let index = Index::from_entries(crate::index::scan::scan(std::slice::from_ref(&root)));
        let mut sb = Sidebar::default();
        sb.set_saved(vec![root.clone()], &index);
        assert_eq!(all_open(&mut sb.folders[0]), ["a", "a/b", "a/b/c", "a/b/c/deep.md", "top.md"]);
        sb.set_open(&root.join(".nascosta/h.md"), &index);
        assert_eq!(sb.home(), Some(0), "la salvata contiene il file");
        assert!(sb.tree().contains(".nascosta/h.md") && sb.tree().is_selected(".nascosta/h.md"));
        assert!(!sb.tree().contains("a/.bozze/z.md"));
    }

    /// Vincolo 2: la radice del file aperto si apre da sola fino al file. Fuori dalle salvate è
    /// il suo repo, in fondo e non salvato ("Salva"); dentro una salvata è quella (la più
    /// profonda), senza riga in più. Le altre salvate restano chiuse.
    #[test]
    fn the_open_files_folder_opens_by_itself() {
        let (_d, repo) = repo_with(&["docs/a.md", "README.md"]);
        let notes = repo.parent().unwrap().join("Note");
        fs::create_dir_all(notes.join("x")).unwrap();
        fs::write(notes.join("x/n.md"), "x").unwrap();
        let index = Index::from_entries(crate::index::scan::scan(&[repo.clone(), notes.clone()]));
        let mut sb = Sidebar::default();
        sb.set_saved(vec![notes.clone()], &index);
        sb.set_open(&repo.join("docs/a.md"), &index);
        let roots: Vec<(&str, bool, bool)> = sb.folders.iter().map(|f| (f.name.as_str(), f.saved, f.expanded)).collect();
        assert_eq!(roots, [("Note", true, false), ("proj", false, true)]);
        assert_eq!(sb.folders[1].branch.as_deref(), Some("main"));
        assert!(sb.tree().is_selected("docs/a.md") && sb.tree().is_expanded("docs"));
        assert_eq!(sb.take_reveal().as_deref(), Some("1:docs/a.md"));
        let keys: Vec<String> = (0..sb.flat_len()).map(|i| sb.flat_key(i)).collect();
        assert_eq!(keys, ["0:", "1:", "1:docs", "1:docs/a.md", "1:README.md"]);
        // Un file dentro una salvata: si apre quella, la non salvata sparisce.
        sb.set_saved(vec![notes.clone(), repo.clone(), repo.join("docs")], &index);
        sb.set_open(&repo.join("docs/a.md"), &index);
        assert_eq!(sb.folders.len(), 3);
        assert_eq!(sb.home(), Some(2), "la salvata più profonda");
        assert!(sb.folders[2].expanded && sb.tree().is_selected("a.md"));
        assert!(!sb.folders[1].tree.is_selected("docs/a.md"), "una riga scelta sola");
        sb.toggle_folder(2);
        assert!(!sb.folders[2].expanded);
        assert_eq!(sb.flat_len(), 1 + 1 + sb.folders[1].rows());
    }

    /// Vincoli 2, 4 e 5 nell'app: "Salva" mette la radice fra le salvate (e in prefs, e fra le
    /// osservate se nessuna la copre); "Togli" la toglie e la radice del file torna con "Salva".
    /// In vista: "Salva" sulla non salvata, "Togli" in hover sulla salvata al posto del branch.
    #[test]
    fn save_and_remove_from_the_row() {
        let (_d, repo) = repo_with(&["docs/a.md"]);
        let mut app = crate::App::bare(crate::index::Library::load(Vec::new(), None));
        app.settings.saved.clear();
        app.open_paths(vec![repo.join("docs/a.md")]);
        assert_eq!(app.sidebar.folders.iter().map(|f| f.saved).collect::<Vec<_>>(), [false]);
        let mut rt = rune::Runtime::new(app, crate::view, (1280.0, 800.0), rune::Env::new(true, 2.0));
        rt.frame(0.0);
        for _ in 0..200 {
            if !rt.frame(1.0 / 120.0) { break; }
        }
        let find = |rt: &rune::Runtime<crate::App>, key: &str| rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key) && !e.exiting);
        let salva = find(&rt, "folder-save-0").expect("\"Salva\" sulla radice non salvata").rect;
        let branch = find(&rt, "folder-branch-0").expect("il branch resta").rect;
        assert!(salva[0] >= branch[0] + branch[2] - 0.5, "\"Salva\" dopo il branch");
        // La radice su una riga (mockup "al posto"): icona, nome, a destra branch e "Salva".
        let (name, icon) = (find(&rt, "folder-name-0").unwrap().rect, find(&rt, "folder-icon-0").unwrap().rect);
        let mid = |r: [f32; 4]| r[1] + r[3] / 2.0;
        assert!(branch[0] >= name[0] + name[2] - 0.5, "branch a destra del nome: {name:?} {branch:?}");
        for r in [icon, branch, salva] {
            assert!((mid(r) - mid(name)).abs() < 1.5, "una riga: {r:?} contro {name:?}");
        }
        let (x, y) = (salva[0] + salva[2] / 2.0, salva[1] + salva[3] / 2.0);
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        for _ in 0..200 {
            if !rt.frame(1.0 / 120.0) { break; }
        }
        assert_eq!(rt.state.settings.saved, std::slice::from_ref(&repo));
        assert!(rt.state.library.roots().contains(&repo), "osservata: il suo albero ha i suoi Markdown");
        let f = &rt.state.sidebar.folders[0];
        assert!(f.saved && f.expanded, "la radice resta aperta");
        assert_eq!(rt.state.sidebar.folders.len(), 1, "niente riga non salvata");
        // Il puntatore è sopra la riga: "Togli" al posto del branch.
        assert!(find(&rt, "folder-save-0").is_none());
        let togli = find(&rt, "folder-remove-0").expect("\"Togli\" in hover").rect;
        let (x, y) = (togli[0] + togli[2] / 2.0, togli[1] + togli[3] / 2.0);
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        for _ in 0..200 {
            if !rt.frame(1.0 / 120.0) { break; }
        }
        assert!(rt.state.settings.saved.is_empty());
        let f = &rt.state.sidebar.folders[0];
        assert!(!f.saved && f.expanded, "torna non salvata, aperta");
        // Fuori dalla riga torna il branch... sulla salvata; qui la riga non salvata ha "Salva".
        rt.pointer_move(1000.0, 400.0);
        rt.frame(1.0 / 120.0);
        assert!(find(&rt, "folder-save-0").is_some());
    }

    /// "Togli" compare solo col puntatore sopra; fuori torna il branch.
    #[test]
    fn hover_swaps_the_branch_for_remove() {
        let (_d, repo) = repo_with(&["a.md"]);
        let mut sb = Sidebar::default();
        sb.set_saved(vec![repo.clone()], &Index::default());
        assert!(sb.hover_folder(&repo, true));
        assert!(!sb.hover_folder(&repo, true), "invariato");
        assert!(!sb.hover_folder(Path::new("/altro"), false), "un'altra riga non spegne questa");
        assert!(sb.hover_folder(&repo, false));
        assert_eq!(sb.hover_folder, None);
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
        assert!(sb.folders.is_empty(), "la radice non salvata sparisce");
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
        assert!(sb.tree().contains("secret/b.md"));
    }

    /// Un risveglio per cambio di etichetta, non uno per poll; se scatta prima del cambio sul
    /// muro se ne arma un altro.
    #[test]
    fn one_label_wake_per_change() {
        let mut sb = Sidebar { recent: vec![RecentRow { path: PathBuf::from("/a.md"), name: "a.md".into(), qualifier: None, mtime: at(0) }], ..Sidebar::default() };
        assert!(sb.tick(at(30)));
        assert_eq!(sb.labels, ["now"]);
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
        assert!(!sb.tree().contains("new.md"));
        sb.refresh(&index, &[root.join("new.md")], at(5));
        assert!(sb.tree().contains("new.md"));
        // Una scansione di una cartella sopra la radice la riguarda.
        let index = Index::from_entries(vec![entry(&root.join("a.md"), 1), entry(&root.join("new.md"), 2), entry(&root.join("z.md"), 2)]);
        sb.refresh(&index, &[root.parent().unwrap().to_owned()], at(5));
        assert!(sb.tree().contains("z.md"));
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
        let shown: Vec<String> = (0..sb.tree().visible_len()).map(|i| sb.tree().visible_id(i).unwrap().to_owned()).collect();
        assert_eq!(shown, ["note.md", "other.md"]);
    }

    /// I risultati di Cmd+P: un nome doppio prende il repo o la cartella, come in Recenti; un nome
    /// unico prende il repo (la cartella fuori da un repo), come la seconda riga di Recenti.
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
        assert_eq!(sb.quick_rows[0].qualifier.as_deref(), Some("docs"));
    }

    /// Le righe di ⌘P dicono "repo / cartella" (mockup "compatto"); un file nella radice del repo
    /// dice solo il repo.
    #[test]
    fn quick_rows_show_repo_and_folder() {
        let (_d, root) = repo_with(&["docs/decisions/a-dec.md", "DECISIONS.md"]);
        let index = Index::from_entries(vec![entry(&root.join("docs/decisions/a-dec.md"), 1), entry(&root.join("DECISIONS.md"), 2)]);
        let mut sb = Sidebar::default();
        sb.quick.edit.replace_text("dec");
        sb.requery(&index);
        let q = |name: &str| sb.quick_rows.iter().find(|r| r.name == name).unwrap().qualifier.clone().unwrap();
        assert_eq!(q("a-dec.md"), "proj / docs/decisions");
        assert_eq!(q("DECISIONS.md"), "proj");
    }

    /// Ricerca vuota: il pannello mostra Recenti, con l'ora di ognuno.
    #[test]
    fn an_empty_query_shows_recent_files() {
        let index = Index::from_entries(vec![entry(Path::new("/r/a.md"), 1), entry(Path::new("/r/b.md"), 2)]);
        let recent = vec![RecentRow { path: "/r/b.md".into(), name: "b.md".into(), qualifier: None, mtime: at(2) }, RecentRow { path: "/r/a.md".into(), name: "a.md".into(), qualifier: None, mtime: at(1) }];
        let mut sb = Sidebar { recent, ..Sidebar::default() };
        sb.quick.show();
        sb.requery(&index);
        let names: Vec<&str> = sb.quick_rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["b.md", "a.md"]);
        assert_eq!(sb.quick_labels.len(), 2);
        assert!(sb.quick_labels.iter().all(|l| !l.is_empty()));
    }

    /// Il pannello di Cmd+P è modale: perdere il fuoco non lo chiude (lo chiudono Esc e il clic fuori).
    #[test]
    fn blur_does_not_close_the_quick_panel() {
        let index = Index::from_entries(vec![entry(Path::new("/r/a.md"), 1)]);
        let mut sb = Sidebar::default();
        sb.quick.show();
        sb.quick.edit.replace_text("a");
        sb.requery(&index);
        sb.quick.edit.focused = false;
        assert!(!sb.blur_close());
        assert!(sb.quick.active() && sb.quick_rows.len() == 1);
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
