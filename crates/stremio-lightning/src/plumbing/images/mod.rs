//! Posters, addon logos, and preview/details backdrops: one cache keyed by URL + variant suffix,
//! fetched off the UI thread, decoded to plain `DecodedImage` buffers (zero Slint imports), and
//! derived locally whenever a suitable source size is already cached or in flight.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

mod cache;
mod decode;
mod fetch;

pub(crate) use cache::DecodedImage;
pub(crate) use decode::{backdrop_key, blur_key, logo_key, page_key};
pub(crate) use fetch::tracing;

use cache::Cached;
use decode::{find_derivation_source, is_urgent, prepare_variant, source};
use fetch::{fetch, trace, Fetched};

type Waiter = (String, Box<dyn FnOnce() + Send>);

const BULK_FETCHES: usize = 16;
const URGENT_FETCHES: usize = 4;
const DECODE_CONCURRENCY: usize = 4;

#[derive(Default)]
struct PoolState {
    cache: Cached,
    requested: HashSet<String>,
    waiting: HashMap<String, Vec<Waiter>>,
    wanted: HashSet<String>,
}

impl PoolState {
    fn forget(&mut self, keys: Vec<String>) {
        for key in keys {
            self.requested.remove(&key);
        }
    }

    fn insert_and_evict(&mut self, key: String, image: DecodedImage) {
        let let_go = self.cache.insert(key, image, &self.wanted);
        self.forget(let_go);
    }

    fn take_waiters(&mut self, base: &str) -> Vec<Waiter> {
        self.waiting.remove(base).unwrap_or_default()
    }

    fn abandon_waiters(&mut self, base: &str) {
        for (key, _) in self.take_waiters(base) {
            self.requested.remove(&key);
        }
    }
}

#[derive(Clone)]
pub(crate) struct Images {
    client: reqwest::Client,
    state: Arc<Mutex<PoolState>>,
    bulk: Arc<tokio::sync::Semaphore>,
    urgent: Arc<tokio::sync::Semaphore>,
    decode: Arc<tokio::sync::Semaphore>,
    runtime: tokio::runtime::Handle,
}

impl Default for Images {
    fn default() -> Self {
        Self {
            client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(10))
                .timeout(std::time::Duration::from_secs(25))
                .build()
                .unwrap_or_default(),
            state: Default::default(),
            bulk: Arc::new(tokio::sync::Semaphore::new(BULK_FETCHES)),
            urgent: Arc::new(tokio::sync::Semaphore::new(URGENT_FETCHES)),
            decode: Arc::new(tokio::sync::Semaphore::new(DECODE_CONCURRENCY)),
            runtime: tokio::runtime::Handle::current(),
        }
    }
}

impl Images {
    /// Pins the currently visible keys and trims off-screen entries back under budget.
    /// Returns whether any cached entries were evicted.
    pub(crate) fn set_wanted(&self, keys: impl IntoIterator<Item = String>) -> bool {
        let keys: HashSet<String> = keys.into_iter().collect();
        let mut state = self.state.lock().unwrap();
        if state.wanted == keys {
            return false;
        }
        state.wanted = keys.clone();
        state.cache.touch_all(&keys);
        let let_go = state.cache.trim(&keys, None);
        let evicted = !let_go.is_empty();
        state.forget(let_go);
        evicted
    }

    /// Reads a cached image without advancing its LRU timestamp.
    pub(crate) fn get(&self, key: &Option<String>) -> Option<DecodedImage> {
        key.as_ref()
            .and_then(|key| self.state.lock().unwrap().cache.peek(key))
    }

    pub(crate) fn contains(&self, key: &str) -> bool {
        self.state.lock().unwrap().cache.contains_key(key)
    }

    /// Requests a key if not already cached, in flight, or known missing (4xx).
    pub(crate) fn request(&self, key: String, on_ready: impl FnOnce() + Send + 'static) {
        let derivation_src = {
            let mut state = self.state.lock().unwrap();
            if !state.requested.insert(key.clone()) {
                return;
            }
            match find_derivation_source(&state.cache, &key) {
                Some(src) => Some(src),
                None => {
                    let (url, _, blur) = source(&key);
                    let base = url.to_owned();
                    if should_wait(
                        blur.is_some(),
                        &base,
                        &state.requested,
                        state.cache.contains_key(&base),
                    ) {
                        state
                            .waiting
                            .entry(base)
                            .or_default()
                            .push((key, Box::new(on_ready)));
                        return;
                    }
                    None
                }
            }
        };

        let client = self.client.clone();
        let state = self.state.clone();
        let gate = if is_urgent(&key) {
            self.urgent.clone()
        } else {
            self.bulk.clone()
        };
        let decode_gate = self.decode.clone();
        let runtime = self.runtime.clone();

        self.runtime.spawn(async move {
            let base = source(&key).0.to_owned();
            let result = match derivation_src {
                Some(src) => {
                    let k = key.clone();
                    match tokio::task::spawn_blocking(move || prepare_variant(&src, &k)).await {
                        Ok(image) => {
                            trace(format_args!("made {key}"));
                            Fetched::Ready(image)
                        }
                        Err(_) => Fetched::Retry,
                    }
                }
                None => fetch(&client, &key, &gate, &decode_gate).await,
            };

            match result {
                Fetched::Ready(image) => {
                    let waiters = {
                        let mut pool = state.lock().unwrap();
                        pool.insert_and_evict(key, image);
                        pool.take_waiters(&base)
                    };
                    on_ready();
                    release(&runtime, &state, waiters);
                }
                Fetched::Missing => {
                    trace(format_args!("missing {key}"));
                    state.lock().unwrap().abandon_waiters(&base);
                }
                Fetched::Retry => {
                    trace(format_args!("retry {key}"));
                    let mut pool = state.lock().unwrap();
                    pool.requested.remove(&key);
                    pool.abandon_waiters(&base);
                }
            }
        });
    }
}

fn release(runtime: &tokio::runtime::Handle, state: &Arc<Mutex<PoolState>>, waiters: Vec<Waiter>) {
    for (key, on_ready) in waiters {
        let state = state.clone();
        runtime.spawn(async move {
            let src = {
                let pool = state.lock().unwrap();
                find_derivation_source(&pool.cache, &key)
            };
            let made = match src {
                Some(src) => {
                    let k = key.clone();
                    tokio::task::spawn_blocking(move || prepare_variant(&src, &k))
                        .await
                        .ok()
                }
                None => None,
            };
            match made {
                Some(image) => {
                    state.lock().unwrap().insert_and_evict(key, image);
                    on_ready();
                }
                None => {
                    state.lock().unwrap().requested.remove(&key);
                }
            }
        });
    }
}

fn should_wait(blur: bool, base: &str, requested: &HashSet<String>, cached: bool) -> bool {
    blur && !cached && requested.contains(base)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blur_waits_only_for_inflight_uncached_base() {
        let mut inflight = HashSet::new();
        inflight.insert("https://example.com/a.jpg".to_owned());
        assert!(should_wait(
            true,
            "https://example.com/a.jpg",
            &inflight,
            false
        ));
        assert!(!should_wait(
            true,
            "https://example.com/a.jpg",
            &HashSet::new(),
            false
        ));
        assert!(!should_wait(
            true,
            "https://example.com/a.jpg",
            &inflight,
            true
        ));
        assert!(!should_wait(
            false,
            "https://example.com/a.jpg",
            &inflight,
            false
        ));
    }
}
