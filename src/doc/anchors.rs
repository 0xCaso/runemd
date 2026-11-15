//! I link `#titolo` dentro il file: lo slug di ogni titolo come lo fa GitHub.

/// Lo slug di GitHub del testo di un titolo: minuscolo, via la punteggiatura (non `-` e `_`),
/// spazi in `-`.
pub fn slug(text: &str) -> String {
    text.trim()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            '-' | '_' => Some(c),
            c if c.is_alphanumeric() => Some(c),
            _ => None,
        })
        .flat_map(char::to_lowercase)
        .collect()
}

/// Il testo di un titolo ATX (`## Titolo ##`), senza i `#` e i segni di enfasi e codice.
fn heading_text(source: &str) -> Option<String> {
    let line = source.lines().next()?.trim_start();
    let hashes = line.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&hashes) || source.contains('\n') {
        return None;
    }
    let rest = &line[hashes..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    let rest = rest.trim().trim_end_matches('#').trim_end();
    Some(rest.chars().filter(|c| !matches!(c, '*' | '`' | '~')).collect())
}

/// Il blocco del titolo a cui porta `#anchor`, fra i sorgenti `blocks` in ordine; i titoli
/// con lo stesso slug prendono `-1`, `-2`, … come su GitHub.
pub fn find<'a>(blocks: impl Iterator<Item = &'a str>, anchor: &str) -> Option<usize> {
    let want = anchor.strip_prefix('#')?.to_lowercase();
    let mut seen = std::collections::HashMap::<String, usize>::new();
    for (i, b) in blocks.enumerate() {
        let Some(text) = heading_text(b) else { continue };
        let base = slug(&text);
        let n = seen.entry(base.clone()).or_insert(0);
        let s = if *n == 0 { base } else { format!("{base}-{n}") };
        *n += 1;
        if s == want {
            return Some(i);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_follow_github() {
        assert_eq!(slug("La fine"), "la-fine");
        assert_eq!(slug("Task 1: Crate, `paths.rs` & co."), "task-1-crate-pathsrs--co");
        assert_eq!(slug("Perché sì"), "perché-sì");
    }

    #[test]
    fn an_anchor_finds_its_heading() {
        let blocks = ["# Titolo", "testo", "## La *fine* ##", "## La fine", "```\n# no\n```"];
        assert_eq!(find(blocks.into_iter(), "#titolo"), Some(0));
        assert_eq!(find(blocks.into_iter(), "#la-fine"), Some(2));
        assert_eq!(find(blocks.into_iter(), "#la-fine-1"), Some(3));
        assert_eq!(find(blocks.into_iter(), "#no"), None);
        assert_eq!(find(blocks.into_iter(), "titolo"), None);
    }
}
