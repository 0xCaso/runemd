//! The sidebar (DESIGN.md, "Layout" and "Sidebar row"): the "Open file ⌘P" field, "Recent" on
//! two lines and, below a rule, the saved folders, each with its tree; the folder of the
//! open file opens by itself, with "Save" if it is not saved (record
//! `2026-11-15-folders-instead-of-tree`). At the bottom, "Add folder".
//!
//! The state (`Sidebar`) is recomputed only when the index, the open file, the saved folders or the
//! branch change: the view reads rows that are already built, without touching the disk. The trees come
//! from the index (only folders with Markdown, spec "Sidebar"), not from a disk read.

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

/// Maximum number of Recent rows (spec, "Sidebar": 20, ipotesi).
pub const RECENT_MAX: usize = 20;
/// Recent takes at most this share of the sidebar height, then scrolls.
pub const RECENT_SHARE: f32 = 0.4;
/// The space between Recent rows, with or without rings (`recent-gap`).
pub const RECENT_GAP: f32 = 3.0;
/// A Recent row is this tall (`recent-row`): the name on top, folder or repo and time below
/// (record `2026-11-15-recent-two-lines`, mockup A).
pub const RECENT_ROW: f32 = 44.0;
/// The line height of the name (13 pt) and of the second line (12 pt), and the space between them.
pub const RECENT_NAME_LH: f32 = 16.0;
pub const RECENT_META_LH: f32 = 15.0;
pub const RECENT_LINE_GAP: f32 = 2.0;
/// Between the branch icon and the branch.
const BRANCH_GAP: f32 = 4.0;
/// The branch icon is smaller than the others (mockup A, 12 against 14).
const BRANCH_ICON: f32 = 12.0;
/// The rule between Recent and the repo: 12 above, 8 below, `ROW_PAD` at the sides (mockup A, `.sep`).
const SEP_TOP: f32 = 12.0;
const SEP_BOTTOM: f32 = 8.0;

/// The maximum height of Recent in a sidebar `sidebar_h` tall: `RECENT_SHARE`, rounded
/// down to the point.
pub fn recent_cap(sidebar_h: f32) -> f32 {
    (sidebar_h * RECENT_SHARE).floor()
}

/// The repo: a book with the spine on the left (mockup A), drawn like the kit icons
/// (`glyphs::FOLDER`: viewBox 16, tratto 1, giunti tondi).
pub static REPO_ICON: Icon = Icon {
    name: "runemd-repo",
    view: 16.0,
    body: r#"<path d="M3.5 12.45V3a1.2 1.2 0 0 1 1.2-1.2H12.5V11.3H4.7a1.15 1.15 0 0 0 0 2.3H12.5" stroke="currentColor" stroke-linejoin="round" stroke-linecap="round"/><path d="M6.5 4.8H10" stroke="currentColor" stroke-linecap="round"/>"#,
};

/// The branch: two commits on the line and a branch that leaves it (mockup A). Drawn at 12 pt, so
/// the stroke is 1.2 in the 16 viewBox, to match the stroke of the 14 pt icons (0.9 pt).
pub static BRANCH_ICON_GLYPH: Icon = Icon {
    name: "runemd-branch",
    view: 16.0,
    body: r#"<circle cx="4.5" cy="3.5" r="1.75" stroke="currentColor" stroke-width="1.2"/><circle cx="4.5" cy="12.5" r="1.75" stroke="currentColor" stroke-width="1.2"/><circle cx="11.5" cy="5" r="1.75" stroke="currentColor" stroke-width="1.2"/><path d="M4.5 5.25V10.75M11.5 6.75C11.5 9.75 4.5 8.35 4.5 10.75" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#,
};

/// The level 0 guide line sits below the root arrow; each level adds one
/// `TREE_INDENT` further right, below the arrow of the folder above.
const GUIDE_X: f32 = TREE_ROW_PAD + CHEVRON / 2.0;
/// "Save" and "Remove": 6 pt at the sides, 1 above and below, radius 4 (mockup "al posto", `.pin`).
const CHIP_PAD_X: f32 = 6.0;
const CHIP_PAD_Y: f32 = 1.0;
const CHIP_R: f32 = 4.0;
/// A root name does not shrink and goes up to 120 pt (mockup "al posto", `.froot .rn`,
/// `flex:none`): the branch shrinks instead, and keeps at least 48 pt.
const ROOT_NAME_MAX: f32 = 120.0;
const ROOT_BRANCH_MIN: f32 = 48.0;

/// The maximum width of a root name in a sidebar `width` wide: `ROOT_NAME_MAX`, or
/// less if the row does not fit with arrow, icon and `ROOT_BRANCH_MIN` for the branch.
fn root_name_max(width: f32) -> f32 {
    let room = width - 2.0 * SIDEBAR_PAD - 2.0 * TREE_ROW_PAD - CHEVRON - ICON - 3.0 * ROW_GAP - ROOT_BRANCH_MIN;
    room.clamp(0.0, ROOT_NAME_MAX)
}
/// Rows at level `d` start here (+ `d * TREE_INDENT`): the folder arrow falls
/// on the guide line of the level below.
const TREE_X: f32 = GUIDE_X + TREE_INDENT - TREE_ROW_PAD - CHEVRON / 2.0;
/// Space inside the tree scroll for the ring, which sits `RING_W` outside the row.
const RING_ROOM: f32 = fx::ring::RING_W + HAIRLINE;

/// A Recent row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentRow {
    pub path: PathBuf,
    pub name: String,
    /// Where the file is. For a repeated name, the repo or folder that tells it apart: the files
    /// in `.worktrees/` and `.repos/` stay in Recent and are told apart this way (Waggle decision
    /// c_453c875b). In Recent it is always there, because it is the second line: for a unique name it is the repo
    /// (the folder outside a repo). In ⌘P it is always "repo / folder" (`place`).
    pub qualifier: Option<String>,
    pub mtime: SystemTime,
}

