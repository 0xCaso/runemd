//! Splits a document into its top-level blocks (paragraph, heading, list, code, table...),
//! each with its own source, so every block is a virtual row.

use pulldown_cmark::{Event, Options, Parser};

///
/// A YAML frontmatter at byte 0 (`doc::frontmatter`) is the first block, as it is: it does not go
/// through the parser, which would make a horizontal rule and a setext heading of it.
pub fn split(source: &str) -> Vec<String> {
    match super::frontmatter::len(source) {
        Some(n) => std::iter::once(source[..n].to_owned()).chain(markdown(&source[n..])).collect(),
        None => markdown(source),
    }
}

fn markdown(source: &str) -> Vec<String> {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (event, range) in Parser::new_ext(source, opts).into_offset_iter() {
        match event {
            Event::Start(_) => {
                // The parser starts an indented code block after its indent: take the line from
                // its start, or on its own the block becomes a paragraph (ledger
                // `2026-11-24-indented-code`).
                if depth == 0 {
                    let line = source[..range.start].rfind('\n').map_or(0, |i| i + 1);
                    start = if source[line..range.start].bytes().all(|b| b == b' ' || b == b'\t') { line } else { range.start };
                }
                depth += 1;
            }
            Event::End(_) => {
                depth -= 1;
                if depth == 0 { out.push(source[start..range.end].trim_end().to_owned()); }
            }
            Event::Rule if depth == 0 => out.push("---".to_owned()),
            _ => {}
        }
    }
    out.retain(|b| !b.is_empty());
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn splits_top_level_blocks_only() {
        let src = "# T\n\npara *a*\n\n- x\n- y\n\n```rs\nfn a() {}\n```\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n---\n";
        let b = super::split(src);
        assert_eq!(b, ["# T", "para *a*", "- x\n- y", "```rs\nfn a() {}\n```", "| a | b |\n| - | - |\n| 1 | 2 |", "---"]);
    }

    /// The frontmatter at byte 0 is a block of its own, as it is; the rest is split as always.
    /// Scrawl 1795533537078 (ledger `2026-11-24-indented-code`): an indented code block keeps its
    /// indent, so on its own it is still code and not a paragraph with links.
    #[test]
    fn an_indented_code_block_keeps_its_indent() {
        let src = "Build:\n\n    make app    # builds build/RuneMD.app\n    open build/RuneMD.app\n\n## Next\n";
        assert_eq!(super::split(src), ["Build:", "    make app    # builds build/RuneMD.app\n    open build/RuneMD.app", "## Next"]);
    }

    #[test]
    fn frontmatter_is_one_block_as_written() {
        let src = "---\nname: RuneMD\ncolors:\n  fg: \"x\"\n---\n\n# Design\n\nText.\n";
        assert_eq!(super::split(src), ["---\nname: RuneMD\ncolors:\n  fg: \"x\"\n---", "# Design", "Text."]);
        // Without a closing fence it stays Markdown: a horizontal rule and a paragraph.
        assert_eq!(super::split("---\nname: x\n\nText.\n"), ["---", "name: x", "Text."]);
    }
}
