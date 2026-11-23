//! Pure stream-opening resolution for Details, Library/Continue Watching, and Episode Watch.
//! Never launches external browsers directly; returns external URLs to `shell/`.

use stremio_core::constants::SUBTITLES_RESOURCE_NAME;
use stremio_core::models::common::Loadable;
use stremio_core::models::meta_details::MetaDetails;
use stremio_core::models::player::Selected as PlayerSelected;
use stremio_core::runtime::msg::{Action, ActionLoad};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::addon::{ResourcePath, ResourceRequest};
use stremio_core::types::resource::StreamSource;
use stremio_core::types::streams::StreamsItemKey;

use crate::app::{AppModel, AppModelField, AppRuntime};
use crate::screens::details;

/// Classification of a resolved stream: load in mpv, or open an external http(s) URL in `shell/`.
#[derive(Debug, PartialEq)]
pub(crate) enum OpenOutcome {
    Load(Box<PlayerSelected>),
    OpenExternal(String),
}

/// Result of clicking "Watch" on an Episode.
#[derive(Debug, PartialEq)]
pub(crate) enum VideoOpen {
    Opened,
    OpenExternal(String),
    PickFromStreams,
}

fn ready_meta_request(details: &MetaDetails) -> Option<ResourceRequest> {
    details
        .meta_items
        .iter()
        .find(|item| matches!(item.content, Some(Loadable::Ready(_))))
        .map(|item| item.request.clone())
}

fn picked(model: &AppModel, index: usize) -> Option<PlayerSelected> {
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
    Some(PlayerSelected {
        stream: stream.clone(),
        stream_request: Some(request.clone()),
        meta_request: ready_meta_request(&model.meta_details),
        subtitles_path: Some(ResourcePath::without_extra(
            SUBTITLES_RESOURCE_NAME,
            &request.path.r#type,
            &request.path.id,
        )),
    })
}

/// Pure classification: external streams validate their http(s) URL; playable streams load.
pub(crate) fn classify_selected(selected: PlayerSelected) -> Result<OpenOutcome, String> {
    if let StreamSource::External { external_url, .. } = &selected.stream.source {
        let Some(url) = external_url.as_ref() else {
            return Err("this external stream has no link to open".to_owned());
        };
        if !matches!(url.scheme(), "http" | "https") {
            return Err("this external stream uses an unsupported link".to_owned());
        }
        return Ok(OpenOutcome::OpenExternal(url.as_str().to_owned()));
    }
    Ok(OpenOutcome::Load(Box::new(selected)))
}

fn dispatch_or_external(
    runtime: &AppRuntime,
    selected: PlayerSelected,
) -> Result<Option<String>, String> {
    match classify_selected(selected)? {
        OpenOutcome::OpenExternal(url) => Ok(Some(url)),
        OpenOutcome::Load(selected) => {
            runtime.dispatch(RuntimeAction {
                field: Some(AppModelField::Player),
                action: Action::Load(ActionLoad::Player(selected)),
            });
            Ok(None)
        }
    }
}

/// Opens a saved title from Library / Continue Watching. Returns `Some(url)` if external.
pub(crate) fn open_for(runtime: &AppRuntime, id: &str) -> Result<Option<String>, String> {
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
    dispatch_or_external(runtime, selected)
}

/// Plays an Episode directly if its `Video` carries an embedded stream, else `PickFromStreams`.
pub(crate) fn open_video(runtime: &AppRuntime, video_id: &str) -> Result<VideoOpen, String> {
    let model = runtime
        .model()
        .map_err(|_| "the app state is not available".to_owned())?;
    let stream = details::meta_item(&model.meta_details)
        .and_then(|item| item.videos.iter().find(|v| v.id == video_id))
        .and_then(|video| video.stream().map(|s| s.into_owned()));
    let Some(stream) = stream else {
        return Ok(VideoOpen::PickFromStreams);
    };
    let meta_request = ready_meta_request(&model.meta_details);
    let selected = PlayerSelected {
        stream,
        stream_request: meta_request.clone(),
        meta_request: meta_request.clone(),
        subtitles_path: meta_request.as_ref().map(|r| {
            ResourcePath::without_extra(SUBTITLES_RESOURCE_NAME, &r.path.r#type, video_id)
        }),
    };
    drop(model);
    Ok(match dispatch_or_external(runtime, selected)? {
        Some(url) => VideoOpen::OpenExternal(url),
        None => VideoOpen::Opened,
    })
}

/// Loads the Stream the Details Screen lists at `index`. Returns `Some(url)` if external.
pub(crate) fn open(runtime: &AppRuntime, index: usize) -> Result<Option<String>, String> {
    let model = runtime
        .model()
        .map_err(|_| "the app state is not available".to_owned())?;
    let selected = picked(&model, index).ok_or("the list changed, pick it again")?;
    drop(model);
    dispatch_or_external(runtime, selected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use stremio_core::types::resource::Stream;

    fn selected_with(source: StreamSource) -> PlayerSelected {
        PlayerSelected {
            stream: Stream {
                source,
                name: None,
                description: None,
                thumbnail: None,
                subtitles: vec![],
                behavior_hints: Default::default(),
            },
            stream_request: None,
            meta_request: None,
            subtitles_path: None,
        }
    }

    #[test]
    fn classify_separates_valid_external_links_invalid_external_links_and_playable_streams() {
        let ext = selected_with(StreamSource::External {
            external_url: Some(url::Url::parse("https://example.com/watch").unwrap()),
            android_tv_url: None,
            tizen_url: None,
            webos_url: None,
        });
        assert_eq!(
            classify_selected(ext),
            Ok(OpenOutcome::OpenExternal(
                "https://example.com/watch".into()
            ))
        );

        let missing = selected_with(StreamSource::External {
            external_url: None,
            android_tv_url: None,
            tizen_url: None,
            webos_url: None,
        });
        let ftp = selected_with(StreamSource::External {
            external_url: Some(url::Url::parse("ftp://example.com/file").unwrap()),
            android_tv_url: None,
            tizen_url: None,
            webos_url: None,
        });
        assert!(classify_selected(missing).is_err());
        assert!(classify_selected(ftp).is_err());

        let playable = selected_with(StreamSource::Url {
            url: url::Url::parse("https://example.com/movie.mp4").unwrap(),
        });
        assert!(matches!(
            classify_selected(playable),
            Ok(OpenOutcome::Load(_))
        ));
    }
}
