//! Core playback control and video-switching action dispatchers for the Player screen.

use stremio_core::constants::SUBTITLES_RESOURCE_NAME;
use stremio_core::models::player::{Player, Selected as PlayerSelected, VideoParams};
use stremio_core::runtime::msg::{Action, ActionLoad, ActionPlayer};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::addon::{ResourcePath, ResourceRequest};
use stremio_core::types::player::VideoScale;

use crate::app::{AppModelField, AppRuntime};

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

fn selected_for_video(player: &Player, video_id: &str) -> Result<Option<PlayerSelected>, String> {
    let current = player.selected.clone().ok_or("nothing is playing")?;
    let is_next = player.next_video.as_ref().is_some_and(|v| v.id == video_id);
    let stream = player
        .next_stream
        .clone()
        .filter(|_| is_next)
        .unwrap_or_else(|| current.stream.clone());
    let Some(request) = current
        .stream_request
        .as_ref()
        .map(|r| request_for(r, video_id))
    else {
        return Ok(None);
    };
    let subtitles_path = Some(ResourcePath::without_extra(
        SUBTITLES_RESOURCE_NAME,
        &request.path.r#type,
        video_id,
    ));
    Ok(Some(PlayerSelected {
        stream,
        stream_request: Some(request),
        meta_request: current.meta_request,
        subtitles_path,
    }))
}

/// Advances to the core's `next_video` (dispatches `NextVideo` then reloads player on it).
pub(crate) fn next(runtime: &AppRuntime) -> Result<(), String> {
    let model = runtime
        .model()
        .map_err(|_| "the app state is not available".to_owned())?;
    let next_id = model
        .player
        .next_video
        .as_ref()
        .map(|v| v.id.clone())
        .ok_or("there is no next Video")?;
    let next_selected = selected_for_video(&model.player, &next_id)?;
    drop(model);
    dispatch_player(runtime, Action::Player(ActionPlayer::NextVideo));
    if let Some(selected) = next_selected {
        dispatch_player(
            runtime,
            Action::Load(ActionLoad::Player(Box::new(selected))),
        );
    }
    Ok(())
}

/// Switches playback to `video_id` from the player's side drawer.
pub(crate) fn load_video(runtime: &AppRuntime, video_id: &str) -> Result<(), String> {
    let model = runtime
        .model()
        .map_err(|_| "the app state is not available".to_owned())?;
    let next_selected = selected_for_video(&model.player, video_id)?;
    drop(model);
    if let Some(selected) = next_selected {
        dispatch_player(
            runtime,
            Action::Load(ActionLoad::Player(Box::new(selected))),
        );
    }
    Ok(())
}

fn dispatch_player(runtime: &AppRuntime, action: Action) {
    runtime.dispatch(RuntimeAction {
        field: Some(AppModelField::Player),
        action,
    });
}

pub(crate) fn ended(runtime: &AppRuntime) {
    dispatch_player(runtime, Action::Player(ActionPlayer::Ended));
}

pub(crate) fn time_changed(runtime: &AppRuntime, time: f64, duration: f64, device: String) {
    dispatch_player(
        runtime,
        Action::Player(ActionPlayer::TimeChanged {
            time: time.max(0.0).round() as u64,
            duration: duration.max(0.0).round() as u64,
            device,
        }),
    );
}

pub(crate) fn seek(runtime: &AppRuntime, time: f64, duration: f64, device: String) {
    dispatch_player(
        runtime,
        Action::Player(ActionPlayer::Seek {
            time: time.max(0.0).round() as u64,
            duration: duration.max(0.0).round() as u64,
            device,
        }),
    );
}

pub(crate) fn paused_changed(runtime: &AppRuntime, paused: bool) {
    dispatch_player(
        runtime,
        Action::Player(ActionPlayer::PausedChanged { paused }),
    );
}

pub(crate) fn video_params_changed(
    runtime: &AppRuntime,
    size: Option<u64>,
    filename: Option<String>,
) {
    dispatch_player(
        runtime,
        Action::Player(ActionPlayer::VideoParamsChanged {
            video_params: Some(VideoParams {
                hash: None,
                size,
                filename,
            }),
        }),
    );
}

pub(crate) fn scale_changed(runtime: &AppRuntime, scale: VideoScale) {
    let Ok(model) = runtime.model() else {
        return;
    };
    let mut state = model.player.stream_state.clone().unwrap_or_default();
    state.video_scale = Some(scale);
    drop(model);
    dispatch_player(
        runtime,
        Action::Player(ActionPlayer::StreamStateChanged { state }),
    );
    dispatch_player(
        runtime,
        Action::Player(ActionPlayer::VideoScaleChanged { video_scale: scale }),
    );
}

pub(crate) fn unload(runtime: &AppRuntime) {
    dispatch_player(runtime, Action::Unload);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_for_keeps_addon_and_type() {
        let req = ResourceRequest {
            base: url::Url::parse("https://example.com/manifest.json").unwrap(),
            path: ResourcePath::without_extra("stream", "series", "tt1:1:1"),
        };
        let next = request_for(&req, "tt1:1:2");
        assert_eq!(next.base, req.base);
        assert_eq!(
            (
                next.path.resource.as_str(),
                next.path.r#type.as_str(),
                next.path.id.as_str()
            ),
            ("stream", "series", "tt1:1:2")
        );
    }
}
