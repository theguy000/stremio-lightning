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

/// Blurs thumbnails of unwatched Episodes when the `hide_spoilers` preference is enabled.
pub(crate) fn apply_spoiler_blur(episodes: &mut [Episode], hide_spoilers: bool) {
    if !hide_spoilers {
        return;
    }
    for ep in episodes {
        if !ep.watched {
            ep.thumbnail = ep.thumbnail.as_ref().map(crate::plumbing::images::blur_key);
        }
    }
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

    fn video(id: &str, season: u32, episode: u32, title: &str) -> Video {
        Video {
            id: id.to_owned(),
            title: title.to_owned(),
            released: None,
            overview: None,
            thumbnail: None,
            streams: vec![],
            series_info: Some(SeriesInfo { season, episode }),
            epg_info: None,
            trailer_streams: vec![],
        }
    }

    fn film(id: &str, title: &str) -> Video {
        Video {
            series_info: None,
            ..video(id, 0, 0, title)
        }
    }

    #[test]
    fn seasons_order_and_default_selection() {
        let vs = [
            video("a", 1, 1, "Pilot"),
            video("b", 1, 2, "Second"),
            video("c", 2, 1, "Next year"),
            video("s", 0, 1, "Bonus"),
        ];
        let listed: Vec<_> = seasons(&vs)
            .into_iter()
            .map(|s| (s.number, s.label))
            .collect();
        assert_eq!(
            listed,
            [
                (1, "Season 1".into()),
                (2, "Season 2".into()),
                (0, "Special".into())
            ]
        );
        assert_eq!(default_season(&vs, Some("c")), Some(2));
        assert_eq!(
            default_season(&[video("s", 0, 1, "B"), video("a", 1, 1, "P")], None),
            Some(1)
        );
        assert_eq!(default_season(&[video("s", 0, 1, "B")], None), Some(0));
    }

    #[test]
    fn episode_labels_and_picked_labels_use_special_for_season_zero() {
        let vs = [
            video("a", 1, 2, "Second"),
            video("s", 0, 3, "Bonus"),
            film("m", "A film"),
        ];
        let ep = &episodes_for_season(&vs, 1, None, None, None)[0];
        assert_eq!(
            (ep.id.as_str(), ep.label.as_str(), ep.title.as_str()),
            ("a", "S1E2", "2. Second")
        );
        assert_eq!(
            episodes_for_season(&vs, 0, None, None, None)[0].label,
            "SPECIAL 3"
        );
        assert_eq!(picked_label(&vs, "a"), Some("S1E2 Second".into()));
        assert_eq!(picked_label(&vs, "s"), Some("SPECIAL 3 Bonus".into()));
        assert_eq!(picked_label(&vs, "m"), Some("A film".into()));
        assert_eq!(picked_label(&vs, "gone"), None);
    }
}
