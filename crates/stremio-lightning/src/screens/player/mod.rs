//! Player screen: snapshot data from stremio-core's `Player` model, options menu, links,
//! pure subtitle selection (`subtitles`), and stream opening/control (`actions`).

mod actions;
mod open;
pub(crate) mod skip;
mod subtitles;

pub use crate::screens::videos::Episode;
pub(crate) use actions::{
    ended, load_video, next, paused_changed, scale_changed, seek, time_changed, unload,
    video_params_changed,
};
pub(crate) use open::{open, open_for, open_video, should_open, VideoOpen};
pub use subtitles::{auto_subtitle, Subtitle, SubtitleChoice, SubtitleTrackFacts};
pub(crate) use subtitles::{
    browsed_language, same_language, subtitle_languages, subtitle_rows, variant_label, TrackInfo,
};

use stremio_core::models::common::Loadable;
use stremio_core::models::player::Player;
use stremio_core::types::player::VideoScale;
use stremio_core::types::resource::{MetaItem, StreamUrls};

use crate::app::AppModel;
use crate::screens::videos::{self, Season};
use crate::screens::Preview;

/// One row of the player's options menu.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuOption {
    pub label: String,
    pub action: &'static str,
}

/// Everything the player screen draws that comes from the core's model.
#[derive(Clone, Default)]
pub struct Data {
    pub open: bool,
    pub video_id: String,
    pub title: String,
    pub logo: Option<String>,
    pub background: Option<String>,
    pub url: Option<String>,
    pub key: String,
    pub resume_at: f64,
    pub error: String,
    pub next_label: String,
    pub has_next: bool,
    pub next_notice: f64,
    pub season: usize,
    pub seasons: Vec<(String, u32)>,
    pub episodes: Vec<Episode>,
    pub every_thumbnail: Vec<String>,
    pub preview: Option<Preview>,
    pub options: Vec<MenuOption>,
    pub subtitles: Vec<Subtitle>,
    pub scale: &'static str,
}

/// Sentinel stored in the player drawer's season-index slot while it follows the playing Video.
/// Unlike `details::NO_SEASON` (which stores a `u32` Season number), this slot stores a list index.
pub const FOLLOW_PLAYING_SEASON: usize = usize::MAX;

/// The drawer season row index the shell remembers, or `None` while following the playing Video.
pub(crate) fn chosen_season_index(stored: usize) -> Option<usize> {
    (stored != FOLLOW_PLAYING_SEASON).then_some(stored)
}

fn season_index(seasons: &[Season], wanted: Option<usize>, video: u32) -> usize {
    match wanted {
        Some(index) if index < seasons.len() => index,
        _ => seasons.iter().position(|s| s.number == video).unwrap_or(0),
    }
}

fn address(urls: &StreamUrls) -> Option<String> {
    urls.streaming_url
        .as_ref()
        .or(urls.download_url.as_ref())
        .map(ToString::to_string)
}

fn title_of(meta: &MetaItem, video_id: Option<&str>) -> String {
    let name = meta.preview.name.clone();
    let Some(video) = video_id.and_then(|id| meta.videos.iter().find(|v| v.id == id)) else {
        return name;
    };
    let series = video
        .series_info
        .as_ref()
        .map(|i| format!("({}x{})", i.season, i.episode));
    match (series, video.title.trim()) {
        (Some(series), "") => format!("{name} {series}"),
        (Some(series), title) => format!("{name} - {title} {series}"),
        (None, "") => name,
        (None, title) => format!("{name} - {title}"),
    }
}