/// The qualifier of each row with a repeated name: the repo (or worktree) if the duplicates are
/// in different repos and that repo has only one of them, otherwise the folder. `repo_name` gives the
/// repo name of a path (from the sidebar cache: no disk access for a path already seen).
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

/// Root and repo name of a file (its folder outside a repo).
fn repo_of(path: &Path) -> (PathBuf, String) {
    let r = repo::of(path);
    (r.root, r.name)
}

/// Where a file is in ⌘P: "repo / folder" with the folder relative to the repo root, or
/// only the repo for a file in the root.
fn place(path: &Path, root: &Path, name: &str) -> String {
    match path.parent().and_then(|d| d.strip_prefix(root).ok()).filter(|rel| !rel.as_os_str().is_empty()) {
        Some(rel) => format!("{name} / {}", rel.to_string_lossy()),
        None => name.to_owned(),
    }
}

fn parent_name(path: &Path) -> String {
    path.parent().map(file_name).unwrap_or_default()
}

/// A root below Recent: a saved folder, or the unsaved folder of the open file (with
/// "Save"). Its tree has only the folders with a Markdown file, also deeper down.
pub struct Folder {
    pub root: PathBuf,
    pub name: String,
    pub branch: Option<String>,
    /// The root has a `.git`: repo icon and branch.
    pub git: bool,
    pub saved: bool,
    /// The root is expanded: its rows are visible.
    pub expanded: bool,
    pub tree: TreeModel<FileNode<PathBuf>>,
    /// The files the tree is built from, so it is not rebuilt if they do not change.
    files: Vec<PathBuf>,
    /// The expanded folders, to keep them expanded when the tree is rebuilt.
    open_dirs: HashSet<String>,
}

impl Folder {
    /// A new root, collapsed. The disk is read here (`.git`, branch), once per root.
    fn new(root: PathBuf, saved: bool, repo: Option<&Repo>) -> Folder {
        let git = root.join(".git").exists();
        let (name, branch) = match repo.filter(|r| r.root == root) {
            Some(r) => (r.name.clone(), r.branch.clone()),
            None => (file_name(&root), if git { repo::branch(&root) } else { None }),
        };
        Folder { root, name, branch: branch.filter(|_| git), git, saved, expanded: false, tree: TreeModel::default(), files: Vec::new(), open_dirs: HashSet::new() }
    }

    /// Everything below is shown for a saved folder or a repo; the unsaved folder
    /// of a file outside a repo shows only its files (a `~/Desktop/nota.md` does not bring all
    /// the Desktop).
    fn recursive(&self) -> bool {
        self.saved || self.git
    }

    /// Rebuilds the tree if its files changed (not for an mtime). True if it changed.
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

    /// The tree id of `path` (relative to the root), if it is below it.
    fn id_of(&self, path: &Path) -> Option<String> {
        let rel = path.strip_prefix(&self.root).ok()?;
        Some(rel.to_string_lossy().into_owned())
    }

    /// The visible rows: the root and, if expanded, its tree.
    fn rows(&self) -> usize {
        1 + if self.expanded { self.tree.visible_len() } else { 0 }
    }
}

/// A hidden folder (`.worktrees`, `.claude`): left out of the tree.
fn hidden(name: &std::ffi::OsStr) -> bool {
    name.to_string_lossy().starts_with('.')
}

/// The files to show under `root` (record `2026-11-15-folders-instead-of-tree`, constraint 3):
/// the index has only Markdown, so folders without Markdown, empty ones too, do not get in; left out
/// are hidden folders and, if not `recursive`, subfolders. The open file `open` is always there
/// if it is under `root`, also in a hidden folder or before the scan.
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

/// The sidebar state.
#[derive(Default)]
pub struct Sidebar {
    pub recent: Vec<RecentRow>,
    /// The relative time of each Recent row, computed in `tick` (the view does not read the clock).
    labels: Vec<String>,
    /// The wake-up armed for the next label change: the instant of the change and when
    /// Rune fires it (the `poll` clock).
    label_wake: Option<(SystemTime, f64)>,
    /// The Recent candidates of the last recompute (the `2n` most recent paths, in order),
    /// and among them the ones that are no longer files: the disk is checked only for a new candidate.
    candidates: Vec<PathBuf>,
    missing: HashSet<PathBuf>,
    /// Root and repo name of each path already seen (`repo::of`, once per path).
    repo_names: HashMap<PathBuf, (PathBuf, String)>,
    /// The open file (canonical, like the index paths).
    open: Option<PathBuf>,
    /// The repo of the open file (or its folder outside a repo), with the branch.
    pub repo: Option<Repo>,
    /// The saved folders (canonical), in the order of `Settings::saved`.
    saved: Vec<PathBuf>,
    /// The roots below Recent: the saved ones and, at the end, the open file's one if it is not in a
    /// saved one (record `2026-11-15-folders-instead-of-tree`).
    pub folders: Vec<Folder>,
    /// Grows at each change of the rows below Recent: Rune reads the keys again.
    pub tree_version: u64,
    /// The saved root under the pointer: its row shows "Remove" instead of the branch.
    hover_folder: Option<PathBuf>,
    /// The row pressed now (its key): scales to 0.98 until the pointer goes up.
    pressed: Option<String>,
    /// macOS denies reading a watched folder (Desktop): the sidebar says so.
    pub no_access: bool,
    /// Cmd+P: the field and the results; the result rows carry the qualifier for repeated names.
    pub quick: QuickOpen,
    pub quick_rows: Vec<RecentRow>,
    /// The time of each row of `quick_rows`, computed when the results change.
    pub quick_labels: Vec<String>,
    /// Cmd+N: the "New file" field in place of the Cmd+P one.
    pub new_file: super::new_file::NewFile,
    /// The tree row to scroll into view (the file just opened); `poll` takes it.
    reveal: Option<String>,
}

impl Sidebar {
    /// The open file is `path`: its root (the deepest saved one that contains it, or its
    /// repo with "Save") expands by itself down to the file (constraint 2). The disk is read here
    /// (`repo::of`), once per opened file.
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

