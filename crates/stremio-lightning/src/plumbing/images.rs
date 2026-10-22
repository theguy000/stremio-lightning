//! Posters, addon logos and the preview's logo and background: one cache keyed by URL, one
//! request per key, fetched in the background and cut down to size before it is handed to Slint.
//!
//! A key is a URL plus a suffix saying which size it wants (`#logo`, `#blur`, `#page`,
//! `#backdrop`). The suffix is part of the key because the same URL is wanted at more than one
//! size. Those sizes are made from each other rather than fetched again: the Discover panel's
//! blurred backdrop comes from the card's poster, and the details column's from the artwork the
//! page has just downloaded, so a picture is only ever put on the wire once.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

/// The size each kind of image is stored at, so a 4K poster is never held in memory.
pub(crate) const LOGO_SUFFIX: &str = "#logo";
/// Web: `MetaPreview .background-image { filter: blur(10px); opacity: 0.3 }`. Slint has no
/// filter, so the blur is painted here and the panel scales the result up.
pub(crate) const BLUR_SUFFIX: &str = "#blur";
/// Artwork that fills the window, so it must not be squeezed into a poster.
pub(crate) const PAGE_SUFFIX: &str = "#page";
/// Web: the details column's `backdrop-filter: blur(15px)` over the artwork behind it.
pub(crate) const BACKDROP_SUFFIX: &str = "#backdrop";

pub(crate) type Cache = Arc<Mutex<HashMap<String, SharedPixelBuffer<Rgba8Pixel>>>>;

#[derive(Clone)]
pub(crate) struct Images {
    client: reqwest::Client,
    cache: Cache,
    requested: Arc<Mutex<HashSet<String>>>,
    /// The runtime the fetches run on. Carried rather than assumed: a request can be made from the
    /// window's thread (a repaint asks for its own pictures), where there is no runtime context to
    /// `tokio::spawn` into.
    runtime: tokio::runtime::Handle,
}

impl Default for Images {
    fn default() -> Self {
        Self {
            // With no timeout a host that accepts the connection and then says nothing hangs the
            // fetch forever: the key stays "asked for", nothing is cached, no error is raised, and
            // the row is a grey box for the rest of the session. Measured on
            // `episodes.metahub.space`, which does exactly that from here.
            client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(25))
                .build()
                .unwrap_or_default(),
            cache: Default::default(),
            requested: Default::default(),
            runtime: tokio::runtime::Handle::current(),
        }
    }
}

impl Images {
    /// The image for a key, if it has arrived. Read while the window is being filled in.
    pub(crate) fn get(&self, key: &Option<String>) -> Option<Image> {
        key.as_ref()
            .and_then(|key| self.cache.lock().unwrap().get(key).cloned())
            .map(Image::from_rgba8)
    }

    /// Asks for a key. A key is only ever asked for once *while it is in flight, cached, or known
    /// to be missing*; when it arrives, `on_ready` runs on the tokio worker so the caller can
    /// repaint.
    ///
    /// A fetch that could work next time is forgotten rather than remembered as asked, so the next
    /// snapshot tries again -- keeping it asked was how a single hiccup on the image host left a
    /// row grey for the rest of the session. A 404 is the exception: it will 404 again, and asking
    /// on every snapshot hammers the host for nothing.
    pub(crate) fn request(&self, key: String, on_ready: impl FnOnce() + Send + 'static) {
        if !self.requested.lock().unwrap().insert(key.clone()) {
            return;
        }
        let (client, cache, requested) = (
            self.client.clone(),
            self.cache.clone(),
            self.requested.clone(),
        );
        self.runtime.clone().spawn(async move {
            // Another size of the same picture is here often enough to matter: the Discover panel
            // wants the poster it already shows blurred, and the details column wants the artwork
            // the page has just downloaded blurred. Making the second one is a resize and a blur
            // off the UI thread, where asking the host again was a whole extra download -- the
            // pause the panel used to open with.
            let made = tokio::task::spawn_blocking({
                let cache = cache.clone();
                let key = key.clone();
                move || derive(&cache, &key)
            })
            .await
            .ok()
            .flatten();
            let result = match made {
                Some(buffer) => Fetched::Ready(buffer),
                None => fetch(&client, &key).await,
            };
            match result {
                Fetched::Ready(buffer) => {
                    cache.lock().unwrap().insert(key, buffer);
                    on_ready();
                }
                Fetched::Missing => {}
                Fetched::Retry => {
                    requested.lock().unwrap().remove(&key);
                }
            }
        });
    }
}

