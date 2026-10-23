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

/// How much decoded picture the cache may hold before the least recently used ones are let go. A
/// poster is 300x450, 540 kB, so this is about ninety of them -- five screens of Discover, which is
/// as far back as scrolling usually goes. The artwork that fills the window is 1920x1080, 8 MB, so
/// only a few of those are kept at a time.
const CACHE_LIMIT: usize = 48 * 1024 * 1024;

pub(crate) type Cache = Arc<Mutex<Cached>>;

/// Decoded pictures by key, oldest use first out once the cache is over its limit.
#[derive(Default)]
pub(crate) struct Cached {
    entries: HashMap<String, (SharedPixelBuffer<Rgba8Pixel>, usize, u64)>,
    bytes: usize,
    /// Counts up on every use, so the picture used longest ago is the smallest.
    tick: u64,
    limit: usize,
}

impl Cached {
    #[cfg(test)]
    fn with_limit(limit: usize) -> Self {
        Self {
            limit,
            ..Self::default()
        }
    }

    fn get(&mut self, key: &str) -> Option<SharedPixelBuffer<Rgba8Pixel>> {
        self.tick += 1;
        let (buffer, _, used) = self.entries.get_mut(key)?;
        *used = self.tick;
        Some(buffer.clone())
    }

    fn contains_key(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    /// Puts a picture in, and says which keys it had to let go of to make room. Those have to be
    /// asked for again if they are ever wanted again, so the caller takes them off its "asked for"
    /// list: a poster let go of here must not look like one already on its way.
    ///
    /// `keep` is what the window is showing: those are never let go. If they are all that is left,
    /// the cache goes over its limit rather than blanking a card the user is looking at -- a
    /// screenful of posters is about eleven megabytes, so that ceiling is a real one.
    fn insert(
        &mut self,
        key: String,
        buffer: SharedPixelBuffer<Rgba8Pixel>,
        keep: &HashSet<String>,
    ) -> Vec<String> {
        let bytes = buffer.width() as usize * buffer.height() as usize * 4;
        self.tick += 1;
        if let Some((_, was, _)) = self.entries.insert(key.clone(), (buffer, bytes, self.tick)) {
            self.bytes -= was;
        }
        self.bytes += bytes;
        let limit = if self.limit == 0 { CACHE_LIMIT } else { self.limit };
        let mut let_go = Vec::new();
        while self.bytes > limit {
            // Never the picture just made, and never one that is on screen.
            let Some(oldest) = self
                .entries
                .iter()
                .filter(|(k, _)| **k != key && !keep.contains(*k))
                .min_by_key(|(_, (_, _, used))| *used)
                .map(|(k, _)| k.clone())
            else {
                break;
            };
            if let Some((_, bytes, _)) = self.entries.remove(&oldest) {
                self.bytes -= bytes;
                let_go.push(oldest);
            }
        }
        let_go
    }

    fn iter(&self) -> impl Iterator<Item = (&String, &SharedPixelBuffer<Rgba8Pixel>)> {
        self.entries.iter().map(|(key, (buffer, _, _))| (key, buffer))
    }

    #[cfg(test)]
    fn bytes(&self) -> usize {
        self.bytes
    }
}

/// A key held back because the picture it would be made from is being downloaded: the key itself
/// and what to run once it has been made.
type Waiter = (String, Box<dyn FnOnce() + Send>);

/// How many pictures are downloaded at once. A catalog's whole grid was asked for together -- 88
/// connections to one host, the first done in a third of a second and the last in thirty-six -- so
/// the host is not willing to serve that many friends of ours at the same time. This is a queue:
/// the keys are taken in the order they were asked for, so the cards on screen are the first
/// through. With HTTP/2 these are streams on one connection rather than connections, which is why
/// it can be wider than the six a browser opens per host over HTTP/1.1.
const BULK_FETCHES: usize = 16;
/// The pictures a click is waiting on -- the panel's backdrop and its logo -- have a lane of their
/// own, so they are never queued behind a grid's worth of posters.
const URGENT_FETCHES: usize = 4;

#[derive(Clone)]
pub(crate) struct Images {
    client: reqwest::Client,
    cache: Cache,
    requested: Arc<Mutex<HashSet<String>>>,
    /// Keys waiting on a download of their own picture at full size, by URL. Held rather than
    /// fetched again, and made from it the moment it lands.
    waiting: Arc<Mutex<HashMap<String, Vec<Waiter>>>>,
    /// The keys the window is showing right now. The cache will let go of anything rather than one
    /// of these: a screenful's posters are what the user is looking at, and letting one go to make
    /// room for a picture nobody asked for is what left a grid of blank cards.
    wanted: Arc<Mutex<HashSet<String>>>,
    bulk: Arc<tokio::sync::Semaphore>,
    urgent: Arc<tokio::sync::Semaphore>,
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
            waiting: Default::default(),
            wanted: Default::default(),
            bulk: Arc::new(tokio::sync::Semaphore::new(BULK_FETCHES)),
            urgent: Arc::new(tokio::sync::Semaphore::new(URGENT_FETCHES)),
            runtime: tokio::runtime::Handle::current(),
        }
    }
}