    /// The saved folders are these (canonical, in order): the roots are rebuilt, keeping the
    /// state of the ones that stay.
    pub fn set_saved(&mut self, saved: Vec<PathBuf>, index: &Index) {
        self.saved = saved;
        self.sync_folders(index);
        self.tree_version += 1;
    }

    /// The root of the open file: the deepest saved one that contains it, or the unsaved one.
    pub fn home(&self) -> Option<usize> {
        let open = self.open.as_ref()?;
        let saved = self.folders.iter().enumerate().filter(|(_, f)| f.saved && open.starts_with(&f.root)).max_by_key(|(_, f)| f.root.components().count());
        saved.or_else(|| self.folders.iter().enumerate().find(|(_, f)| !f.saved)).map(|(k, _)| k)
    }

    /// The tree of the open file's root (tests).
    #[cfg(test)]
    pub fn tree(&self) -> &TreeModel<FileNode<PathBuf>> {
        &self.folders[self.home().expect("root of the open file")].tree
    }

    /// Rebuilds the list of roots: the saved ones, then the open file's one if no saved one
    /// contains it. A root that was there keeps its tree and expanded folders (also when going from
    /// unsaved to saved); all trees are checked again.
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

    /// The tree row of the file just opened, to scroll into view once
    /// (`Tx::scroll_into_view_in("tree", …)`: the scroll moves only if needed).
    pub fn take_reveal(&mut self) -> Option<String> {
        self.reveal.take()
    }

    /// No open file (the last tab closed): the unsaved root goes away, the saved ones
    /// stay with no selected row, and the branch is no longer updated (the HEAD folder may no
    /// longer be watched, `live::Live::unfollow`).
    pub fn clear_open(&mut self) {
        self.open = None;
        self.repo = None;
        self.folders.retain(|f| f.saved);
        for f in &mut self.folders {
            f.tree.clear_selection();
        }
        self.tree_version += 1;
    }

    /// How many rows there are below Recent: for each root its row and, if expanded, the tree.
    pub fn flat_len(&self) -> usize {
        self.folders.iter().map(Folder::rows).sum()
    }

    /// Row `i` below Recent: root `k` and, for a tree row, its position
    /// among the visible rows.
    pub fn flat(&self, mut i: usize) -> Option<(usize, Option<usize>)> {
        for (k, f) in self.folders.iter().enumerate() {
            if i < f.rows() {
                return Some((k, i.checked_sub(1)));
            }
            i -= f.rows();
        }
        None
    }

    /// The key of row `i` below Recent (see [`flat_id`]).
    pub fn flat_key(&self, i: usize) -> String {
        match self.flat(i) {
            Some((k, None)) => flat_id(k, ""),
            Some((k, Some(r))) => flat_id(k, self.folders[k].tree.visible_id(r).unwrap_or_default()),
            None => String::new(),
        }
    }

