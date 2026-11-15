//! Git repo of a file: root, name and branch, reading files only (no `git`).

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub root: PathBuf,
    pub name: String,
    pub branch: Option<String>,
}

/// Finds the first folder with `.git` going up from the file's folder;
/// if there is none, the root is the file's folder and the branch is `None`.
pub fn of(file: &Path) -> Repo {
    let dir = file.parent().unwrap_or(file);
    let root = dir
        .ancestors()
        .find(|d| d.join(".git").exists())
        .unwrap_or(dir)
        .to_path_buf();
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let branch = branch(&root);
    Repo { root, name, branch }
}

/// The branch of the repo with root `root` (short sha with detached HEAD); `None` if it cannot be read.
pub fn branch(root: &Path) -> Option<String> {
    head_path(root)
        .and_then(|h| std::fs::read_to_string(h).ok())
        .and_then(|t| parse_head(&t))
}

/// HEAD file the repo really uses; with a `.git` file (worktree) it follows `gitdir:`.
pub fn head_path(root: &Path) -> Option<PathBuf> {
    let git = root.join(".git");
    if git.is_dir() {
        return Some(git.join("HEAD"));
    }
    let text = std::fs::read_to_string(&git).ok()?;
    let target = text.lines().find_map(|l| l.strip_prefix("gitdir:"))?.trim();
    if target.is_empty() {
        return None;
    }
    // A relative path starts from the folder that holds the `.git` file.
    Some(root.join(target).join("HEAD"))
}

fn parse_head(text: &str) -> Option<String> {
    let t = text.trim();
    if let Some(r) = t.strip_prefix("ref:") {
        let r = r.trim();
        let b = r.strip_prefix("refs/heads/").unwrap_or(r);
        return (!b.is_empty()).then(|| b.to_string());
    }
    // HEAD staccato: sha esadecimale, ne mostriamo 7 caratteri.
    if t.len() >= 7 && t.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(t.chars().take(7).collect());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn repo_with_branch() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("proj");
        fs::create_dir_all(root.join(".git")).unwrap();
        fs::create_dir_all(root.join("docs/a")).unwrap();
        fs::write(root.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
        fs::write(root.join("docs/a/x.md"), "").unwrap();
        let r = of(&root.join("docs/a/x.md"));
        assert_eq!(r.root, root);
        assert_eq!(r.name, "proj");
        assert_eq!(r.branch.as_deref(), Some("main"));
    }

    #[test]
    fn head_staccato() {
        let d = tempfile::tempdir().unwrap();
        fs::create_dir_all(d.path().join(".git")).unwrap();
        fs::write(d.path().join(".git/HEAD"), "0123456789abcdef0123\n").unwrap();
        fs::write(d.path().join("x.md"), "").unwrap();
        assert_eq!(
            of(&d.path().join("x.md")).branch.as_deref(),
            Some("0123456")
        );
    }

    #[test]
    fn outside_a_repo() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("x.md"), "").unwrap();
        let r = of(&d.path().join("x.md"));
        assert_eq!(r.root, d.path());
        assert_eq!(r.branch, None);
    }

    #[test]
    fn worktree_gitdir_assoluto_e_relativo() {
        let d = tempfile::tempdir().unwrap();
        let real = d.path().join("main/.git/worktrees/wt");
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("HEAD"), "ref: refs/heads/feat/x\n").unwrap();
        let wt = d.path().join("wt");
        fs::create_dir_all(&wt).unwrap();
        fs::write(wt.join("x.md"), "").unwrap();
        fs::write(wt.join(".git"), format!("gitdir: {}\n", real.display())).unwrap();
        let r = of(&wt.join("x.md"));
        assert_eq!(r.name, "wt");
        assert_eq!(r.branch.as_deref(), Some("feat/x"));
        assert_eq!(head_path(&wt), Some(real.join("HEAD")));

        fs::write(wt.join(".git"), "gitdir: ../main/.git/worktrees/wt\n").unwrap();
        assert_eq!(of(&wt.join("x.md")).branch.as_deref(), Some("feat/x"));
    }

    #[test]
    fn malformed_without_panic() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join(".git"), "boh").unwrap();
        fs::write(d.path().join("x.md"), "").unwrap();
        assert_eq!(of(&d.path().join("x.md")).branch, None);
        assert_eq!(head_path(d.path()), None);
        fs::remove_file(d.path().join(".git")).unwrap();
        fs::create_dir(d.path().join(".git")).unwrap(); // HEAD assente
        assert_eq!(of(&d.path().join("x.md")).branch, None);
    }
}
