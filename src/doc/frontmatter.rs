//! Il frontmatter YAML: un blocco `---` … `---` (o `...`) che apre il file al byte 0. In lettura è
//! un blocco di metadati a sé (`ui::doc_view`), non un titolo setext gigante; l'indice dei titoli
//! lo salta; il sorgente (Cmd+E) resta com'è.

/// La lunghezza in byte del frontmatter all'inizio di `source`, riga di chiusura compresa (senza
/// l'a capo che la segue). `None` se il file non comincia con una riga `---` o se nessuna riga
/// `---` o `...` la chiude.
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

/// La riga senza l'a capo e gli spazi in coda.
fn bare(line: &str) -> &str {
    line.trim_end_matches(['\n', '\r'])
        .trim_end_matches([' ', '\t'])
}

/// Se `block` (il sorgente di un blocco di `doc::blocks::split`) è un frontmatter. Solo il primo
/// blocco può esserlo: chi chiama controlla anche la posizione.
pub fn is_block(block: &str) -> bool {
    len(block) == Some(block.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_closed_block_at_byte_zero_is_frontmatter() {
        let src = "---\nname: RuneMD\ndescription: Lettore\n---\n\n# Titolo\n";
        assert_eq!(
            len(src),
            Some("---\nname: RuneMD\ndescription: Lettore\n---".len())
        );
        assert_eq!(
            len("---\na: 1\n...\ntesto\n"),
            Some("---\na: 1\n...".len()),
            "chiusura con ..."
        );
        assert_eq!(
            len("---\r\na: 1\r\n---\r\n"),
            Some("---\r\na: 1\r\n---".len()),
            "a capo CRLF"
        );
        assert_eq!(
            len("---\na: 1\n---"),
            Some(12),
            "file che finisce sulla chiusura"
        );
        assert_eq!(
            len("---  \na: 1\n---\t\n"),
            Some("---  \na: 1\n---\t".len()),
            "spazi in coda alle righe"
        );
    }

    #[test]
    fn without_a_closing_fence_it_is_not_frontmatter() {
        assert_eq!(len("---\nname: x\n\nUn paragrafo.\n"), None);
        assert_eq!(len("---\n"), None);
        assert_eq!(len("---"), None);
    }

    #[test]
    fn a_fence_not_at_the_start_is_markdown() {
        assert_eq!(len("\n---\na: 1\n---\n"), None, "riga vuota prima");
        assert_eq!(len(" ---\na: 1\n---\n"), None, "spazio prima");
        assert_eq!(
            len("# T\n\n---\na: 1\n---\n"),
            None,
            "più in basso: hr e setext"
        );
        assert_eq!(len("----\na: 1\n----\n"), None, "quattro trattini");
        assert_eq!(len("---a\nb\n---\n"), None);
        assert_eq!(len("---\na: 1\n--- x\n"), None, "chiusura con testo dopo");
    }

    #[test]
    fn a_block_is_frontmatter_only_when_it_is_all_of_it() {
        assert!(is_block("---\na: 1\n---"));
        assert!(!is_block("---"));
        assert!(!is_block("---\na: 1\n---\n\ntesto"));
    }
}
