//! Details Screen: title metadata, Episode Picker, Streams List, ratings, and share links.

mod actions;
mod stepper;
mod streams;

pub use crate::screens::videos::{Episode, Season};
pub(crate) use actions::{
    back_to_episodes, library_video_id, open, pick, rate, toggle_library, toggle_watched,
};
pub use actions::{mark_season_watched, mark_video_watched};
pub use stepper::{shown_pair, step, stepped_video, steps, Steps};

use stremio_core::constants::STREAM_RESOURCE_NAME;
use stremio_core::models::common::Loadable;
use stremio_core::models::meta_details::{MetaDetails, Selected as DetailsSelected};
use stremio_core::types::addon::ResourcePath;
use stremio_core::types::rating::Rating;
use stremio_core::types::resource::{MetaItem, Video};

use crate::screens::videos::{
    default_season, episodes_for_season, season_all_watched, seasons,
};
use crate::screens::Preview;
use crate::AppModel;

const MAX_STREAMS: usize = 100;

/// One source in the Streams List: what the addon called it, its description, and the addon name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub name: String,
    pub description: String,
    pub addon: String,
    pub link: String,
    pub is_magnet: bool,
    pub last_used: bool,
}

/// Metadata for the currently picked episode, shown in the Details hero.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PickedEpisode {
    pub title: String,
    pub overview: String,
    pub released: String,
    pub thumbnail: Option<String>,
}

/// Snapshot of the Details Screen ready for projection into the window.
#[derive(Clone, Default)]
pub struct Data {
    pub preview: Option<Preview>,
    pub streams: Vec<Source>,
    pub sources_loading: usize,
    pub waits_for_streams: bool,
    pub rating: Option<Rating>,
    pub rating_known: bool,
    pub rating_ready: bool,
    pub show_picker: bool,
    pub seasons: Vec<Season>,
    pub season: usize,
    pub episodes: Vec<Episode>,
    pub season_watched: bool,
    pub picked: Option<String>,
    pub picked_label: Option<String>,
    pub picked_episode: Option<PickedEpisode>,
    pub resume_label: String,
    pub resume_sublabel: String,
    pub resume_video_id: Option<String>,
    pub next_episode: Option<String>,
    pub steps: Option<Steps>,
}

/// Sentinel stored in the browsed-Season slot before the Episode Picker has been touched.
pub const NO_SEASON: usize = usize::MAX;

/// The browsed Season number (never a list index), or `None` if untouched.
pub fn chosen_season(stored: usize) -> Option<u32> {
    (stored != NO_SEASON)
        .then(|| u32::try_from(stored).ok())
        .flatten()
}

/// Web: short share route (`https://www.strem.io/s/{kind}/{slug}-{number}`).
pub fn share_link(name: &str, kind: &str, id: &str) -> String {
    let raw: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let slug = raw
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let number = id.trim_start_matches(|c: char| !c.is_numeric());
    format!("https://www.strem.io/s/{kind}/{slug}-{number}")
}

/// Pressing Like or Love clears if already active, otherwise replaces with the pressed rating.
pub fn next_rating(current: Option<&Rating>, like: bool) -> Option<Rating> {
    match (current, like) {
        (Some(Rating::Liked), true) | (Some(Rating::Loved), false) => None,
        (_, true) => Some(Rating::Liked),
        (_, false) => Some(Rating::Loved),
    }
}

/// The ready `MetaItem` answered by the addons, if any.
pub(crate) fn meta_item(details: &MetaDetails) -> Option<&MetaItem> {
    details
        .meta_items
        .iter()
        .find_map(|page| match &page.content {
            Some(Loadable::Ready(item)) => Some(item),
            _ => None,
        })
}

/// Whether placeholder rows should show while Streams load (signed in + an addon supports Stream).
pub(crate) fn waits_for_streams(
    model: &AppModel,
    selected: Option<&DetailsSelected>,
    picked: Option<&str>,
) -> bool {
    let Some(selected) = selected else {
        return false;
    };
    if model.ctx.profile.auth.is_none() {
        return false;
    }
    let path = ResourcePath::without_extra(
        STREAM_RESOURCE_NAME,
        &selected.meta_path.r#type,
        picked.unwrap_or(&selected.meta_path.id),
    );
    model
        .ctx
        .profile
        .addons
        .iter()
        .any(|addon| addon.manifest.is_resource_supported(&path))
}

