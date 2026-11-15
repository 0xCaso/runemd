//! Percorsi e nomi di file nel testo (record 2026-11-16-paths-name-and-link): dove sono in un
//! blocco, con o senza backtick, e a quale file sul disco portano.
//!
//! Il riconoscimento è prudente (vincolo 5): un percorso finisce con un nome di file con
//! un'estensione che ha almeno una lettera (`v1.2`, `0.5`, `ecc.` no); senza `/` vale solo con
//! un'estensione nota e non come nome di libreria (`Node.js`). Gli URL, i link, i link
//! automatici, l'HTML e i blocchi di codice non si toccano (vincolo 4).

use std::ops::Range;
use std::path::{Path, PathBuf};

/// Un percorso trovato in un blocco.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// I byte del sorgente che copre, backtick compresi.
    pub range: Range<usize>,
    /// Il percorso come è scritto, senza `:riga`.
    pub path: String,
    /// La riga di `:riga`, se c'è.
    pub line: Option<u32>,
}

impl Found {
    /// Il percorso diviso in cartelle (con l'ultima `/`) e nome del file.
    pub fn parts(&self) -> (&str, &str) {
        match self.path.rfind('/') {
            Some(i) => self.path.split_at(i + 1),
            None => ("", &self.path),
        }
    }
}

/// Le estensioni con cui un nome senza `/` è un file.
const KNOWN: &[&str] = &[
    "md", "markdown", "mdx", "txt", "rs", "toml", "lock", "json", "jsonl", "yaml", "yml", "sh", "zsh", "py", "rb", "go", "c", "h", "cc", "cpp", "hpp", "swift", "kt", "java", "js", "mjs", "cjs", "ts", "tsx", "jsx", "css", "scss", "html", "svg", "png", "jpg", "jpeg", "gif", "webp", "pdf", "csv", "sql", "xml", "plist", "wgsl", "metal", "env", "ini", "cfg", "conf", "log", "diff", "patch",
];

