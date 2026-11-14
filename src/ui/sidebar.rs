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

use rune::{El, GrowthSpec, ScrollSpec, TextStyle, Tx, col, el, icon, row, text};
use rune_kit::{FileNode, FileTree, TreeModel, glyphs};

use super::relative_time;
use super::theme::{R_FIELD, R_KBD, R_ROW, ROW, TREE_INDENT, TREE_ROW, Theme, UI_SMALL, UI_TEXT};
use crate::fx::{self, Mark};
use crate::index::Index;
use crate::repo::{self, Repo};

/// Righe di Recenti al massimo (spec, "Sidebar": 20, ipotesi).
pub const RECENT_MAX: usize = 20;

/// Spazio sopra il campo: i semafori stanno sopra la sidebar (mockup `scheda`, `.side`).
const TOP: f32 = 44.0;
/// Margini interni della sidebar (mockup: `padding: 44px 8px 10px`).
const SIDE_X: f32 = 8.0;
const SIDE_BOTTOM: f32 = 10.0;
/// Altezza del campo "Apri file" e della riga del repo (DESIGN.md, `open-file-field`; mockup `.repo`).
const FIELD_H: f32 = 30.0;
/// Margine orizzontale dentro le righe (DESIGN.md, `sidebar-row`: `0 10px`); 8 nell'albero.
const ROW_PAD: f32 = 10.0;
const TREE_PAD: f32 = 8.0;
/// Spazio fra icona, nome e tempo (mockup: `gap: 8px`).
const GAP: f32 = 8.0;
/// Icone di campo e repo, e la freccia delle cartelle (mockup: 14 e 10 pt).
const ICON: f32 = 14.0;
const CHEVRON: f32 = 10.0;
/// La linea guida del livello 0 sta sotto il centro dell'icona del repo; ogni livello ne aggiunge
/// una `TREE_INDENT` più a destra, sotto la freccia della cartella sopra.
const GUIDE_X: f32 = ROW_PAD + ICON / 2.0;
/// Le righe di livello `d` partono da qui (+ `d * TREE_INDENT`): la freccia della cartella cade
/// sulla linea guida del livello sotto.
const TREE_X: f32 = GUIDE_X + TREE_INDENT - TREE_PAD - CHEVRON / 2.0;
/// Riga premuta: scala 0,98 (DESIGN.md, "Eccezioni volute").
const PRESS: f32 = 0.98;
/// Spazio dentro lo scroll dell'albero per l'anello, che sta fuori dalla riga di `RING_W`.
const RING_ROOM: f32 = 2.0;
/// Le linee sono di 0,5 pt (DESIGN.md, "Shapes").
const HAIRLINE: f32 = 0.5;

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