impl Images {
    /// Says which pictures the window is showing, so the cache keeps them while it makes room for
    /// others. Called whenever the screen's pictures change: the visible cards' posters and the
    /// panel's logo and backdrop.
    pub(crate) fn set_wanted(&self, keys: impl IntoIterator<Item = String>) {
        let keys: HashSet<String> = keys.into_iter().collect();
        let mut wanted = self.wanted.lock().unwrap();
        if *wanted != keys {
            *wanted = keys;
        }
    }

    /// The image for a key, if it has arrived. Read while the window is being filled in.
    pub(crate) fn get(&self, key: &Option<String>) -> Option<Image> {
        key.as_ref()
            .and_then(|key| self.cache.lock().unwrap().get(key))
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
        let (client, cache, requested, waiting, wanted) = (
            self.client.clone(),
            self.cache.clone(),
            self.requested.clone(),
            self.waiting.clone(),
            self.wanted.clone(),
        );
        let gate = if is_urgent(&key) {
            self.urgent.clone()
        } else {
            self.bulk.clone()
        };
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
            // The picture this would be made from may be downloading right now -- the panel's
            // blurred poster is asked for while the grid's copy of that same poster is still on its
            // way. Wait for it there instead of downloading it a second time; landing it releases
            // this key. (The waiting lock is taken before the cache is read, and the other side
            // writes the cache before reading the lock, so neither can miss the other.)
            let (url, _, blur) = source(&key);
            let base = url.to_owned();
            let mut held = waiting.lock().unwrap();
            if made.is_none()
                && should_wait(
                    blur.is_some(),
                    &base,
                    &requested.lock().unwrap(),
                    cache.lock().unwrap().contains_key(&base),
                )
            {
                held.entry(base)
                    .or_default()
                    .push((key, Box::new(on_ready)));
                return;
            }
            drop(held);
            let result = match made {
                Some(buffer) => {
                    trace(format_args!("made {key}"));
                    Fetched::Ready(buffer)
                }
                None => fetch(&client, &key, &gate).await,
            };
            match result {
                Fetched::Ready(buffer) => {
                    // What is on screen is read before the cache is locked, so the two locks are
                    // always taken in the same order.
                    let keep = wanted.lock().unwrap().clone();
                    let let_go = cache.lock().unwrap().insert(key, buffer, &keep);
                    forget(&requested, let_go);
                    on_ready();
                    release(&waiting, &cache, &requested, &wanted, &base);
                }
                Fetched::Missing => {
                    trace(format_args!("missing {key}"));
                    abandon(&waiting, &requested, &base);
                }
                Fetched::Retry => {
                    trace(format_args!("retry {key}"));
                    requested.lock().unwrap().remove(&key);
                    abandon(&waiting, &requested, &base);
                }
            }
        });
    }
}

/// A key whose picture a click is waiting on: the panel's backdrop, the details page's artwork, its
/// logo. These take the urgent lane so they are not queued behind a grid of posters.
fn is_urgent(key: &str) -> bool {
    is_blurred(key) || key.ends_with(LOGO_SUFFIX) || key.ends_with(PAGE_SUFFIX)
}