/// I percorsi di `source`, un blocco top-level, in ordine. Niente nei blocchi di codice e
/// nell'HTML.
pub fn find(source: &str) -> Vec<Found> {
    let first = source.trim_start_matches([' ', '\n']);
    if first.starts_with("```") || first.starts_with("~~~") || first.starts_with('<') || source.starts_with("    ") || source.starts_with('\t') {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut start = 0;
    for line in source.split_inclusive('\n') {
        if !is_reference(line) {
            in_line(line, start, &mut out);
        }
        start += line.len();
    }
    out
}

/// Una definizione di riferimento `[x]: url`: non si tocca.
fn is_reference(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with('[') && t.find("]:").is_some()
}

fn in_line(line: &str, at: usize, out: &mut Vec<Found>) {
    let b = line.as_bytes();
    let mut i = 0;
    let mut word = None;
    let flush = |word: &mut Option<usize>, end: usize, out: &mut Vec<Found>| {
        if let Some(s) = word.take()
            && let Some((r, path, line_no)) = token(&line[s..end])
        {
            out.push(Found { range: at + s + r.start..at + s + r.end, path, line: line_no });
        }
    };
    while i < b.len() {
        let c = b[i];
        match c {
            b'`' => {
                flush(&mut word, i, out);
                let run = b[i..].iter().take_while(|&&x| x == b'`').count();
                let fence = &line[i..i + run];
                match line[i + run..].find(fence) {
                    Some(j) => {
                        let inner = &line[i + run..i + run + j];
                        if let Some((r, path, line_no)) = token(inner.trim())
                            && r.len() == inner.trim().len()
                        {
                            out.push(Found { range: at + i..at + i + run + j + run, path, line: line_no });
                        }
                        i += run + j + run;
                    }
                    None => i += run,
                }
                continue;
            }
            // Un link (`[testo](destinazione)`, anche immagine): resta com'è.
            b'[' => {
                flush(&mut word, i, out);
                i = skip_link(line, i);
                continue;
            }
            // Un link automatico o HTML in linea.
            b'<' => {
                flush(&mut word, i, out);
                i = line[i..].find('>').map_or(i + 1, |j| i + j + 1);
                continue;
            }
            b' ' | b'\t' | b'\n' | b'\r' | b'|' => flush(&mut word, i, out),
            _ => {
                if word.is_none() {
                    word = Some(i);
                }
            }
        }
        i += 1;
    }
    flush(&mut word, b.len(), out);
}

/// Dopo il link che comincia in `i` (`[`), o subito dopo `[` se non è un link.
fn skip_link(line: &str, i: usize) -> usize {
    let Some(close) = line[i..].find("](") else { return i + 1 };
    let dest = i + close + 2;
    match line[dest..].find(')') {
        Some(j) => dest + j + 1,
        None => i + 1,
    }
}

/// Il percorso dentro una parola: i byte che copre (senza la punteggiatura intorno, con `:riga`),
/// il percorso e la riga.
fn token(word: &str) -> Option<(Range<usize>, String, Option<u32>)> {
    if word.contains("://") || word.to_ascii_lowercase().starts_with("www.") {
        return None;
    }
    let lead = word.len() - word.trim_start_matches(['(', '"', '\'', '*', '“', '«']).len();
    let mut end = word.len();
    while let Some(c) = word[lead..end].chars().next_back() {
        if matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | ')' | '"' | '\'' | '*' | '”' | '»') {
            end -= c.len_utf8();
        } else {
            break;
        }
    }
    let body = &word[lead..end];
    let (core, line) = match body.rsplit_once(':') {
        Some((p, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => (p, n.parse().ok()),
        _ => (body, None),
    };
    is_path(core).then(|| (lead..end, core.to_owned(), line))
}

/// `s` è un percorso o un nome di file.
fn is_path(s: &str) -> bool {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/' | b'~' | b'+' | b'@')) {
        return false;
    }
    if s.contains("//") || (s.contains('~') && !s.starts_with("~/")) || s[1..].contains('~') {
        return false;
    }
    let name = s.rsplit('/').next().unwrap_or(s);
    let Some((stem, ext)) = name.rsplit_once('.') else { return false };
    let stem_ok = stem.bytes().any(|b| b.is_ascii_alphanumeric());
    let ext_ok = (1..=8).contains(&ext.len()) && ext.bytes().all(|b| b.is_ascii_alphanumeric()) && ext.bytes().any(|b| b.is_ascii_alphabetic());
    if !stem_ok || !ext_ok {
        return false;
    }
    if s.contains('/') {
        return true;
    }
    // Senza `/`: un'estensione nota, un nome con almeno una lettera, e non un nome di libreria
    // con la maiuscola (`Node.js`, `Vue.js`).
    let lib = stem.chars().next().is_some_and(char::is_uppercase) && stem.chars().all(char::is_alphabetic) && matches!(ext, "js" | "ts");
    KNOWN.contains(&ext) && stem.bytes().any(|b| b.is_ascii_alphabetic()) && !lib
}

/// Dove si cercano i percorsi relativi: la cartella del file e la radice del suo repo.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Places {
    pub dir: PathBuf,
    pub root: Option<PathBuf>,
    pub home: Option<PathBuf>,
}

impl Places {
    /// I posti del file `file` (repo da `repo::of`, casa da `$HOME`).
    pub fn of(file: &Path) -> Places {
        let dir = file.parent().map(Path::to_path_buf).unwrap_or_default();
        let repo = crate::repo::of(file).root;
        let root = (repo != dir || dir.join(".git").exists()).then_some(repo);
        Places { dir, root, home: std::env::var_os("HOME").map(PathBuf::from) }
    }