    /// The index changed. `touched` are the paths (files or folders) the change is about:
    /// a tree is checked again only if one of them is above or below its root. True if the
    /// view changes.
    pub fn refresh(&mut self, index: &Index, touched: &[PathBuf], now: SystemTime) -> bool {
        let mut changed = self.refresh_recent(index);
        changed |= self.tick(now);
        if self.quick.active() {
            // The index changed, not the query: the selected row stays the same.
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

    /// The Cmd+P query changed, or the index: recompute the results. Repeated names get the
    /// Recent qualifier (`qualify`).
    pub fn requery(&mut self, index: &Index) {
        self.rank_quick(index, false);
    }

    /// `keep`: the query is the same (the index changed), the selected row stays by path.
    /// With an empty query the results are Recent. Each row has the Recent second line:
    /// the repo (the folder outside a repo), for repeated names the one that tells them apart.
    fn rank_quick(&mut self, index: &Index, keep: bool) {
        if self.quick.empty_query() {
            self.quick.set_hits(self.recent.iter().map(|r| r.path.clone()).collect(), keep);
        } else if keep {
            self.quick.refresh(index.names());
        } else {
            self.quick.requery(index.names());
        }
        // Each row says "repo / folder" (mockup "compatto"): it already tells repeated names apart.
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

    /// The Cmd+N field lost focus: it closes. The Cmd+P panel is modal and closes
    /// by itself on a click outside (`quick_open::panel`). True if the field closed.
    pub fn blur_close(&mut self) -> bool {
        self.new_file.blur_close()
    }

    /// Recomputes Recent: the `RECENT_MAX` most recent entries that are still files. The disk is
    /// checked only when a new candidate comes in (an `is_file`), the repo of each row once
    /// per path; an mtime-only change does not touch the disk. True if the rows change.
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
        // The second line: a unique name shows its repo.
        for r in rows.iter_mut().filter(|r| r.qualifier.is_none()) {
            r.qualifier = Some(repo_name(&r.path));
        }
        let changed = rows != self.recent;
        self.recent = rows;
        changed
    }

    /// Checks all trees again (tests). True if one changed.
    #[cfg(test)]
    fn refresh_trees(&mut self, index: &Index) -> bool {
        let open = self.open.clone();
        let changed = self.folders.iter_mut().fold(false, |c, f| f.refresh(index, open.as_deref()) | c);
        if changed {
            self.tree_version += 1;
        }
        changed
    }

    /// A gesture on row `key`: down presses it, up releases it (also when cancelled or outside).
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

    /// `.git/HEAD` of `root` changed (checkout, also in a worktree: `Live` watches the
    /// folder of `repo::head_path`): the branch of the repo and of its root is read again. True if
    /// it changed.
    pub fn git_head(&mut self, root: &Path) -> bool {
        let Some(repo) = self.repo.as_mut().filter(|r| r.root == root) else { return false };
        let branch = repo::branch(root);
        for f in self.folders.iter_mut().filter(|f| f.root == root && f.git) {
            f.branch = branch.clone();
        }
        std::mem::replace(&mut repo.branch, branch.clone()) != branch
    }

    /// Expands or collapses folder `id` in the tree of root `k`.
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

    /// Expands or collapses root `k`.
    pub fn toggle_folder(&mut self, k: usize) {
        if let Some(f) = self.folders.get_mut(k) {
            f.expanded = !f.expanded;
            self.tree_version += 1;
        }
    }

    /// The pointer enters (`on`) or leaves the row of root `root`. True if it changes.
    pub fn hover_folder(&mut self, root: &Path, on: bool) -> bool {
        let next = if on { Some(root.to_owned()) } else { self.hover_folder.clone().filter(|r| r != root) };
        let changed = next != self.hover_folder;
        self.hover_folder = next;
        changed
    }

    /// Recomputes the relative times at `now`. True if a label changed.
    pub fn tick(&mut self, now: SystemTime) -> bool {
        let labels: Vec<String> = self.recent.iter().map(|r| relative_time::format(now.duration_since(r.mtime).unwrap_or_default())).collect();
        let changed = labels != self.labels;
        self.labels = labels;
        changed
    }

    /// The wake-up to request for the next label change, like `rearm` in `main.rs`:
    /// one per change, and another one if the armed one fired before the wall-clock change (Rune
    /// measures from `clock`, the start of the frame). Any poll in between arms nothing.
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

/// The key of a row below Recent: the root index and the tree id (empty for the
/// root). Saved roots can be nested: the path alone is not enough.
pub fn flat_id(k: usize, id: &str) -> String {
    format!("{k}:{id}")
}

/// How the view reaches the app state and what a click does.
pub struct Hooks<S> {
    pub get: fn(&S) -> &Sidebar,
    pub get_mut: fn(&mut S) -> &mut Sidebar,
    /// Single click: opens the file in the preview tab.
    pub open: fn(&mut S, PathBuf, &mut Tx),
    /// Double click: opens the file in a pinned tab.
    pub pin: fn(&mut S, PathBuf, &mut Tx),
    /// The Rune clock time at which the file's "writing" ring turned on; `None` if off.
    pub ring: fn(&S, &Path) -> Option<f64>,
    /// The Cmd+P query changed: recompute the results from the index.
    pub query: fn(&mut S),
    /// Cmd+N, Return: creates the file and opens it in the source. True if the field closes.
    pub create: fn(&mut S, String, &mut Tx) -> bool,
    /// "Save" on the open file's root, or the folder picked with "Add folder".
    pub save: fn(&mut S, PathBuf),
    /// "Remove" on the row of a saved folder.
    pub unsave: fn(&mut S, PathBuf),
}

impl<S> Clone for Hooks<S> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<S> Copy for Hooks<S> {}

/// UI text (13/400) and small UI (12/500, tabular figures) from DESIGN.md.
pub(super) fn ui_font() -> TextStyle {
    TextStyle::new(UI_TEXT, UI_WEIGHT)
}

pub(super) fn small_font() -> TextStyle {
    TextStyle::new(UI_SMALL, UI_SMALL_WEIGHT).features("'tnum' 1")
}

/// The sidebar. `hover`: the pointer is fine and can hover (hover `sel` only then).
pub fn view<S: 'static>(s: &S, hooks: Hooks<S>, width: f32, theme: &Theme, reduced: bool, hover: bool) -> El {
    let sb = (hooks.get)(s);
    let motion = Motion { reduced, hover };
    // Everything in the sidebar sits on the glass: its greys (`Theme::on_glass`).
    let glass = theme.on_glass();
    let theme = &glass;
    let mut side = col().key("sidebar").w(width).h_full().pt(SIDEBAR_TOP).px(SIDEBAR_PAD).pb(SIDEBAR_BOTTOM).child(if sb.new_file.open { super::new_file::field(sb, hooks, theme) } else { quick_open::launcher(hooks, theme) });
    if sb.no_access {
        side = side.child(no_access(sb, hooks, theme, motion));
    }
    if !sb.recent.is_empty() {
        // At most `RECENT_SHARE` of the sidebar height, then it scrolls (with the system
        // overlay scroll bar): the repo and the tree stay in view even with many Recent rows.
        // The scroll clips what goes past its edges: inside there is `RING_ROOM` for the rings, and
        // the negative margin keeps the rows where they were.
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
        // No label: the roots are the titles of their trees, below a rule.
        let sep = el().key("repo-sep").h(HAIRLINE).mx(ROW_PAD).mt(SEP_TOP).mb(SEP_BOTTOM).fill(theme.line);
        side = side.child(sep).child(tree(sb, hooks, width, theme, motion));
    } else {
        side = side.child(el().grow(1.0).min_h(0.0));
    }
    side = side.child(add_folder(sb, hooks, theme, motion));
    side
}

/// Where macOS keeps the "Files and Folders" permission.
const PRIVACY_URL: &str = "x-apple.systempreferences:com.apple.preference.security?Privacy_FilesAndFolders";

/// The notice below the field: the text in `fg`, and below it a pressable row in [`row_meta`] that opens
/// System Settings. Only DESIGN.md tokens: no accent, no pill.
fn no_access<S: 'static>(sb: &Sidebar, hooks: Hooks<S>, theme: &Theme, motion: Motion) -> El {
    let link = row()
        .w_full()
        .h(ROW)
        .px(ROW_PAD)
        .center_y()
        .child(text("Open System Settings").font(small_font()).color(row_meta(theme)).truncate().shrink().min_w(0.0))
        .on_press_tx(|_: &mut S, _: &mut Tx| {
            if let Err(e) = std::process::Command::new("open").arg(PRIVACY_URL).spawn() {
                eprintln!("runemd: could not open System Settings: {e}");
            }
        });
    let link = pressable(link, "no-access-link".into(), sb, hooks, theme, motion, true).key("no-access-link");
    col()
        .key("no-access")
        .w_full()
        .child(text("No access to Desktop").font(ui_font()).color(theme.fg).px(ROW_PAD).pt(SECTION_PAD_BOTTOM).pb(SECTION_PAD_BOTTOM))
        .child(link)
}

/// A section label: 12/500 in `fg3` (`section-pad`).
fn section(label: &'static str, theme: &Theme) -> El {
    text(label).key(label).font(small_font()).color(theme.fg3).pt(SECTION_PAD_TOP).px(ROW_PAD).pb(SECTION_PAD_BOTTOM)
}

/// How a row moves (DESIGN.md, `motion`); with "reduce motion" everything is immediate.
#[derive(Clone, Copy)]
pub(super) struct Motion {
    pub(super) reduced: bool,
    /// The pointer is fine and can hover: hover `sel` only then.
    pub(super) hover: bool,
}

impl Motion {
    /// L'hover `sel` entra ed esce in 120 ms (`ease`).
    fn hover_fade(self) -> Anim {
        if self.reduced { Anim::INSTANT } else { Curve::tween(HOVER_FADE, Easing::EASE).into() }
    }