/// Milliseconds since this process started asking for pictures.
fn clock() -> u128 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis()
}

/// Whether to trace every load. Off unless `LIGHTNING_TRACE_IMAGES` is set: the timeline of a
/// screen's pictures is what says whether the queue is doing its job.
pub(crate) fn tracing() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("LIGHTNING_TRACE_IMAGES").is_some())
}

fn trace(message: std::fmt::Arguments<'_>) {
    if tracing() {
        eprintln!("[img] t={} {message}", clock());
    }
}

/// Whether this key should wait for the picture another key is already downloading whole. Every
/// card's poster is asked for on each snapshot, so the blurred copy of one that is still on its way
/// waits for it: fetching it here too is the second download a click used to cost.
fn should_wait(blur: bool, base: &str, requested: &HashSet<String>, cached: bool) -> bool {
    blur && !cached && requested.contains(base)
}

/// Takes the keys the cache let go of off the "asked for" list, so a picture that was dropped to
/// make room is fetched again if it is ever wanted again, rather than staying blank for the rest of
/// the session.
fn forget(requested: &Arc<Mutex<HashSet<String>>>, let_go: Vec<String>) {
    if let_go.is_empty() {
        return;
    }
    let mut requested = requested.lock().unwrap();
    for key in let_go {
        requested.remove(&key);
    }
}

/// Hands the keys that were waiting on `url` the picture that has just landed: each is made from
/// it, never downloaded again, and its caller repainted.
fn release(
    waiting: &Arc<Mutex<HashMap<String, Vec<Waiter>>>>,
    cache: &Cache,
    requested: &Arc<Mutex<HashSet<String>>>,
    wanted: &Arc<Mutex<HashSet<String>>>,
    url: &str,
) {
    let ready = waiting.lock().unwrap().remove(url).unwrap_or_default();
    for (key, on_ready) in ready {
        let (cache, requested, wanted, key) = (cache.clone(), requested.clone(), wanted.clone(), key.clone());
        tokio::spawn(async move {
            let made = tokio::task::spawn_blocking({
                let cache = cache.clone();
                let key = key.clone();
                move || derive(&cache, &key)
            })
            .await
            .ok()
            .flatten();
            match made {
                Some(buffer) => {
                    let keep = wanted.lock().unwrap().clone();
                    let let_go = cache.lock().unwrap().insert(key, buffer, &keep);
                    forget(&requested, let_go);
                    on_ready();
                }
                None => {
                    requested.lock().unwrap().remove(&key);
                }
            }
        });
    }
}

