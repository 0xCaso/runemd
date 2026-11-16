//! Which paths matter to RuneMD: only Markdown files outside build folders and `.git`.

use std::path::Path;

/// Folders never watched (the files inside are not user documents).
const SKIPPED_DIRS: [&str; 3] = [".git", "node_modules", "target"];

/// True if `name` is a skipped folder.
pub fn skipped_name(name: &std::ffi::OsStr) -> bool {
    SKIPPED_DIRS.iter().any(|s| name == *s)
}

/// True if `path` is below (or is) a skipped folder.
pub fn skipped(path: &Path) -> bool {
    path.components().any(|c| skipped_name(c.as_os_str()))
}

/// True for a `.md` or `.markdown` file (any case) that is not below a skipped folder.
pub fn wanted(path: &Path) -> bool {
    markdown_name(path.as_os_str()) && !skipped(path)
}

/// True if the file name (or path) `name` ends in `.md` or `.markdown`, any case.
pub fn markdown_name(name: &std::ffi::OsStr) -> bool {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
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
