//! HTTP transport seam and external ID / season resolution for AniSkip (Kitsu, ARM, Jikan).

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;

use super::parse::{arm_lookup, kitsu_mal, relations_of, Relation};

const ARM: &str = "https://arm.haglund.dev/api/v2";
const JIKAN: &str = "https://api.jikan.moe/v4/anime";
const KITSU: &str = "https://kitsu.io/api/edge/anime";
const RELATION_CACHE_LIMIT: usize = 512;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub enum TransportReply {
    Ok(Value),
    NotFound,
    RateLimited,
    Rejected {
        status: u16,
        message: Option<String>,
    },
    Error(String),
}

/// Transport seam for true external HTTP endpoints (AniSkip, ARM, Jikan, Kitsu).
pub trait HttpTransport: Send + Sync {
    fn get_json<'a>(&'a self, url: &'a str) -> BoxFuture<'a, TransportReply>;
    fn post_json<'a>(&'a self, url: &'a str, body: &'a Value) -> BoxFuture<'a, TransportReply>;
}

/// Production HTTP adapter backed by `reqwest::Client`.
pub struct ReqwestTransport {
    client: Option<reqwest::Client>,
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        let client = reqwest::Client::builder()
            .user_agent(concat!("stremio-lightning/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(12))
            .build()
            .ok();
        Self { client }
    }
}

impl ReqwestTransport {
    async fn read_response(response: reqwest::Response) -> TransportReply {
        let status = response.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return TransportReply::NotFound;
        }
        if status.as_u16() == 429 {
            return TransportReply::RateLimited;
        }
        let body = response.json::<Value>().await;
        if status.is_success() {
            return match body {
                Ok(value) => TransportReply::Ok(value),
                Err(error) => TransportReply::Error(error.to_string()),
            };
        }
        let message = body
            .ok()
            .and_then(|v| v.get("message").and_then(Value::as_str).map(str::to_owned));
        TransportReply::Rejected {
            status: status.as_u16(),
            message,
        }
    }
}

impl HttpTransport for ReqwestTransport {
    fn get_json<'a>(&'a self, url: &'a str) -> BoxFuture<'a, TransportReply> {
        Box::pin(async move {
            let Some(client) = &self.client else {
                return TransportReply::Error("No HTTP client.".to_owned());
            };
            match client.get(url).send().await {
                Ok(response) => Self::read_response(response).await,
                Err(error) => TransportReply::Error(error.to_string()),
            }
        })
    }

    fn post_json<'a>(&'a self, url: &'a str, body: &'a Value) -> BoxFuture<'a, TransportReply> {
        Box::pin(async move {
            let Some(client) = &self.client else {
                return TransportReply::Error("No HTTP client.".to_owned());
            };
            match client.post(url).json(body).send().await {
                Ok(response) => Self::read_response(response).await,
                Err(error) => TransportReply::Error(error.to_string()),
            }
        })
    }
}

async fn get_ok<T: HttpTransport>(transport: &T, url: &str) -> Option<Value> {
    match transport.get_json(url).await {
        TransportReply::Ok(value) => Some(value),
        _ => None,
    }
}

pub(super) async fn mal_for<T: HttpTransport>(
    transport: &T,
    relation_cache: &Mutex<HashMap<u32, Vec<Relation>>>,
    jikan_gap: Duration,
    meta_id: &str,
    season: u32,
) -> Option<u32> {
    let (mal, exact_season) = mal_id(transport, meta_id, season).await?;
    if exact_season || season <= 1 {
        Some(mal)
    } else {
        season_mal(transport, relation_cache, jikan_gap, mal, season).await
    }
}

async fn mal_id<T: HttpTransport>(
    transport: &T,
    meta_id: &str,
    season: u32,
) -> Option<(u32, bool)> {
    let (source, id) = match meta_id.split_once(':') {
        Some(("kitsu", rest)) => ("kitsu", rest.split(':').next().unwrap_or(rest)),
        _ if meta_id.starts_with("tt") => ("imdb", meta_id.split(':').next().unwrap_or(meta_id)),
        _ => return None,
    };

    if source == "kitsu" {
        if let Some(mal) = get_ok(transport, &format!("{KITSU}/{id}/mappings"))
            .await
            .and_then(|v| kitsu_mal(&v))
        {
            return Some((mal, false));
        }
    }

    let url = match source {
        "kitsu" => format!("{ARM}/ids?source=kitsu&id={id}&include=myanimelist"),
        _ => format!("{ARM}/imdb?id={id}&include=myanimelist,thetvdb-season"),
    };
    get_ok(transport, &url)
        .await
        .and_then(|v| arm_lookup(&v, season))
}

async fn season_mal<T: HttpTransport>(
    transport: &T,
    relation_cache: &Mutex<HashMap<u32, Vec<Relation>>>,
    jikan_gap: Duration,
    initial: u32,
    target: u32,
) -> Option<u32> {
    let mut current = initial;
    let mut seen = HashSet::new();
    for _ in 0..10 {
        seen.insert(current);
        let rels = relations(transport, relation_cache, jikan_gap, current).await;
        match rels.into_iter().find_map(|r| match r {
            Relation::Prequel(mal) if !seen.contains(&mal) => Some(mal),
            _ => None,
        }) {
            Some(prequel) => current = prequel,
            None => break,
        }
    }

    let mut seasons = vec![current];
    seen.clear();
    for _ in 0..10 {
        seen.insert(current);
        let rels = relations(transport, relation_cache, jikan_gap, current).await;
        match rels.into_iter().find_map(|r| match r {
            Relation::Sequel(mal) if !seen.contains(&mal) => Some(mal),
            _ => None,
        }) {
            Some(sequel) => {
                seasons.push(sequel);
                current = sequel;
            }
            None => break,
        }
    }

    target
        .checked_sub(1)
        .and_then(|index| seasons.get(index as usize))
        .copied()
}

async fn relations<T: HttpTransport>(
    transport: &T,
    relation_cache: &Mutex<HashMap<u32, Vec<Relation>>>,
    jikan_gap: Duration,
    mal: u32,
) -> Vec<Relation> {
    if let Ok(cache) = relation_cache.lock() {
        if let Some(hit) = cache.get(&mal) {
            return hit.clone();
        }
    }
    if !jikan_gap.is_zero() {
        tokio::time::sleep(jikan_gap).await;
    }
    let Some(value) = get_ok(transport, &format!("{JIKAN}/{mal}/relations")).await else {
        return Vec::new();
    };
    let found = relations_of(&value);
    if let Ok(mut cache) = relation_cache.lock() {
        if cache.len() >= RELATION_CACHE_LIMIT {
            cache.clear();
        }
        cache.insert(mal, found.clone());
    }
    found
}
