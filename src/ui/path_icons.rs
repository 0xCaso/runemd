//! The icons of path links (decision 2026-11-23-t3-style-paths,
//! mockup `colori`): viewBox 16, stroke 1.25, round caps and joins, as the mockup's sprite.

use rune::{Color, Icon};

use crate::doc::paths::Kind;

macro_rules! stroked {
    ($id:ident, $name:literal, $body:literal) => {
        pub static $id: Icon = Icon { name: $name, view: 16.0, body: concat!(r#"<g fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round">"#, $body, "</g>") };
    };
}

stroked!(FOLDER, "runemd-path-dir", r#"<path d="M2 4.6c0-.9.7-1.6 1.6-1.6h2.6l1.5 1.6h4.7c.9 0 1.6.7 1.6 1.6v5.2c0 .9-.7 1.6-1.6 1.6H3.6c-.9 0-1.6-.7-1.6-1.6z"/>"#);
stroked!(JSON, "runemd-path-json", r#"<path d="M6 2.8h-.4c-.9 0-1.5.7-1.5 1.5v2.1L3 8l1.1 1.6v2.1c0 .8.6 1.5 1.5 1.5H6M10 2.8h.4c.9 0 1.5.7 1.5 1.5v2.1L13 8l-1.1 1.6v2.1c0 .8-.6 1.5-1.5 1.5H10"/>"#);
stroked!(RUST, "runemd-path-rs", r#"<circle cx="8" cy="8" r="4.3"/><circle cx="8" cy="8" r="1.6"/><path d="M8 1.8v1.9M8 12.3v1.9M1.8 8h1.9M12.3 8h1.9M3.6 3.6l1.3 1.3M11.1 11.1l1.3 1.3M3.6 12.4l1.3-1.3M11.1 4.9l1.3-1.3"/>"#);
stroked!(MARKDOWN, "runemd-path-md", r#"<rect x="1.6" y="3.6" width="12.8" height="8.8" rx="1.6"/><path d="M4.2 10V6l1.9 2 1.9-2v4M11.2 6v4M9.8 8.7l1.4 1.4 1.4-1.4"/>"#);
stroked!(IMAGE, "runemd-path-img", r#"<rect x="2" y="2.8" width="12" height="10.4" rx="1.6"/><circle cx="5.8" cy="6.3" r="1.1"/><path d="m2.4 11.6 3.4-3.2 2.4 2.2 2-1.8 3.4 3"/>"#);
stroked!(SHELL, "runemd-path-sh", r#"<rect x="1.8" y="2.8" width="12.4" height="10.4" rx="1.6"/><path d="m4.6 6.4 1.9 1.6-1.9 1.6M8.2 10h3"/>"#);
stroked!(FILE, "runemd-path-file", r#"<path d="M4 1.9h5l3 3v8c0 .7-.5 1.2-1.2 1.2H4c-.7 0-1.2-.5-1.2-1.2V3.1c0-.7.5-1.2 1.2-1.2z"/><path d="M8.8 2v3.1H12M5.4 8.2h5.2M5.4 10.8h3.6"/>"#);
stroked!(COPY, "runemd-copy", r#"<rect x="5.2" y="5.2" width="8.4" height="8.4" rx="1.6"/><path d="M10.8 5.2V3.6c0-.9-.7-1.6-1.6-1.6H4c-.9 0-1.6.7-1.6 1.6v5.2c0 .9.7 1.6 1.6 1.6h1.2"/>"#);
stroked!(WRAP, "runemd-wrap", r#"<path d="M2.5 4h11M2.5 8h9a2 2 0 0 1 0 4H8.5M10 10.4 8.4 12l1.6 1.6M2.5 12h3"/>"#);

/// The icon of a kind of path.
pub fn icon(kind: Kind) -> &'static Icon {
    match kind {
        Kind::Folder => &FOLDER,
        Kind::Json => &JSON,
        Kind::Rust => &RUST,
        Kind::Markdown => &MARKDOWN,
        Kind::Image => &IMAGE,
        Kind::Shell => &SHELL,
        Kind::Other => &FILE,
    }
}

/// The kind's color, T3 Code's (mockup `colori`): the only colors in the document
/// (DESIGN.md, the path icons exception).
pub fn color(kind: Kind, dark: bool) -> Color {
    let hex = match (kind, dark) {
        (Kind::Json, false) => "#b58a12",
        (Kind::Json, true) => "#f2c94c",
        (Kind::Rust, false) => "#c2621d",
        (Kind::Rust, true) => "#ffa359",
        (Kind::Markdown, false) => "#1a77c2",
        (Kind::Markdown, true) => "#69b1ff",
        (Kind::Image, false) => "#9b2fb2",
        (Kind::Image, true) => "#d568ea",
        (Kind::Shell, false) => "#16883a",
        (Kind::Shell, true) => "#5ecc71",
        (Kind::Folder, false) => "#5d6f8f",
        (Kind::Folder, true) => "#9fb3d6",
        (Kind::Other, false) => "#6f6f76",
        (Kind::Other, true) => "#adadb1",
    };
    rune::color::hex(hex).unwrap_or(Color::TRANSPARENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_a_color_in_both_themes() {
        for k in [Kind::Folder, Kind::Json, Kind::Rust, Kind::Markdown, Kind::Image, Kind::Shell, Kind::Other] {
            for dark in [false, true] {
                assert!(color(k, dark).a > 0.99, "{k:?} {dark}");
            }
        }
    }

}
