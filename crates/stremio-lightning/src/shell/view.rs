//! Turning a snapshot into window properties.
//!
//! This is the only place in the app that knows both a screen's data and the window it lands in.
//! It decides nothing: every value here was decided by the screen that produced it.

use slint::{ModelRc, SharedString, VecModel, Weak};

use crate::app::Snapshot;
use crate::plumbing::images::Images;
use crate::screens::player::Data as PlayerData;
use crate::screens::settings::Data as SettingsData;
use crate::screens::Preview;
use crate::screens::{Card, Input};
use crate::{
    AddonData, CatalogRow, LinkGroup, MainWindow, PlayerEpisode, PlayerOption, PlayerTrack,
    PosterData, PreviewData, SelectInputData, SelectOption, SettingsViewData, StreamData, VideoRow,
};

/// Copies everything a snapshot describes onto the window. Runs on the window's thread.
pub(crate) fn show(weak: &Weak<MainWindow>, snapshot: Snapshot, images: &Images) {
    let images = images.clone();
    let _ = weak.upgrade_in_event_loop(move |window| apply(&window, &snapshot, &images));
}

fn apply(window: &MainWindow, snapshot: &Snapshot, images: &Images) {
    window.set_status(snapshot.status.clone().into());
    window.set_rows(ModelRc::new(VecModel::from(
        snapshot
            .rows
            .iter()
            .map(|row| CatalogRow {
                title: row.title.clone().into(),
                shape: row.shape.into(),
                cards: ModelRc::new(VecModel::from(
                    row.cards
                        .iter()
                        .map(|card| poster_data(card, images))
                        .collect::<Vec<_>>(),
                )),
            })
            .collect::<Vec<_>>(),
    )));

    window.set_discover_shape(snapshot.discover.shape.into());
    window.set_discover_cards(ModelRc::new(VecModel::from(
        snapshot
            .discover
            .cards
            .iter()
            .map(|card| poster_data(card, images))
            .collect::<Vec<_>>(),
    )));
    window.set_discover_inputs(inputs(snapshot.discover.inputs.clone()));
    window.set_discover_previews(ModelRc::new(VecModel::from(
        snapshot
            .discover
            .previews
            .iter()
            .map(|preview| preview_data(preview, images))
            .collect::<Vec<_>>(),
    )));

    window.set_addons(ModelRc::new(VecModel::from(
        crate::screens::addons::visible(&snapshot.addons.cards, &window.get_addons_query())
            .into_iter()
            .map(|card| {
                let logo = images.get(&card.logo);
                AddonData {
                    name: card.name.clone().into(),
                    version: card.version.clone().into(),
                    types: card.types.clone().into(),
                    description: card.description.clone().into(),
                    has_logo: logo.is_some(),
                    logo: logo.unwrap_or_default(),
                }
            })
            .collect::<Vec<_>>(),
    )));
    window.set_addon_inputs(inputs(snapshot.addons.inputs.clone()));
    window.set_addons_message(snapshot.addons.message.clone().into());

    window.set_settings(settings_view(&snapshot.settings));
    player_view(window, &snapshot.player, images);

    let details = &snapshot.details;
    window.set_details_has_preview(details.preview.is_some());
    if let Some(preview) = &details.preview {
        window.set_details_preview(preview_data(preview, images));
    }
    window.set_streams(ModelRc::new(VecModel::from(
        details
            .streams
            .iter()
            .map(|source| StreamData {
                name: source.name.clone().into(),
                description: source.description.clone().into(),
                addon: source.addon.clone().into(),
            })
            .collect::<Vec<_>>(),
    )));
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
    window
        .set_details_can_show_streams(details.steps.map(|steps| steps.ask_again).unwrap_or(false));
    window.set_details_season_index(i32::try_from(details.season).unwrap_or_default());
    window.set_details_picked_label(details.picked_label.clone().unwrap_or_default().into());
    window.set_details_wrap_streams(snapshot.settings.wrap_streams);
    window.set_details_has_next_episode(details.next_episode.is_some());
    window.set_details_season_title(
        details
            .seasons
            .get(details.season)
            .map(|season| season.label.clone())
            .unwrap_or_default()
            .into(),
    );
    window.set_details_seasons(ModelRc::new(VecModel::from(
        details
            .seasons
            .iter()
            .enumerate()
            .map(|(index, season)| SelectOption {
                label: season.label.clone().into(),
                selected: index == details.season,
            })
            .collect::<Vec<_>>(),
    )));
    window.set_details_episodes(ModelRc::new(VecModel::from(
        details
            .episodes
            .iter()
            .map(|episode| {
                let thumbnail = images.get(&episode.thumbnail);
                VideoRow {
                    id: episode.id.clone().into(),
                    title: episode.title.clone().into(),
                    released: episode.released.clone().into(),
                    has_thumbnail: thumbnail.is_some(),
                    thumbnail: thumbnail.unwrap_or_default(),
                    selected: details.picked.as_deref() == Some(episode.id.as_str()),
                    watched: episode.watched,
                    progress: episode.progress as f32,
                }
            })
            .collect::<Vec<_>>(),
    )));
}

