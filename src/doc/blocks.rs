//! Divide un documento nei suoi blocchi top-level (paragrafo, titolo, lista, codice, tabella...),
//! ognuno con il proprio sorgente, così ogni blocco è una riga virtuale.

use pulldown_cmark::{Event, Options, Parser};

///
/// Un frontmatter YAML al byte 0 (`doc::frontmatter`) è il primo blocco, così com'è: non passa dal
/// parser, che ne farebbe una riga orizzontale e un titolo setext.
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
                if depth == 0 { start = range.start; }
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

    /// Il frontmatter al byte 0 è un blocco a sé, così com'è; il resto si divide come sempre.
    #[test]
    fn frontmatter_is_one_block_as_written() {
        let src = "---\nname: RuneMD\ncolors:\n  fg: \"x\"\n---\n\n# Design\n\nTesto.\n";
        assert_eq!(super::split(src), ["---\nname: RuneMD\ncolors:\n  fg: \"x\"\n---", "# Design", "Testo."]);
        // Senza chiusura resta Markdown: una riga orizzontale e un paragrafo.
        assert_eq!(super::split("---\nname: x\n\nTesto.\n"), ["---", "name: x", "Testo."]);
    }
}
