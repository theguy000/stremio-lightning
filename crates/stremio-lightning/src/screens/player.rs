//! The player screen: what it shows, and the actions it can ask the core for.
//!
//! The screen is driven by stremio-core's `Player` model, exactly as stremio-web's `/player`
//! route is. The core resolves the stream into an address (through the streaming server for
//! torrents), finds the title around it, offers the subtitles, and remembers where the Video was
//! left. This module turns that into plain data; `plumbing::player` plays it; `shell` joins the
//! two.

use stremio_core::constants::SUBTITLES_RESOURCE_NAME;
use stremio_core::models::common::Loadable;
use stremio_core::models::player::{Player, Selected as PlayerSelected};
use stremio_core::runtime::msg::{Action, ActionLoad, ActionPlayer};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::addon::{ResourcePath, ResourceRequest};
use stremio_core::types::player::VideoScale;
use stremio_core::types::resource::{MetaItem, Subtitles};
use stremio_core::types::streams::StreamsItemKey;

use crate::app::{AppModel, AppRuntime};
use crate::screens::{details, Preview};

/// One Video in the player's side drawer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Episode {
    pub id: String,
    pub label: String,
    pub title: String,
    pub picked: bool,
    pub watched: bool,
    /// How far through this Episode the library says the viewer is, as a percentage. The core
    /// defines it for the one Video the title's library entry is sitting on
    /// (`LibraryItem::progress()`: `time_offset / duration * 100`), and there is no number for any
    /// other Episode -- the web draws no bar for those either.
    pub progress: f64,
    /// The date the row shows, empty when the title gives none.
    pub released: String,
    /// The row's thumbnail, when the title supplies one.
    pub thumbnail: Option<String>,
}

/// One subtitle the stream or an addon offers. `id` is what the screen sends back when it is
/// chosen; `url` is the file mpv is given.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Subtitle {
    pub id: String,
    /// What the row says: the addon's own label for the track, or the stream's.
    pub title: String,
    /// The language code the track is in, as the addon declared it ("eng", "ger").
    pub language: String,
    /// Where it came from -- an addon's name, or "Stream" for one the stream carried itself.
    pub origin: String,
    pub url: String,
}

/// One row of the player's options menu.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MenuOption {
    pub label: String,
    /// "copy-stream", "copy-magnet", "download" or "download-subtitles".
    pub action: &'static str,
}

/// Everything the player screen draws that comes from the core's model.
#[derive(Clone, Default)]
pub struct Data {
    /// Whether a Stream is selected: the player is the screen on show.
    pub open: bool,
    pub title: String,
    pub logo: Option<String>,
    pub background: Option<String>,
    /// The address mpv opens. `key` says whether it already has it.
    pub url: Option<String>,
    pub key: String,
    /// Where a half-watched Video left off, in seconds.
    pub resume_at: f64,
    /// Why there is nothing to play, when there is nothing to play.
    pub error: String,
    pub next_label: String,
    pub has_next: bool,
    /// How close to the end the next Video is offered, in seconds: the profile's own setting.
    pub next_notice: f64,
    /// The Season the drawer is showing, as an index into `seasons`.
    pub season: usize,
    pub seasons: Vec<(String, u32)>,
    pub episodes: Vec<Episode>,
    /// The thumbnails of every Episode of every Season, so the drawer can be asked for any of
    /// them without a wait.
    pub every_thumbnail: Vec<String>,
    pub preview: Option<Preview>,
    pub options: Vec<MenuOption>,
    pub subtitles: Vec<Subtitle>,
    /// The video scale the core remembers for this stream: "contain", "cover" or "fill".
    pub scale: &'static str,
}

impl Data {
    /// The stream's own subtitles first, then every addon's, as one list the menu can show.
    fn subtitles_of(player: &Player) -> Vec<Subtitle> {
        let mut rows: Vec<Subtitle> = Vec::new();
        let mut push = |source: &str, subtitles: &[Subtitles]| {
            for subtitle in subtitles {
                let url = subtitle.url.to_string();
                rows.push(Subtitle {
                    id: format!("{source}:{}", subtitle.id),
                    // `label` only when the addon gave a real one: the web throws away anything
                    // that starts with "http" and falls back to the language's own name.
                    title: subtitle
                        .label
                        .as_deref()
                        .map(str::trim)
                        .filter(|label| !label.is_empty() && !label.starts_with("http"))
                        .map(str::to_owned)
                        .unwrap_or_default(),
                    language: subtitle.lang.clone(),
                    origin: source.to_owned(),
                    url,
                });
            }
        };
        if let Some(selected) = player.selected.as_ref() {
            push("Stream", &selected.stream.subtitles);
        }
        for page in &player.subtitles {
            let Some(Loadable::Ready(list)) = page.content.as_ref() else {
                continue;
            };
            push(&addon_name(&page.request), list);
        }
        rows
    }
}

