//! `<img>` tags in a document's raw HTML, as GitHub shows them: a README's
//! `<p align="center"><img src="logo.png" width="420"></p>` draws the picture, centred, 420 wide.
//! Every other tag stays unshown, as before.

/// One `<img>` read out of raw HTML.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct HtmlImg {
    pub src: String,
    pub alt: String,
    pub title: String,
    /// The `width` attribute in points (`420` or `420px`); `None` for a percentage or none.
    pub width: Option<f32>,
    /// The `href` of the `<a>` it sits in, if any.
    pub href: Option<String>,
}

/// What an HTML block shows, in order: its pictures and the text between them.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Piece {
    Img(HtmlImg),
    /// Text with its tags left out, entities decoded and spaces collapsed; never empty.
    Text(String),
}

/// Every complete `<img …>` tag in `html`, in order; a tag still arriving (no `>`) is left out.
pub(super) fn imgs(html: &str) -> Vec<HtmlImg> {
    imgs_at(html).into_iter().map(|(_, img)| img).collect()
}

/// `html`'s pictures and the text around them, as GitHub shows an HTML block: a README's
/// `<p align="center">A tagline</p>` is its text. Comments, `<script>` and `<style>` stay unshown.
pub(super) fn pieces(html: &str) -> Vec<Piece> {
    let mut out = Vec::new();
    let mut at = 0;
    let text = |s: &str, out: &mut Vec<Piece>| {
        let t = visible_text(s);
        if !t.is_empty() {
            out.push(Piece::Text(t));
        }
    };
    for (range, img) in imgs_at(html) {
        text(&html[at..range.start], &mut out);
        out.push(Piece::Img(img));
        at = range.end;
    }
    text(&html[at..], &mut out);
    out
}

/// Each complete `<img>` with the bytes its tag spans, `<` to `>`.
fn imgs_at(html: &str) -> Vec<(std::ops::Range<usize>, HtmlImg)> {
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = lower[at..].find("<img") {
        let open = at + i;
        let start = open + 4;
        // `<imgx` is another tag.
        if lower[start..].chars().next().is_some_and(|c| !(c.is_whitespace() || c == '/' || c == '>')) {
            at = start;
            continue;
        }
        let Some(end) = tag_end(&html[start..]) else { break };
        if let Some(mut img) = img(&html[start..start + end]) {
            img.href = link_around(html, &lower, start);
            out.push((open..start + end + 1, img));
        }
        at = start + end + 1;
    }
    out
}

/// The text a browser shows for `html`: tags dropped, comments and `<script>`/`<style>` bodies
/// skipped, entities decoded, runs of white space one space. A tag still arriving is dropped.
fn visible_text(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut raw = String::new();
    let mut at = 0;
    while let Some(i) = lower[at..].find('<') {
        raw.push_str(&html[at..at + i]);
        let open = at + i;
        let rest = &lower[open..];
        at = if rest.starts_with("<!--") {
            rest.find("-->").map_or(html.len(), |e| open + e + 3)
        } else if let Some(name) = ["script", "style"].into_iter().find(|n| rest[1..].starts_with(n)) {
            rest.find(&format!("</{name}")).map_or(html.len(), |e| open + e).max(open + 1)
        } else {
            tag_end(&html[open + 1..]).map_or(html.len(), |e| open + 1 + e + 1)
        };
    }
    raw.push_str(&html[at..]);
    decode(&raw.replace("&nbsp;", " ")).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The `href` of the `<a>` still open at `at`.
fn link_around(html: &str, lower: &str, at: usize) -> Option<String> {
    let before = &lower[..at];
    let open = before.rfind("<a ").or_else(|| before.rfind("<a\n"))?;
    if before.rfind("</a").is_some_and(|close| close > open) {
        return None;
    }
    let end = tag_end(&html[open + 2..]).map_or(at, |e| open + 2 + e);
    parse_attrs(&html[open + 2..end]).into_iter().find(|(n, _)| n.eq_ignore_ascii_case("href")).map(|(_, v)| v).filter(|v| !v.trim().is_empty())
}

/// Whether `html` centres what it holds: `align="center"` on a wrapper, or `<center>`.
pub(super) fn centered(html: &str) -> bool {
    let lower = html.to_ascii_lowercase();
    lower.contains("<center") || attrs_of_tags(&lower).any(|(name, value)| name == "align" && value == "center")
}

/// The `<img` tag's attributes (the text between `<img` and `>`) as an image; `None` without a `src`.
fn img(attrs: &str) -> Option<HtmlImg> {
    let (mut src, mut alt, mut title, mut width) = (None, String::new(), String::new(), None);
    for (name, value) in parse_attrs(attrs) {
        match name.to_ascii_lowercase().as_str() {
            "src" => src = Some(value),
            "alt" => alt = value,
            "title" => title = value,
            "width" => width = points(&value),
            _ => {}
        }
    }
    let src = src.filter(|s| !s.trim().is_empty())?;
    Some(HtmlImg { src: src.trim().to_owned(), alt, title, width, href: None })
}

/// `420` or `420px` as points; a percentage or anything else is `None`.
fn points(v: &str) -> Option<f32> {
    let v = v.trim();
    let v = v.strip_suffix("px").unwrap_or(v);
    v.parse::<f32>().ok().filter(|w| w.is_finite() && *w > 0.0)
}

/// Where the tag that `s` is the inside of ends (its `>`), skipping `>` in quoted values.
fn tag_end(s: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in s.char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(i),
            _ => {}
        }
    }
    None
}