    /// Il file a cui porta `path`: assoluto, `~/`, relativo alla cartella del file o alla
    /// radice del repo, in quest'ordine. `None` se non è un file che esiste.
    pub fn resolve(&self, path: &str) -> Option<PathBuf> {
        let tries: Vec<PathBuf> = if let Some(rest) = path.strip_prefix("~/") {
            self.home.iter().map(|h| h.join(rest)).collect()
        } else if Path::new(path).is_absolute() {
            vec![PathBuf::from(path)]
        } else {
            std::iter::once(self.dir.join(path)).chain(self.root.iter().map(|r| r.join(path))).collect()
        };
        tries.into_iter().find(|p| p.is_file())
    }
}

/// `source` come si disegna (vincoli 1–3): un percorso a un file che esiste diventa un link
/// Markdown verso [`open_target`], cartelle e `:riga` come testo del link, il nome del file in
/// grassetto, senza backtick; uno che non esiste diventa codice in linea (monospaziato, non un
/// link: niente mano né clic), con cartelle e `:riga` in [`Shown::dims`]. `None` se non c'è
/// nessun percorso.
pub fn shown(source: &str, places: &Places) -> Option<Shown> {
    let found = find(source);
    if found.is_empty() {
        return None;
    }
    let esc = |t: &str| -> String {
        let mut o = String::with_capacity(t.len());
        for c in t.chars() {
            if super::breaks::needs_escape(c) {
                o.push('\\');
            }
            o.push(c);
        }
        o
    };
    let mut names = Vec::new();
    let mut dims = Vec::new();
    let mut missing = false;
    let mut shift = 0isize;
    let edits: Vec<(Range<usize>, String)> = found
        .iter()
        .map(|f| {
            let (dir, name) = f.parts();
            let line = f.line.map(|n| format!(":{n}")).unwrap_or_default();
            let at = (f.range.start as isize + shift) as usize;
            let text = match places.resolve(&f.path) {
                Some(file) => {
                    let head = format!("[{}**", esc(dir));
                    let name = esc(name);
                    names.push(at + head.len()..at + head.len() + name.len());
                    format!("{head}{name}**{line}](<{}>)", open_target(&file, f.line))
                }
                None => {
                    missing = true;
                    let n = at + 1 + dir.len();
                    dims.push(at + 1..n);
                    names.push(n..n + name.len());
                    dims.push(n + name.len()..n + name.len() + line.len());
                    format!("`{dir}{name}{line}`")
                }
            };
            shift += text.len() as isize - f.range.len() as isize;
            (f.range.clone(), text)
        })
        .collect();
    // Altro codice in linea nel blocco: allora i percorsi che non esistono tengono la pastiglia
    // del codice, che il kit dà a tutto il codice di un blocco.
    let mut rest = String::new();
    let mut from = 0;
    for f in &found {
        rest.push_str(&source[from..f.range.start]);
        from = f.range.end;
    }
    rest.push_str(&source[from..]);
    let bare = missing && !rest.contains('`');
    dims.retain(|r| !r.is_empty());
    Some(Shown { edited: super::footnotes::Edited::new(source, edits), names, dims, bare })
}

/// Il testo disegnato di un blocco con dei percorsi ([`shown`]).
#[derive(Debug)]
pub struct Shown {
    pub edited: super::footnotes::Edited,
    /// I byte del nome di ogni percorso nel testo nuovo, per il colore del nome.
    pub names: Vec<Range<usize>>,
    /// Cartelle e `:riga` dei percorsi che non esistono (codice in linea), in `fg-3`.
    pub dims: Vec<Range<usize>>,
    /// Il solo codice in linea del blocco sono percorsi che non esistono: la vista lo disegna
    /// senza pastiglia (vincolo 1).
    pub bare: bool,
}

/// La destinazione di un link a un file che esiste: `runemd-open:` più il percorso assoluto e,
/// se c'è, `#L` e la riga.
pub const OPEN: &str = "runemd-open:";

/// La destinazione per `file` alla riga `line`.
pub fn open_target(file: &Path, line: Option<u32>) -> String {
    match line {
        Some(n) => format!("{OPEN}{}#L{n}", file.display()),
        None => format!("{OPEN}{}", file.display()),
    }
}