/// The addon a request goes to, by the name its manifest gives: the subtitles menu labels a
/// language by where it came from.
fn addon_name(request: &ResourceRequest) -> String {
    request
        .base
        .host_str()
        .map(str::to_owned)
        .unwrap_or_else(|| "Add-on".to_owned())
}

/// The address mpv should open for the stream the core resolved.
fn address(urls: &stremio_core::types::resource::StreamUrls) -> Option<String> {
    urls.streaming_url
        .as_ref()
        .or(urls.download_url.as_ref())
        .map(ToString::to_string)
}

/// Web: the player's nav bar title -- the title's name, the Video's own name, and its Season and
/// Episode, so "Carrie - FIRST PERIOD (1x1)".
fn title_of(meta: &MetaItem, video_id: Option<&str>) -> String {
    let name = meta.preview.name.clone();
    let Some(video) = video_id.and_then(|id| meta.videos.iter().find(|video| video.id == id))
    else {
        return name;
    };
    let series = video
        .series_info
        .as_ref()
        .map(|info| format!("({}x{})", info.season, info.episode));
    match (series, video.title.trim()) {
        (Some(series), "") => format!("{name} {series}"),
        (Some(series), title) => format!("{name} - {title} {series}"),
        (None, "") => name,
        (None, title) => format!("{name} - {title}"),
    }
}

/// The stream the details screen lists at `index`, with the requests the player needs to find the
/// title and the subtitles around it again.
fn picked(model: &AppModel, index: usize) -> Option<PlayerSelected> {
    let pages = model.meta_details.streams.iter();
    let (stream, request) = pages
        .filter_map(|page| match page.content.as_ref() {
            Some(Loadable::Ready(list)) => {
                Some(list.iter().map(move |stream| (stream, &page.request)))
            }
            _ => None,
        })
        .flatten()
        .nth(index)?;
    // The title's own request: the addon that answered with the MetaItem.
    let meta_request = model
        .meta_details
        .meta_items
        .iter()
        .find(|item| matches!(item.content, Some(Loadable::Ready(_))))
        .map(|item| item.request.clone());
    Some(PlayerSelected {
        stream: stream.clone(),
        stream_request: Some(request.clone()),
        meta_request,
        subtitles_path: Some(ResourcePath::without_extra(
            SUBTITLES_RESOURCE_NAME,
            &request.path.r#type,
            &request.path.id,
        )),
    })
}

