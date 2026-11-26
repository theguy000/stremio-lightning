//! Core action dispatchers for the Details Screen (open, pick Episode, rating, library, watched).

use stremio_core::constants::{META_RESOURCE_NAME, STREAM_RESOURCE_NAME};
use stremio_core::models::meta_details::Selected as DetailsSelected;
use stremio_core::runtime::msg::{Action, ActionCtx, ActionLoad, ActionMetaDetails};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::addon::ResourcePath;
use stremio_core::types::rating::Rating;

use super::meta_item;
use crate::screens::videos::season_all_watched;
use crate::AppRuntime;

/// Web: `MetaDetails` / `Rate` — updates the title's like/love rating.
pub(crate) fn rate(runtime: &AppRuntime, status: Option<Rating>) {
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::MetaDetails(ActionMetaDetails::Rate(status)),
    });
}

/// Web: adds or removes the open title from the library.
pub(crate) fn toggle_library(runtime: &AppRuntime) {
    let Ok(model) = runtime.model() else {
        return;
    };
    let Some(item) = meta_item(&model.meta_details).map(|item| item.preview.clone()) else {
        return;
    };
    let (in_library, _) = crate::screens::library_state(&model, &item.id);
    drop(model);
    let action = if in_library {
        ActionCtx::RemoveFromLibrary(item.id)
    } else {
        ActionCtx::AddToLibrary(item)
    };
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Ctx(action),
    });
}

/// Web: toggles watched state for the open title as a whole.
pub(crate) fn toggle_watched(runtime: &AppRuntime) {
    let Ok(model) = runtime.model() else {
        return;
    };
    let Some(id) = meta_item(&model.meta_details).map(|item| item.preview.id.clone()) else {
        return;
    };
    let (_, watched) = crate::screens::library_state(&model, &id);
    drop(model);
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::MetaDetails(ActionMetaDetails::MarkAsWatched(!watched)),
    });
}

/// Web: Episode context menu -> "Mark as watched" / "Mark as non-watched".
pub fn mark_video_watched(runtime: &AppRuntime, video_id: &str) {
    let Ok(model) = runtime.model() else {
        return;
    };
    let Some(video) = meta_item(&model.meta_details)
        .and_then(|item| item.videos.iter().find(|video| video.id == video_id))
        .cloned()
    else {
        return;
    };
    let watched = model
        .meta_details
        .watched
        .as_ref()
        .is_some_and(|w| w.get_video(video_id));
    drop(model);
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::MetaDetails(ActionMetaDetails::MarkVideoAsWatched(video, !watched)),
    });
}

/// Web: Episode context menu -> "Mark rest as watched" / "Mark rest as non-watched".
pub fn mark_season_watched(runtime: &AppRuntime, season: u32) {
    let Ok(model) = runtime.model() else {
        return;
    };
    let Some(item) = meta_item(&model.meta_details) else {
        return;
    };
    let watched = season_all_watched(&item.videos, season, model.meta_details.watched.as_ref());
    drop(model);
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::MetaDetails(ActionMetaDetails::MarkSeasonAsWatched(season, !watched)),
    });
}

/// Opens a title on the Details Screen (series guess a stream until an Episode is picked).
pub(crate) fn open(runtime: &AppRuntime, id: &str, kind: &str) {
    let (stream_path, guess_stream) = if kind == "movie" {
        (
            Some(ResourcePath::without_extra(STREAM_RESOURCE_NAME, kind, id)),
            false,
        )
    } else {
        (None, false)
    };
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Load(ActionLoad::MetaDetails(DetailsSelected {
            meta_path: ResourcePath::without_extra(META_RESOURCE_NAME, kind, id),
            stream_path,
            guess_stream,
        })),
    });
}

/// Picks an Episode by Video id and requests its Streams.
pub(crate) fn pick(runtime: &AppRuntime, video_id: &str) {
    let Ok(model) = runtime.model() else {
        return;
    };
    let Some(selected) = model.meta_details.selected.clone() else {
        return;
    };
    let kind = selected.meta_path.r#type.clone();
    drop(model);
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Load(ActionLoad::MetaDetails(DetailsSelected {
            meta_path: selected.meta_path,
            stream_path: Some(ResourcePath::without_extra(
                STREAM_RESOURCE_NAME,
                &kind,
                video_id,
            )),
            guess_stream: true,
        })),
    });
}

/// Returns from a picked Episode's Streams List to the Episode Picker.
pub(crate) fn back_to_episodes(runtime: &AppRuntime) {
    let Ok(model) = runtime.model() else {
        return;
    };
    let Some(selected) = model.meta_details.selected.clone() else {
        return;
    };
    drop(model);
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Load(ActionLoad::MetaDetails(DetailsSelected {
            meta_path: selected.meta_path,
            stream_path: None,
            guess_stream: false,
        })),
    });
}