/// What a fetch came back with. The difference matters: a missing picture is missing for good,
/// while a timeout or a 5xx may well work on the next snapshot.
enum Fetched {
    Ready(SharedPixelBuffer<Rgba8Pixel>),
    /// The host says it has no such picture.
    Missing,
    /// Anything that could have gone differently.
    Retry,
}

/// How hard the web blurs a picture, in the pixels of the picture itself: a CSS `blur(10px)` over
/// a 29rem panel is 10px across ~485px of poster, and `blur(15px)` across the ~1090px the details
/// column's backdrop is drawn at. Both are applied at the size the picture is *stored* at, which
/// is what makes the result look the same at any window size.
const BLUR_SIGMA: f32 = 6.0;
const BACKDROP_SIGMA: f32 = 7.0;

/// The URL, the box a key wants its picture cut to, and the blur it is stored through.
fn source(key: &str) -> (&str, (u32, u32), Option<f32>) {
    if let Some(url) = key.strip_suffix(LOGO_SUFFIX) {
        (url, (672, 168), None)
    } else if let Some(url) = key.strip_suffix(BLUR_SUFFIX) {
        (url, (300, 450), Some(BLUR_SIGMA))
    } else if let Some(url) = key.strip_suffix(BACKDROP_SUFFIX) {
        (url, (480, 270), Some(BACKDROP_SIGMA))
    } else if let Some(url) = key.strip_suffix(PAGE_SUFFIX) {
        (url, (1920, 1080), None)
    } else {
        (key, (300, 450), None)
    }
}

/// The blurred copy of a page-size artwork that the details column paints under itself.
pub(crate) fn backdrop_key(key: &str) -> String {
    key.strip_suffix(PAGE_SUFFIX)
        .map_or_else(|| key.to_owned(), |url| format!("{url}{BACKDROP_SUFFIX}"))
}

/// A key whose picture is stored through a blur, so it carries no detail worth preserving.
fn is_blurred(key: &str) -> bool {
    key.ends_with(BLUR_SUFFIX) || key.ends_with(BACKDROP_SUFFIX)
}

/// The pixels of a buffer as a picture the `image` crate can resize and blur.
fn to_image(buffer: &SharedPixelBuffer<Rgba8Pixel>) -> Option<image::RgbaImage> {
    let bytes: Vec<u8> = buffer
        .as_slice()
        .iter()
        .flat_map(|pixel| [pixel.r, pixel.g, pixel.b, pixel.a])
        .collect();
    image::RgbaImage::from_raw(buffer.width(), buffer.height(), bytes)
}

/// Cuts a picture down to a key's box and blurs it if the web blurs it.
fn prepare(
    buffer: &SharedPixelBuffer<Rgba8Pixel>,
    (width, height): (u32, u32),
    blur: Option<f32>,
) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    if blur.is_none() && (buffer.width(), buffer.height()) == (width, height) {
        return Some(buffer.clone());
    }
    let image = to_image(buffer)?;
    let image = if (image.width(), image.height()) == (width, height) {
        image
    } else {
        image::imageops::thumbnail(&image, width, height)
    };
    let image = match blur {
        Some(sigma) => image::imageops::blur(&image, sigma),
        None => image,
    };
    Some(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        image.as_raw(),
        image.width(),
        image.height(),
    ))
}

