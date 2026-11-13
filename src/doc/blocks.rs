//! Divide un documento nei suoi blocchi top-level (paragrafo, titolo, lista, codice, tabella...),
//! ognuno con il proprio sorgente, così ogni blocco è una riga virtuale.

use pulldown_cmark::{Event, Options, Parser};

pub fn split(source: &str) -> Vec<String> {
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
}