/// Plays a title from a card, the way the web's `#/player/<type>/<id>/<videoId>` link does: the
/// Stream it opens is the one the core already resolved and kept for the Video the title is
/// sitting on -- the same condition that gave the card its play button in the first place.
pub(crate) fn open_for(runtime: &AppRuntime, id: &str) -> Result<(), String> {
    let model = runtime
        .model()
        .map_err(|_| "the app state is not available".to_owned())?;
    let item = model
        .ctx
        .library
        .items
        .get(id)
        .ok_or("this title is not in the library")?;
    let video_id = item
        .state
        .video_id
        .clone()
        .ok_or("nothing has been played from this title yet")?;
    let streams_item = model
        .ctx
        .streams
        .items
        .get(&StreamsItemKey {
            meta_id: item.id.clone(),
            video_id,
        })
        .ok_or("this title has no stream to play")?;
    let selected = PlayerSelected {
        stream: streams_item.stream.clone(),
        stream_request: Some(ResourceRequest {
            base: streams_item.stream_transport_url.clone(),
            path: ResourcePath::without_extra(
                "stream",
                &streams_item.r#type,
                &streams_item.video_id,
            ),
        }),
        meta_request: Some(ResourceRequest {
            base: streams_item.meta_transport_url.clone(),
            path: ResourcePath::without_extra("meta", &streams_item.r#type, &streams_item.meta_id),
        }),
        subtitles_path: Some(ResourcePath::without_extra(
            SUBTITLES_RESOURCE_NAME,
            &streams_item.r#type,
            &streams_item.meta_id,
        )),
    };
    drop(model);
    runtime.dispatch(RuntimeAction {
        field: Some(crate::app::AppModelField::Player),
        action: Action::Load(ActionLoad::Player(Box::new(selected))),
    });
    Ok(())
}

/// Loads the Stream the details screen lists at `index`.
pub(crate) fn open(runtime: &AppRuntime, index: usize) -> Result<(), String> {
    let model = runtime
        .model()
        .map_err(|_| "the app state is not available".to_owned())?;
    let selected = picked(&model, index).ok_or("the list changed, pick it again")?;
    drop(model);
    runtime.dispatch(RuntimeAction {
        field: Some(crate::app::AppModelField::Player),
        action: Action::Load(ActionLoad::Player(Box::new(selected))),
    });
    Ok(())
}

/// The request that names the same title as `request`, for another Video of it.
fn request_for(request: &ResourceRequest, video_id: &str) -> ResourceRequest {
    ResourceRequest {
        base: request.base.clone(),
        path: ResourcePath::without_extra(
            request.path.resource.as_str(),
            &request.path.r#type,
            video_id,
        ),
    }
}

/// Next Video: the core is told the Video ended, then the same stream source is asked for the
/// Video that follows. The core's own `next_stream` is used when it found a binge-matched source
/// for it, as the web's deep link does.
pub(crate) fn next(runtime: &AppRuntime) -> Result<(), String> {
    let model = runtime
        .model()
        .map_err(|_| "the app state is not available".to_owned())?;
    let next_video = model
        .player
        .next_video
        .clone()
        .ok_or("there is no next Video")?;
    let current = model.player.selected.clone().ok_or("nothing is playing")?;
    // The core looks for a binge-matched source for the next Video; when it found none, the same
    // source carries on and the addon is asked for the next Video as it was for this one.
    let stream = model
        .player
        .next_stream
        .clone()
        .unwrap_or_else(|| current.stream.clone());
    let stream_request = current
        .stream_request
        .as_ref()
        .map(|request| request_for(request, &next_video.id));
    drop(model);

    runtime.dispatch(RuntimeAction {
        field: Some(crate::app::AppModelField::Player),
        action: Action::Player(ActionPlayer::NextVideo),
    });
    // Picking a Video means loading the player for it, as the details screen's rows do, so the
    // request names the Video rather than being carried over from the last one.
    if let Some(request) = stream_request.clone() {
        runtime.dispatch(RuntimeAction {
            field: Some(crate::app::AppModelField::Player),
            action: Action::Load(ActionLoad::Player(Box::new(PlayerSelected {
                stream,
                subtitles_path: Some(ResourcePath::without_extra(
                    SUBTITLES_RESOURCE_NAME,
                    &request.path.r#type,
                    &next_video.id,
                )),
                stream_request: Some(request),
                meta_request: current.meta_request,
            }))),
        });
    }
    Ok(())
}

/// An Episode picked in the player's side drawer: the same title, the same stream source, another
/// Video. The core's own `next_stream` is used when it happens to be the one picked.
pub(crate) fn load_video(runtime: &AppRuntime, video_id: &str) -> Result<(), String> {
    let model = runtime
        .model()
        .map_err(|_| "the app state is not available".to_owned())?;
    let current = model.player.selected.clone().ok_or("nothing is playing")?;
    let for_this_video = model
        .player
        .next_video
        .as_ref()
        .is_some_and(|video| video.id == video_id);
    let stream = model
        .player
        .next_stream
        .clone()
        .filter(|_| for_this_video)
        .unwrap_or_else(|| current.stream.clone());
    let stream_request = current
        .stream_request
        .as_ref()
        .map(|request| request_for(request, video_id));
    let subtitles_path = stream_request.as_ref().map(|request| {
        ResourcePath::without_extra(SUBTITLES_RESOURCE_NAME, &request.path.r#type, video_id)
    });
    drop(model);
    if let Some(request) = stream_request {
        runtime.dispatch(RuntimeAction {
            field: Some(crate::app::AppModelField::Player),
            action: Action::Load(ActionLoad::Player(Box::new(PlayerSelected {
                stream,
                stream_request: Some(request),
                meta_request: current.meta_request,
                subtitles_path,
            }))),
        });
    }
    Ok(())
}

/// The Video reached its end. The core records it; whether another Video follows is the caller's
/// business, because that is a decision about the screen, not about the model.
pub(crate) fn ended(runtime: &AppRuntime) {
    runtime.dispatch(RuntimeAction {
        field: Some(crate::app::AppModelField::Player),
        action: Action::Player(ActionPlayer::Ended),
    });
}

/// The play head moved. The core turns these into the library's resume point, so it only hears
/// about them while the picture is actually playing.
pub(crate) fn time_changed(runtime: &AppRuntime, time: f64, duration: f64, device: String) {
    runtime.dispatch(RuntimeAction {
        field: Some(crate::app::AppModelField::Player),
        action: Action::Player(ActionPlayer::TimeChanged {
            time: time.max(0.0).round() as u64,
            duration: duration.max(0.0).round() as u64,
            device,
        }),
    });
}

/// The user moved the play head themselves.
pub(crate) fn seek(runtime: &AppRuntime, time: f64, duration: f64, device: String) {
    runtime.dispatch(RuntimeAction {
        field: Some(crate::app::AppModelField::Player),
        action: Action::Player(ActionPlayer::Seek {
            time: time.max(0.0).round() as u64,
            duration: duration.max(0.0).round() as u64,
            device,
        }),
    });
}

pub(crate) fn paused_changed(runtime: &AppRuntime, paused: bool) {
    runtime.dispatch(RuntimeAction {
        field: Some(crate::app::AppModelField::Player),
        action: Action::Player(ActionPlayer::PausedChanged { paused }),
    });
}

pub(crate) fn video_params_changed(
    runtime: &AppRuntime,
    size: Option<u64>,
    filename: Option<String>,
) {
    use stremio_core::models::player::VideoParams;
    runtime.dispatch(RuntimeAction {
        field: Some(crate::app::AppModelField::Player),
        action: Action::Player(ActionPlayer::VideoParamsChanged {
            video_params: Some(VideoParams {
                hash: None,
                size,
                filename,
            }),
        }),
    });
}

/// The video scale the user chose, kept on the stream so the next Video opens the same way. The
/// stream's own state is carried over: only the scale changes here, not the chosen tracks.
pub(crate) fn scale_changed(runtime: &AppRuntime, scale: VideoScale) {
    let Ok(model) = runtime.model() else {
        return;
    };
    let mut state = model.player.stream_state.clone().unwrap_or_default();
    state.video_scale = Some(scale);
    drop(model);
    runtime.dispatch(RuntimeAction {
        field: Some(crate::app::AppModelField::Player),
        action: Action::Player(ActionPlayer::StreamStateChanged { state }),
    });
    runtime.dispatch(RuntimeAction {
        field: Some(crate::app::AppModelField::Player),
        action: Action::Player(ActionPlayer::VideoScaleChanged { video_scale: scale }),
    });
}

/// Leaving the player: the core drops the selected Stream, and with it everything the screen was
/// showing.
pub(crate) fn unload(runtime: &AppRuntime) {
    runtime.dispatch(RuntimeAction {
        field: Some(crate::app::AppModelField::Player),
        action: Action::Unload,
    });
}

/// Which row of `seasons` the drawer shows. A Season the viewer picked is a row of that list, so
/// it is used as given; without one, the row holding the Video that is playing.
pub(crate) fn season_index(
    seasons: &[details::Season],
    wanted: Option<usize>,
    video: u32,
) -> usize {
    match wanted {
        Some(index) if index < seasons.len() => index,
        _ => seasons
            .iter()
            .position(|season| season.number == video)
            .unwrap_or(0),
    }
}

/// The Season the drawer should show, as the shell remembers it. `NO_SEASON` follows the Video
/// that is playing, which is what the drawer does until a Season is picked.
pub const NO_SEASON: usize = usize::MAX;

/// The row the shell remembers, or `None` while it is still following the Video.
pub(crate) fn chosen_season(stored: usize) -> Option<usize> {
    (stored != NO_SEASON).then_some(stored)
}

/// The whole screen, from the model and the Season the drawer is showing.
pub fn data(model: &AppModel, wanted_season: Option<usize>) -> Data {
    let player = &model.player;
    let Some(selected) = player.selected.as_ref() else {
        return Data::default();
    };
    let meta = player
        .meta_item
        .as_ref()
        .and_then(|item| item.content.as_ref())
        .and_then(|content| match content {
            Loadable::Ready(meta) => Some(meta),
            _ => None,
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
        .filter(|item| {
            video_id
                .as_ref()
                .is_some_and(|id| item.state.video_id.as_deref() == Some(id.as_str()))
        })
        .map(|item| item.state.time_offset as f64)
        .unwrap_or_default();
    let series_info = player.series_info.as_ref();
    let playing_season = series_info.map(|info| info.season).unwrap_or_default();
    let seasons = meta
        .map(|meta| details::seasons(&meta.videos))
        .unwrap_or_default();
    let season = season_index(&seasons, wanted_season, playing_season);
    let number = seasons
        .get(season)
        .map(|season| season.number)
        .unwrap_or(playing_season);
    let every_thumbnail: Vec<String> = meta
        .map(|meta| {
            meta.videos
                .iter()
                .filter_map(|video| video.thumbnail.clone())
                .collect()
        })
        .unwrap_or_default();
    let episodes = meta
        .map(|meta| {
            details::episodes(&meta.videos, number)
                .into_iter()
                .map(|episode| Episode {
                    picked: video_id.as_deref() == Some(episode.id.as_str()),
                    watched: player
                        .watched
                        .as_ref()
                        .is_some_and(|watched| watched.get_video(&episode.id)),
                    progress: player
                        .library_item
                        .as_ref()
                        .filter(|item| item.state.video_id.as_deref() == Some(episode.id.as_str()))
                        .map(|item| item.progress())
                        .unwrap_or_default(),
                    id: episode.id,
                    label: episode.label,
                    title: episode.title,
                    released: episode.released,
                    thumbnail: episode.thumbnail,
                })
                .collect()
        })
        .unwrap_or_default();
    let next_label = player
        .next_video
        .as_ref()
        .and_then(|video| {
            meta.map(|meta| {
                details::picked_label(&meta.videos, &video.id)
                    .unwrap_or_else(|| title_of(meta, Some(&video.id)))
            })
        })
        .unwrap_or_default();
    let options = options_of(player);
    let scale = player
        .stream_state
        .as_ref()
        .and_then(|state| state.video_scale)
        .or(player.video_scale)
        .unwrap_or(VideoScale::Contain);

    Data {
        open: true,
        title: meta
            .map(|meta| title_of(meta, video_id.as_deref()))
            .unwrap_or_default(),
        logo: meta.and_then(|meta| {
            meta.preview
                .logo
                .as_ref()
                .map(|url| format!("{url}{}", crate::plumbing::images::LOGO_SUFFIX))
        }),
        background: meta.and_then(|meta| {
            meta.preview
                .background
                .as_ref()
                .or(meta.preview.poster.as_ref())
                .map(|url| format!("{url}{}", crate::plumbing::images::BLUR_SUFFIX))
        }),
        key: format!(
            "{}|{}",
            url.clone().unwrap_or_default(),
            video_id.clone().unwrap_or_default()
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
            .map(|season| (season.label.clone(), season.number))
            .collect(),
        episodes,
        every_thumbnail,
        preview: meta.map(|meta| crate::screens::preview_of(&meta.preview)),
        options,
        subtitles: Data::subtitles_of(player),
        scale: match scale {
            VideoScale::Contain => "contain",
            VideoScale::Cover => "cover",
            VideoScale::Fill => "fill",
        },
    }
}

/// Web: OptionsMenu -- the links the stream itself offers, in the order the web lists them.
fn options_of(player: &Player) -> Vec<MenuOption> {
    let Some(Loadable::Ready((urls, _))) = player.stream.as_ref() else {
        return Vec::new();
    };
    let mut options = Vec::new();
    if urls.streaming_url.is_some() || urls.download_url.is_some() {
        options.push(MenuOption {
            label: "Copy stream link".to_owned(),
            action: "copy-stream",
        });
    }
    if urls.magnet_url.is_some() {
        options.push(MenuOption {
            label: "Copy magnet link".to_owned(),
            action: "copy-magnet",
        });
    }
    if urls.download_url.is_some() {
        options.push(MenuOption {
            label: "Download video".to_owned(),
            action: "download",
        });
    }
    if player
        .selected
        .as_ref()
        .is_some_and(|selected| !selected.stream.subtitles.is_empty())
        || player
            .subtitles
            .iter()
            .any(|page| matches!(page.content, Some(Loadable::Ready(_))))
    {
        options.push(MenuOption {
            label: "Download subtitles".to_owned(),
            action: "download-subtitles",
        });
    }
    options
}

/// The stream's own address for the options menu: streaming, else download.
pub(crate) fn stream_link(player: &Player) -> Option<String> {
    match player.stream.as_ref() {
        Some(Loadable::Ready((urls, _))) => address(urls),
        _ => None,
    }
}

pub(crate) fn magnet_link(player: &Player) -> Option<String> {
    match player.stream.as_ref() {
        Some(Loadable::Ready((urls, _))) => urls.magnet_url.as_ref().map(ToString::to_string),
        _ => None,
    }
}

pub(crate) fn download_link(player: &Player) -> Option<String> {
    match player.stream.as_ref() {
        Some(Loadable::Ready((urls, _))) => urls.download_url.as_ref().map(ToString::to_string),
        _ => None,
    }
}

/// What the viewer has said about subtitles in this player session. The web keeps the same thing
/// in the core's `SubtitlePreference` (`enabled`, `source`, `language`) and gives it priority over
/// the profile's preferred language until the player closes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SubtitleChoice {
    /// The viewer has picked a track or switched subtitles off themselves, so auto-selection must
    /// not overrule them.
    pub chosen: bool,
    pub enabled: bool,
    /// The picked track came from a file beside the video rather than from inside it.
    pub external: bool,
    pub language: Option<String>,
}

impl SubtitleChoice {
    /// The web's `disableSubtitles`: remember that they are off, and which language and side they
    /// were on, so switching them back on returns to the same place.
    pub fn off(self) -> Self {
        Self {
            chosen: true,
            enabled: false,
            ..self
        }
    }

    /// The web's `rememberTrack`.
    pub fn picked(language: &str, external: bool) -> Self {
        Self {
            chosen: true,
            enabled: true,
            external,
            language: Some(language.to_owned()).filter(|language| !language.is_empty()),
        }
    }
}

/// One subtitle track as mpv describes it, in the two facts the choice is made from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubtitleTrackFacts {
    pub id: String,
    pub language: String,
    pub external: bool,
}

/// Which subtitle track to start on, following stremio-web's `useSubtitles`:
/// `buildCandidates` orders the languages it will accept -- the session's language, then the
/// profile's preferred language, then English -- and, within each language, prefers the side the
/// viewer last used (embedded unless they picked an external track); `resolveBestCandidate` takes
/// the first of those that a track actually exists for. No match means no subtitles, which is what
/// the web does too: it leaves both track ids null rather than guessing.
pub fn auto_subtitle(
    tracks: &[SubtitleTrackFacts],
    choice: &SubtitleChoice,
    preferred_language: &str,
) -> Option<String> {
    if choice.chosen && !choice.enabled {
        return None;
    }
    let mut languages: Vec<String> = Vec::new();
    let mut push = |language: Option<&str>| {
        if let Some(language) = language.map(str::trim).filter(|language| !language.is_empty()) {
            if !languages.iter().any(|seen| same_language(seen, language)) {
                languages.push(language.to_owned());
            }
        }
    };
    if choice.chosen {
        push(choice.language.as_deref());
    }
    push(Some(preferred_language));
    if choice.chosen {
        // CONSTANTS.DEFAULT_SUBTITLES_LANGUAGE.
        push(Some("eng"));
    }
    let sources = if choice.chosen && choice.external {
        [true, false]
    } else {
        [false, true]
    };
    for language in &languages {
        for external in sources {
            if let Some(track) = tracks
                .iter()
                .find(|track| track.external == external && same_language(&track.language, language))
            {
                return Some(track.id.clone());
            }
        }
    }
    None
}
/// The rows the Variants column shows for one language: the tracks inside the file first, then the
/// files the addons offer. An addon row already loaded beside the video is dropped -- mpv lists it
/// too, and offering one file twice would be two rows for the same subtitle.
pub(crate) fn subtitle_rows(
    tracks: &[crate::plumbing::player::Track],
    candidates: &[Subtitle],
    language: &str,
) -> Vec<crate::PlayerTrack> {
    let language_label = super::language_names::label_or_code(language);
    let mut rows: Vec<crate::PlayerTrack> = tracks
        .iter()
        .filter(|track| same_language(&track.language, language))
        .map(|track| crate::PlayerTrack {
            id: track.id.clone().into(),
            title: if track.title.trim().is_empty() {
                language_label.clone().into()
            } else {
                track.title.clone().into()
            },
            language: track.language.clone().into(),
            selected: track.selected,
            external: track.external,
            origin: Default::default(),
        })
        .collect();
    rows.extend(
        candidates
            .iter()
            .filter(|candidate| same_language(&candidate.language, language))
            // An offer that is already beside the video is the same row twice: `sub-add` names the
            // track with the label the row would have shown, so the labels match.
            .filter(|candidate| {
                let label = variant_label(candidate);
                !tracks
                    .iter()
                    .any(|track| track.external && track.title == label)
            })
            .map(|candidate| crate::PlayerTrack {
                id: candidate.id.clone().into(),
                title: variant_label(candidate).into(),
                language: candidate.language.clone().into(),
                selected: false,
                external: true,
                origin: candidate.origin.clone().into(),
            }),
    );
    rows
}

/// What a variant row reads: the addon's own label when it gave a usable one, and the language's
/// own name otherwise -- the web's `hasValidLabel(track.label) ? track.label : languages.label(lang)`.
/// The same string names the track when it is added to mpv, so the offer's row and the loaded
/// track's row are recognisably the same thing.
pub(crate) fn variant_label(candidate: &Subtitle) -> String {
    let label = candidate.title.trim();
    if label.is_empty() {
        super::language_names::label_or_code(&candidate.language)
    } else {
        label.to_owned()
    }
}

/// The languages the Subtitles menu offers: the ones inside the file, then the ones the addons
/// offer for it. The web's `subtitlesLanguages` does the same union of both track lists.
pub(crate) fn subtitle_languages(
    tracks: &[crate::plumbing::player::Track],
    candidates: &[Subtitle],
    priorities: &[&str],
) -> Vec<(String, String)> {
    let mut codes: Vec<String> = Vec::new();
    let mut collect = |language: &str| {
        let language = language.trim();
        if !language.is_empty() && !codes.iter().any(|code| same_language(code, language)) {
            codes.push(language.to_owned());
        }
    };
    for track in tracks {
        collect(&track.language);
    }
    for candidate in candidates {
        collect(&candidate.language);
    }
    // The web sorts the codes alphabetically and then pulls the ones that matter to the top, in
    // order: the viewer's own language, then the interface's (`sortByValues` in SubtitlesMenu.js).
    codes.sort();
    let mut ordered: Vec<String> = Vec::new();
    for priority in priorities {
        if priority.is_empty() {
            continue;
        }
        if let Some(position) = codes
            .iter()
            .position(|code| same_language(code, priority))
        {
            ordered.push(codes.remove(position));
        }
    }
    ordered.extend(codes);
    ordered
        .into_iter()
        .map(|code| {
            let label = super::language_names::label_or_code(&code);
            (code, label)
        })
        .collect()
}

pub(crate) fn same_language(left: &str, right: &str) -> bool {
    let left = left.trim().to_ascii_lowercase();
    let right = right.trim().to_ascii_lowercase();
    if left.is_empty() || right.is_empty() {
        return false;
    }
    left == right
        || (left.len() == 2 && right.starts_with(&left))
        || (right.len() == 2 && left.starts_with(&right))
}

#[cfg(test)]
mod tests {
    use super::*;
    use stremio_core::types::resource::Stream;

    fn track(id: &str, language: &str, external: bool) -> SubtitleTrackFacts {
        SubtitleTrackFacts {
            id: id.to_owned(),
            language: language.to_owned(),
            external,
        }
    }

    fn file_track(id: &str, language: &str, external: bool, title: &str) -> crate::plumbing::player::Track {
        crate::plumbing::player::Track {
            id: id.to_owned(),
            title: title.to_owned(),
            language: language.to_owned(),
            selected: false,
            external,
        }
    }

    fn offered(language: &str, title: &str, origin: &str) -> Subtitle {
        Subtitle {
            id: format!("{origin}:{title}"),
            title: title.to_owned(),
            language: language.to_owned(),
            origin: origin.to_owned(),
            url: format!("https://example.com/{title}.srt"),
        }
    }

    #[test]
    fn the_variants_column_shows_the_addons_offers_too() {
        // Nothing inside the file; an addon offers two English files and a German one.
        let candidates = [
            offered("eng", "OpenSubtitles", "OpenSubtitles v3"),
            offered("eng", "Community", "Stremio Community Subtitles"),
            offered("ger", "Untertitel", "OpenSubtitles v3"),
        ];
        let rows = subtitle_rows(&[], &candidates, "eng");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].origin.as_str(), "OpenSubtitles v3");
        assert!(rows.iter().all(|row| row.external));
    }

    #[test]
    fn a_language_with_only_an_addon_offer_still_gets_a_row() {
        let candidates = [offered("ger", "Untertitel", "OpenSubtitles v3")];
        let languages = subtitle_languages(&[], &candidates, &[]);
        assert_eq!(languages, vec![("ger".to_owned(), "Deutsch".to_owned())]);
    }

    #[test]
    fn languages_inside_the_file_come_before_the_offers() {
        let tracks = [file_track("1", "eng", false, "English")];
        let candidates = [offered("ger", "Untertitel", "OpenSubtitles v3")];
        assert_eq!(
            subtitle_languages(&tracks, &candidates, &[]),
            vec![
                ("eng".to_owned(), "English".to_owned()),
                ("ger".to_owned(), "Deutsch".to_owned()),
            ]
        );
    }

    #[test]
    fn the_viewers_language_comes_first_then_the_interface_one_then_the_rest() {
        let candidates = [
            offered("spa", "", "OpenSubtitles v3"),
            offered("fre", "", "OpenSubtitles v3"),
            offered("eng", "", "OpenSubtitles v3"),
            offered("ger", "", "OpenSubtitles v3"),
        ];
        let codes: Vec<String> = subtitle_languages(&[], &candidates, &["ger", "en"])
            .into_iter()
            .map(|(code, _)| code)
            .collect();
        assert_eq!(codes, vec!["ger", "eng", "fre", "spa"]);
    }

    #[test]
    fn a_track_with_no_name_of_its_own_reads_as_its_language_own_name() {
        // An embedded track whose container names it nothing: "English", never "eng".
        let tracks = [file_track("1", "eng", false, "")];
        let rows = subtitle_rows(&tracks, &[], "eng");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title.as_str(), "English");
    }

    #[test]
    fn a_variant_without_a_label_reads_as_its_language_own_name() {
        let candidates = [offered("fre", "", "OpenSubtitles v3")];
        let rows = subtitle_rows(&[], &candidates, "fre");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title.as_str(), "Français");
        assert_eq!(rows[0].origin.as_str(), "OpenSubtitles v3");
    }

    #[test]
    fn a_variant_labelled_by_its_addon_keeps_that_label() {
        let candidates = [offered("eng", "SDH", "OpenSubtitles v3")];
        let rows = subtitle_rows(&[], &candidates, "eng");
        assert_eq!(rows[0].title.as_str(), "SDH");
    }

    #[test]
    fn an_offer_already_beside_the_video_is_not_listed_twice() {
        let beside = file_track("9", "eng", true, "OpenSubtitles");
        let candidates = [offered("eng", "OpenSubtitles", "OpenSubtitles v3")];
        let rows = subtitle_rows(&[beside], &candidates, "eng");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id.as_str(), "9");
    }

    #[test]
    fn the_profile_language_is_chosen_before_english() {
        let tracks = [track("1", "ger", false), track("2", "eng", false)];
        let choice = SubtitleChoice::default();
        assert_eq!(auto_subtitle(&tracks, &choice, "ger").as_deref(), Some("1"));
    }

    #[test]
    fn embedded_wins_over_an_extra_file_in_the_same_language() {
        let tracks = [track("7", "eng", true), track("3", "eng", false)];
        let choice = SubtitleChoice::default();
        assert_eq!(auto_subtitle(&tracks, &choice, "eng").as_deref(), Some("3"));
    }

    #[test]
    fn the_viewers_own_pick_outranks_the_profile() {
        // They chose German earlier in this session; the profile still says English.
        let tracks = [track("1", "eng", false), track("2", "ger", false)];
        let choice = SubtitleChoice::picked("ger", false);
        assert_eq!(auto_subtitle(&tracks, &choice, "eng").as_deref(), Some("2"));
    }

    #[test]
    fn a_pick_from_an_extra_file_is_found_again() {
        let tracks = [track("5", "eng", false), track("9", "eng", true)];
        let choice = SubtitleChoice::picked("eng", true);
        assert_eq!(auto_subtitle(&tracks, &choice, "eng").as_deref(), Some("9"));
    }

    #[test]
    fn switching_them_off_leaves_them_off() {
        let tracks = [track("1", "eng", false)];
        let choice = SubtitleChoice::picked("eng", false).off();
        assert_eq!(auto_subtitle(&tracks, &choice, "eng"), None);
    }

    #[test]
    fn english_is_the_last_resort_only_when_the_viewer_chose() {
        let tracks = [track("1", "eng", false)];
        // Nothing in the profile, nothing chosen: the web sets no track at all.
        assert_eq!(auto_subtitle(&tracks, &SubtitleChoice::default(), ""), None);
        // With a pick on record, English is tried after the picked language.
        let choice = SubtitleChoice::picked("ger", false);
        assert_eq!(auto_subtitle(&tracks, &choice, "").as_deref(), Some("1"));
    }

    #[test]
    fn a_two_letter_code_matches_the_three_letter_track() {
        let tracks = [track("1", "eng", false)];
        let choice = SubtitleChoice::default();
        assert_eq!(auto_subtitle(&tracks, &choice, "en").as_deref(), Some("1"));
    }

    #[test]
    fn a_request_for_another_video_keeps_the_addon_and_the_type() {
        let request = ResourceRequest {
            base: url::Url::parse("https://example.com/manifest.json").unwrap(),
            path: ResourcePath::without_extra("stream", "series", "tt1:1:1"),
        };
        let next = request_for(&request, "tt1:1:2");
        assert_eq!(next.base, request.base);
        assert_eq!(next.path.r#type, "series");
        assert_eq!(next.path.id, "tt1:1:2");
        assert_eq!(next.path.resource, "stream");
    }

    #[test]
    fn torrents_play_through_the_stream_server_and_plain_urls_as_they_are() {
        // The player hands mpv whatever the core resolved, so this is the address the picture
        // comes from: a torrent becomes the stream server's own URL, a plain one is left alone.
        let server = url::Url::parse("http://127.0.0.1:11470/").unwrap();
        let torrent = Stream {
            source: stremio_core::types::resource::StreamSource::Torrent {
                info_hash: [0xab; 20],
                file_idx: None,
                announce: vec![],
                file_must_include: vec![],
            },
            name: None,
            description: None,
            thumbnail: None,
            subtitles: vec![],
            behavior_hints: Default::default(),
        };
        let converted = torrent.convert(Some(&server)).unwrap();
        let urls = stremio_core::types::resource::StreamUrls::new(converted, Some(&server));
        let resolved = address(&urls).unwrap();
        let url = url::Url::parse(&resolved).unwrap();
        assert_eq!(url.host_str(), Some("127.0.0.1"));
        assert_eq!(url.port(), Some(11470));
        assert_eq!(url.path(), format!("/{}/-1", "ab".repeat(20)));

        let plain = Stream {
            source: stremio_core::types::resource::StreamSource::Url {
                url: url::Url::parse("https://example.com/movie.mp4").unwrap(),
            },
            name: None,
            description: None,
            thumbnail: None,
            subtitles: vec![],
            behavior_hints: Default::default(),
        };
        let converted = plain.convert(Some(&server)).unwrap();
        let urls = stremio_core::types::resource::StreamUrls::new(converted, Some(&server));
        assert_eq!(
            address(&urls).unwrap(),
            "https://example.com/movie.mp4".to_owned()
        );
    }

    #[test]
    fn the_season_index_follows_the_seasons_list() {
        let seasons = vec![
            details::Season {
                number: 0,
                label: "Specials".to_owned(),
            },
            details::Season {
                number: 1,
                label: "Season 1".to_owned(),
            },
        ];
        // Nothing picked: the row holding the Video that is playing.
        assert_eq!(season_index(&seasons, None, 1), 1);
        // A picked row is the row -- this is what made the drawer's Next button look dead, when
        // the row was read as a Season number and row 1 meant "Season 1" again.
        assert_eq!(season_index(&seasons, Some(0), 0), 0);
        assert_eq!(season_index(&seasons, Some(1), 1), 1);
        // A row that no longer exists falls back to the playing Video's.
        assert_eq!(season_index(&seasons, Some(9), 1), 1);
    }
}
