//! Byte-budgeted LRU cache for decoded RGBA8 images with viewport pinning.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// How much decoded picture the cache may hold before evicting least-recently-used entries.
///
/// A poster is 240x351 RGBA8 (~337 kB). Because `wanted` pins only the active screen's visible
/// viewport (+ one slack row, ~7-10 MB), 12 MB holds ~36 posters while keeping CPU RAM tight.
const CACHE_LIMIT: usize = 12 * 1024 * 1024;

static NEXT_REV: AtomicU64 = AtomicU64::new(1);

/// Plain decoded RGBA8 image shared across callers without any UI-toolkit dependency.
#[derive(Clone)]
pub(crate) struct DecodedImage {
    rev: u64,
    image: Arc<image::RgbaImage>,
}

impl DecodedImage {
    pub(crate) fn new(image: image::RgbaImage) -> Self {
        Self {
            rev: NEXT_REV.fetch_add(1, Ordering::Relaxed),
            image: Arc::new(image),
        }
    }

    pub(crate) fn rev(&self) -> u64 {
        self.rev
    }

    pub(crate) fn width(&self) -> u32 {
        self.image.width()
    }

    pub(crate) fn height(&self) -> u32 {
        self.image.height()
    }

    pub(crate) fn as_raw(&self) -> &[u8] {
        self.image.as_raw()
    }

    pub(crate) fn rgba(&self) -> &image::RgbaImage {
        &self.image
    }

    pub(crate) fn byte_len(&self) -> usize {
        self.width() as usize * self.height() as usize * 4
    }
}

/// Decoded pictures by key, oldest use first out once the cache is over its limit.
#[derive(Default)]
pub(crate) struct Cached {
    entries: HashMap<String, (DecodedImage, usize, u64)>,
    bytes: usize,
    tick: u64,
    limit: usize,
}

impl Cached {
    #[cfg(test)]
    pub(crate) fn with_limit(limit: usize) -> Self {
        Self {
            limit,
            ..Self::default()
        }
    }

    #[cfg(test)]
    pub(crate) fn get(&mut self, key: &str) -> Option<DecodedImage> {
        self.tick += 1;
        let (image, _, used) = self.entries.get_mut(key)?;
        *used = self.tick;
        Some(image.clone())
    }

    /// Reads a cached image without advancing its LRU timestamp, so syncing off-screen rows does
    /// not make scrolled-past posters look recently used.
    pub(crate) fn peek(&self, key: &str) -> Option<DecodedImage> {
        self.entries.get(key).map(|(image, _, _)| image.clone())
    }

    pub(crate) fn touch_all(&mut self, keys: &HashSet<String>) {
        self.tick += 1;
        let tick = self.tick;
        for key in keys {
            if let Some((_, _, used)) = self.entries.get_mut(key) {
                *used = tick;
            }
        }
    }

    pub(crate) fn contains_key(&self, key: &str) -> bool {
        self.entries.contains_key(key)
    }

    pub(crate) fn trim(&mut self, keep: &HashSet<String>, protect: Option<&str>) -> Vec<String> {
        let limit = if self.limit == 0 {
            CACHE_LIMIT
        } else {
            self.limit
        };
        let mut let_go = Vec::new();
        let stale_variants: Vec<String> = self
            .entries
            .keys()
            .filter(|k| k.contains('#') && protect != Some(k.as_str()) && !keep.contains(*k))
            .cloned()
            .collect();
        for key in stale_variants {
            if let Some((_, bytes, _)) = self.entries.remove(&key) {
                self.bytes -= bytes;
                let_go.push(key);
            }
        }
        while self.bytes > limit {
            let Some(oldest) = self
                .entries
                .iter()
                .filter(|(k, _)| protect != Some(k.as_str()) && !keep.contains(*k))
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

    /// Inserts an image and returns keys evicted to stay under budget (never evicting `keep` or
    /// the key just inserted).
    pub(crate) fn insert(
        &mut self,
        key: String,
        image: DecodedImage,
        keep: &HashSet<String>,
    ) -> Vec<String> {
        let bytes = image.byte_len();
        self.tick += 1;
        if let Some((_, was, _)) = self.entries.insert(key.clone(), (image, bytes, self.tick)) {
            self.bytes -= was;
        }
        self.bytes += bytes;
        self.trim(keep, Some(&key))
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&String, &DecodedImage)> {
        self.entries.iter().map(|(key, (image, _, _))| (key, image))
    }

    #[cfg(test)]
    pub(crate) fn bytes(&self) -> usize {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture() -> DecodedImage {
        DecodedImage::new(image::RgbaImage::new(4, 4))
    }

    fn on_nothing() -> HashSet<String> {
        HashSet::new()
    }

    #[test]
    fn evicts_oldest_when_full_and_respects_lru_touch() {
        let mut cache = Cached::with_limit(64 + 8);
        cache.insert("a".to_owned(), picture(), &on_nothing());
        cache.insert("b".to_owned(), picture(), &on_nothing());
        assert!(!cache.contains_key("a"));
        assert!(cache.contains_key("b") && cache.bytes() <= 64 + 8);

        let mut lru = Cached::with_limit(128);
        lru.insert("a".to_owned(), picture(), &on_nothing());
        lru.insert("b".to_owned(), picture(), &on_nothing());
        assert!(lru.get("a").is_some());
        lru.insert("c".to_owned(), picture(), &on_nothing());
        assert!(lru.contains_key("a") && lru.contains_key("c") && !lru.contains_key("b"));
    }

    #[test]
    fn protects_newest_and_visible_keys_while_peek_preserves_order() {
        let mut tiny = Cached::with_limit(1);
        tiny.insert("a".to_owned(), picture(), &on_nothing());
        assert!(tiny.contains_key("a"));

        let mut cache = Cached::with_limit(64);
        let both: HashSet<String> = ["a".to_owned(), "b".to_owned()].into_iter().collect();
        cache.insert("a".to_owned(), picture(), &both);
        cache.insert("b".to_owned(), picture(), &both);
        assert!(cache.peek("a").is_some());
        let only_b: HashSet<String> = ["b".to_owned()].into_iter().collect();
        assert_eq!(cache.trim(&only_b, None), vec!["a".to_owned()]);
        assert!(!cache.contains_key("a") && cache.contains_key("b") && cache.bytes() == 64);
    }

    #[test]
    fn scrolling_evicts_offscreen_entries_never_pinned_board_posters() {
        let mut cache = Cached::with_limit(4 * 64);
        let on_screen: HashSet<String> = (0..4).map(|i| format!("board-{i}")).collect();
        for key in &on_screen {
            cache.insert(key.clone(), picture(), &on_screen);
        }
        for i in 0..40 {
            cache.insert(format!("discover-{i}"), picture(), &on_screen);
        }
        for key in &on_screen {
            assert!(cache.contains_key(key), "{key} was let go");
        }
        assert!(!cache.contains_key("discover-0") && cache.contains_key("discover-39"));
    }
}
