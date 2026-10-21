//! Turning a snapshot into window properties.
//!
//! This is the only place in the app that knows both a screen's data and the window it lands in.
//! It decides nothing: every value here was decided by the screen that produced it.

use slint::{ModelRc, SharedString, VecModel, Weak};

use crate::app::Snapshot;
use crate::plumbing::images::Images;
use crate::screens::settings::Data as SettingsData;
use crate::screens::{Card, Input};
use crate::{
    AddonData, CatalogRow, LinkGroup, MainWindow, PosterData, PreviewData, SelectInputData,
    SettingsViewData, StreamData,
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
            .map(|preview| {
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
            })
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

    let details = &snapshot.details;
    if let Some(title) = details.title.clone() {
        let poster = images.get(&details.poster);
        window.set_details_title(title.into());
        window.set_details_description(details.description.clone().into());
        window.set_details_has_poster(poster.is_some());
        window.set_details_poster(poster.unwrap_or_default());
    }
    window.set_details_status(details.status.clone().into());
    window.set_streams(ModelRc::new(VecModel::from(
        details
            .streams
            .iter()
            .map(|(name, description)| StreamData {
                name: name.clone().into(),
                description: description.clone().into(),
            })
            .collect::<Vec<_>>(),
    )));
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
