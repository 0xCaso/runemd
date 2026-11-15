//! The YAML frontmatter: a `---` … `---` (or `...`) block that opens the file at byte 0. In reading mode it is
//! a metadata block of its own (`ui::doc_view`), not a giant setext heading; the heading outline
//! skips it; the source (Cmd+E) stays as it is.

/// The length in bytes of the frontmatter at the start of `source`, closing line included (without
/// the newline that follows it). `None` if the file does not start with a `---` line or if no `---`
/// or `...` line closes it.
pub fn len(source: &str) -> Option<usize> {
    let mut lines = source.split_inclusive('\n');
    let first = lines.next()?;
    if !first.ends_with('\n') || bare(first) != "---" {
        return None;
    }
    let mut at = first.len();
    for line in lines {
        if matches!(bare(line), "---" | "...") {
            return Some(at + line.trim_end_matches(['\n', '\r']).len());
        }
        at += line.len();
    }
    None
}

/// The line without the newline and trailing spaces.
fn bare(line: &str) -> &str {
    line.trim_end_matches(['\n', '\r'])
        .trim_end_matches([' ', '\t'])
}

/// Whether `block` (the source of a block from `doc::blocks::split`) is a frontmatter. Only the first
/// block can be: the caller also checks the position.
pub fn is_block(block: &str) -> bool {
    len(block) == Some(block.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_block_at_byte_zero_is_frontmatter() {
        let src = "---\nname: RuneMD\ndescription: Reader\n---\n\n# Title\n";
        assert_eq!(
            len(src),
            Some("---\nname: RuneMD\ndescription: Reader\n---".len())
        );
        assert_eq!(
            len("---\na: 1\n...\ntext\n"),
            Some("---\na: 1\n...".len()),
            "closing with ..."
        );
        assert_eq!(
            len("---\r\na: 1\r\n---\r\n"),
            Some("---\r\na: 1\r\n---".len()),
            "CRLF newlines"
        );
        assert_eq!(
            len("---\na: 1\n---"),
            Some(12),
            "file that ends on the closing line"
        );
        assert_eq!(
            len("---  \na: 1\n---\t\n"),
            Some("---  \na: 1\n---\t".len()),
            "trailing spaces on the lines"
        );
    }

    #[test]
    fn without_a_closing_fence_it_is_not_frontmatter() {
        assert_eq!(len("---\nname: x\n\nA paragraph.\n"), None);
        assert_eq!(len("---\n"), None);
        assert_eq!(len("---"), None);
    }

    #[test]
    fn a_fence_not_at_the_start_is_markdown() {
        assert_eq!(len("\n---\na: 1\n---\n"), None, "empty line before");
        assert_eq!(len(" ---\na: 1\n---\n"), None, "space before");
        assert_eq!(
            len("# T\n\n---\na: 1\n---\n"),
            None,
            "further down: hr and setext"
        );
        assert_eq!(len("----\na: 1\n----\n"), None, "four dashes");
        assert_eq!(len("---a\nb\n---\n"), None);
        assert_eq!(len("---\na: 1\n--- x\n"), None, "closing with text after");
    }

    #[test]
    fn a_block_is_frontmatter_only_when_it_is_all_of_it() {
        assert!(is_block("---\na: 1\n---"));
        assert!(!is_block("---"));
        assert!(!is_block("---\na: 1\n---\n\ntext"));
    }
}