    /// The scale of the pressed row goes and comes back in 160 ms ease-out.
    fn press(self) -> Anim {
        if self.reduced { Anim::INSTANT } else { Curve::tween(PRESS_TIME, EASE_OUT).into() }
    }
}

/// Makes `e` (the row with key `key`) a pressable row: hover `sel` with a fade, only with a
/// fine pointer (`hover_ok`), and scale 0.98 in 160 ms while pressed. Rune has no curve for
/// `press_scale` (down at once and back with a spring): the press is tracked with `on_drag` (down/up) and
/// the scale with `scale` + `scale_motion`. The click stays with `on_press` (release inside the row).
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

/// The host of the "writing" ring around a row: always there, so the structure
/// does not change when it turns on.
fn ring_host(row: El, mark: Mark, since: Option<f64>, reduced: bool, dark: bool) -> El {
    let host = el().w_full().radius(R_ROW).child(row);
    match since {
        Some(t) => fx::ring::slot(host, mark, t, reduced, dark),
        None => host,
    }
}

/// The colour of small text inside a pressable row (Recent time, repeated name, link):
/// the hover `sel` on the glass would bring `fg3` below 4.5:1 (DESIGN.md, "Contrast").
pub fn row_meta(theme: &Theme) -> Color {
    theme.fg2
}

/// A Recent row on two lines (record `2026-11-15-recent-two-lines`): the name on top, as wide
/// as the row and cut in the middle only if it does not fit; below, the folder or repo (cut in
/// the end) and the time on the right, in [`row_meta`] also on hover.
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

/// The "+" of "Add folder", drawn like the kit icons (viewBox 16, stroke 1).
pub static PLUS_ICON: Icon = Icon {
    name: "runemd-plus",
    view: 16.0,
    body: r#"<path d="M8 3V13M3 8H13" stroke="currentColor" stroke-linecap="round"/>"#,
};

/// A small action on a root row ("Save", "Remove"): 12/500 in [`row_meta`] inside
/// a half-point `line` border, radius 4 (mockup "al posto", `.pin`).
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

/// A root row (mockup "al posto", `.froot`): arrow, repo (or folder) icon in
/// `fg3`, name; on the right the branch in `fg3` (it shrinks first) and "Save" if not saved; on
/// a saved one, on hover, "Remove" takes the place of the branch (constraints 2 and 5). A saved name is 13/600 in `fg`, an unsaved one 13/400 in
/// `fg2`. A click expands or collapses the root.
fn folder_row<S: 'static>(s: &S, hooks: Hooks<S>, k: usize, width: f32, theme: &Theme, motion: Motion) -> El {
    let sb = (hooks.get)(s);
    let Some(f) = sb.folders.get(k) else { return el() };
    let chevron = icon(&glyphs::CHEVRON_DOWN, CHEVRON).color(theme.fg3).icon_rotation(if f.expanded { 0.0 } else { -std::f32::consts::FRAC_PI_2 });
    let glyph = if f.git { &REPO_ICON } else { &glyphs::FOLDER };
    let (weight, color) = if f.saved { (UI_STRONG_WEIGHT, theme.fg) } else { (UI_WEIGHT, row_meta(theme)) };
    // The name does not shrink (up to `root_name_max`): the branch shrinks.
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

/// "Add folder" at the bottom of the sidebar (mockup "al posto"): the macOS dialog, and the
/// picked folder is saved.
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
                Err(e) => eprintln!("runemd: folder dialog: {e}"),
            });
        });
    pressable(e, "add-folder".into(), sb, hooks, theme, motion, true).key("add-folder").mt(SEP_BOTTOM)
}

