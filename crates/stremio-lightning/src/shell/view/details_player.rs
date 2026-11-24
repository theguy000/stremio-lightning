//! Details, Player, and Settings screen projections.

use slint::{ModelRc, VecModel};

use super::sync::{image_rev, preview_data, select_input_data, slint_image, with_mirrors};
use crate::plumbing::images::{self, Images};
use crate::screens::details::Data as DetailsData;
use crate::screens::player::Data as PlayerData;
use crate::screens::settings::Data as SettingsData;
use crate::{
    MainWindow, PlayerEpisode, PlayerOption, PlayerTrack, PreviewData, SelectOption,
    SettingsViewData, StreamData, VideoRow,
};

pub(super) fn apply_details(
    window: &MainWindow,
    details: &DetailsData,
    wrap_streams: bool,
    images: &Images,
) {
    window.set_details_has_preview(details.preview.is_some());
    if let Some(preview) = &details.preview {
        let mut data = preview_data(preview, images, true);
        let backdrop = preview.background.as_ref().map(|k| images::backdrop_key(k));
        let backdrop = slint_image(images, &backdrop);
        data.has_panel_backdrop = backdrop.is_some();
        data.panel_backdrop = backdrop.unwrap_or_default();
        window.set_details_preview(data);
    } else {
        window.set_details_preview(PreviewData::default());
    }
    window.set_streams_loading(i32::try_from(details.sources_loading).unwrap_or_default());
    window.set_details_waits_for_streams(details.waits_for_streams);
    window.set_details_rate_visible(details.rating_known);
    window.set_details_rate_enabled(details.rating_ready);
    window.set_details_rated_liked(
        details.rating == Some(stremio_core::types::rating::Rating::Liked),
    );
    window.set_details_rated_loved(
        details.rating == Some(stremio_core::types::rating::Rating::Loved),
    );
    window.set_details_show_picker(details.show_picker);
    window.set_details_has_steps(details.steps.is_some());
    window.set_details_season_number(
        details
            .steps
            .map(|steps| i32::try_from(steps.season).unwrap_or_default())
            .unwrap_or_default(),
    );
    window.set_details_episode_number(
        details
            .steps
            .map(|steps| i32::try_from(steps.episode).unwrap_or_default())
            .unwrap_or_default(),
    );
    window.set_details_can_show_streams(details.steps.is_some_and(|s| s.ask_again));
    window.set_details_season_index(i32::try_from(details.season).unwrap_or_default());
    window.set_details_season_watched(details.season_watched);
    window.set_details_picked_label(details.picked_label.clone().unwrap_or_default().into());
    window.set_details_wrap_streams(wrap_streams);
    window.set_details_has_next_episode(details.next_episode.is_some());
    window.set_details_season_title(
        details
            .seasons
            .get(details.season)
            .map(|season| season.label.clone())
            .unwrap_or_default()
            .into(),
    );

    let season_opts: Vec<(String, bool)> = details
        .seasons
        .iter()
        .enumerate()
        .map(|(idx, s)| (s.label.clone(), idx == details.season))
        .collect();
    let picked = details.picked.as_deref();

    with_mirrors(|m| {
        if let Some(replaced) = m.details_streams.sync(
            &window.get_streams(),
            &details.streams,
            |_, src| src.clone(),
            |_, src| StreamData {
                name: src.name.clone().into(),
                description: src.description.clone().into(),
                addon: src.addon.clone().into(),
            },
        ) {
            window.set_streams(replaced);
        }
        if let Some(replaced) = m.details_seasons.sync(
            &window.get_details_seasons(),
            &season_opts,
            |_, opt| opt.clone(),
            |_, (label, selected)| SelectOption {
                label: label.clone().into(),
                selected: *selected,
            },
        ) {
            window.set_details_seasons(replaced);
        }
        if let Some(replaced) = m.details_episodes.sync(
            &window.get_details_episodes(),
            &details.episodes,
            |_, ep| {
                (
                    ep.clone(),
                    picked == Some(ep.id.as_str()),
                    image_rev(images, &ep.thumbnail),
                )
            },
            |_, ep| {
                let thumbnail = slint_image(images, &ep.thumbnail);
                VideoRow {
                    id: ep.id.clone().into(),
                    title: ep.title.clone().into(),
                    released: ep.released.clone().into(),
                    has_thumbnail: thumbnail.is_some(),
                    thumbnail: thumbnail.unwrap_or_default(),
                    selected: picked == Some(ep.id.as_str()),
                    watched: ep.watched,
                    progress: ep.progress as f32,
                }
            },
        ) {
            window.set_details_episodes(replaced);
        }
    });
}