/// The preview panel's content — the same panel Discover opens on hover and the details screen
/// shows beside its list.
fn preview_data(preview: &Preview, images: &Images) -> PreviewData {
    let logo = images.get(&preview.logo);
    let background = images.get(&preview.background);
    PreviewData {
        title: preview.title.clone().into(),
        runtime: preview.runtime.clone().into(),
        release_info: preview.release.clone().into(),
        rating: preview.rating.clone().into(),
        description: preview.description.clone().into(),
        groups: ModelRc::new(VecModel::from(
            preview
                .groups
                .iter()
                .map(|(label, links)| LinkGroup {
                    label: label.clone().into(),
                    links: ModelRc::new(VecModel::from(
                        links
                            .iter()
                            .map(|link| SharedString::from(link.clone()))
                            .collect::<Vec<_>>(),
                    )),
                })
                .collect::<Vec<_>>(),
        )),
        has_trailer: preview.has_trailer,
        has_logo: logo.is_some(),
        logo: logo.unwrap_or_default(),
        has_background: background.is_some(),
        background: background.unwrap_or_default(),
    }
}

fn poster_data(card: &Card, images: &Images) -> PosterData {
    let poster = images.get(&card.poster);
    PosterData {
        item_id: card.id.clone().into(),
        kind: card.kind.clone().into(),
        title: card.title.clone().into(),
        has_poster: poster.is_some(),
        poster: poster.unwrap_or_default(),
    }
}

fn inputs(inputs: Vec<Input>) -> ModelRc<SelectInputData> {
    ModelRc::new(VecModel::from(
        inputs
            .into_iter()
            .map(crate::screens::select_input_data)
            .collect::<Vec<_>>(),
    ))
}

pub(crate) fn settings_view(data: &SettingsData) -> SettingsViewData {
    let select = crate::screens::select_input_data;
    SettingsViewData {
        user_email: data.user_email.clone().into(),
        logged_in: data.logged_in,
        trakt_authenticated: data.trakt_authenticated,
        streaming_server_url: data.streaming_server_url.clone().into(),
        ui_language: select(data.ui_language.clone()),
        quit_on_close: data.quit_on_close,
        esc_exit_fullscreen: data.esc_exit_fullscreen,
        hide_spoilers: data.hide_spoilers,
        subtitles_language: select(data.subtitles_language.clone()),
        subtitles_size: select(data.subtitles_size.clone()),
        subtitles_text_color: select(data.subtitles_text_color.clone()),
        subtitles_bg_color: select(data.subtitles_bg_color.clone()),
        subtitles_outline_color: select(data.subtitles_outline_color.clone()),
        audio_language: select(data.audio_language.clone()),
        surround_sound: data.surround_sound,
        seek_time: select(data.seek_time.clone()),
        seek_short_time: select(data.seek_short_time.clone()),
        play_in_background: data.play_in_background,
        binge_watching: data.binge_watching,
        next_video_duration: select(data.next_video_duration.clone()),
        external_player: select(data.external_player.clone()),
        hardware_decoding: data.hardware_decoding,
        pause_on_minimize: data.pause_on_minimize,
        caching: select(data.caching.clone()),
        torrent_profile: select(data.torrent_profile.clone()),
        transcoding_profile: select(data.transcoding_profile.clone()),
    }
}

/// Everything the player screen draws that the core decided. The values mpv decides -- where the
/// play head is, the tracks, the volume -- are set by the wiring as they change, so they are left
/// alone here and do not flicker back to their defaults on every core event.
fn player_view(window: &MainWindow, data: &PlayerData, images: &Images) {
    window.set_playing(data.open);
    window.set_player_title(data.title.clone().into());
    window.set_player_error(data.error.clone().into());
    let logo = images.get(&data.logo);
    window.set_player_has_logo(logo.is_some());
    window.set_player_logo(logo.unwrap_or_default());
    let backdrop = images.get(&data.background);
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
                thumbnail: images.get(&episode.thumbnail).unwrap_or_default(),
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
        window.set_player_preview(preview_data(preview, images));
    }
    window.set_player_next_label(data.next_label.clone().into());
    window.set_player_has_next(data.has_next);
    window.set_player_next_notice(data.next_notice as f32);
    window.set_player_scale(data.scale.into());
}

/// The tracks mpv offers, as the menus draw them.
pub(crate) fn tracks(tracks: &[crate::plumbing::player::Track]) -> ModelRc<PlayerTrack> {
    ModelRc::new(VecModel::from(
        tracks
            .iter()
            .map(|track| PlayerTrack {
                id: track.id.clone().into(),
                title: track.title.clone().into(),
                language: track.language.clone().into(),
                selected: track.selected,
            })
            .collect::<Vec<_>>(),
    ))
}