/// Lets the keys waiting on `url` go, because the picture they wanted is not coming: forgetting
/// them is what lets a later snapshot ask again.
fn abandon(
    waiting: &Arc<Mutex<HashMap<String, Vec<Waiter>>>>,
    requested: &Arc<Mutex<HashSet<String>>>,
    url: &str,
) {
    for (key, _) in waiting.lock().unwrap().remove(url).unwrap_or_default() {
        requested.lock().unwrap().remove(&key);
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

async fn fetch(
    client: &reqwest::Client,
    key: &str,
    gate: &Arc<tokio::sync::Semaphore>,
) -> Fetched {
    let (url, size, blur) = source(key);
    let permit = match gate.acquire().await {
        Ok(permit) => permit,
        Err(_) => return Fetched::Retry,
    };
    let started = std::time::Instant::now();
    trace(format_args!("start {key}"));
    let response = match client.get(url).send().await {
        Ok(response) => response,
        Err(error) => {
            eprintln!("[image] {url}: {error}");
            return Fetched::Retry;
        }
    };
    let version = response.version();
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
    // The wire is free again as soon as the body is in; decoding happens on the blocking pool, and
    // holding the permit through it would keep the next picture waiting on the CPU instead.
    drop(permit);
    let net = started.elapsed().as_millis();
    let length = bytes.len();
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
    trace(format_args!(
        "done {:?} net={net}ms decode={}ms {}kB {key}",
        version,
        started.elapsed().as_millis() - net,
        length / 1024
    ));
    match decoded {
        Some(buffer) => Fetched::Ready(buffer),
        None => Fetched::Retry,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4x4 picture, 64 bytes: small enough to make a cache limit out of whole ones.
    fn picture() -> SharedPixelBuffer<Rgba8Pixel> {
        SharedPixelBuffer::<Rgba8Pixel>::new(4, 4)
    }

    /// Nothing is on screen for these: the cache is free to let go of anything.
    fn on_nothing() -> HashSet<String> {
        HashSet::new()
    }

    #[test]
    fn the_cache_lets_go_of_the_oldest_picture_once_it_is_full() {
        let mut cache = Cached::with_limit(64 + 8);
        cache.insert("a".to_owned(), picture(), &on_nothing());
        cache.insert("b".to_owned(), picture(), &on_nothing());
        assert!(!cache.contains_key("a"), "the oldest should have been let go");
        assert!(cache.contains_key("b"));
        assert!(cache.bytes() <= 64 + 8);
    }

    #[test]
    fn a_picture_used_again_is_the_last_to_go() {
        let mut cache = Cached::with_limit(128);
        cache.insert("a".to_owned(), picture(), &on_nothing());
        cache.insert("b".to_owned(), picture(), &on_nothing());
        // Reading "a" makes "b" the least recently used, so "b" is what goes next.
        assert!(cache.get("a").is_some());
        cache.insert("c".to_owned(), picture(), &on_nothing());
        assert!(cache.contains_key("a"));
        assert!(cache.contains_key("c"));
        assert!(!cache.contains_key("b"));
    }

    #[test]
    fn the_picture_just_made_is_never_let_go() {
        // A limit under one picture would otherwise evict the one that is about to be drawn.
        let mut cache = Cached::with_limit(1);
        cache.insert("a".to_owned(), picture(), &on_nothing());
        assert!(cache.contains_key("a"));
    }

    #[test]
    fn a_picture_on_screen_is_not_let_go_to_make_room_for_one_that_is_not() {
        // Room for one picture, two wanted: the screen wins and the cache goes over its limit
        // rather than blanking a card the user is looking at. This is the bug that made a grid of
        // grey boxes after some scrolling.
        let mut cache = Cached::with_limit(64);
        let on_screen: HashSet<String> = ["a".to_owned()].into_iter().collect();
        cache.insert("a".to_owned(), picture(), &on_screen);
        cache.insert("b".to_owned(), picture(), &on_screen);
        assert!(cache.contains_key("a"), "the picture on screen must stay");
        assert!(cache.contains_key("b"), "and the one just made stays too");

        // Once nothing is on screen, the oldest goes as usual.
        cache.insert("c".to_owned(), picture(), &on_nothing());
        assert!(cache.contains_key("c"));
    }

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
    fn a_blur_waits_for_the_picture_that_is_already_on_its_way() {
        let mut on_their_way = HashSet::new();
        on_their_way.insert("https://example.com/a.jpg".to_owned());
        assert!(should_wait(
            true,
            "https://example.com/a.jpg",
            &on_their_way,
            false
        ));
        // Nothing of the sort on its way: the blur has to fetch the picture itself.
        assert!(!should_wait(
            true,
            "https://example.com/a.jpg",
            &HashSet::new(),
            false
        ));
        // Already here: it is made from that, not held.
        assert!(!should_wait(
            true,
            "https://example.com/a.jpg",
            &on_their_way,
            true
        ));
        // A key that wants the real thing is never held back.
        assert!(!should_wait(
            false,
            "https://example.com/a.jpg",
            &on_their_way,
            false
        ));
    }

    #[test]
    fn a_blur_can_be_made_from_the_poster_that_is_already_here() {
        let cache: Cache = Default::default();
        cache.lock().unwrap().insert(
            "https://example.com/a.jpg".to_owned(),
            SharedPixelBuffer::<Rgba8Pixel>::new(300, 450),
            &on_nothing(),
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
            &on_nothing(),
        );
        // The page wants the real artwork and only a blurred copy of it is here: that would be a
        // small picture blown up, so it has to be fetched.
        assert!(derive(&cache, "https://example.com/a.jpg#page").is_none());
        // Another picture entirely is never a source.
        assert!(derive(&cache, "https://example.com/b.jpg#page").is_none());
    }
}
