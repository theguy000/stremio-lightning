//! Network fetch + blocking-pool decode pipeline and optional `LIGHTNING_TRACE_IMAGES` logging.

use std::sync::Arc;

use super::cache::DecodedImage;
use super::decode::{decode_bytes, source};

pub(super) enum Fetched {
    Ready(DecodedImage),
    Missing,
    Retry,
}

pub(super) async fn fetch(
    client: &reqwest::Client,
    key: &str,
    gate: &Arc<tokio::sync::Semaphore>,
    decode_gate: &Arc<tokio::sync::Semaphore>,
) -> Fetched {
    let (url, _, _) = source(key);
    let Ok(permit) = gate.acquire().await else {
        return Fetched::Retry;
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
            let missing = error.status().is_some_and(|s| s.is_client_error());
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
    drop(permit);
    let Ok(_decode_permit) = decode_gate.acquire().await else {
        return Fetched::Retry;
    };
    let net = started.elapsed().as_millis();
    let length = bytes.len();
    let key_owned = key.to_owned();
    let decoded = tokio::task::spawn_blocking(move || decode_bytes(&bytes, &key_owned))
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
        Some(image) => Fetched::Ready(image),
        None => Fetched::Retry,
    }
}

fn clock() -> u128 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_millis()
}

pub(crate) fn tracing() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("LIGHTNING_TRACE_IMAGES").is_some())
}

pub(super) fn trace(message: std::fmt::Arguments<'_>) {
    if tracing() {
        eprintln!("[img] t={} {message}", clock());
    }
}
