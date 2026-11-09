//! Skip times for anime openings, endings and recaps: AniSkip v2's data, fetched by the app.
//!
//! The chain is the one the AniSkip plugin used on the web (theguy000/stremio-aniskip-v2):
//! a Kitsu or IMDb id becomes a MyAnimeList id through Kitsu's mappings or Anime Relations
//! Mapping, a season past the first walks Jikan's prequel/sequel relations, and the skip times
//! themselves come from `api.aniskip.com`. Every step is optional: no answer is not an error,
//! it is no button.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

const ANISKIP: &str = "https://api.aniskip.com/v2/skip-times";
const ARM: &str = "https://arm.haglund.dev/api/v2";
const JIKAN: &str = "https://api.jikan.moe/v4/anime";
const KITSU: &str = "https://kitsu.io/api/edge/anime";

/// Jikan allows three requests a second, and the season walk makes one per hop.
const JIKAN_GAP: std::time::Duration = std::time::Duration::from_millis(350);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Opening,
    Ending,
    Recap,
}

impl Kind {
    /// The words the player's button shows.
    pub fn label(self) -> &'static str {
        match self {
            Self::Opening => "Skip Opening",
            Self::Ending => "Skip Ending",
            Self::Recap => "Skip Recap",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Segment {
    pub kind: Kind,
    pub start: f64,
    pub end: f64,
}

impl Segment {
    /// Whether the play head is inside the segment.
    #[must_use]
    pub fn holds(&self, time: f64) -> bool {
        time >= self.start && time < self.end
    }
}

/// The skip times for one episode, empty when there are none or nothing answered.
///
/// `meta_id` is the show's id (`tt0944947`, `kitsu:11614`), `season` the season being watched
/// (zero when the type has no seasons) and `duration` the episode's length in seconds.
///
/// `nominal` is the show's own runtime ("24 min") when it says one. AniSkip keys its submissions
/// by the length the release had, and one release's length can match a submission whose times are
/// wrong: asking with the show's runtime instead of the file's exact length lands on the entry most
/// releases share. The file's length is the fallback.
pub async fn segments(
    meta_id: &str,
    season: u32,
    episode: u32,
    duration: f64,
    nominal: Option<f64>,
) -> Vec<Segment> {
    if !(duration.is_finite() && duration > 0.0) || meta_id.is_empty() || episode == 0 {
        return Vec::new();
    }
    let asked = nominal.filter(|length| length.is_finite() && *length > 0.0).unwrap_or(duration);
    let key = format!("{meta_id}:{season}:{episode}:{}", asked.round() as u64);
    if let Some(hit) = cached(&key) {
        return hit;
    }

    let Some(mal) = mal_id(meta_id, season).await else {
        return Vec::new();
    };
    let mal = if season > 0 {
        season_mal(mal, season).await
    } else {
        mal
    };
    let found = skip_times(mal, episode, asked).await;
    remember(&key, &found);
    found
}

fn client() -> Option<&'static reqwest::Client> {
    static CLIENT: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    CLIENT
        .get_or_init(|| {
            reqwest::Client::builder()
                .user_agent(concat!("stremio-lightning/", env!("CARGO_PKG_VERSION")))
                .timeout(std::time::Duration::from_secs(12))
                .build()
                .ok()
        })
        .as_ref()
}

async fn get_json(url: &str) -> Option<Value> {
    let client = client()?;
    let response = client.get(url).send().await.ok()?;
    if !response.status().is_success() {
        return None;
    }
    response.json::<Value>().await.ok()
}

fn cached(key: &str) -> Option<Vec<Segment>> {
    cache().lock().ok()?.get(key).cloned()
}

fn remember(key: &str, segments: &[Segment]) {
    if let Ok(mut cache) = cache().lock() {
        cache.insert(key.to_owned(), segments.to_vec());
    }
}

fn cache() -> &'static Mutex<HashMap<String, Vec<Segment>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Vec<Segment>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The MyAnimeList id for a show, before season resolution.
async fn mal_id(meta_id: &str, season: u32) -> Option<u32> {
    let (source, id) = match meta_id.split_once(':') {
        Some(("kitsu", rest)) => ("kitsu", rest.split(':').next().unwrap_or(rest)),
        _ if meta_id.starts_with("tt") => ("imdb", meta_id.split(':').next().unwrap_or(meta_id)),
        _ => return None,
    };

    if source == "kitsu" {
        if let Some(mal) = get_json(&format!("{KITSU}/{id}/mappings"))
            .await
            .and_then(|value| kitsu_mal(&value))
        {
            return Some(mal);
        }
    }

    let url = match source {
        "kitsu" => format!("{ARM}/ids?source=kitsu&id={id}&include=myanimelist"),
        _ => format!("{ARM}/imdb?id={id}&include=myanimelist,thetvdb-season"),
    };
    get_json(&url)
        .await
        .and_then(|value| arm_mal(&value, season))
}

/// Kitsu's mappings answer: the entry that names `myanimelist/anime`.
fn kitsu_mal(value: &Value) -> Option<u32> {
    value
        .get("data")?
        .as_array()?
        .iter()
        .find(|entry| {
            entry
                .pointer("/attributes/externalSite")
                .and_then(Value::as_str)
                == Some("myanimelist/anime")
        })?
        .pointer("/attributes/externalId")
        .and_then(|id| match id {
            Value::String(text) => text.trim().parse().ok(),
            Value::Number(number) => number.as_u64().and_then(|n| u32::try_from(n).ok()),
            _ => None,
        })
}

/// Anime Relations Mapping's answer: the entry for this season, else the one at its index,
/// else the first it has.
fn arm_mal(value: &Value, season: u32) -> Option<u32> {
    let positive = |value: Option<&Value>| -> Option<u32> {
        let number = value.and_then(Value::as_u64)?;
        u32::try_from(number).ok().filter(|n| *n > 0)
    };
    let Some(entries) = value.as_array() else {
        return positive(value.get("myanimelist"));
    };
    if let Some(found) = entries
        .iter()
        .find(|entry| positive(entry.get("thetvdb-season")) == Some(season).filter(|s| *s > 0))
    {
        if let Some(mal) = positive(found.get("myanimelist")) {
            return Some(mal);
        }
    }
    let index = if season > 0 {
        season as usize - 1
    } else {
        usize::MAX
    };
    if let Some(mal) = entries
        .get(index)
        .and_then(|entry| positive(entry.get("myanimelist")))
    {
        return Some(mal);
    }
    entries
        .iter()
        .find_map(|entry| positive(entry.get("myanimelist")))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Relation {
    Prequel(u32),
    Sequel(u32),
}

/// The season's own MyAnimeList id: walk the prequels to season one, then the sequels to the
/// season wanted, and fall back to what was given.
async fn season_mal(initial: u32, target: u32) -> u32 {
    season_mal_with(initial, target, |mal| async move { relations(mal).await }).await
}

async fn season_mal_with<F, Fut>(initial: u32, target: u32, fetch: F) -> u32
where
    F: Fn(u32) -> Fut,
    Fut: std::future::Future<Output = Vec<Relation>>,
{
    let mut current = initial;
    let mut seen = HashSet::new();
    for _ in 0..10 {
        seen.insert(current);
        match fetch(current)
            .await
            .into_iter()
            .find_map(|relation| match relation {
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
        match fetch(current)
            .await
            .into_iter()
            .find_map(|relation| match relation {
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
        .unwrap_or(initial)
}

async fn relations(mal: u32) -> Vec<Relation> {
    if let Ok(cache) = relation_cache().lock() {
        if let Some(hit) = cache.get(&mal) {
            return hit.clone();
        }
    }
    tokio::time::sleep(JIKAN_GAP).await;
    let value = get_json(&format!("{JIKAN}/{mal}/relations")).await;
    let found = value.as_ref().map(relations_of).unwrap_or_default();
    if let Ok(mut cache) = relation_cache().lock() {
        cache.insert(mal, found.clone());
    }
    found
}

fn relations_of(value: &Value) -> Vec<Relation> {
    let mut found = Vec::new();
    for group in value
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let kind = group.get("relation").and_then(Value::as_str);
        for entry in group
            .get("entry")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if entry.get("type").and_then(Value::as_str) != Some("anime") {
                continue;
            }
            let Some(mal) = entry
                .get("mal_id")
                .and_then(Value::as_u64)
                .and_then(|id| u32::try_from(id).ok())
            else {
                continue;
            };
            match kind {
                Some("Prequel") => found.push(Relation::Prequel(mal)),
                Some("Sequel") => found.push(Relation::Sequel(mal)),
                _ => {}
            }
        }
    }
    found
}

fn relation_cache() -> &'static Mutex<HashMap<u32, Vec<Relation>>> {
    static CACHE: OnceLock<Mutex<HashMap<u32, Vec<Relation>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn skip_times(mal: u32, episode: u32, duration: f64) -> Vec<Segment> {
    let url = format!(
        "{ANISKIP}/{mal}/{episode}?types[]=op&types[]=ed&types[]=recap&episodeLength={}",
        duration.round() as u64
    );
    get_json(&url)
        .await
        .map(|value| segments_of(&value))
        .unwrap_or_default()
}

fn segments_of(value: &Value) -> Vec<Segment> {
    if value.get("found").and_then(Value::as_bool) != Some(true) {
        return Vec::new();
    }
    let mut found = Vec::new();
    for result in value
        .get("results")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let kind = match result.get("skipType").and_then(Value::as_str) {
            Some("op") => Kind::Opening,
            Some("ed") => Kind::Ending,
            Some("recap") => Kind::Recap,
            _ => continue,
        };
        let Some(start) = result
            .pointer("/interval/startTime")
            .and_then(Value::as_f64)
        else {
            continue;
        };
        let Some(end) = result.pointer("/interval/endTime").and_then(Value::as_f64) else {
            continue;
        };
        if end > start {
            found.push(Segment { kind, start, end });
        }
    }
    found.sort_by(|a, b| a.start.total_cmp(&b.start));
    found
}

/// The seconds a runtime string names: "24 min", "1h 24 min", "24m". Anything else is `None`,
/// and the caller falls back to the file's own length.
#[must_use]
pub fn runtime_seconds(text: &str) -> Option<f64> {
    let mut seconds = 0.0;
    let mut number: Option<f64> = None;
    let mut found = false;
    // "24 min" is a number and a unit in two words; "24min" is one. Both are read.
    for part in text.split_whitespace() {
        let (digits, unit) = match part.find(|c: char| c.is_alphabetic()) {
            Some(at) => (&part[..at], &part[at..]),
            None => (part, ""),
        };
        if let Ok(value) = digits.parse::<f64>() {
            number = Some(value);
        }
        if unit.is_empty() {
            continue;
        }
        let factor = match unit {
            "h" | "hr" | "hrs" | "hour" | "hours" => 3600.0,
            "m" | "min" | "mins" | "minute" | "minutes" => 60.0,
            _ => continue,
        };
        if let Some(value) = number.take() {
            seconds += value * factor;
            found = true;
        }
    }
    found.then_some(seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn kitsu_mappings_name_the_anime_list_entry() {
        let value = json!({
            "data": [
                { "attributes": { "externalSite": "trakt", "externalId": "1" } },
                { "attributes": { "externalSite": "myanimelist/anime", "externalId": "32281" } }
            ]
        });
        assert_eq!(kitsu_mal(&value), Some(32281));
    }

    #[test]
    fn arm_matches_the_season_before_the_position() {
        let value = json!([
            { "myanimelist": 16498, "thetvdb-season": 1 },
            { "myanimelist": 25777, "thetvdb-season": 2 },
            { "myanimelist": 35760, "thetvdb-season": 3 }
        ]);
        assert_eq!(arm_mal(&value, 3), Some(35760));
        assert_eq!(arm_mal(&value, 9), Some(16498));
        assert_eq!(arm_mal(&json!({ "myanimelist": 4242 }), 1), Some(4242));
        assert_eq!(arm_mal(&json!([]), 1), None);
    }

    #[test]
    fn the_season_walk_finds_the_wanted_season() {
        let relations = HashMap::from([
            (1, vec![Relation::Sequel(2)]),
            (2, vec![Relation::Prequel(1), Relation::Sequel(3)]),
            (3, vec![Relation::Prequel(2)]),
        ]);
        let wanted = futures::executor::block_on(season_mal_with(2, 3, |mal| {
            let found = relations.get(&mal).cloned().unwrap_or_default();
            async move { found }
        }));
        assert_eq!(wanted, 3);
        let first = futures::executor::block_on(season_mal_with(2, 1, |mal| {
            let found = relations.get(&mal).cloned().unwrap_or_default();
            async move { found }
        }));
        assert_eq!(first, 1);
    }

    #[test]
    fn a_runtime_string_becomes_seconds() {
        assert_eq!(runtime_seconds("24 min"), Some(1440.0));
        assert_eq!(runtime_seconds("1h 24 min"), Some(5040.0));
        assert_eq!(runtime_seconds("24m"), Some(1440.0));
        assert_eq!(runtime_seconds(""), None);
        assert_eq!(runtime_seconds("feature length"), None);
    }

    #[test]
    fn skip_times_are_read_in_order() {
        let value = json!({
            "found": true,
            "results": [
                { "skipType": "ed", "interval": { "startTime": 1342.795, "endTime": 1430.616 } },
                { "skipType": "op", "interval": { "startTime": 128.406, "endTime": 218.406 } },
                { "skipType": "mixed", "interval": { "startTime": 1.0, "endTime": 2.0 } }
            ]
        });
        let found = segments_of(&value);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].kind, Kind::Opening);
        assert!(found[0].holds(150.0));
        assert!(!found[0].holds(220.0));
        assert_eq!(found[1].kind, Kind::Ending);
        assert_eq!(segments_of(&json!({ "found": false })).len(), 0);
    }
}
