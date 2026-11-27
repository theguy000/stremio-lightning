//! Pure stream-opening resolution for Details, Library/Continue Watching, and Episode Watch.
//! Never launches external browsers or players directly; returns launch plans to `shell/`.

use stremio_core::constants::SUBTITLES_RESOURCE_NAME;
use stremio_core::models::common::Loadable;
use stremio_core::models::meta_details::MetaDetails;
use stremio_core::models::player::Selected as PlayerSelected;
use stremio_core::runtime::msg::{Action, ActionLoad, ActionMetaDetails};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::addon::{ResourcePath, ResourceRequest};
use stremio_core::types::resource::{Stream, StreamSource, StreamUrls, Video};
use stremio_core::types::streams::StreamsItemKey;
use url::Url;

use crate::app::{AppModel, AppModelField, AppRuntime};
use crate::plumbing::external_player::{ExternalLaunch, ExternalPlayerKind};
use crate::screens::details;

#[derive(Debug, PartialEq)]
pub(crate) enum OpenOutcome {
    Load(Box<PlayerSelected>),
    OpenExternal(String),
    ExternalPlayer(ExternalLaunch),
}

#[derive(Debug, PartialEq)]
pub(crate) enum OpenResult {
    Loaded,
    OpenBrowser(String),
    ExternalPlayer(ExternalLaunch),
}

#[derive(Debug, PartialEq)]
pub(crate) enum VideoOpen {
    Opened,
    OpenBrowser(String),
    ExternalPlayer(ExternalLaunch),
    PickFromStreams,
}

fn ready_meta_request(details: &MetaDetails) -> Option<ResourceRequest> {
    details
        .meta_items
        .iter()
        .find(|item| matches!(item.content, Some(Loadable::Ready(_))))
        .map(|item| item.request.clone())
}

fn picked(model: &AppModel, index: usize) -> Option<(PlayerSelected, Option<Video>)> {
    let (stream, request) = model
        .meta_details
        .streams
        .iter()
        .filter_map(|page| match page.content.as_ref() {
            Some(Loadable::Ready(list)) => Some(list.iter().map(move |s| (s, &page.request))),
            _ => None,
        })
        .flatten()
        .nth(index)?;
    let video = details::meta_item(&model.meta_details)
        .and_then(|item| item.videos.iter().find(|v| v.id == request.path.id))
        .cloned();
    Some((
        PlayerSelected {
            stream: stream.clone(),
            stream_request: Some(request.clone()),
            meta_request: ready_meta_request(&model.meta_details),
            subtitles_path: Some(ResourcePath::without_extra(
                SUBTITLES_RESOURCE_NAME,
                &request.path.r#type,
                &request.path.id,
            )),
        },
        video,
    ))
}

/// Pure classification: external URL streams open in browser; configured `player_type` resolves
/// a direct http(s) streaming URL for VLC/MPV/M3U; otherwise the stream loads into embedded mpv.
pub(crate) fn classify_selected(
    selected: PlayerSelected,
    player_type: Option<&str>,
    server_base_url: Option<&Url>,
) -> Result<OpenOutcome, String> {
    if let StreamSource::External { external_url, .. } = &selected.stream.source {
        let Some(url) = external_url.as_ref() else {
            return Err("this external stream has no link to open".to_owned());
        };
        if !matches!(url.scheme(), "http" | "https") {
            return Err("this external stream uses an unsupported link".to_owned());
        }
        return Ok(OpenOutcome::OpenExternal(url.as_str().to_owned()));
    }
    if let Some(kind) = player_type.and_then(ExternalPlayerKind::parse) {
        let url = resolve_external_stream_url(&selected.stream, server_base_url)?;
        return Ok(OpenOutcome::ExternalPlayer(ExternalLaunch { kind, url }));
    }
    Ok(OpenOutcome::Load(Box::new(selected)))
}

fn resolve_external_stream_url(
    stream: &Stream,
    server_base_url: Option<&Url>,
) -> Result<String, String> {
    if matches!(stream.source, StreamSource::Torrent { .. }) && server_base_url.is_none() {
        return Err("Streaming server is not available.".to_owned());
    }
    let converted = stream
        .convert(server_base_url)
        .map_err(|_| "Streaming server is not available.".to_owned())?;
    let urls = StreamUrls::new(converted, server_base_url);
    urls.streaming_url
        .map(|u| u.to_string())
        .ok_or_else(|| "this stream cannot be opened in an external player".to_owned())
}

