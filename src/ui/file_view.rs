//! The views of files that are not Markdown (decision 2026-11-23-open-non-markdown): the file's
//! title over its code card, JSON viewer or image (mockup `codice`, `.dochead`).

use std::path::Path;

use rune::{Color, El, TextStyle, col};

/// Title (mockup `.dochead h1`: 28/680) and meta line (`.meta`: 12/500 in `fg3`, 16 apart).
const TITLE: f32 = 28.0;
const TITLE_WEIGHT: f32 = 680.0;
const META: f32 = 12.0;
const META_WEIGHT: f32 = 500.0;
const META_GAP: f32 = 16.0;
/// Between the title and the meta line, and under the meta line (`.meta` margins 8 and 20).
const META_TOP: f32 = 8.0;
pub const HEAD_BOTTOM: f32 = 20.0;

/// The file's name in `fg`, then its folder, size and `extra` (lines, records) in `fg3`.
pub fn head(path: &Path, extra: Option<String>, fg: Color, fg3: Color) -> El {
    let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
    let title = rune::text(name).font(TextStyle { line_height: TITLE * 1.2, tracking: -0.015 * TITLE, ..TextStyle::new(TITLE, TITLE_WEIGHT) }).color(fg).wrap().w_full().selectable(true);
    let folder = path.parent().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned());
    let size = std::fs::metadata(path).ok().map(|m| crate::doc::paths::size(m.len()));
    let meta = rune::row().gap(META_GAP).mt(META_TOP).children(folder.into_iter().chain(size).chain(extra).map(|t| rune::text(t).font(TextStyle::new(META, META_WEIGHT)).color(fg3).selectable(false)));
    col().key("file-head").w_full().pb(HEAD_BOTTOM).child(title).child(meta)
}

/// "1 line", "377 lines".
pub fn lines(n: usize) -> String {
    if n == 1 { "1 line".to_owned() } else { format!("{n} lines") }
}
