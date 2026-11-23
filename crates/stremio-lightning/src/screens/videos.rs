//! Shared domain model for Series Seasons, Specials, and Episodes (`GLOSSARY.md`).
//! Pure in-process data and formatting shared by Details Screen, Player, and Calendar.

use chrono::{DateTime, Datelike, Utc};
use stremio_core::types::library::LibraryItem;
use stremio_core::types::resource::{SeriesInfo, Video};
use stremio_core::types::watched_bitfield::WatchedBitField;

/// One Season of a series, as the Episode Picker lists it. `label` is `Season N`, or `Special`
/// for season 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Season {
    pub number: u32,
    pub label: String,
}

/// One Episode row in the Details Episode Picker or Player side drawer.
/// `label` is `S2E5`, or `SPECIAL 5` for a Video in season 0.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Episode {
    pub id: String,
    pub label: String,
    pub title: String,
    /// Release date as shown in a row (`Sep 1, 2023`), empty when unknown.
    pub released: String,
    pub thumbnail: Option<String>,
    /// Whether the library's watched bitfield marks this Video as watched.
    pub watched: bool,
    /// Percentage watched for the single Video the title's library item is currently sitting on.
    pub progress: f64,
    /// Whether this Episode is the one currently playing (Player side drawer).
    pub picked: bool,
}

/// Canonical Episode code: `SPECIAL {episode}` for season 0, else `S{season}E{episode}`.
pub fn episode_code(info: &SeriesInfo) -> String {
    if info.season == 0 {
        format!("SPECIAL {}", info.episode)
    } else {
        format!("S{}E{}", info.season, info.episode)
    }
}

/// Seasons a series offers, preserving the core's order (regular seasons first, Specials last).
pub(crate) fn seasons(videos: &[Video]) -> Vec<Season> {
    let mut listed: Vec<Season> = Vec::new();
    for video in videos {
        let Some(info) = &video.series_info else {
            continue;
        };
        if listed.iter().any(|season| season.number == info.season) {
            continue;
        }
        listed.push(Season {
            number: info.season,
            label: if info.season == 0 {
                "Special".to_owned()
            } else {
                format!("Season {}", info.season)
            },
        });
    }
    listed
}

/// The Season the Episode Picker opens on: the one holding `default_video_id`, else the first
/// regular Season, else Specials.
pub(crate) fn default_season(videos: &[Video], default_video_id: Option<&str>) -> Option<u32> {
    let listed = seasons(videos);
    default_video_id
        .and_then(|id| videos.iter().find(|video| video.id == id))
        .and_then(|video| video.series_info.as_ref())
        .map(|info| info.season)
        .or_else(|| listed.iter().map(|s| s.number).find(|n| *n != 0))
        .or_else(|| listed.first().map(|s| s.number))
}

/// Web: `Oct 7, 2026`. Slint has no locale formatter, so this spells the English format directly.
pub(crate) fn released_label(date: Option<&DateTime<Utc>>) -> String {
    match date {
        Some(date) => format!(
            "{} {}, {}",
            date.format("%b"),
            date.day(),
            date.format("%Y")
        ),
        None => String::new(),
    }
}

/// Episodes of `season` enriched with optional watched state, watch progress, and current-video
/// highlight.
pub(crate) fn episodes_for_season(
    videos: &[Video],
    season: u32,
    watched: Option<&WatchedBitField>,
    library_item: Option<&LibraryItem>,
    picked_id: Option<&str>,
) -> Vec<Episode> {
    videos
        .iter()
        .filter_map(|video| {
            let info = video.series_info.as_ref()?;
            if info.season != season {
                return None;
            }
            let title = video.title.trim();
            Some(Episode {
                id: video.id.clone(),
                label: episode_code(info),
                title: if title.is_empty() {
                    format!("{}.", info.episode)
                } else {
                    format!("{}. {title}", info.episode)
                },
                released: released_label(video.released.as_ref()),
                thumbnail: video.thumbnail.clone(),
                watched: watched.is_some_and(|w| w.get_video(&video.id)),
                progress: library_item
                    .filter(|item| item.state.video_id.as_deref() == Some(video.id.as_str()))
                    .map_or(0.0, LibraryItem::progress),
                picked: picked_id == Some(video.id.as_str()),
            })
        })
        .collect()
}

/// Episodes of `season` without library/player enrichment (for callers without library state).
pub(crate) fn episodes(videos: &[Video], season: u32) -> Vec<Episode> {
    episodes_for_season(videos, season, None, None, None)
}

/// Web: names a picked Episode as `S1E2 Title` (or `SPECIAL 1 Title`) for Streams List / Next Video.
pub(crate) fn picked_label(videos: &[Video], picked: &str) -> Option<String> {
    let video = videos.iter().find(|video| video.id == picked)?;
    let label = video
        .series_info
        .as_ref()
        .map(episode_code)
        .unwrap_or_default();
    Some(match (label.is_empty(), video.title.trim().is_empty()) {
        (true, _) => video.title.clone(),
        (false, true) => label,
        (false, false) => format!("{label} {}", video.title),
    })
}

/// Whether every Video in `season` is marked watched in `watched_field`.
pub(crate) fn season_all_watched(
    videos: &[Video],
    season: u32,
    watched_field: Option<&WatchedBitField>,
) -> bool {
    let mut any = false;
    let all = videos
        .iter()
        .filter(|v| v.series_info.as_ref().is_some_and(|i| i.season == season))
        .all(|v| {
            any = true;
            watched_field.is_some_and(|w| w.get_video(&v.id))
        });
    any && all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn episode_code_uses_special_for_season_zero() {
        assert_eq!(
            episode_code(&SeriesInfo {
                season: 0,
                episode: 3
            }),
            "SPECIAL 3"
        );
        assert_eq!(
            episode_code(&SeriesInfo {
                season: 2,
                episode: 5
            }),
            "S2E5"
        );
    }
}