/// Makes the key's picture from one already here, when there is one to make it from.
///
/// Only an unblurred picture of the same URL can be a source, and only one at least as big --
/// except for a key that wants a blur, where a smaller source is fine because a blur is all that
/// is wanted and scaling a soft picture up hides the difference.
fn derive(cache: &Cache, key: &str) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
    let (url, size, blur) = source(key);
    // Cloned out of the cache so the lock is not held through the resize: the window's thread
    // reads it on every repaint.
    let source = {
        let cache = cache.lock().unwrap();
        cache
            .iter()
            .filter(|(other, _)| !is_blurred(other) && source(other).0 == url)
            .map(|(_, buffer)| buffer)
            .filter(|buffer| {
                blur.is_some() || (buffer.width() >= size.0 && buffer.height() >= size.1)
            })
            .max_by_key(|buffer| buffer.width() * buffer.height())
            .cloned()
    }?;
    prepare(&source, size, blur)
}

async fn fetch(client: &reqwest::Client, key: &str) -> Fetched {
    let (url, size, blur) = source(key);
    let response = match client.get(url).send().await {
        Ok(response) => response,
        Err(error) => {
            eprintln!("[image] {url}: {error}");
            return Fetched::Retry;
        }
    };
    let response = match response.error_for_status() {
        Ok(response) => response,
        Err(error) => {
            eprintln!("[image] {url}: {error}");
            let missing = error
                .status()
                .is_some_and(|status| status.is_client_error());
            return if missing {
                Fetched::Missing
            } else {
                Fetched::Retry
            };
        }
    };
    let bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("[image] {url}: {error}");
            return Fetched::Retry;
        }
    };
    let decoded = tokio::task::spawn_blocking(move || {
        let image = image::load_from_memory(&bytes)
            .map_err(|error| eprintln!("[image] decode failed: {error}"))
            .ok()?
            .thumbnail(size.0, size.1)
            .to_rgba8();
        let image = match blur {
            Some(sigma) => image::imageops::blur(&image, sigma),
            None => image,
        };
        Some(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
            image.as_raw(),
            image.width(),
            image.height(),
        ))
    })
    .await
    .ok()
    .flatten();
    match decoded {
        Some(buffer) => Fetched::Ready(buffer),
        None => Fetched::Retry,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_says_which_size_it_wants() {
        assert_eq!(
            source("https://example.com/a.jpg#logo"),
            ("https://example.com/a.jpg", (672, 168), None)
        );
        assert_eq!(
            source("https://example.com/a.jpg#blur"),
            ("https://example.com/a.jpg", (300, 450), Some(BLUR_SIGMA))
        );
        assert_eq!(
            source("https://example.com/a.jpg#backdrop"),
            ("https://example.com/a.jpg", (480, 270), Some(BACKDROP_SIGMA))
        );
        assert_eq!(
            source("https://example.com/a.jpg"),
            ("https://example.com/a.jpg", (300, 450), None)
        );
    }

    #[test]
    fn the_column_backdrop_is_the_page_artwork_blurred() {
        assert_eq!(
            backdrop_key("https://example.com/a.jpg#page"),
            "https://example.com/a.jpg#backdrop"
        );
        // A key that is not page-sized has no column backdrop of its own.
        assert_eq!(
            backdrop_key("https://example.com/a.jpg#blur"),
            "https://example.com/a.jpg#blur"
        );
    }

    #[test]
    fn a_blur_can_be_made_from_the_poster_that_is_already_here() {
        let cache: Cache = Default::default();
        cache.lock().unwrap().insert(
            "https://example.com/a.jpg".to_owned(),
            SharedPixelBuffer::<Rgba8Pixel>::new(300, 450),
        );
        let blurred = derive(&cache, "https://example.com/a.jpg#blur").unwrap();
        assert_eq!((blurred.width(), blurred.height()), (300, 450));
    }

    #[test]
    fn a_bigger_picture_is_not_made_from_a_smaller_one() {
        let cache: Cache = Default::default();
        cache.lock().unwrap().insert(
            "https://example.com/a.jpg#blur".to_owned(),
            SharedPixelBuffer::<Rgba8Pixel>::new(300, 450),
        );
        // The page wants the real artwork and only a blurred copy of it is here: that would be a
        // small picture blown up, so it has to be fetched.
        assert!(derive(&cache, "https://example.com/a.jpg#page").is_none());
        // Another picture entirely is never a source.
        assert!(derive(&cache, "https://example.com/b.jpg#page").is_none());
    }
}