/// Il file e la riga di una destinazione `runemd-open:`.
pub fn parse_target(url: &str) -> Option<(PathBuf, Option<u32>)> {
    let rest = url.strip_prefix(OPEN)?;
    match rest.rsplit_once("#L") {
        Some((p, n)) if n.bytes().all(|b| b.is_ascii_digit()) && !n.is_empty() => Some((PathBuf::from(p), n.parse().ok())),
        _ => Some((PathBuf::from(rest), None)),
    }
}

/// La riga `line` (da 1) di `text`, il file intero, cade nel blocco numero…: l'ultimo blocco che
/// comincia prima o su quella riga. `blocks` sono i sorgenti dei blocchi, in ordine, ognuno un
/// pezzo di `text`.
pub fn block_of_line<'a>(text: &str, blocks: impl Iterator<Item = &'a str>, line: u32) -> usize {
    let mut from = 0;
    let mut found = 0;
    for (i, b) in blocks.enumerate() {
        let Some(off) = text[from..].find(b).map(|o| from + o) else { continue };
        let first = text[..off].matches('\n').count() as u32 + 1;
        if first > line {
            break;
        }
        found = i;
        from = off + b.len();
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(s: &str) -> Vec<String> {
        find(s).into_iter().map(|f| f.path).collect()
    }

    #[test]
    fn paths_with_and_without_backticks_are_found() {
        let s = "Ho spostato il calcolo in src/live/below.rs:88 e aggiornato `src/ui/doc_view.rs`. La prova è in ~/Desktop/GitHub/runemd/docs/proof/fire-dark.png.";
        let f = find(s);
        assert_eq!(paths(s), ["src/live/below.rs", "src/ui/doc_view.rs", "~/Desktop/GitHub/runemd/docs/proof/fire-dark.png"]);
        assert_eq!(f[0].line, Some(88));
        assert_eq!(&s[f[0].range.clone()], "src/live/below.rs:88");
        assert_eq!(&s[f[1].range.clone()], "`src/ui/doc_view.rs`");
        assert_eq!(f[0].parts(), ("src/live/", "below.rs"));
    }

    #[test]
    fn bare_file_names_need_a_known_extension() {
        assert_eq!(paths("Modify: `Cargo.toml`, README.md e bench/fire_settings.sh, poi /tmp/a.log"), ["Cargo.toml", "README.md", "bench/fire_settings.sh", "/tmp/a.log"]);
        assert_eq!(paths("- Create: src/fx/fire.rs, src/live/below.rs"), ["src/fx/fire.rs", "src/live/below.rs"]);
        assert_eq!(paths("| src/fx/fire.rs | 412 |"), ["src/fx/fire.rs"]);
    }

    #[test]
    fn ordinary_words_with_a_dot_stay_text() {
        let s = "Restano testo: ecc., v1.2, 0.5 s, Node.js, Vue.js, e.g. questo, 3.5x, example.com e https://github.com/anomalyco/rune/blob/main/README.md (già un link), www.foo.dev/a.md.";
        assert_eq!(paths(s), Vec::<String>::new());
        assert!(paths("[il file](src/a.rs) e <src/b.rs> e ![x](img/c.png)").is_empty());
        assert!(paths("and/or km/h 1/2 a//b.rs ~x/a.rs").is_empty());
        assert!(paths("`cargo test --profile perf`").is_empty(), "codice con spazi non è un percorso");
    }

    #[test]
    fn code_blocks_and_references_are_left_alone() {
        assert!(paths("```\ncat src/live/below.rs\n```\n").is_empty());
        assert!(paths("    cat src/live/below.rs\n").is_empty());
        assert!(paths("[x]: docs/a.md\n").is_empty());
    }

    fn tree() -> (PathBuf, Places) {
        let t = std::env::temp_dir().join(format!("runemd-paths-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&t);
        std::fs::create_dir_all(t.join("repo/.git")).unwrap();
        std::fs::create_dir_all(t.join("repo/docs/plan")).unwrap();
        std::fs::create_dir_all(t.join("repo/src/live")).unwrap();
        std::fs::create_dir_all(t.join("home/notes")).unwrap();
        std::fs::write(t.join("repo/docs/plan/progress.md"), "x").unwrap();
        std::fs::write(t.join("repo/docs/plan/beside.md"), "x").unwrap();
        std::fs::write(t.join("repo/src/live/below.rs"), "x").unwrap();
        std::fs::write(t.join("home/notes/a.md"), "x").unwrap();
        let places = Places { home: Some(t.join("home")), ..Places::of(&t.join("repo/docs/plan/progress.md")) };
        (t, places)
    }

    #[test]
    fn paths_resolve_beside_the_file_in_the_repo_absolute_and_home() {
        let (t, p) = tree();
        assert_eq!(p.root.as_deref(), Some(t.join("repo").as_path()));
        assert_eq!(p.resolve("beside.md"), Some(t.join("repo/docs/plan/beside.md")));
        assert_eq!(p.resolve("src/live/below.rs"), Some(t.join("repo/src/live/below.rs")));
        let abs = t.join("repo/src/live/below.rs");
        assert_eq!(p.resolve(abs.to_str().unwrap()), Some(abs.clone()));
        assert_eq!(p.resolve("~/notes/a.md"), Some(t.join("home/notes/a.md")));
        assert_eq!(p.resolve("src/old/gone.rs"), None);
        assert_eq!(p.resolve("src/live"), None, "una cartella non è un file");
        std::fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn shown_text_makes_each_path_a_link_with_a_bold_name() {
        let (t, p) = tree();
        let s = "Vedi `src/live/below.rs` e src/old/fire_x.rs:3.";
        let sh = shown(s, &p).unwrap();
        let (e, names) = (&sh.edited, &sh.names);
        let file = t.join("repo/src/live/below.rs");
        assert_eq!(e.text, format!("Vedi [src/live/**below.rs**](<{OPEN}{}>) e `src/old/fire_x.rs:3`.", file.display()));
        let got: Vec<&str> = names.iter().map(|r| &e.text[r.clone()]).collect();
        assert_eq!(got, ["below.rs", "fire_x.rs"]);
        let dims: Vec<&str> = sh.dims.iter().map(|r| &e.text[r.clone()]).collect();
        assert_eq!(dims, ["src/old/", ":3"]);
        assert!(sh.bare, "nessun altro codice nel blocco");
        assert!(!shown("src/old/a.rs e `cargo test`", &p).unwrap().bare);
        assert!(shown("niente qui, ecc.", &p).is_none());
        std::fs::remove_dir_all(&t).unwrap();
    }

    #[test]
    fn targets_round_trip_with_the_line() {
        let t = open_target(Path::new("/a/b.rs"), Some(88));
        assert_eq!(parse_target(&t), Some((PathBuf::from("/a/b.rs"), Some(88))));
        assert_eq!(parse_target(&open_target(Path::new("/a/b.rs"), None)), Some((PathBuf::from("/a/b.rs"), None)));
        assert_eq!(parse_target("https://x.dev"), None);
    }

    #[test]
    fn a_line_falls_in_its_block() {
        let text = "# T\n\nuno\ndue\n\ntre\n";
        let blocks = ["# T", "uno\ndue", "tre"];
        assert_eq!(block_of_line(text, blocks.into_iter(), 1), 0);
        assert_eq!(block_of_line(text, blocks.into_iter(), 4), 1);
        assert_eq!(block_of_line(text, blocks.into_iter(), 5), 1);
        assert_eq!(block_of_line(text, blocks.into_iter(), 6), 2);
        assert_eq!(block_of_line(text, blocks.into_iter(), 99), 2);
    }
}