fn dispatch_or_external(
    runtime: &AppRuntime,
    selected: PlayerSelected,
    player_type: Option<&str>,
    server_base_url: Option<&Url>,
    details_video: Option<Video>,
) -> Result<OpenResult, String> {
    let stream_for_record = selected.stream.clone();
    match classify_selected(selected, player_type, server_base_url)? {
        OpenOutcome::OpenExternal(url) => Ok(OpenResult::OpenBrowser(url)),
        OpenOutcome::ExternalPlayer(launch) => {
            if let Some(video) = details_video {
                runtime.dispatch(RuntimeAction {
                    field: Some(AppModelField::MetaDetails),
                    action: Action::MetaDetails(ActionMetaDetails::MarkVideoAsWatched(video, true)),
                });
                runtime.dispatch(RuntimeAction {
                    field: Some(AppModelField::MetaDetails),
                    action: Action::MetaDetails(ActionMetaDetails::ExternalPlayerStreamOpened(
                        stream_for_record,
                    )),
                });
            }
            Ok(OpenResult::ExternalPlayer(launch))
        }
        OpenOutcome::Load(selected) => {
            runtime.dispatch(RuntimeAction {
                field: Some(AppModelField::Player),
                action: Action::Load(ActionLoad::Player(selected)),
            });
            Ok(OpenResult::Loaded)
        }
    }
}

pub(crate) fn open_for(runtime: &AppRuntime, id: &str) -> Result<OpenResult, String> {
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
    let player_type = model.ctx.profile.settings.player_type.clone();
    let server_base = model.streaming_server.base_url.clone();
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
    dispatch_or_external(
        runtime,
        selected,
        player_type.as_deref(),
        server_base.as_ref(),
        None,
    )
}

pub(crate) fn open_video(runtime: &AppRuntime, video_id: &str) -> Result<VideoOpen, String> {
    let model = runtime
        .model()
        .map_err(|_| "the app state is not available".to_owned())?;
    let video = details::meta_item(&model.meta_details)
        .and_then(|item| item.videos.iter().find(|v| v.id == video_id))
        .cloned();
    let Some(stream) = video
        .as_ref()
        .and_then(|v| v.stream().map(|s| s.into_owned()))
    else {
        return Ok(VideoOpen::PickFromStreams);
    };
    let meta_request = ready_meta_request(&model.meta_details);
    let player_type = model.ctx.profile.settings.player_type.clone();
    let server_base = model.streaming_server.base_url.clone();
    let selected = PlayerSelected {
        stream,
        stream_request: meta_request.clone(),
        meta_request: meta_request.clone(),
        subtitles_path: meta_request.as_ref().map(|r| {
            ResourcePath::without_extra(SUBTITLES_RESOURCE_NAME, &r.path.r#type, video_id)
        }),
    };
    drop(model);
    Ok(
        match dispatch_or_external(
            runtime,
            selected,
            player_type.as_deref(),
            server_base.as_ref(),
            video,
        )? {
            OpenResult::OpenBrowser(url) => VideoOpen::OpenBrowser(url),
            OpenResult::ExternalPlayer(launch) => VideoOpen::ExternalPlayer(launch),
            OpenResult::Loaded => VideoOpen::Opened,
        },
    )
}

pub(crate) fn open(runtime: &AppRuntime, index: usize) -> Result<OpenResult, String> {
    let model = runtime
        .model()
        .map_err(|_| "the app state is not available".to_owned())?;
    let (selected, video) = picked(&model, index).ok_or("the list changed, pick it again")?;
    let player_type = model.ctx.profile.settings.player_type.clone();
    let server_base = model.streaming_server.base_url.clone();
    drop(model);
    dispatch_or_external(
        runtime,
        selected,
        player_type.as_deref(),
        server_base.as_ref(),
        video,
    )
}

pub(crate) fn should_open(key: &str, loaded: &str, attempted: Option<&str>) -> bool {
    key != loaded && Some(key) != attempted
}

#[cfg(test)]
#[path = "open_tests.rs"]
mod tests;
