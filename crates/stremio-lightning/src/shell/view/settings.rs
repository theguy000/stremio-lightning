//! Projection from `screens::settings::Data` to Slint `SettingsViewData`.

use std::sync::Mutex;

use slint::{Color, ComponentHandle, ModelRc, VecModel};

use super::sync::{select_input_data, slint_image};
use crate::plumbing::images::Images;
use crate::plumbing::links;
use crate::screens::settings::Data as SettingsData;
use crate::{MainWindow, PlayerUi, ServerUrlRow, SettingsUi, SettingsViewData, Theme};

static LAST_EXPORT_URL: Mutex<String> = Mutex::new(String::new());

fn parse_argb(hex: &str, fallback: Color) -> Color {
    let s = hex.trim().trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(s.get(i..i + 2)?, 16).ok();
    (|| match s.len() {
        8 => Some(Color::from_argb_u8(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
        6 => Some(Color::from_rgb_u8(byte(0)?, byte(2)?, byte(4)?)),
        _ => None,
    })()
    .unwrap_or(fallback)
}

pub(crate) fn settings_view(data: &SettingsData, images: &Images) -> SettingsViewData {
    let server_urls = ModelRc::new(VecModel::from(
        data.streaming
            .server_urls
            .iter()
            .map(|u| ServerUrlRow {
                url: u.url.clone().into(),
                selected: u.selected,
                removable: u.removable,
            })
            .collect::<Vec<_>>(),
    ));
    let text_hex = parse_argb(
        &data.player_config.subtitles_text_color,
        Color::from_rgb_u8(255, 255, 255),
    );
    let bg_hex = parse_argb(
        &data.player_config.subtitles_background_color,
        Color::from_argb_u8(0, 0, 0, 0),
    );
    let outline_hex = parse_argb(
        &data.player_config.subtitles_outline_color,
        Color::from_rgb_u8(0, 0, 0),
    );

    let preview_bg = slint_image(images, &data.spotlight.preview_bg);
    let preview_logo = slint_image(images, &data.spotlight.preview_logo);

    SettingsViewData {
        user_email: data.user_email.clone().into(),
        logged_in: data.logged_in,
        trakt_authenticated: data.trakt_authenticated,
        streaming_server_url: data.streaming_server_url.clone().into(),
        ui_language: select_input_data(&data.ui_language),
        interface_scale: data.interface_scale as i32,
        quit_on_close: data.quit_on_close,
        esc_exit_fullscreen: data.esc_exit_fullscreen,
        hide_spoilers: data.hide_spoilers,
        gamepad_support: data.gamepad_support,
        subtitles_language: select_input_data(&data.subtitles_language),
        subtitles_size: select_input_data(&data.subtitles_size),
        subtitles_size_val: data.player_config.subtitles_size as i32,
        subtitles_offset: select_input_data(&data.subtitles_offset),
        subtitles_offset_val: data.subtitles_offset_val as i32,
        subtitles_text_color: select_input_data(&data.subtitles_text_color),
        subtitles_text_color_hex: text_hex,
        subtitles_bg_color: select_input_data(&data.subtitles_bg_color),
        subtitles_bg_color_hex: bg_hex,
        subtitles_outline_color: select_input_data(&data.subtitles_outline_color),
        subtitles_outline_color_hex: outline_hex,
        subtitles_bold: data.subtitles_bold,
        ass_subtitles_styling: data.ass_subtitles_styling,
        audio_language: select_input_data(&data.audio_language),
        surround_sound: data.surround_sound,
        audio_passthrough: data.audio_passthrough,
        seek_time: select_input_data(&data.seek_time),
        seek_short_time: select_input_data(&data.seek_short_time),
        play_in_background: data.play_in_background,
        pause_on_minimize: data.pause_on_minimize,
        binge_watching: data.binge_watching,
        next_video_duration: select_input_data(&data.next_video_duration),
        external_player: select_input_data(&data.external_player),
        hardware_decoding: data.hardware_decoding,
        caching: select_input_data(&data.caching),
        torrent_profile: select_input_data(&data.torrent_profile),
        transcoding_profile: select_input_data(&data.transcoding_profile),
        has_https_endpoint: data.streaming.has_https_endpoint,
        https_endpoint: select_input_data(&data.streaming.https_endpoint),
        has_transcoding_profile: data.streaming.has_transcoding_profile,
        server_online: data.streaming.status == "online",
        server_loading: data.streaming.status == "loading",
        server_version: data.streaming.server_version.clone().into(),
        remote_url: data.streaming.remote_url.clone().into(),
        server_urls,
        data_export_loading: data.data_export_loading,
        has_cache_root: data.streaming.cache_root.supported,
        cache_root: select_input_data(&data.streaming.cache_root.menu),
        cache_root_path: data.streaming.cache_root.current.clone().into(),
        cache_root_updating: data.streaming.cache_root.updating,
        cache_root_error: data.streaming.cache_root.error.clone().into(),
        mpv_config: select_input_data(&data.mpv_config_menu),
        mpv_config_path: data.mpv_config_path.clone().into(),
        discord_rpc_enabled: data.discord_rpc_enabled,
        spotlight_enabled: data.spotlight.enabled,
        spotlight_show_rows: data.spotlight.show_catalog_rows,
        spotlight_is_default: data.spotlight.is_default,
        spotlight_empty: data.spotlight.catalogs.is_empty(),
        spotlight_picks_count: data.spotlight.picks_count as i32,
        spotlight_picks_summary: data.spotlight.picks_summary.clone().into(),
        spotlight_picks_titles: data.spotlight.picks_titles.clone().into(),
        spotlight_picks_addon: data.spotlight.picks_addon.clone().into(),
        spotlight_has_preview_bg: preview_bg.is_some(),
        spotlight_preview_bg: preview_bg.unwrap_or_default(),
        spotlight_has_preview_logo: preview_logo.is_some(),
        spotlight_preview_logo: preview_logo.unwrap_or_default(),
        spotlight_preview_title: data.spotlight.preview_title.clone().into(),
        spotlight_preview_count: data.spotlight.preview_count.clone().into(),
        spotlight_preview_slides_count: data.spotlight.preview_slides_count as i32,
    }
}

pub(crate) fn apply_settings(window: &MainWindow, data: &SettingsData, images: &Images) {
    let ui = window.global::<SettingsUi>();
    ui.set_data(settings_view(data, images));
    super::spotlight::apply_spotlight_picker(window, &data.spotlight, images);
    let scale = (data.interface_scale as f32 / 100.0).clamp(0.75, 1.75);
    window.global::<Theme>().set_ui_scale(scale);
    let player_ui = window.global::<PlayerUi>();
    player_ui.set_seek_step(data.seek_time_secs);
    player_ui.set_seek_short_step(data.seek_short_time_secs);
    player_ui.set_esc_exit_fullscreen(data.esc_exit_fullscreen);

    if !data.data_export_url.is_empty() {
        let mut last = LAST_EXPORT_URL.lock().unwrap_or_else(|e| e.into_inner());
        if *last != data.data_export_url {
            *last = data.data_export_url.clone();
            links::open(&data.data_export_url);
        }
    }
}