/// Le `n` voci più recenti dell'indice che esistono ancora come file, con il qualificatore per i
/// nomi doppi. Un file sparito senza evento (cartella senza permessi, disco staccato) non si
/// mostra; l'indice lo toglie al suo evento.
pub fn recent_rows(index: &Index, n: usize) -> Vec<RecentRow> {
    let mut rows: Vec<RecentRow> = index
        .recent(n * 2)
        .into_iter()
        .filter(|e| e.path.is_file())
        .take(n)
        .map(|e| RecentRow { name: file_name(&e.path), path: e.path.clone(), qualifier: None, mtime: e.mtime })
        .collect();
    let mut by_name: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        by_name.entry(r.name.as_str()).or_default().push(i);
    }
    let doubles: Vec<Vec<usize>> = by_name.into_values().filter(|v| v.len() > 1).collect();
    for group in doubles {
        let repos: Vec<String> = group.iter().map(|&i| repo::of(&rows[i].path).name).collect();
        let distinct = repos.iter().collect::<HashSet<_>>().len() > 1;
        for (k, &i) in group.iter().enumerate() {
            rows[i].qualifier = Some(if distinct { repos[k].clone() } else { parent_name(&rows[i].path) });
        }
    }
    rows
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
    /// Il file aperto (canonico, come i percorsi dell'indice).
    open: Option<PathBuf>,
    pub repo: Option<Repo>,
    pub tree: TreeModel<FileNode<PathBuf>>,
    /// I file da cui è costruito l'albero, per non ricostruirlo se non cambiano.
    tree_files: Vec<PathBuf>,
    /// Le cartelle aperte, per tenerle aperte quando l'albero si ricostruisce.
    open_dirs: HashSet<String>,
    /// Cresce a ogni cambio delle righe dell'albero: Rune rilegge le chiavi.
    pub tree_version: u64,
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
        }
        self.repo = Some(repo);
        self.open = Some(path);
        self.refresh_tree(index);
        if let Some(id) = self.open_id() {
            self.tree.expand_to(&id);
            self.tree.select(&id);
            let mut dir = Path::new(&id).parent();
            while let Some(d) = dir.filter(|d| !d.as_os_str().is_empty()) {
                self.open_dirs.insert(d.to_string_lossy().into_owned());
                dir = d.parent();
            }
            self.tree_version += 1;
        }
    }

    /// L'id nell'albero del file aperto (il percorso relativo alla radice del repo).
    fn open_id(&self) -> Option<String> {
        let root = &self.repo.as_ref()?.root;
        let rel = self.open.as_ref()?.strip_prefix(root).ok()?;
        Some(rel.to_string_lossy().into_owned())
    }

    /// L'indice è cambiato: Recenti e, se cambiano i file del repo, l'albero. Vero se la vista
    /// cambia.
    pub fn refresh(&mut self, index: &Index, now: SystemTime) -> bool {
        let recent = recent_rows(index, RECENT_MAX);
        let mut changed = recent != self.recent;
        self.recent = recent;
        changed |= self.tick(now);
        changed | self.refresh_tree(index)
    }

    /// Ricostruisce l'albero se i file sotto la radice del repo sono cambiati (non per un mtime).
    fn refresh_tree(&mut self, index: &Index) -> bool {
        let Some(root) = self.repo.as_ref().map(|r| r.root.clone()) else { return false };
        let mut files: Vec<PathBuf> = index.under(&root).iter().map(|e| e.path.clone()).collect();
        // Il file aperto c'è sempre, anche fuori dalle cartelle osservate o prima della scansione.
        if let Some(open) = self.open.as_ref().filter(|p| !files.contains(p)) {
            files.push(open.clone());
        }
        if files == self.tree_files && !self.tree.is_empty() {
            return false;
        }
        let rel = |p: &PathBuf| p.strip_prefix(&root).ok().map(|r| (r.to_string_lossy().into_owned(), p.clone()));
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
}

impl<S> Clone for Hooks<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S> Copy for Hooks<S> {}

/// Testo UI (13/400) e UI piccola (12/500, cifre tabellari) di DESIGN.md.
fn ui_font() -> TextStyle {
    TextStyle::new(UI_TEXT, 400.0)
}

fn small_font() -> TextStyle {
    TextStyle::new(UI_SMALL, 500.0).features("'tnum' 1")
}

/// La sidebar. `hover`: il puntatore è fine e può passare sopra (hover `sel` solo allora).
pub fn view<S: 'static>(s: &S, hooks: Hooks<S>, theme: &Theme, reduced: bool, hover: bool) -> El {
    let sb = (hooks.get)(s);
    let mut side = col().key("sidebar").w(super::theme::SIDEBAR).h_full().pt(TOP).px(SIDE_X).pb(SIDE_BOTTOM).child(field(theme));
    if !sb.recent.is_empty() {
        side = side.child(section("Recenti", theme));
        for (i, r) in sb.recent.iter().enumerate() {
            let label = sb.labels.get(i).cloned().unwrap_or_default();
            side = side.child(recent_row(s, hooks, i, r, label, theme, reduced, hover));
        }
    }
    if let Some(repo) = &sb.repo {
        side = side.child(section("Repo del file", theme)).child(repo_row(repo, theme)).child(tree(sb, hooks, theme, reduced, hover));
    }
    side
}