pub(crate) fn settings_view(data: &SettingsData) -> SettingsViewData {
    SettingsViewData {
        user_email: data.user_email.clone().into(),
        logged_in: data.logged_in,
        trakt_authenticated: data.trakt_authenticated,
        streaming_server_url: data.streaming_server_url.clone().into(),
        ui_language: select_input_data(&data.ui_language),
        quit_on_close: data.quit_on_close,
        esc_exit_fullscreen: data.esc_exit_fullscreen,
        hide_spoilers: data.hide_spoilers,
        subtitles_language: select_input_data(&data.subtitles_language),
        subtitles_size: select_input_data(&data.subtitles_size),
        subtitles_text_color: select_input_data(&data.subtitles_text_color),
        subtitles_bg_color: select_input_data(&data.subtitles_bg_color),
        subtitles_outline_color: select_input_data(&data.subtitles_outline_color),
        audio_language: select_input_data(&data.audio_language),
        surround_sound: data.surround_sound,
        seek_time: select_input_data(&data.seek_time),
        seek_short_time: select_input_data(&data.seek_short_time),
        play_in_background: data.play_in_background,
        binge_watching: data.binge_watching,
        next_video_duration: select_input_data(&data.next_video_duration),
        external_player: select_input_data(&data.external_player),
        hardware_decoding: data.hardware_decoding,
        pause_on_minimize: data.pause_on_minimize,
        caching: select_input_data(&data.caching),
        torrent_profile: select_input_data(&data.torrent_profile),
        transcoding_profile: select_input_data(&data.transcoding_profile),
    }
}

pub(super) fn player_view(window: &MainWindow, data: &PlayerData, images: &Images) {
    window.set_playing(data.open);
    window.set_player_title(data.title.clone().into());
    window.set_player_error(data.error.clone().into());
    let logo = slint_image(images, &data.logo);
    window.set_player_has_logo(logo.is_some());
    window.set_player_logo(logo.unwrap_or_default());
    let backdrop = slint_image(images, &data.background);
    window.set_player_has_backdrop(backdrop.is_some());
    window.set_player_backdrop(backdrop.unwrap_or_default());
    window.set_player_options(ModelRc::new(VecModel::from(
        data.options
            .iter()
            .map(|option| PlayerOption {
                label: option.label.clone().into(),
                action: option.action.into(),
            })
            .collect::<Vec<_>>(),
    )));
    window.set_player_episodes(ModelRc::new(VecModel::from(
        data.episodes
            .iter()
            .map(|episode| PlayerEpisode {
                id: episode.id.clone().into(),
                label: episode.label.clone().into(),
                title: episode.title.clone().into(),
                selected: episode.picked,
                watched: episode.watched,
                progress: episode.progress as f32,
                released: episode.released.clone().into(),
                thumbnail: slint_image(images, &episode.thumbnail).unwrap_or_default(),
            })
            .collect::<Vec<_>>(),
    )));
    window.set_player_seasons(ModelRc::new(VecModel::from(
        data.seasons
            .iter()
            .enumerate()
            .map(|(index, (label, _))| SelectOption {
                label: label.clone().into(),
                selected: index == data.season,
            })
            .collect::<Vec<_>>(),
    )));
    window.set_player_season_index(i32::try_from(data.season).unwrap_or_default());
    window.set_player_season_title(
        data.seasons
            .get(data.season)
            .map(|(label, _)| label.clone())
            .unwrap_or_default()
            .into(),
    );
    window.set_player_has_episodes(data.seasons.len() > 1 || !data.episodes.is_empty());
    window.set_player_has_preview(data.preview.is_some());
    if let Some(preview) = &data.preview {
        window.set_player_preview(preview_data(preview, images, true));
    } else {
        window.set_player_preview(PreviewData::default());
    }
    window.set_player_next_label(data.next_label.clone().into());
    window.set_player_has_next(data.has_next);
    window.set_player_next_notice(data.next_notice as f32);
    window.set_player_scale(data.scale.into());
}

pub(crate) fn tracks(tracks: &[crate::plumbing::player::Track]) -> ModelRc<PlayerTrack> {
    ModelRc::new(VecModel::from(
        tracks
            .iter()
            .map(|track| PlayerTrack {
                id: track.id.clone().into(),
                title: track.title.clone().into(),
                language: crate::screens::language_names::label_or_code(&track.language).into(),
                selected: track.selected,
                external: track.external,
                origin: Default::default(),
            })
            .collect::<Vec<_>>(),
    ))
}
