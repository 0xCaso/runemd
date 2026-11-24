//! The views of files that are not Markdown (decision 2026-11-23-open-non-markdown): the file's
//! title over its code card, JSON viewer or image (mockup `codice`, `.dochead`).

use std::path::Path;

use rune::{Color, El, TextStyle, col, el};

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

/// An image's size in the view: its own size (1 image pixel to 1 point), smaller to fit `room`
/// wide, never larger.
pub fn fit(natural: [f32; 2], room: f32) -> [f32; 2] {
    if !(natural[0] > 0.0 && natural[1] > 0.0) {
        return [0.0, 0.0];
    }
    let k = (room.max(0.0) / natural[0]).min(1.0);
    [natural[0] * k, natural[1] * k]
}

/// An SVG's own size in points: its `width` and `height` (px or unitless), else its `viewBox`.
pub fn svg_size(svg: &str) -> Option<[f32; 2]> {
    let start = svg.find("<svg")?;
    let tag = &svg[start..start + svg[start..].find('>')?];
    let attr = |name: &str| -> Option<&str> {
        let mut rest = tag;
        loop {
            let at = rest.find(name)?;
            let before = rest[..at].chars().last();
            rest = &rest[at + name.len()..];
            let r = rest.trim_start();
            if before.is_some_and(char::is_whitespace) && let Some(r) = r.strip_prefix('=') {
                let r = r.trim_start();
                let q = r.chars().next()?;
                let r = &r[q.len_utf8()..];
                return Some(&r[..r.find(q)?]);
            }
        }
    };
    let num = |v: &str| v.trim().strip_suffix("px").unwrap_or(v.trim()).parse::<f32>().ok().filter(|n| *n > 0.0);
    if let (Some(w), Some(h)) = (attr("width").and_then(num), attr("height").and_then(num)) {
        return Some([w, h]);
    }
    let vb: Vec<f32> = attr("viewBox")?.split([' ', ',']).filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).collect();
    (vb.len() == 4 && vb[2] > 0.0 && vb[3] > 0.0).then(|| [vb[2], vb[3]])
}

/// The side an SVG is rasterized at, in points (at 2 pixels per point): its own largest side,
/// [`SVG_DEFAULT`] without one, kept between 16 and [`SVG_MAX`].
pub fn svg_side(svg: &str) -> f32 {
    svg_size(svg).map_or(SVG_DEFAULT, |[w, h]| w.max(h)).clamp(16.0, SVG_MAX)
}

/// The largest side an SVG is rasterized at, in points (at 2 pixels per point).
pub const SVG_MAX: f32 = 2048.0;
/// An SVG with no size of its own.
pub const SVG_DEFAULT: f32 = 512.0;

/// What an image tab shows (decision 2026-11-23-open-non-markdown): the file's title, then the
/// picture centred, fitted to the column, never past its own size; an error line when it cannot
/// be decoded. `picture` is the decoded image and its pixels per point; `room` the width
/// the picture has (from the last layout of `image-box`).
pub fn image_pane(path: &Path, picture: Option<(rune::Image, f32)>, room: Option<f32>, column: f32, theme: &super::theme::Theme) -> El {
    let natural = picture.as_ref().and_then(|(i, k)| i.size().map(|s| [s[0] as f32 / k, s[1] as f32 / k]));
    let dims = picture.as_ref().and_then(|(i, _)| i.size()).map(|[w, h]| format!("{w} × {h}"));
    let head = head(path, dims, theme.fg, theme.fg3);
    let line = |t: String| rune::text(t).font(TextStyle::new(super::theme::UI_TEXT, super::theme::UI_WEIGHT)).color(theme.fg2).wrap().w_full().selectable(true);
    let body = match (&picture, natural) {
        (None, _) => line("This image can't be shown here.".to_owned()),
        (Some((img, _)), _) if img.error().is_some() => line(format!("Can't show this image: {}", img.error().map(|e| e.0.as_str()).unwrap_or_default())).key("image-error"),
        (Some(_), None) => line("Loading…".to_owned()),
        (Some((img, _)), Some(n)) => {
            let [w, h] = fit(n, room.unwrap_or(n[0]));
            el().key("image").size(w, h).self_center().radius(IMAGE_RADIUS).clip_rounded(IMAGE_RADIUS).pixels(img.pixels())
        }
    };
    let page = super::theme::column(col().w_full(), column).self_center().px(super::theme::ROW_PAD_X).child(head).child(col().key("image-box").w_full().child(body));
    let scroll = col().key("doc").w_full().h_full().scroll(rune::ScrollSpec { stick_bottom: false }).child(col().w_full().pt(super::doc_view::TOP).pb(IMAGE_BOTTOM).child(page));
    el().key(format!("image-{}", path.display())).w_full().grow(1.0).min_h(0.0).child(scroll)
}

/// The image's corners (the document's pictures: 4).
const IMAGE_RADIUS: f32 = 4.0;
/// Room under the picture.
const IMAGE_BOTTOM: f32 = 40.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_fits_the_width_and_is_never_upscaled() {
        assert_eq!(fit([400.0, 300.0], 600.0), [400.0, 300.0], "smaller than the room: its own size");
        assert_eq!(fit([1200.0, 600.0], 600.0), [600.0, 300.0], "wider: fitted, same aspect");
        assert_eq!(fit([600.0, 900.0], 600.0), [600.0, 900.0]);
        assert_eq!(fit([0.0, 10.0], 600.0), [0.0, 0.0], "no size, nothing drawn");
        assert_eq!(fit([100.0, 50.0], -5.0), [0.0, 0.0]);
    }

    #[test]
    fn an_svg_says_its_size() {
        assert_eq!(svg_size(r#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" width="120" height="80px">"#), Some([120.0, 80.0]));
        assert_eq!(svg_size(r#"<svg viewBox="0 0 24 16" stroke-width="2">"#), Some([24.0, 16.0]));
        assert_eq!(svg_size(r#"<svg width="100%" height="100%" viewBox="0,0,300,150">"#), Some([300.0, 150.0]));
        assert_eq!(svg_size("<svg>"), None);
        assert_eq!(svg_size("not svg"), None);
    }
}