/// The roots and their trees: 26 pt rows, 16 indent, guide lines, the open file's root
/// expanded down to the file.
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
    // On the right, the room for the ring: the tree scroll clips what goes past its edges.
    let mut outer = row().w_full().h(TREE_ROW).pr(RING_ROOM);
    // One line per level, from the root down to the row's level: together they make continuous lines.
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
    // The selected row keeps its `sel`: no hover on top.
    let e = pressable(e, format!("tree-{}", flat_id(k, r.id)), sb, hooks, theme, motion, !r.selected);
    let since = node.data.as_deref().and_then(|p| (hooks.ring)(s, p));
    // The host is as wide as the row: with the indent it must shrink, or it goes out of the tree (name
    // cut without "…", fill and ring without the right side).
    outer.child(ring_host(e, Mark::TreeRing(i as u32), since, motion.reduced, theme.dark).grow(1.0).shrink().min_w(0.0).ml(TREE_X + depth * TREE_INDENT))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::Entry;
    use std::fs;

    /// How many Recent rows fit whole in `cap` (the rows and the `RECENT_GAP` between them).
    fn recent_rows_in(cap: f32) -> usize {
        ((cap + RECENT_GAP) / (RECENT_ROW + RECENT_GAP)).floor().max(0.0) as usize
    }

    /// The two-line row sizes: name (16) + 2 + second line (15) fit in 44 with
    /// 5.5 above and below; in 40 % of the sidebar about 5 Recent rows fit (record, constraint 3)
    /// instead of 7–8 at 28.
    #[test]
    fn recent_row_height_maths() {
        let pad = (RECENT_ROW - RECENT_NAME_LH - RECENT_LINE_GAP - RECENT_META_LH) / 2.0;
        assert_eq!(pad, 5.5);
        assert_eq!((recent_cap(616.0), recent_cap(800.0), recent_cap(1000.5)), (246.0, 320.0, 400.0));
        // The mockup window (sidebar 616 tall), an 800 window and a short 600 one.
        assert_eq!(recent_rows_in(recent_cap(616.0)), 5);
        assert_eq!(recent_rows_in(recent_cap(800.0)), 6);
        assert_eq!(recent_rows_in(recent_cap(600.0)), 5);
        assert_eq!(recent_rows_in(recent_cap(560.0)), 4);
        let old = |cap: f32| ((cap + RECENT_GAP) / (ROW + RECENT_GAP)).floor() as usize;
        assert_eq!(old(recent_cap(616.0)), 8, "before: 28 pt rows");
        assert_eq!(recent_rows_in(0.0), 0);
    }

    /// Recent rows of an index, from a new sidebar.
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

    /// A repo with `.git/HEAD` on `main` and the files `files` (relative), all written.
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

    /// A file three folders deep in the repo, long name: every tree row (fill,
    /// radius and ring) ends inside the tree, the indent does not push it past the right edge.
    #[test]
    fn deep_tree_rows_stay_inside_the_tree() {
        let deep = "docs/architecture/synchronization/2026-11-14-architecture-of-the-offline-synchronization-system-v2.md";
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
        assert!(rows.len() >= 5, "tree rows: {}", rows.len());
        for e in rows {
            assert!(e.rect[0] + e.rect[2] <= right + 0.5, "{} esce dall'albero: {:?}, bordo destro {right}", e.path, e.rect);
        }
    }

    /// D1, D2, D3 and D5 (`docs/decisions/2026-11-14-sidebar-tabs-long-names.md`), with 60 files
    /// in one folder and long names: Recent ≤ 40 % of the sidebar height and 3 pt between the
    /// rows; the tree scrolls down to the open file; the branch below the repo name; the tab
    /// at most 240 wide.
    #[test]
    fn long_lists_and_names_keep_their_bounds() {
        let long = "2026-11-14-architecture-of-the-offline-synchronization-system-v2-final";
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
        let rect = |key: &str| els.iter().find(|e| e.key.as_deref() == Some(key) && !e.exiting).unwrap_or_else(|| panic!("{key} missing")).rect;
        let side = rect("sidebar");
        let recent = rect("recent");
        assert!(recent[3] <= side[3] * RECENT_SHARE + 0.5, "Recenti alto {} su {}", recent[3], side[3]);
        let mut rows: Vec<[f32; 4]> = els.iter().filter(|e| !e.exiting && e.key.as_deref().is_some_and(|k| k.starts_with("recent-"))).map(|e| e.rect).collect();
        rows.sort_by(|a, b| a[1].total_cmp(&b[1]));
        assert!(rows.len() >= 3);
        for w in rows.windows(2) {
            assert!((w[1][1] - (w[0][1] + w[0][3]) - RECENT_GAP).abs() < 0.5, "space between rows: {w:?}");
        }
        for r in &rows {
            assert!((r[3] - RECENT_ROW).abs() < 0.5, "Recent row {} tall", r[3]);
        }
        assert!(els.iter().all(|e| e.key.as_deref() != Some("Repo del file")), "no \"Repo del file\" label");
        let tree = rect("tree");
        let id = flat_id(0, &files[59]);
        let row = rect(&id);
        assert!(row[1] >= tree[1] - 0.5 && row[1] + row[3] <= tree[1] + tree[3] + 0.5, "open file row out of view: {row:?} in {tree:?}");
        let tab = rect(&format!("t-{:x}", rt.state.tabs[0].doc.id));
        assert!(tab[2] <= crate::ui::theme::TAB_MAX_W + 0.5, "tab {} wide", tab[2]);
    }

    /// Two rows with the same name: below the name the repo (worktree), or the folder if the
    /// repo is the same. A unique name has its repo below.
    #[test]
    fn doubles_show_their_repo_or_folder() {
        let (_d, root) = repo_with(&["PLAN.md", "docs/PLAN.md", "README.md", ".worktrees/feat/README.md"]);
        fs::write(root.join(".worktrees/feat/.git"), "gitdir: x\n").unwrap();
        let index = Index::from_entries(["PLAN.md", "docs/PLAN.md", "README.md", ".worktrees/feat/README.md"].iter().enumerate().map(|(i, f)| entry(&root.join(f), i as u64)).collect());
        let rows = recent_rows(&index);
        let q = |rel: &str| rows.iter().find(|r| r.path == root.join(rel)).unwrap().qualifier.clone();
        assert_eq!(q("README.md").as_deref(), Some("proj"));
        assert_eq!(q(".worktrees/feat/README.md").as_deref(), Some("feat"));
        assert_eq!(q("PLAN.md").as_deref(), Some("proj"), "same repo: the folder");
        assert_eq!(q("docs/PLAN.md").as_deref(), Some("docs"));
        let index = Index::from_entries(vec![entry(&root.join("PLAN.md"), 1)]);
        assert_eq!(recent_rows(&index)[0].qualifier.as_deref(), Some("proj"), "unique name: the repo, second line");
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

    /// The tree comes from the index, expanded down to the open file and with the file selected; an
    /// mtime-only change does not rebuild it, a new file does, and expanded folders stay expanded.
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
        assert!(sb.tree().is_expanded("design") && sb.tree().is_expanded("docs/spike"), "expanded folders kept");
        assert!(sb.tree().contains("docs/new.md") && sb.tree().is_selected("docs/spike/a.md"));
    }

    /// A root with its rows expanded, all folders expanded: the visible ids.
    fn all_open(f: &mut Folder) -> Vec<String> {
        f.expanded = true;
        f.tree.expand_all();
        (0..f.tree.visible_len()).map(|i| f.tree.visible_id(i).unwrap().to_owned()).collect()
    }

    /// Constraint 3 (`2026-11-15-folders-instead-of-tree`): only folders with a Markdown file, also
    /// deeper down; left out are empty ones, ones with other files and hidden ones. A file open in a
    /// hidden folder stays reachable.
    #[test]
    fn only_folders_with_markdown_show() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap().join("Note");
        for f in ["a/b/c/deep.md", "top.md", "text-only/x.txt", ".hidden/h.md", "a/.drafts/z.md", "empty/inside/nothing.txt"] {
            let p = root.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "x").unwrap();
        }
        fs::create_dir_all(root.join("fully-empty")).unwrap();
        let index = Index::from_entries(crate::index::scan::scan(std::slice::from_ref(&root)));
        let mut sb = Sidebar::default();
        sb.set_saved(vec![root.clone()], &index);
        assert_eq!(all_open(&mut sb.folders[0]), ["a", "a/b", "a/b/c", "a/b/c/deep.md", "top.md"]);
        sb.set_open(&root.join(".hidden/h.md"), &index);
        assert_eq!(sb.home(), Some(0), "the saved folder contains the file");
        assert!(sb.tree().contains(".hidden/h.md") && sb.tree().is_selected(".hidden/h.md"));
        assert!(!sb.tree().contains("a/.drafts/z.md"));
    }

    /// Constraint 2: the open file's root expands by itself down to the file. Outside the saved ones it is
    /// its repo, at the end and unsaved ("Save"); inside a saved one it is that one (the
    /// deepest), with no extra row. The other saved ones stay collapsed.
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
        // A file inside a saved folder: that one expands, the unsaved one goes away.
        sb.set_saved(vec![notes.clone(), repo.clone(), repo.join("docs")], &index);
        sb.set_open(&repo.join("docs/a.md"), &index);
        assert_eq!(sb.folders.len(), 3);
        assert_eq!(sb.home(), Some(2), "the deepest saved folder");
        assert!(sb.folders[2].expanded && sb.tree().is_selected("a.md"));
        assert!(!sb.folders[1].tree.is_selected("docs/a.md"), "only one selected row");
        sb.toggle_folder(2);
        assert!(!sb.folders[2].expanded);
        assert_eq!(sb.flat_len(), 1 + 1 + sb.folders[1].rows());
    }

    /// Constraints 2, 4 and 5 in the app: "Save" puts the root among the saved ones (and in prefs, and among the
    /// watched ones if none covers it); "Remove" removes it and the file's root comes back with "Save".
    /// In the view: "Save" on the unsaved one, "Remove" on hover on the saved one in place of the branch.
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
        let save = find(&rt, "folder-save-0").expect("\"Save\" on the unsaved root").rect;
        let branch = find(&rt, "folder-branch-0").expect("the branch stays").rect;
        assert!(save[0] >= branch[0] + branch[2] - 0.5, "\"Save\" after the branch");
        // The root on one line (mockup "al posto"): icon, name, on the right branch and "Save".
        let (name, icon) = (find(&rt, "folder-name-0").unwrap().rect, find(&rt, "folder-icon-0").unwrap().rect);
        let mid = |r: [f32; 4]| r[1] + r[3] / 2.0;
        assert!(branch[0] >= name[0] + name[2] - 0.5, "branch right of the name: {name:?} {branch:?}");
        for r in [icon, branch, save] {
            assert!((mid(r) - mid(name)).abs() < 1.5, "one line: {r:?} against {name:?}");
        }
        let (x, y) = (save[0] + save[2] / 2.0, save[1] + save[3] / 2.0);
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        for _ in 0..200 {
            if !rt.frame(1.0 / 120.0) { break; }
        }
        assert_eq!(rt.state.settings.saved, std::slice::from_ref(&repo));
        assert!(rt.state.library.roots().contains(&repo), "watched: its tree has its Markdown files");
        let f = &rt.state.sidebar.folders[0];
        assert!(f.saved && f.expanded, "the root stays expanded");
        assert_eq!(rt.state.sidebar.folders.len(), 1, "no unsaved row");
        // The pointer is over the row: "Remove" in place of the branch.
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
        assert!(!f.saved && f.expanded, "back to unsaved, expanded");
        // Off the row the branch comes back... on the saved one; here the unsaved row has "Save".
        rt.pointer_move(1000.0, 400.0);
        rt.frame(1.0 / 120.0);
        assert!(find(&rt, "folder-save-0").is_some());
    }

    /// "Remove" shows only with the pointer over it; off it the branch comes back.
    #[test]
    fn hover_swaps_the_branch_for_remove() {
        let (_d, repo) = repo_with(&["a.md"]);
        let mut sb = Sidebar::default();
        sb.set_saved(vec![repo.clone()], &Index::default());
        assert!(sb.hover_folder(&repo, true));
        assert!(!sb.hover_folder(&repo, true), "invariato");
        assert!(!sb.hover_folder(Path::new("/other"), false), "another row does not turn this one off");
        assert!(sb.hover_folder(&repo, false));
        assert_eq!(sb.hover_folder, None);
    }

    /// After the last tab closes there is no open file: no repo, no tree, no selected
    /// row, and a HEAD change no longer touches anything.
    #[test]
    fn clear_open_drops_the_repo_and_the_selection() {
        let (_d, root) = repo_with(&["a.md"]);
        let mut sb = Sidebar::default();
        sb.set_open(&root.join("a.md"), &Index::default());
        assert!(sb.repo.is_some());
        sb.clear_open();
        assert!(sb.repo.is_none() && sb.open.is_none());
        assert!(sb.folders.is_empty(), "the unsaved root goes away");
        assert!(!sb.git_head(&root));
    }

    /// A checkout in the open file's repo changes the branch; in another repo it does not.
    #[test]
    fn git_head_rereads_the_branch() {
        let (_d, root) = repo_with(&["a.md"]);
        let mut sb = Sidebar::default();
        sb.set_open(&root.join("a.md"), &Index::default());
        fs::write(root.join(".git/HEAD"), "ref: refs/heads/feat/x\n").unwrap();
        assert!(!sb.git_head(Path::new("/other")));
        assert!(sb.git_head(&root));
        assert_eq!(sb.repo.as_ref().unwrap().branch.as_deref(), Some("feat/x"));
        assert!(!sb.git_head(&root), "invariato");
    }

    /// A folder that becomes unreadable mid-session: no panic, its rows go away
    /// from Recent, the tree stays the index one.
    #[test]
    fn an_unreadable_folder_does_not_panic() {
        use std::os::unix::fs::PermissionsExt;
        /// Restores the permissions even if an assertion fails, so the tempdir gets deleted.
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

    /// One wake-up per label change, not one per poll; if it fires before the wall-clock
    /// change, another one is armed.
    #[test]
    fn one_label_wake_per_change() {
        let mut sb = Sidebar { recent: vec![RecentRow { path: PathBuf::from("/a.md"), name: "a.md".into(), qualifier: None, mtime: at(0) }], ..Sidebar::default() };
        assert!(sb.tick(at(30)));
        assert_eq!(sb.labels, ["now"]);
        let wake = sb.label_wake(at(30), 100.0).expect("at the minute");
        assert_eq!(wake, Duration::from_secs(30) + Duration::from_millis(1));
        assert_eq!(sb.label_wake(at(31), 101.0), None, "a poll in between");
        // It fires early: the wall clock still says 59 s.
        let early = 100.0 + wake.as_secs_f64();
        assert!(!sb.tick(at(59)));
        assert!(sb.label_wake(at(59), early).is_some(), "arms another one");
        assert!(sb.tick(at(60)));
        assert_eq!(sb.labels, ["1 min"]);
    }

    /// Three rows with the same name, two in the same repo: the repo is not enough to tell them apart,
    /// those two get the folder.
    #[test]
    fn doubles_in_the_same_repo_fall_back_to_the_folder() {
        let (_d, root) = repo_with(&["a/PLAN.md", "b/PLAN.md", ".worktrees/feat/PLAN.md"]);
        fs::write(root.join(".worktrees/feat/.git"), "gitdir: x\n").unwrap();
        let index = Index::from_entries(["a/PLAN.md", "b/PLAN.md", ".worktrees/feat/PLAN.md"].iter().map(|f| entry(&root.join(f), 1)).collect());
        let rows = recent_rows(&index);
        let q = |rel: &str| rows.iter().find(|r| r.path == root.join(rel)).unwrap().qualifier.clone().unwrap();
        assert_eq!((q("a/PLAN.md").as_str(), q("b/PLAN.md").as_str(), q(".worktrees/feat/PLAN.md").as_str()), ("a", "b", "feat"));
    }

    /// An mtime-only change (an agent rewriting a file already in Recent) does not look at the
    /// disk: neither `is_file` nor the repo of the duplicates. A new file among the candidates does.
    #[test]
    fn an_mtime_only_change_does_not_touch_the_disk() {
        let (_d, root) = repo_with(&["README.md", "x/README.md", "b.md"]);
        let entries = |m: u64| vec![entry(&root.join("README.md"), m), entry(&root.join("x/README.md"), 2), entry(&root.join("b.md"), 3)];
        let mut sb = Sidebar::default();
        sb.refresh_recent(&Index::from_entries(entries(1)));
        // From here the disk says something else: if it were read again, the rows would change.
        fs::remove_file(root.join("b.md")).unwrap();
        fs::remove_dir_all(root.join(".git")).unwrap();
        assert!(sb.refresh_recent(&Index::from_entries(entries(9))), "the order changes");
        assert_eq!(sb.recent.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), ["README.md", "b.md", "README.md"]);
        assert_eq!(sb.recent[0].qualifier.as_deref(), Some("proj"), "folder from the cache: no re-read");
        // A new candidate is checked on disk.
        let mut more = entries(9);
        more.push(entry(&root.join("gone.md"), 10));
        sb.refresh_recent(&Index::from_entries(more));
        assert!(sb.recent.iter().all(|r| r.name != "gone.md"));
    }

    /// A change outside the repo root does not check the tree again; one inside does.
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
        // A scan of a folder above the root is about it.
        let index = Index::from_entries(vec![entry(&root.join("a.md"), 1), entry(&root.join("new.md"), 2), entry(&root.join("z.md"), 2)]);
        sb.refresh(&index, &[root.parent().unwrap().to_owned()], at(5));
        assert!(sb.tree().contains("z.md"));
    }

    /// Outside a repo the tree shows only the files of the file's folder, not the subfolders.
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

    /// The Cmd+P results: a repeated name gets the repo or the folder, as in Recent; a unique
    /// name gets the repo (the folder outside a repo), like the Recent second line.
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

    /// The ⌘P rows say "repo / folder" (mockup "compatto"); a file in the repo root
    /// says only the repo.
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

    /// Empty query: the panel shows Recent, each with its time.
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

    /// The Cmd+P panel is modal: losing focus does not close it (Esc and a click outside close it).
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

    /// A write elsewhere (the index changes) does not move the row selected with the arrows.
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

    /// The gesture: down presses the row, up releases it; the up of another row does not touch it.
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

    /// Measure: the UI thread cost of `Sidebar::refresh` for an
    /// index change, with the real index of the watched folders and `big.md` open.
    /// `cargo test --profile perf sidebar_refresh_cost -- --ignored --nocapture`. Each round
    /// prepares (outside the measure) an index with one write and measures only `refresh`.
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
        case("write to a file already in Recent", &in_recent, false);
        case("write to a file of the open repo", &in_repo, false);
        case("write outside the open repo", &elsewhere, false);
        case("new file in the open repo (tree rebuilt)", &in_repo, true);
    }

    /// Measure: the UI thread cost of one Cmd+P key press, with
    /// the real index (`RUNEMD_ROOTS` not set, scan of the watched folders). Each query
    /// is typed letter by letter; each key calls `requery` (rank + qualifiers).
    /// `cargo test --profile perf quick_open_cost -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn quick_open_cost() {
        use std::time::Instant;
        // No cache on this machine: the watched folders are scanned, as the app does.
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
