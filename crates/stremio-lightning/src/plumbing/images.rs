//! Posters, addon logos and the preview's logo and background: one cache keyed by URL, one
//! request per key, fetched in the background and resized before it is handed to Slint.
//!
//! A key is a URL plus a suffix saying which size it wants (`#logo`, `#blur`). The suffix is
//! part of the key because the same URL is fetched at more than one size.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use slint::{Image, Rgba8Pixel, SharedPixelBuffer};

/// The size each kind of image is stored at, so a 4K poster is never held in memory.
pub(crate) const LOGO_SUFFIX: &str = "#logo";
pub(crate) const BLUR_SUFFIX: &str = "#blur";
/// Artwork that fills the window, so it must not be squeezed into a poster.
pub(crate) const PAGE_SUFFIX: &str = "#page";

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
            match fetch(&client, &key).await {
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

/// The URL and the size a cache key asks for.
fn source(key: &str) -> (&str, (u32, u32)) {
    if let Some(url) = key.strip_suffix(LOGO_SUFFIX) {
        (url, (672, 168))
    } else if let Some(url) = key.strip_suffix(BLUR_SUFFIX) {
        (url, (24, 36))
    } else if let Some(url) = key.strip_suffix(PAGE_SUFFIX) {
        (url, (1920, 1080))
    } else {
        (key, (300, 450))
    }
}

async fn fetch(client: &reqwest::Client, key: &str) -> Fetched {
    let (url, (width, height)) = source(key);
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
            .thumbnail(width, height)
            .to_rgba8();
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
            ("https://example.com/a.jpg", (672, 168))
        );
        assert_eq!(
            source("https://example.com/a.jpg#blur"),
            ("https://example.com/a.jpg", (24, 36))
        );
        assert_eq!(
            source("https://example.com/a.jpg"),
            ("https://example.com/a.jpg", (300, 450))
        );
    }
}