fn shows_picker(picked: Option<&str>, seasons: &[Season]) -> bool {
    picked.is_none() && !seasons.is_empty()
}

pub(crate) fn next_episode(
    meta: &MetaItem,
    picked: &str,
    now: chrono::DateTime<chrono::Utc>,
) -> Option<String> {
    meta.next_video(picked, &now).map(|video| video.id.clone())
}

pub fn data(
    model: &AppModel,
    chosen: Option<u32>,
    picked: Option<&str>,
    moved: Option<(u32, u32)>,
    query: &str,
) -> Data {
    let details = &model.meta_details;
    let item = meta_item(details);
    let videos: &[Video] = item.map(|i| i.videos.as_slice()).unwrap_or_default();
    let listed = seasons(videos);
    let default_video_id = item.and_then(|i| i.preview.behavior_hints.default_video_id.as_deref());
    let shown = chosen
        .filter(|c| listed.iter().any(|s| s.number == *c))
        .or_else(|| default_season(videos, default_video_id));
    let season = shown
        .and_then(|n| listed.iter().position(|s| s.number == n))
        .unwrap_or_default();
    let needle = query.trim().to_lowercase();
    let watched_field = details.watched.as_ref();
    let season_watched = shown.is_some_and(|n| season_all_watched(videos, n, watched_field));
    let mut season_episodes = shown
        .map(|n| {
            episodes_for_season(
                videos,
                n,
                watched_field,
                details.library_item.as_ref(),
                None,
            )
        })
        .unwrap_or_default();
    if !needle.is_empty() {
        season_episodes.retain(|ep| ep.title.to_lowercase().contains(&needle));
    }
    let loading = details
        .streams
        .iter()
        .filter(|p| {
            !matches!(
                &p.content,
                Some(Loadable::Ready(_)) | Some(Loadable::Err(_))
            )
        })
        .count();
    let meta_id = item.map(|m| m.preview.id.as_str());
    let streams = streams::source_rows(model, details, meta_id, picked)
        .into_iter()
        .take(MAX_STREAMS)
        .collect();
    let (resume_label, resume_sublabel, resume_video_id) =
        streams::resume_summary(details, videos, picked);

    Data {
        preview: item.map(|item| {
            let mut preview = crate::screens::preview_of(&item.preview);
            preview.background = item
                .preview
                .background
                .as_ref()
                .map(crate::plumbing::images::page_key);
            (preview.in_library, preview.watched) =
                crate::screens::library_state(model, &item.preview.id);
            preview
        }),
        steps: picked.and_then(|p| steps(videos, p, moved)),
        streams,
        sources_loading: loading,
        waits_for_streams: waits_for_streams(model, details.selected.as_ref(), picked),
        rating: match &details.rating_info {
            Some(Loadable::Ready(info)) => info.status.clone(),
            _ => None,
        },
        rating_known: details.rating_info.is_some(),
        rating_ready: matches!(&details.rating_info, Some(Loadable::Ready(_))),
        show_picker: shows_picker(picked, &listed),
        picked_label: picked.and_then(|p| streams::episode_header(videos, p)),
        picked_episode: picked.and_then(|id| {
            videos.iter().find(|v| v.id == id).map(|v| PickedEpisode {
                title: if v.title.is_empty() { v.id.clone() } else { v.title.clone() },
                overview: v.overview.clone().unwrap_or_default(),
                released: v.released.map(|d| d.format("%b %d, %Y").to_string()).unwrap_or_default(),
                thumbnail: v.thumbnail.as_ref().map(crate::plumbing::images::page_key),
            })
        }),
        resume_label,
        resume_sublabel,
        resume_video_id,
        next_episode: picked
            .and_then(|p| item.and_then(|meta| next_episode(meta, p, chrono::Utc::now()))),
        picked: picked.map(str::to_owned),
        seasons: listed,
        season,
        episodes: season_episodes,
        season_watched,
    }
}



#[cfg(test)]
mod tests;
