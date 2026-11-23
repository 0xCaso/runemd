//! A table block as CSV, for "Copy table" (decision 2026-11-23-t3-style-paths, feature 5).

/// The cells of a GFM table block, row by row, without the delimiter row: each cell trimmed,
/// `\|` read as `|`, each row as wide as the header. `None` if the block is not a table.
pub fn rows(source: &str) -> Option<Vec<Vec<String>>> {
    if !crate::fx::doc_fx::is_table(source) {
        return None;
    }
    let mut rows: Vec<Vec<String>> = source.lines().enumerate().filter(|(i, l)| *i != 1 && !l.trim().is_empty()).map(|(_, l)| cells(l)).collect();
    // As GFM draws it: every row as wide as the header, short ones padded, extra cells dropped.
    let width = rows.first().map_or(0, Vec::len);
    for r in &mut rows {
        r.resize(width, String::new());
    }
    Some(rows)
}

/// The cells of one table line: split on `|` that is not escaped and not inside a code span.
fn cells(line: &str) -> Vec<String> {
    let line = line.trim();
    let line = line.strip_prefix('|').unwrap_or(line);
    let line = if line.ends_with('|') && !line.ends_with("\\|") { &line[..line.len() - 1] } else { line };
    let mut out = Vec::new();
    let mut cell = String::new();
    let (mut chars, mut code) = (line.chars().peekable(), false);
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => {
                cell.push('|');
                chars.next();
            }
            '`' => {
                code = !code;
                cell.push(c);
            }
            '|' if !code => out.push(std::mem::take(&mut cell).trim().to_owned()),
            _ => cell.push(c),
        }
    }
    out.push(cell.trim().to_owned());
    out
}

/// The table as CSV (RFC 4180): a cell with a comma, a quote or a line break goes in quotes,
/// its quotes doubled; rows end with CRLF.
pub fn to_csv(source: &str) -> Option<String> {
    let quote = |c: &String| if c.contains([',', '"', '\n', '\r']) { format!("\"{}\"", c.replace('"', "\"\"")) } else { c.clone() };
    Some(rows(source)?.iter().map(|r| r.iter().map(quote).collect::<Vec<_>>().join(",") + "\r\n").collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_table_becomes_csv() {
        let src = "| Name | Notes |\n|:-----|------:|\n| a.rs | one, two |\n| `x|y` | say \"hi\" |\n| pipe | a \\| b |\n";
        assert_eq!(to_csv(src).unwrap(), "Name,Notes\r\na.rs,\"one, two\"\r\n`x|y`,\"say \"\"hi\"\"\"\r\npipe,a | b\r\n");
    }

    #[test]
    fn a_table_without_outer_pipes_and_an_empty_cell() {
        let src = "a | b\n--|--\n1 |\n";
        assert_eq!(rows(src).unwrap(), vec![vec!["a", "b"], vec!["1", ""]]);
    }

    #[test]
    fn a_paragraph_is_not_a_table() {
        assert!(to_csv("Just | text\nmore").is_none());
    }
}