/// Snapshot of the player screen from the core model and optional drawer season row index.
pub fn data(model: &AppModel, wanted_season: Option<usize>) -> Data {
    let player = &model.player;
    let Some(selected) = player.selected.as_ref() else {
        return Data::default();
    };
    let selected_meta_id = selected.meta_request.as_ref().map(|r| r.path.id.as_str());
    let meta = player
        .meta_item
        .as_ref()
        .and_then(|item| item.content.as_ref())
        .and_then(|content| match content {
            Loadable::Ready(meta) => Some(meta),
            _ => None,
        })
        .or_else(|| {
            crate::screens::details::meta_item(&model.meta_details).filter(|item| {
                selected_meta_id.is_none() || Some(item.preview.id.as_str()) == selected_meta_id
            })
        });
    let video_id = selected.stream_request.as_ref().map(|r| r.path.id.clone());
    let (urls, error) = match player.stream.as_ref() {
        Some(Loadable::Ready((urls, _))) => (Some(urls), String::new()),
        Some(Loadable::Err(error)) => (None, error.message()),
        Some(Loadable::Loading) | None => (None, String::new()),
    };
    let url = urls.and_then(address);
    let resume_at = player
        .library_item
        .as_ref()
        .filter(|item| video_id.as_deref() == item.state.video_id.as_deref())
        .map_or(0.0, |item| item.state.time_offset as f64);
    let playing_season = player.series_info.as_ref().map_or(0, |i| i.season);
    let seasons = meta.map(|m| videos::seasons(&m.videos)).unwrap_or_default();
    let season = season_index(&seasons, wanted_season, playing_season);
    let number = seasons.get(season).map_or(playing_season, |s| s.number);
    let hide_spoilers = model.ctx.profile.settings.hide_spoilers;
    let every_thumbnail = meta
        .map(|m| {
            m.videos
                .iter()
                .filter_map(|v| {
                    let url = v.thumbnail.as_ref()?;
                    let unwatched = player.watched.as_ref().is_none_or(|w| !w.get_video(&v.id));
                    Some(if hide_spoilers && unwatched {
                        crate::plumbing::images::blur_key(url)
                    } else {
                        url.clone()
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let mut episodes = meta
        .map(|m| {
            videos::episodes_for_season(
                &m.videos,
                number,
                player.watched.as_ref(),
                player.library_item.as_ref(),
                video_id.as_deref(),
            )
        })
        .unwrap_or_default();
    videos::apply_spoiler_blur(&mut episodes, model.ctx.profile.settings.hide_spoilers);
    let next_label = player
        .next_video
        .as_ref()
        .and_then(|v| {
            meta.map(|m| {
                videos::picked_label(&m.videos, &v.id).unwrap_or_else(|| title_of(m, Some(&v.id)))
            })
        })
        .unwrap_or_default();
    let scale = player
        .stream_state
        .as_ref()
        .and_then(|s| s.video_scale)
        .or(player.video_scale)
        .unwrap_or(VideoScale::Contain);

    Data {
        open: true,
        video_id: video_id.clone().unwrap_or_default(),
        title: meta
            .map(|m| title_of(m, video_id.as_deref()))
            .unwrap_or_default(),
        logo: meta.and_then(|m| {
            m.preview
                .logo
                .as_ref()
                .map(crate::plumbing::images::logo_key)
        }),
        background: meta.and_then(|m| {
            m.preview
                .background
                .as_ref()
                .or(m.preview.poster.as_ref())
                .map(crate::plumbing::images::blur_key)
        }),
        key: format!(
            "{}|{}",
            url.as_deref().unwrap_or_default(),
            video_id.as_deref().unwrap_or_default()
        ),
        url,
        resume_at,
        error,
        next_label,
        has_next: player.next_video.is_some(),
        next_notice: f64::from(model.ctx.profile.settings.next_video_notification_duration)
            / 1000.0,
        season,
        seasons: seasons
            .iter()
            .map(|s| (s.label.clone(), s.number))
            .collect(),
        episodes,
        every_thumbnail,
        preview: meta.map(|m| crate::screens::preview_of(&m.preview)),
        options: options_of(player),
        subtitles: subtitles::subtitles_of(model),
        scale: match scale {
            VideoScale::Contain => "contain",
            VideoScale::Cover => "cover",
            VideoScale::Fill => "fill",
        },
    }
}

fn options_of(player: &Player) -> Vec<MenuOption> {
    let Some(Loadable::Ready((urls, _))) = player.stream.as_ref() else {
        return Vec::new();
    };
    let mut options = Vec::new();
    if urls.streaming_url.is_some() || urls.download_url.is_some() {
        options.push(MenuOption {
            label: "Copy stream link".into(),
            action: "copy-stream",
        });
    }
    if urls.magnet_url.is_some() {
        options.push(MenuOption {
            label: "Copy magnet link".into(),
            action: "copy-magnet",
        });
    }
    if urls.download_url.is_some() {
        options.push(MenuOption {
            label: "Download video".into(),
            action: "download",
        });
    }
    if player
        .selected
        .as_ref()
        .is_some_and(|s| !s.stream.subtitles.is_empty())
        || player
            .subtitles
            .iter()
            .any(|p| matches!(p.content, Some(Loadable::Ready(_))))
    {
        options.push(MenuOption {
            label: "Download subtitles".into(),
            action: "download-subtitles",
        });
    }
    options
}

fn ready_urls(player: &Player) -> Option<&StreamUrls> {
    match player.stream.as_ref() {
        Some(Loadable::Ready((urls, _))) => Some(urls),
        _ => None,
    }
}

pub(crate) fn stream_link(player: &Player) -> Option<String> {
    ready_urls(player).and_then(address)
}

pub(crate) fn magnet_link(player: &Player) -> Option<String> {
    ready_urls(player)?
        .magnet_url
        .as_ref()
        .map(ToString::to_string)
}

pub(crate) fn download_link(player: &Player) -> Option<String> {
    ready_urls(player)?
        .download_url
        .as_ref()
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests;