/// Il campo "Apri file ⌘P": solo da vedere (Cmd+P è il task 18).
fn field(theme: &Theme) -> El {
    let kbd = el().key("kbd").border(HAIRLINE, theme.line).radius(R_KBD).px(5.0).py(1.0).child(text("⌘P").font(TextStyle::new(11.0, 400.0)).color(theme.fg2));
    row()
        .key("open-file")
        .h(FIELD_H)
        .mx(2.0)
        .mb(6.0)
        .px(ROW_PAD)
        .gap(GAP)
        .center_y()
        .radius(R_FIELD)
        .fill(theme.field)
        .border(HAIRLINE, theme.line)
        .child(icon(&glyphs::SEARCH, ICON).color(theme.fg2))
        .child(text("Apri file").font(ui_font()).color(theme.fg2))
        .child(el().grow(1.0))
        .child(kbd)
}

/// Un'etichetta di sezione: 12/500 in `fg3` (mockup `.sec`: `padding: 14px 10px 6px`).
fn section(label: &'static str, theme: &Theme) -> El {
    text(label).key(label).font(small_font()).color(theme.fg3).pt(14.0).px(ROW_PAD).pb(6.0)
}

/// Fa di `e` una riga premibile: hover `sel` solo con puntatore fine, scala 0,98 premuta.
fn pressable(e: El, theme: &Theme, hover: bool) -> El {
    let e = e.radius(R_ROW).press_scale(PRESS);
    if hover { e.hover(theme.sel) } else { e }
}

/// L'ospite dell'anello "sta scrivendo" attorno a una riga: sempre presente, così la struttura
/// non cambia quando si accende.
fn ring_host(row: El, mark: Mark, since: Option<f64>, reduced: bool) -> El {
    let host = el().w_full().radius(R_ROW).child(row);
    match since {
        Some(t) => fx::ring::slot(host, mark, t, reduced),
        None => host,
    }
}

#[allow(clippy::too_many_arguments)]
fn recent_row<S: 'static>(s: &S, hooks: Hooks<S>, i: usize, r: &RecentRow, label: String, theme: &Theme, reduced: bool, hover: bool) -> El {
    let mut name = row().gap(6.0).center_y().grow(1.0).shrink().min_w(0.0).child(text(r.name.clone()).font(ui_font()).color(theme.fg).truncate().shrink().min_w(0.0));
    if let Some(q) = &r.qualifier {
        name = name.child(text(q.clone()).font(small_font()).color(theme.fg3).truncate().shrink().min_w(0.0));
    }
    let path = r.path.clone();
    let e = row()
        .w_full()
        .h(ROW)
        .px(ROW_PAD)
        .gap(GAP)
        .center_y()
        .child(name)
        .child(text(label).font(small_font()).color(theme.fg3))
        .on_press_tx(move |s: &mut S, tx: &mut Tx| (hooks.open)(s, path.clone(), tx));
    let since = (hooks.ring)(s, &r.path);
    ring_host(pressable(e, theme, hover), Mark::RecentRing(i as u32), since, reduced).key(format!("recent-{}", r.path.display()))
}

/// La riga del repo: icona cartella, nome 600, branch in `fg3`. Non sta al livello dei suoi file.
fn repo_row(repo: &Repo, theme: &Theme) -> El {
    let mut e = row()
        .key("repo")
        .h(FIELD_H)
        .px(ROW_PAD)
        .gap(GAP)
        .center_y()
        .child(icon(&glyphs::FOLDER, ICON).color(theme.fg3))
        .child(text(repo.name.clone()).font(TextStyle::new(UI_TEXT, 600.0)).color(theme.fg).truncate().shrink().min_w(0.0))
        .child(el().grow(1.0));
    if let Some(b) = &repo.branch {
        e = e.child(text(b.clone()).font(small_font()).color(theme.fg3).truncate().shrink().min_w(0.0));
    }
    e
}

/// L'albero del repo: righe da 26 pt, rientro 16, linee guida, cartelle aperte fino al file.
fn tree<S: 'static>(sb: &Sidebar, hooks: Hooks<S>, theme: &Theme, reduced: bool, hover: bool) -> El {
    let theme = *theme;
    let rows = col()
        .key("tree-rows")
        .w_full()
        .growth(GrowthSpec::default())
        .virtual_rows(
            sb.tree.visible_len(),
            TREE_ROW,
            move |s: &S, i| Cow::Owned((hooks.get)(s).tree.visible_id(i).unwrap_or_default().to_owned()),
            move |s: &S, i| tree_row(s, hooks, i, &theme, reduced, hover),
        )
        .virtual_rows_version(sb.tree_version);
    col().key("tree").pt(RING_ROOM).w_full().grow(1.0).min_h(0.0).scroll(ScrollSpec { stick_bottom: false }).child(rows)
}

