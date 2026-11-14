//! Quali percorsi interessano a RuneMD: solo i Markdown fuori dalle cartelle di build e da `.git`.

use std::path::Path;

/// Cartelle che non si guardano mai (i file dentro non sono documenti dell'utente).
const SKIPPED_DIRS: [&str; 3] = [".git", "node_modules", "target"];

/// Vero se `name` è una cartella saltata.
pub fn skipped_name(name: &std::ffi::OsStr) -> bool {
    SKIPPED_DIRS.iter().any(|s| name == *s)
}

/// Vero se `path` sta sotto (o è) una cartella saltata.
pub fn skipped(path: &Path) -> bool {
    path.components().any(|c| skipped_name(c.as_os_str()))
}

/// Vero per un file `.md` o `.markdown` (maiuscole comprese) che non sta sotto una cartella saltata.
pub fn wanted(path: &Path) -> bool {
    let is_markdown = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"));
    is_markdown && !skipped(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_markdown_in_any_case() {
        assert!(wanted(Path::new("a/b.md")));
        assert!(wanted(Path::new("a/B.MARKDOWN")));
        assert!(wanted(Path::new("a/.claude/plan.md")));
    }

    #[test]
    fn rejects_skipped_dirs_and_other_extensions() {
        assert!(!wanted(Path::new("a/.git/x.md")));
        assert!(!wanted(Path::new("a/node_modules/x.md")));
        assert!(!wanted(Path::new("a/target/x.md")));
        assert!(!wanted(Path::new("a/b.txt")));
        assert!(!wanted(Path::new("a/.git/HEAD")));
    }
}
