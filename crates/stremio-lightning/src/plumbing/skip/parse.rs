//! Pure JSON parsing and release-clustering algorithms for AniSkip, Kitsu, ARM, and Jikan.

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Opening,
    Ending,
    Recap,
}

impl Kind {
    /// AniSkip wire identifier (`"op"`, `"ed"`, `"recap"`).
    pub fn api(self) -> &'static str {
        match self {
            Self::Opening => "op",
            Self::Ending => "ed",
            Self::Recap => "recap",
        }
    }

    pub fn from_api(key: &str) -> Option<Self> {
        match key {
            "op" => Some(Self::Opening),
            "ed" => Some(Self::Ending),
            "recap" => Some(Self::Recap),
            _ => None,
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
    #[must_use]
    pub fn holds(&self, time: f64) -> bool {
        time >= self.start && time < self.end
    }
}

/// One AniSkip row for an episode, carrying its vote/report `id` alongside the `Segment`.
#[derive(Clone, Debug, PartialEq)]
pub struct Rate {
    pub id: String,
    pub segment: Segment,
}

impl Rate {
    #[must_use]
    pub fn segment(&self) -> Segment {
        self.segment
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Relation {
    Prequel(u32),
    Sequel(u32),
}

pub(super) fn kitsu_mal(value: &Value) -> Option<u32> {
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

/// Anime Relations Mapping answer and whether `season` matched `thetvdb-season` directly.
pub(super) fn arm_lookup(value: &Value, season: u32) -> Option<(u32, bool)> {
    let positive = |v: Option<&Value>| -> Option<u32> {
        u32::try_from(v.and_then(Value::as_u64)?)
            .ok()
            .filter(|n| *n > 0)
    };
    let Some(entries) = value.as_array() else {
        return positive(value.get("myanimelist")).map(|mal| (mal, false));
    };
    if season == 0 {
        return None;
    }
    if let Some(found) = entries
        .iter()
        .find(|entry| positive(entry.get("thetvdb-season")) == Some(season))
    {
        if let Some(mal) = positive(found.get("myanimelist")) {
            return Some((mal, true));
        }
    }
    let has_tvdb = entries
        .iter()
        .any(|entry| positive(entry.get("thetvdb-season")).is_some());
    if !has_tvdb {
        if let Some(mal) = entries
            .get(season as usize - 1)
            .and_then(|entry| positive(entry.get("myanimelist")))
        {
            return Some((mal, false));
        }
    }
    entries
        .iter()
        .find_map(|entry| positive(entry.get("myanimelist")))
        .map(|mal| (mal, false))
}

pub(super) fn relations_of(value: &Value) -> Vec<Relation> {
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

/// Clusters AniSkip rows by release length, picks the release closest to `duration`, pins endings
/// to `duration` when the file length differs, and returns rows sorted by start time.
pub(super) fn rates_of(value: &Value, duration: f64) -> Vec<Rate> {
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
        let Some(kind) = result
            .get("skipType")
            .and_then(Value::as_str)
            .and_then(Kind::from_api)
        else {
            continue;
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
        let rate = Rate {
            id: result
                .get("skipId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            segment: Segment { kind, start, end },
        };
        match releases
            .iter_mut()
            .find(|(known, _)| (known - length).abs() <= 5.0)
        {
            Some((_, rates)) => rates.push(rate),
            None => releases.push((length, vec![rate])),
        }
    }
    let Some((length, mut found)) = releases
        .into_iter()
        .min_by(|(a, _), (b, _)| (a - duration).abs().total_cmp(&(b - duration).abs()))
    else {
        return Vec::new();
    };
    for rate in &mut found {
        if rate.segment.kind == Kind::Ending && (duration - length).abs() > 1.0 {
            let span = rate.segment.end - rate.segment.start;
            rate.segment.end = duration;
            rate.segment.start = (duration - span).max(0.0);
        }
    }
    found.retain(|r| r.segment.start < duration);
    for rate in &mut found {
        rate.segment.end = rate.segment.end.min(duration);
    }
    found.retain(|r| r.segment.end > r.segment.start);
    found.sort_by(|a, b| a.segment.start.total_cmp(&b.segment.start));
    found
}