/// Every attribute of every tag in `html`.
fn attrs_of_tags(html: &str) -> impl Iterator<Item = (String, String)> + '_ {
    let mut at = 0;
    std::iter::from_fn(move || {
        let i = html[at..].find('<')?;
        let start = at + i + 1;
        let end = tag_end(&html[start..]).map_or(html.len(), |e| start + e);
        at = end.min(html.len());
        let inside = &html[start..end];
        let name_end = inside.find(|c: char| c.is_whitespace() || c == '/' || c == '>').unwrap_or(inside.len());
        Some(parse_attrs(&inside[name_end..]))
    })
    .flatten()
}

/// `name="value" name='value' name=value name` as (name, value) pairs, entities decoded.
fn parse_attrs(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = s;
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '/');
        if rest.is_empty() {
            return out;
        }
        let name_end = rest.find(|c: char| c.is_whitespace() || c == '=' || c == '/' || c == '>').unwrap_or(rest.len());
        if name_end == 0 {
            rest = &rest[1..];
            continue;
        }
        let name = rest[..name_end].to_owned();
        rest = rest[name_end..].trim_start();
        let Some(after) = rest.strip_prefix('=') else {
            out.push((name, String::new()));
            continue;
        };
        let after = after.trim_start();
        let (value, next) = match after.chars().next() {
            Some(q @ ('"' | '\'')) => match after[1..].find(q) {
                Some(e) => (&after[1..1 + e], &after[e + 2..]),
                None => (&after[1..], ""),
            },
            _ => {
                let e = after.find(|c: char| c.is_whitespace() || c == '>').unwrap_or(after.len());
                (&after[..e], &after[e..])
            }
        };
        out.push((name, decode(value)));
        rest = next;
    }
}

/// The five entities an attribute value meets most.
fn decode(v: &str) -> String {
    if !v.contains('&') {
        return v.to_owned();
    }
    v.replace("&quot;", "\"").replace("&#39;", "'").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_readme_logo_is_read_with_its_width_and_centred() {
        let html = r#"<p align="center"><img src="docs/images/logo.png" alt="RuneMD wordmark: a plate" width="439"></p>"#;
        assert_eq!(imgs(html), vec![HtmlImg { src: "docs/images/logo.png".into(), alt: "RuneMD wordmark: a plate".into(), title: String::new(), width: Some(439.0), href: None }]);
        assert!(centered(html));
    }

    #[test]
    fn quotes_bare_values_and_entities() {
        let got = imgs("<IMG SRC=a.png alt='x &amp; y' title=\"t > u\" width=50px/>");
        assert_eq!(got, vec![HtmlImg { src: "a.png".into(), alt: "x & y".into(), title: "t > u".into(), width: Some(50.0), href: None }]);
        assert_eq!(imgs(r#"<img src="b.png" width="50%">"#)[0].width, None);
    }

    #[test]
    fn several_images_a_tag_still_arriving_and_other_tags() {
        let got = imgs(r#"<div><img src="a.png"> <img src="b.png"><imgx src="c.png"> <img src="d.png""#);
        assert_eq!(got.iter().map(|i| i.src.as_str()).collect::<Vec<_>>(), vec!["a.png", "b.png"]);
        assert!(imgs(r#"<img alt="no source">"#).is_empty());
        assert!(!centered(r#"<p><img src="a.png"></p>"#));
        assert!(centered("<center><img src=a.png></center>"));
    }

    #[test]
    fn text_between_pictures_tags_comments_and_scripts() {
        let got = pieces("<p align=center>Hi &amp; <b>bye</b></p><!-- c --><img src=a.png><script>x()</script> tail <img src=b.png>");
        assert_eq!(got.iter().map(|p| match p { Piece::Img(i) => format!("img {}", i.src), Piece::Text(t) => t.clone() }).collect::<Vec<_>>(), vec!["Hi & bye", "img a.png", "tail", "img b.png"]);
        assert!(pieces("<!-- only a comment -->").is_empty());
    }

    #[test]
    fn an_image_in_a_link_takes_its_href() {
        let got = imgs(r#"<a href="https://x.dev"><img src="a.svg"></a> <img src="b.svg"> <a href='/c'>c</a><img src="d.svg">"#);
        assert_eq!(got.iter().map(|i| i.href.as_deref()).collect::<Vec<_>>(), vec![Some("https://x.dev"), None, None]);
    }
}
