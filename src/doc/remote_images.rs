//! Images a document points to on the internet (`https://…`, a README's badges), loaded always
//! (docs/decisions/2026-11-24-remote-images.md). Rune has no HTTP client on native, so each URL is
//! fetched once, in the background, with the system `curl`; the bytes decode on the main thread
//! through the image cache (an SVG, as a badge is, at its own size at 2 pixels per point), and the
//! waker redraws when they arrive. A failed fetch stays failed
//! until the app quits: the image shows its broken-image glyph and alt text.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use rune::{Image, ImageCache, Waker};

/// The largest image fetched: 20 MB, as a browser would still show it.
const MAX_BYTES: &str = "20000000";
/// A fetch that has not finished in 20 s gives up.
const TIMEOUT_S: &str = "20";

/// What a URL's fetch has come to.
enum Slot {
    Pending,
    Bytes(Arc<[u8]>),
    Failed,
}

/// Fetches the bytes at a URL, or `None` on any failure.
type Fetch = fn(&str) -> Option<Vec<u8>>;

pub struct RemoteImages {
    slots: Arc<Mutex<HashMap<String, Slot>>>,
    /// Fetches finished so far, ever: with [`RemoteImages::ready`] it only grows, so the rows build again
    /// at each arrival and at each decode.
    finished: Arc<AtomicUsize>,
    images: RefCell<HashMap<String, (Image, f32)>>,
    waker: Option<Waker>,
    fetch: Fetch,
}

impl Default for RemoteImages {
    fn default() -> Self {
        Self { slots: Default::default(), finished: Default::default(), images: Default::default(), waker: None, fetch: curl }
    }
}

/// Whether `src` names an image on the internet.
pub fn is_remote(src: &str) -> bool {
    let lower = src.trim_start().get(..8).unwrap_or(src).to_ascii_lowercase();
    lower.starts_with("https://") || lower.starts_with("http://")
}

impl RemoteImages {
    /// The waker that redraws when a fetch finishes (`AppSpec::setup`). Without one nothing is fetched.
    pub fn set_waker(&mut self, waker: Waker) {
        self.waker = Some(waker);
    }

    /// The picture at `url` and its pixels per point: `None` while it is fetched (the first call
    /// starts the fetch) or when it failed.
    pub fn get(&self, url: &str, cache: &ImageCache) -> Option<(Image, f32)> {
        if let Some(picture) = self.images.borrow().get(url) {
            return Some(picture.clone());
        }
        let waker = self.waker.clone()?;
        let mut slots = self.slots.lock().unwrap();
        match slots.get(url) {
            None => {
                slots.insert(url.to_owned(), Slot::Pending);
                let (slots, finished, url, fetch) = (self.slots.clone(), self.finished.clone(), url.to_owned(), self.fetch);
                std::thread::spawn(move || {
                    let slot = fetch(&url).filter(|b| !b.is_empty()).map_or(Slot::Failed, |b| Slot::Bytes(b.into()));
                    slots.lock().unwrap().insert(url, slot);
                    finished.fetch_add(1, Ordering::SeqCst);
                    waker();
                });
                None
            }
            Some(Slot::Bytes(bytes)) => {
                let picture = match svg_text(bytes) {
                    Some(svg) => (cache.svg(bytes.clone(), crate::ui::file_view::svg_side(svg), 2.0), 2.0),
                    None => (cache.load(bytes.clone()), 1.0),
                };
                slots.remove(url);
                self.images.borrow_mut().insert(url.to_owned(), picture.clone());
                Some(picture)
            }
            Some(Slot::Pending | Slot::Failed) => None,
        }
    }

    /// Grows at each finished fetch and each decoded picture: the rows build again when it changes,
    /// to take the bytes that arrived and to draw the pictures that decoded.
    pub fn progress(&self) -> usize {
        self.finished.load(Ordering::SeqCst) + self.images.borrow().values().filter(|(i, _)| i.is_ready()).count()
    }
}

/// The bytes as SVG text, when they are an SVG (a server may send one as any type).
fn svg_text(bytes: &[u8]) -> Option<&str> {
    let text = std::str::from_utf8(bytes).ok()?;
    let head = text.trim_start_matches('\u{feff}').trim_start();
    (head.starts_with('<') && text.get(..text.len().min(4096)).unwrap_or(text).contains("<svg")).then_some(text)
}

/// `curl` from macOS, https and http only, following redirects, failing on HTTP errors.
fn curl(url: &str) -> Option<Vec<u8>> {
    let out = std::process::Command::new("/usr/bin/curl")
        .args(["-sfL", "--proto", "=https,http", "--proto-redir", "=https,http", "--max-time", TIMEOUT_S, "--max-filesize", MAX_BYTES, "-A", "RuneMD", "--url", url])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;
    /// A 1×1 PNG.
    const PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1f, 0x15, 0xc4, 0x89, 0, 0, 0, 0x0d,
        0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0, 0x1f, 0, 0x05, 0, 0x01, 0xff, 0x89, 0x99, 0x3d, 0x1d, 0, 0, 0, 0, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60,
        0x82,
    ];

    fn wait_for(r: &RemoteImages, n: usize) {
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while r.finished.load(Ordering::SeqCst) < n {
            assert!(std::time::Instant::now() < until, "the fetch never finished");
            std::thread::yield_now();
        }
    }

    #[test]
    fn remote_sources_are_http_and_https_only() {
        assert!(is_remote("https://img.shields.io/badge/x.svg"));
        assert!(is_remote("HTTP://example.com/a.png"));
        assert!(!is_remote("docs/images/a.png"));
        assert!(!is_remote("file:///a.png"));
        assert!(!is_remote("/abs/a.png"));
    }

    #[test]
    fn a_url_is_fetched_once_then_decodes_and_a_failure_stays_failed() {
        static WAKES: AtomicUsize = AtomicUsize::new(0);
        let cache = ImageCache::new(Arc::new(|| {}));
        let mut r = RemoteImages { fetch: |url| url.ends_with("ok.png").then(|| PNG.to_vec()), ..Default::default() };
        r.set_waker(Arc::new(|| {
            WAKES.fetch_add(1, Ordering::SeqCst);
        }));

        assert!(r.get("https://x/ok.png", &cache).is_none(), "pending at first");
        assert!(r.get("https://x/bad.png", &cache).is_none());
        wait_for(&r, 2);
        assert_eq!(WAKES.load(Ordering::SeqCst), 2, "each finished fetch redraws");

        let before = r.progress();
        assert_eq!(r.get("https://x/ok.png", &cache).map(|(_, density)| density), Some(1.0), "the fetched bytes go to the cache");
        assert!(r.progress() >= before, "taking the bytes never lowers the progress");
        assert!(r.get("https://x/ok.png", &cache).is_some(), "kept");
        assert!(r.get("https://x/bad.png", &cache).is_none(), "a failure is not fetched again");
        assert_eq!(WAKES.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn an_svg_is_known_by_its_text() {
        assert!(svg_text(br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg" width="90" height="20"></svg>"#).is_some());
        assert!(svg_text(b"\n<svg width='9'/>").is_some());
        assert!(svg_text(PNG).is_none());
        assert!(svg_text(b"<html>no picture</html>").is_none());
    }

    #[test]
    fn without_a_waker_nothing_is_fetched() {
        let cache = ImageCache::new(Arc::new(|| {}));
        let r = RemoteImages { fetch: |_| panic!("no fetch without a waker"), ..Default::default() };
        assert!(r.get("https://x/a.png", &cache).is_none());
        assert_eq!(r.progress(), 0);
    }
}
