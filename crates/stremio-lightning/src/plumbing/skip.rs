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
    /// The words the Logs page uses for this kind.
    pub fn label(self) -> &'static str {
        match self {
            Self::Opening => "Skip Opening",
            Self::Ending => "Skip Ending",
            Self::Recap => "Skip Recap",
        }
    }

    /// The name AniSkip files this kind under in a submission.
    pub fn api(self) -> &'static str {
        match self {
            Self::Opening => "op",
            Self::Ending => "ed",
            Self::Recap => "recap",
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

/// One row AniSkip holds for an episode, with the id a vote or a report names it by. A `Segment`
/// is the same row without the id: what the player needs to skip, and nothing else.
#[derive(Clone, Debug, PartialEq)]
pub struct Rate {
    pub kind: Kind,
    pub start: f64,
    pub end: f64,
    /// AniSkip's own id for this row, empty when the answer carried none.
    pub id: String,
}

impl Rate {
    #[must_use]
    pub fn segment(&self) -> Segment {
        Segment {
            kind: self.kind,
            start: self.start,
            end: self.end,
        }
    }
}

/// The skip times for one episode, empty when there are none or nothing answered.
///
/// `meta_id` is the show's id (`tt0944947`, `kitsu:11614`), `season` the season being watched
/// (zero when the type has no seasons) and `duration` the episode's length in seconds. The length
/// is what AniSkip keys its submissions by, so it is asked first with the file's own length: a
/// submission answers for the release it was measured on.
///
/// AniSkip only matches a length within a couple of percent of a submission's, and a localised
/// broadcast cut can sit further away than that. When the file's own length answers nothing,
/// `nominal` -- the show's runtime, "24 min" -- asks once more, which reaches the release most
/// copies are cut from. The answer is still fitted to the file by `segments_of`.
pub async fn segments(
    meta_id: &str,
    season: u32,
    episode: u32,
    duration: f64,
    nominal: Option<f64>,
) -> Vec<Segment> {
    rates(meta_id, season, episode, duration, nominal)
        .await
        .iter()
        .map(Rate::segment)
        .collect()
}

/// The same answer as [`segments`], row by row, with the id each row votes by. The panel that
/// rates a timing needs the id; the player needs only the times.
pub async fn rates(
    meta_id: &str,
    season: u32,
    episode: u32,
    duration: f64,
    nominal: Option<f64>,
) -> Vec<Rate> {
    if !(duration.is_finite() && duration > 0.0) || meta_id.is_empty() || episode == 0 {
        return Vec::new();
    }
    let key = format!("{meta_id}:{season}:{episode}:{}", duration.round() as u64);
    if let Some(hit) = cached(&key) {
        return hit;
    }

    let Some(mal) = mal_for(meta_id, season).await else {
        return Vec::new();
    };
    let mut found = skip_times(mal, episode, duration).await;
    if found.is_empty() {
        if let Some(nominal) = nominal.filter(|length| {
            length.is_finite() && *length > 0.0 && (*length - duration).abs() > 1.0
        }) {
            found = skip_times(mal, episode, nominal).await;
        }
    }
    remember(&key, &found);
    found
}

/// The MyAnimeList id of the season being watched: the show's own id, walked forward through the
/// sequels for a season past the first.
async fn mal_for(meta_id: &str, season: u32) -> Option<u32> {
    let mal = mal_id(meta_id, season).await?;
    Some(if season > 0 {
        season_mal(mal, season).await
    } else {
        mal
    })
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

fn cached(key: &str) -> Option<Vec<Rate>> {
    cache().lock().ok()?.get(key).cloned()
}

fn remember(key: &str, rates: &[Rate]) {
    if let Ok(mut cache) = cache().lock() {
        cache.insert(key.to_owned(), rates.to_vec());
    }
}

fn cache() -> &'static Mutex<HashMap<String, Vec<Rate>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Vec<Rate>>>> = OnceLock::new();
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

async fn skip_times(mal: u32, episode: u32, duration: f64) -> Vec<Rate> {
    let url = format!(
        "{ANISKIP}/{mal}/{episode}?types[]=op&types[]=ed&types[]=recap&episodeLength={}",
        duration.round() as u64
    );
    get_json(&url)
        .await
        .map(|value| rates_of(&value, duration))
        .unwrap_or_default()
}

/// The segments one AniSkip answer holds, fitted to the file they are for.
///
/// The API matches a file to the nearest release by length, and one answer can carry segments
/// from more than one release. So the answer is read a release at a time, the release closest to
/// the file's own length is the one that answers, and its ending is pinned to the end of the
/// file: releases differ mostly in how much ending they carry, and a file that runs 18 seconds
/// shorter than the submission has its ending 18 seconds earlier while the opening stays where
/// the submission put it.
fn segments_of(value: &Value, duration: f64) -> Vec<Segment> {
    rates_of(value, duration)
        .iter()
        .map(Rate::segment)
        .collect()
}

fn rates_of(value: &Value, duration: f64) -> Vec<Rate> {
    if value.get("found").and_then(Value::as_bool) != Some(true) {
        return Vec::new();
    }
    let mut releases: Vec<(f64, Vec<Rate>)> = Vec::new();
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
        if !(end > start) {
            continue;
        }
        let length = result
            .get("episodeLength")
            .and_then(Value::as_f64)
            .unwrap_or(duration);
        // One release per tenth of a second: two submissions of the same release carry the same
        // length to the tenth, and two releases do not.
        let id = result
            .get("skipId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        match releases
            .iter_mut()
            .find(|(known, _)| (known * 10.0).round() == (length * 10.0).round())
        {
            Some((_, rates)) => rates.push(Rate {
                kind,
                start,
                end,
                id,
            }),
            None => releases.push((
                length,
                vec![Rate {
                    kind,
                    start,
                    end,
                    id,
                }],
            )),
        }
    }
    let Some((length, mut found)) = releases
        .into_iter()
        .min_by(|(a, _), (b, _)| (a - duration).abs().total_cmp(&(b - duration).abs()))
    else {
        return Vec::new();
    };
    for segment in &mut found {
        if segment.kind == Kind::Ending && (duration - length).abs() > 1.0 {
            let span = segment.end - segment.start;
            segment.end = duration;
            segment.start = (duration - span).max(0.0);
        }
    }
    found.retain(|segment| segment.start < duration);
    for segment in &mut found {
        segment.end = segment.end.min(duration);
    }
    found.retain(|segment| segment.end > segment.start);
    found.sort_by(|a, b| a.start.total_cmp(&b.start));
    found
}

/// Sends one skip time to AniSkip and returns the id it was filed under. `provider` names the app
/// in the submission; `submitter` is the UUID this install generated, which is what the API counts
/// submissions by.
#[allow(clippy::too_many_arguments)]
pub async fn submit(
    meta_id: &str,
    season: u32,
    episode: u32,
    kind: Kind,
    start: f64,
    end: f64,
    length: f64,
    provider: &str,
    submitter: &str,
) -> Result<String, String> {
    let Some(mal) = mal_for(meta_id, season).await else {
        return Err("AniSkip does not know this show.".to_owned());
    };
    let body = serde_json::json!({
        "skipType": kind.api(),
        "providerName": provider,
        "startTime": start,
        "endTime": end,
        "episodeLength": length,
        "submitterId": submitter,
    });
    let answer = post_json(&format!("{ANISKIP}/{mal}/{episode}"), &body).await?;
    answer
        .get("skipId")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| "AniSkip took the timing but named no id for it.".to_owned())
}

/// Upvotes or downvotes one row by its id.
pub async fn vote(id: &str, up: bool) -> Result<(), String> {
    let body = serde_json::json!({ "voteType": if up { "upvote" } else { "downvote" } });
    post_json(&format!("{ANISKIP}/vote/{id}"), &body)
        .await
        .map(|_| ())
}

async fn post_json(url: &str, body: &Value) -> Result<Value, String> {
    let client = client().ok_or_else(|| "No HTTP client.".to_owned())?;
    let response = client
        .post(url)
        .json(body)
        .send()
        .await
        .map_err(|why| why.to_string())?;
    let status = response.status();
    let value = response.json::<Value>().await.unwrap_or(Value::Null);
    if status.is_success() {
        return Ok(value);
    }
    if status.as_u16() == 429 {
        return Err(if url.contains("/vote/") {
            "AniSkip takes four votes an hour from one address.".to_owned()
        } else {
            "AniSkip takes ten submissions a day for one episode.".to_owned()
        });
    }
    Err(value
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("AniSkip answered {status}.")))
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
        let found = segments_of(&value, 1440.0);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].kind, Kind::Opening);
        assert!(found[0].holds(150.0));
        assert!(!found[0].holds(220.0));
        assert_eq!(found[1].kind, Kind::Ending);
        assert_eq!(segments_of(&json!({ "found": false }), 1440.0).len(), 0);
    }

    #[test]
    fn the_ending_follows_a_file_that_runs_short() {
        let value = json!({
            "found": true,
            "results": [
                { "skipType": "op", "interval": { "startTime": 194.0, "endTime": 284.0 }, "episodeLength": 1381.0 },
                { "skipType": "ed", "interval": { "startTime": 1291.0, "endTime": 1381.0 }, "episodeLength": 1381.0 }
            ]
        });
        let found = segments_of(&value, 1363.0);
        assert_eq!(found.len(), 2);
        assert_eq!(
            found[0],
            Segment {
                kind: Kind::Opening,
                start: 194.0,
                end: 284.0
            }
        );
        assert_eq!(
            found[1],
            Segment {
                kind: Kind::Ending,
                start: 1273.0,
                end: 1363.0
            }
        );
    }

    #[test]
    fn the_release_closest_to_the_file_answers_alone() {
        let value = json!({
            "found": true,
            "results": [
                { "skipType": "op", "interval": { "startTime": 112.0, "endTime": 202.0 }, "episodeLength": 1440.065 },
                { "skipType": "ed", "interval": { "startTime": 1342.0, "endTime": 1431.0 }, "episodeLength": 1431.984 }
            ]
        });
        let found = segments_of(&value, 1363.0);
        // The nearer release carries only the ending; the opening belonged to the other release
        // and is not borrowed from it.
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind, Kind::Ending);
        assert_eq!(found[0].end, 1363.0);
    }
}
