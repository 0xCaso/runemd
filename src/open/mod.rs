//! Opening files: from Finder / "Open With" (`odoc`), from `open -a`, from the command line.

pub mod kind;
pub mod odoc;
pub mod queue;

use std::path::{Path, PathBuf};

/// Extensions RuneMD opens (the same as `CFBundleTypeExtensions`).
pub fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("md") || e.eq_ignore_ascii_case("markdown"))
}

/// From a `file://` URL (percent-encoded) to a path; `None` for other schemes or extensions.
pub fn markdown_path_from_url(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    // `file://localhost/...` e `file:///...`.
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    let decoded = percent_decode(rest)?;
    let path = PathBuf::from(decoded);
    (path.is_absolute() && is_markdown(&path)).then_some(path)
}

fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            let hex = s.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Call in `main` before `App::run`.
pub fn install() {
    odoc::install();
}

/// Command-line arguments that are Markdown files, or other files that exist (`runemd data.json`;
/// `App::open_paths` sends the ones the app does not draw to the system), relative paths made
/// absolute.
pub fn cli_paths(args: &[String]) -> Vec<PathBuf> {
    args.iter()
        .filter(|a| !a.starts_with('-'))
        .map(PathBuf::from)
        .filter(|p| is_markdown(p) || p.is_file())
        .map(|p| std::path::absolute(&p).unwrap_or(p))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_filters_scheme_and_extension() {
        assert_eq!(markdown_path_from_url("file:///tmp/a%20b.md"), Some(PathBuf::from("/tmp/a b.md")));
        assert_eq!(markdown_path_from_url("file://localhost/tmp/x.MARKDOWN"), Some(PathBuf::from("/tmp/x.MARKDOWN")));
        assert_eq!(markdown_path_from_url("file:///tmp/x.txt"), None);
        assert_eq!(markdown_path_from_url("https://example.com/x.md"), None);
        assert_eq!(markdown_path_from_url("file:///tmp/bad%zz.md"), None);
    }

    #[test]
    fn cli_keeps_markdown_and_existing_files() {
        let json = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/open-files/state.json");
        let got = cli_paths(&["/tmp/a.md".into(), "--flag".into(), "/tmp/missing-b.json".into(), json.into()]);
        assert_eq!(got, vec![PathBuf::from("/tmp/a.md"), PathBuf::from(json)]);
    }
}
