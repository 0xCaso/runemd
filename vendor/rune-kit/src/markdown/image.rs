//! Images (`![alt](src)`), as `markdown.css` draws them: at their own size up to the text's
//! width, rounded 4, 24 above and below; until the app has the picture (or when it never will),
//! Chromium's broken-image glyph and the alt text.

use rune::{Image, Pixels};

use super::*;

/// The picture an app has for an image's source.
pub(super) type ImageWire = Rc<dyn Fn(&str) -> Option<Image>>;
/// Device pixels per point of the picture an app has for a source.
pub(super) type ImageDensity = Rc<dyn Fn(&str) -> f32>;
/// Wraps a shown image (see [`Markdown::image_frame`]).
pub(super) type ImagePress = Rc<dyn Fn(El, &ImageFrame<'_>) -> El>;

/// An image [`Markdown::image_frame`] is wrapping.
#[non_exhaustive]
pub struct ImageFrame<'a> {
    /// Its source as written.
    pub src: &'a str,
    /// Its alt text.
    pub alt: &'a str,
    /// The picture it shows.
    pub image: &'a Image,
    /// The widest it draws, in points: an HTML `<img>`'s `width`, else the picture's own size.
    pub width: f32,
    /// It came from an HTML `<img>`: its alt text describes the picture, as on GitHub, and is no caption.
    pub html: bool,
    /// The destination of the link it is all of (a badge's), which a press on it should open.
    pub href: Option<&'a str>,
    /// It shares its line with other images (a row of badges): it draws at its width, not the text's.
    pub in_line: bool,
}

/// Chromium's 16 point broken-image glyph, as straight-alpha RGBA.
const BROKEN: &[u8; 1024] = include_bytes!("broken-image.rgba");

thread_local! {
    static BROKEN_PIXELS: Pixels = {
        let p = Pixels::new();
        p.set(16, 16, BROKEN.to_vec());
        p
    };
}

impl Markdown {
    /// The picture for an image's source as written (`![alt](src)`): a decoded or decoding
    /// [`Image`] from the app's cache, or `None` when it has none. Until it decodes, and without
    /// one, the image shows Chromium's broken-image glyph and its alt text, as a web page does
    /// for an `<img>` with no picture. Without this, every image does.
    pub fn images(mut self, f: impl Fn(&str) -> Option<Image> + 'static) -> Self {
        self.images = Some(Rc::new(f));
        self
    }

    /// How many of a picture's pixels make one point, by source (`f(src)`): an SVG the app
    /// rasterized at 2 pixels per point draws at its own size, not twice it. Without this, or
    /// for a source it gives 0 or less, a pixel is a point, as a browser draws a PNG at 1x.
    pub fn image_density(mut self, f: impl Fn(&str) -> f32 + 'static) -> Self {
        self.image_density = Some(Rc::new(f));
        self
    }

    /// Draws `f(image, frame)` in place of each image that shows its picture, so an app can
    /// open it larger on a press (`markdown.css`'s `cursor: zoom-in`, a button named by its alt
    /// text) or key it for a flight into a viewer.
    pub fn image_frame(mut self, f: impl Fn(El, &ImageFrame<'_>) -> El + 'static) -> Self {
        self.on_image = Some(Rc::new(f));
        self
    }
}

impl Renderer {
    /// An image block and the images joined to its line after it (a row of badges), wrapping
    /// as words do; centred when its HTML wrapper centres it.
    pub(super) fn images_line(&self, blocks: &[&Block]) -> El {
        let mut center = false;
        let drawn: Vec<El> = blocks.iter().filter_map(|b| match b {
            Block::Image { src, alt, title, width, center: c, html, href, .. } => {
                center |= *c;
                Some(self.image(src, alt, title, *width, *html, href.as_deref(), blocks.len() > 1))
            }
            _ => None,
        }).collect();
        match drawn.len() {
            1 => centred(drawn.into_iter().next().unwrap_or_else(el), center),
            _ => {
                let line = drawn.into_iter().fold(row().w_full().flex_wrap().gap(4.0), El::child);
                if center { line.justify_center() } else { line }
            }
        }
    }

    /// One image: `width` is an HTML `<img>`'s `width` attribute, `html` whether it is an
    /// `<img>`, `href` the link it is all of; `in_line` when it shares its line with others.
    fn image(&self, src: &str, alt: &str, title: &str, width: Option<f32>, html: bool, href: Option<&str>, in_line: bool) -> El {
        let shown = self.images.as_ref().and_then(|f| f(src)).filter(|i| i.is_ready() && i.error().is_none()).and_then(|i| Some((i.size()?, i)));
        let Some(([w, _], picture)) = shown else { return self.broken(alt, in_line) };
        // Its `width`, or its own size in points as a browser draws an image's pixels at 1x; up to
        // the text's width.
        let density = self.image_density.as_ref().map_or(1.0, |f| f(src));
        let width = width.unwrap_or(w as f32 / if density > 0.0 { density } else { 1.0 });
        let e = el().role("image").label(if alt.is_empty() { "Image preview" } else { alt });
        // In a line, at its width (a wider one still keeps to the text's); alone, the text's width at most.
        let e = if in_line { e.w(width).shrink() } else { e.w_full().max_w(width) };
        let mut e = e.pixels(picture.pixels()).radius(4.0).cursor(if href.is_some() { Cursor::Pointer } else { Cursor::ZoomIn });
        if !title.is_empty() {
            e = e.help(title);
        }
        let e = match &self.on_image {
            Some(frame) => frame(e, &ImageFrame { src, alt, image: &picture, width, html, href, in_line }),
            None => e,
        };
        e
    }

    /// An image without its picture: the broken-image glyph and the alt text after it, one
    /// line tall; nothing for an image with no alt text, as a browser leaves it out.
    fn broken(&self, alt: &str, in_line: bool) -> El {
        if alt.is_empty() {
            return if in_line { el().h(0.0) } else { el().w_full().h(0.0) };
        }
        let glyph = BROKEN_PIXELS.with(|p| el().size(16.0, 16.0).pixels(p).a11y_hidden());
        let ink = self.flow.get().ink.unwrap_or(self.s.text);
        let e = row().role("image").label(alt);
        let e = if in_line { e } else { e.w_full() };
        e.cursor(Cursor::ZoomIn).child(glyph).child(self.with_reveal(text(alt.to_owned()).font(self.s.body).color(ink)).shrink())
    }
}

/// `e` in the middle of the text's width when `center`.
fn centred(e: El, center: bool) -> El {
    if center { row().w_full().justify_center().child(e) } else { e }
}
