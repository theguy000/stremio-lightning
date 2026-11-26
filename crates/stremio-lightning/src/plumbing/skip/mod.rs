//! Skip times for anime openings, endings, and recaps via AniSkip v2.
//!
//! `SkipClient` owns its HTTP transport and bounded caches with zero process-global singletons.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::Value;

mod parse;
mod resolve;

pub use parse::{Kind, Rate, Segment};
pub use resolve::{BoxFuture, HttpTransport, ReqwestTransport, TransportReply};

use parse::{rates_of, Relation};
use resolve::mal_for;

const ANISKIP: &str = "https://api.aniskip.com/v2/skip-times";
const JIKAN_GAP: Duration = Duration::from_millis(350);
const RATE_CACHE_LIMIT: usize = 256;

#[derive(Clone, Copy, Debug)]
pub struct EpisodeKey<'a> {
    pub meta_id: &'a str,
    pub season: u32,
    pub episode: u32,
    pub duration: f64,
}

impl EpisodeKey<'_> {
    pub(crate) fn cache_key(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.meta_id,
            self.season,
            self.episode,
            self.duration.round() as u64
        )
    }

    fn is_valid(&self) -> bool {
        self.duration.is_finite()
            && self.duration > 0.0
            && !self.meta_id.is_empty()
            && self.episode > 0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Submission<'a> {
    pub episode: EpisodeKey<'a>,
    pub kind: Kind,
    pub start: f64,
    pub end: f64,
    pub provider: &'a str,
    pub submitter: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SkipError {
    UnknownShow,
    MissingId,
    RateLimited { voting: bool },
    Rejected(String),
    Transport(String),
}

pub struct SkipClient<T: HttpTransport = ReqwestTransport> {
    transport: T,
    rates_cache: Mutex<HashMap<String, Vec<Rate>>>,
    relation_cache: Mutex<HashMap<u32, Vec<Relation>>>,
    jikan_gap: Duration,
}

impl Default for SkipClient<ReqwestTransport> {
    fn default() -> Self {
        Self::new()
    }
}

impl SkipClient<ReqwestTransport> {
    pub fn new() -> Self {
        Self::with_transport(ReqwestTransport::default(), JIKAN_GAP)
    }
}

impl<T: HttpTransport> SkipClient<T> {
    pub fn with_transport(transport: T, jikan_gap: Duration) -> Self {
        Self {
            transport,
            rates_cache: Mutex::new(HashMap::new()),
            relation_cache: Mutex::new(HashMap::new()),
            jikan_gap,
        }
    }

    /// Fetches AniSkip rows for an episode, serving cached hits and avoiding caching transient
    /// server/transport errors.
    pub async fn rates(&self, ep: &EpisodeKey<'_>) -> Vec<Rate> {
        if !ep.is_valid() {
            return Vec::new();
        }
        let key = ep.cache_key();
        if let Ok(cache) = self.rates_cache.lock() {
            if let Some(hit) = cache.get(&key) {
                return hit.clone();
            }
        }
        let Some(mal) = mal_for(
            &self.transport,
            &self.relation_cache,
            self.jikan_gap,
            ep.meta_id,
            ep.season,
        )
        .await
        else {
            return Vec::new();
        };
        let url = format!(
            "{ANISKIP}/{mal}/{}?types[]=op&types[]=ed&types[]=recap&episodeLength={}",
            ep.episode,
            ep.duration.round() as u64
        );
        let found = match self.transport.get_json(&url).await {
            TransportReply::Ok(json) => rates_of(&json, ep.duration),
            TransportReply::NotFound => Vec::new(),
            _ => return Vec::new(),
        };
        if let Ok(mut cache) = self.rates_cache.lock() {
            if cache.len() >= RATE_CACHE_LIMIT {
                cache.clear();
            }
            cache.insert(key, found.clone());
        }
        found
    }

    /// Submits a timing to AniSkip and updates this client's own cache so subsequent queries see it.
    pub async fn submit(
        &self,
        sub: &Submission<'_>,
        cache_duration: Option<f64>,
    ) -> Result<Rate, SkipError> {
        let ep = sub.episode;
        let Some(mal) = mal_for(
            &self.transport,
            &self.relation_cache,
            self.jikan_gap,
            ep.meta_id,
            ep.season,
        )
        .await
        else {
            return Err(SkipError::UnknownShow);
        };
        let body = serde_json::json!({
            "skipType": sub.kind.api(),
            "providerName": sub.provider,
            "startTime": sub.start,
            "endTime": sub.end,
            "episodeLength": ep.duration,
            "submitterId": sub.submitter,
        });
        let value = self
            .post(&format!("{ANISKIP}/{mal}/{}", ep.episode), &body, false)
            .await?;
        let id = value
            .get("skipId")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or(SkipError::MissingId)?;
        let rate = Rate {
            id,
            segment: Segment {
                kind: sub.kind,
                start: sub.start,
                end: sub.end,
            },
        };
        let cache_ep = EpisodeKey {
            duration: cache_duration.unwrap_or(ep.duration),
            ..ep
        };
        if let Ok(mut cache) = self.rates_cache.lock() {
            let rows = cache.entry(cache_ep.cache_key()).or_default();
            if !rows.iter().any(|known| known.id == rate.id) {
                rows.push(rate.clone());
                rows.sort_by(|a, b| a.segment.start.total_cmp(&b.segment.start));
            }
        }
        Ok(rate)
    }

    /// Upvotes or downvotes one row by its AniSkip id.
    pub async fn vote(&self, id: &str, up: bool) -> Result<(), SkipError> {
        let body = serde_json::json!({ "voteType": if up { "upvote" } else { "downvote" } });
        self.post(&format!("{ANISKIP}/vote/{id}"), &body, true)
            .await
            .map(|_| ())
    }

    async fn post(&self, url: &str, body: &Value, voting: bool) -> Result<Value, SkipError> {
        match self.transport.post_json(url, body).await {
            TransportReply::Ok(value) => Ok(value),
            TransportReply::RateLimited => Err(SkipError::RateLimited { voting }),
            TransportReply::Rejected { status, message } => Err(SkipError::Rejected(
                message.unwrap_or_else(|| format!("AniSkip answered {status}.")),
            )),
            TransportReply::NotFound => {
                Err(SkipError::Rejected("AniSkip answered 404.".to_owned()))
            }
            TransportReply::Error(message) => Err(SkipError::Transport(message)),
        }
    }
}

#[cfg(test)]
mod tests;