fn tree_row<S: 'static>(s: &S, hooks: Hooks<S>, i: usize, theme: &Theme, reduced: bool, hover: bool) -> El {
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
        .px(TREE_PAD)
        .gap(GAP)
        .center_y()
        .child(el().size(CHEVRON, CHEVRON).center().child(chevron))
        .child(text(node.name.clone()).font(ui_font()).color(color).truncate().shrink().min_w(0.0));
    if r.selected {
        e = e.fill(theme.sel);
    }
    let id = r.id.to_owned();
    let e = match node.data.clone() {
        Some(path) => e.on_press_tx(move |s: &mut S, tx: &mut Tx| (hooks.open)(s, path.clone(), tx)),
        None => e.on_press(move |s: &mut S| (hooks.get_mut)(s).toggle(&id)),
    };
    let e = if r.selected { e.radius(R_ROW).press_scale(PRESS) } else { pressable(e, theme, hover) };
    let since = node.data.as_deref().and_then(|p| (hooks.ring)(s, p));
    outer.child(ring_host(e, Mark::TreeRing(i as u32), since, reduced).grow(1.0).ml(TREE_X + depth * TREE_INDENT))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Entry;
    use std::fs;

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
        let rows = recent_rows(&index, RECENT_MAX);
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

    /// Due righe con lo stesso nome: accanto al nome il repo (worktree), o la cartella se il
    /// repo è lo stesso. Un nome unico non ha niente accanto.
    #[test]
    fn doubles_show_their_repo_or_folder() {
        let (_d, root) = repo_with(&["PLAN.md", "docs/PLAN.md", "README.md", ".worktrees/feat/README.md"]);
        fs::write(root.join(".worktrees/feat/.git"), "gitdir: x\n").unwrap();
        let index = Index::from_entries(["PLAN.md", "docs/PLAN.md", "README.md", ".worktrees/feat/README.md"].iter().enumerate().map(|(i, f)| entry(&root.join(f), i as u64)).collect());
        let rows = recent_rows(&index, RECENT_MAX);
        let q = |rel: &str| rows.iter().find(|r| r.path == root.join(rel)).unwrap().qualifier.clone();
        assert_eq!(q("README.md").as_deref(), Some("proj"));
        assert_eq!(q(".worktrees/feat/README.md").as_deref(), Some("feat"));
        assert_eq!(q("PLAN.md").as_deref(), Some("proj"), "stesso repo: la cartella");
        assert_eq!(q("docs/PLAN.md").as_deref(), Some("docs"));
        let index = Index::from_entries(vec![entry(&root.join("PLAN.md"), 1)]);
        assert_eq!(recent_rows(&index, RECENT_MAX)[0].qualifier, None);
    }

    #[test]
    fn at_most_twenty_newest_first() {
        let d = tempfile::tempdir().unwrap();
        let entries: Vec<Entry> = (0..30).map(|i| {
            let p = d.path().join(format!("f{i}.md"));
            fs::write(&p, "x").unwrap();
            entry(&p, i)
        }).collect();
        let rows = recent_rows(&Index::from_entries(entries), RECENT_MAX);
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
        let (_d, root) = repo_with(&["a.md", "secret/b.md"]);
        let index = Index::from_entries(vec![entry(&root.join("a.md"), 1), entry(&root.join("secret/b.md"), 2)]);
        let mut sb = Sidebar::default();
        sb.set_open(&root.join("secret/b.md"), &index);
        fs::set_permissions(root.join("secret"), fs::Permissions::from_mode(0o000)).unwrap();
        sb.refresh(&index, at(10));
        sb.set_open(&root.join("secret/b.md"), &index);
        sb.git_head(&root);
        fs::set_permissions(root.join("secret"), fs::Permissions::from_mode(0o755)).unwrap();
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
}
