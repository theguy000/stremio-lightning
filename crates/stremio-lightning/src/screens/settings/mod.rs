//! Settings screen: pure projection from stremio-core's `Profile`, `StreamingServer`,
//! `DataExport`, and `LocalExtras`, plus settings mutation actions.

use serde_json::{json, Value};
use stremio_core::models::common::Loadable;
use stremio_core::types::profile::{Profile, Settings as ProfileSettings};

use super::{AppModel, SelectMenu};

mod actions;
pub mod cache_root;
pub mod content;
pub mod content_filter;
mod options;
pub mod search;
pub mod streaming;
mod tables;

pub(crate) use actions::{
    apply_select_option, apply_toggle_option, dispatch_settings, logout, logout_trakt,
    reload_server, server_status, start_data_export,
};

pub use crate::plumbing::local_settings::LocalExtras;
pub use crate::plumbing::player::PlayerConfig;
pub use cache_root::CacheRootView;
pub use content::ContentSettingsView;
pub use search::{search as search_settings, SearchHit};
pub use streaming::{ServerUrlEntry, StreamingView};

#[derive(Clone, Default)]
pub struct Data {
    pub user_id: String,
    pub user_email: String,
    pub logged_in: bool,
    pub trakt_authenticated: bool,
    pub streaming_server_url: String,
    pub ui_language: SelectMenu,
    pub interface_scale: u32,
    pub quit_on_close: bool,
    pub esc_exit_fullscreen: bool,
    pub hide_spoilers: bool,
    pub gamepad_support: bool,
    pub discord_rpc_enabled: bool,
    pub subtitles_language: SelectMenu,
    pub subtitles_size: SelectMenu,
    pub subtitles_offset: SelectMenu,
    pub subtitles_offset_val: u32,
    pub subtitles_text_color: SelectMenu,
    pub subtitles_bg_color: SelectMenu,
    pub subtitles_outline_color: SelectMenu,
    pub subtitles_bold: bool,
    pub ass_subtitles_styling: bool,
    pub audio_language: SelectMenu,
    pub surround_sound: bool,
    pub audio_passthrough: bool,
    pub seek_time: SelectMenu,
    pub seek_short_time: SelectMenu,
    pub seek_time_secs: f32,
    pub seek_short_time_secs: f32,
    pub play_in_background: bool,
    pub pause_on_minimize: bool,
    pub binge_watching: bool,
    pub next_video_duration: SelectMenu,
    pub external_player: SelectMenu,
    pub hardware_decoding: bool,
    pub caching: SelectMenu,
    pub torrent_profile: SelectMenu,
    pub transcoding_profile: SelectMenu,
    pub streaming: StreamingView,
    pub data_export_loading: bool,
    pub data_export_url: String,
    pub player_config: PlayerConfig,
    pub mpv_config_menu: SelectMenu,
    pub mpv_config_path: String,
    pub spotlight: crate::screens::board::SpotlightSettingsView,
    pub content: ContentSettingsView,
    pub wrap_streams: bool,
}

fn merged_settings_value(settings: &ProfileSettings, extras: &LocalExtras) -> Value {
    let mut val = serde_json::to_value(settings).unwrap_or_else(|_| json!({}));
    if let Some(obj) = val.as_object_mut() {
        for (k, v) in &extras.overrides {
            obj.insert(k.clone(), v.clone());
        }
    }
    val
}

fn bool_field(value: &Value, key: &str, default: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(default)
}

pub fn data(model: &AppModel, extras: &LocalExtras) -> Data {
    let mut out = data_from_profile(&model.ctx.profile, extras);
    let ss = streaming::project(model);
    if let Some(cache) = ss.cache_size.clone() {
        out.caching = cache;
    }
    if let Some(torrent) = ss.torrent_profile.clone() {
        out.torrent_profile = torrent;
    }
    if let Some(transcode) = ss.transcoding_profile.clone() {
        out.transcoding_profile = transcode;
    }
    out.streaming = ss;
    out.spotlight = crate::screens::board::spotlight::settings_view(model, extras);
    out.content = content::project(model, extras);
    if let Some((_, loadable)) = &model.data_export.export_url {
        out.data_export_loading = matches!(loadable, Loadable::Loading);
        if let Loadable::Ready(url) = loadable {
            out.data_export_url = url.to_string();
        }
    }
    out
}

fn data_from_profile(profile: &Profile, extras: &LocalExtras) -> Data {
    let settings_val = merged_settings_value(&profile.settings, extras);
    let input = |key: &str| options::input(key, &settings_val);

    let logged_in = profile.auth.is_some();
    let user_id = profile
        .auth
        .as_ref()
        .map_or_else(String::new, |a| a.user.id.0.clone());
    let user_email = profile
        .auth
        .as_ref()
        .map_or_else(|| "Anonymous user".to_owned(), |a| a.user.email.clone());
    let trakt_authenticated = profile
        .auth
        .as_ref()
        .is_some_and(|a| a.user.trakt.is_some());

    let subtitles_offset_val = options::number_at("subtitlesOffset", &settings_val);
    let subtitles_bold = bool_field(&settings_val, "subtitlesBold", false);
    let ass_subtitles_styling = bool_field(&settings_val, "assSubtitlesStyling", false);
    let surround_sound = bool_field(&settings_val, "surroundSound", false);
    let audio_passthrough = bool_field(&settings_val, "audioPassthrough", false);

    let player_config = PlayerConfig {
        hardware_decoding: bool_field(&settings_val, "hardwareDecoding", true),
        subtitles_size: options::number_at("subtitlesSize", &settings_val),
        subtitles_offset: subtitles_offset_val,
        subtitles_bold,
        ass_subtitles_styling,
        subtitles_language: options::code_at("subtitlesLanguage", &settings_val),
        subtitles_text_color: options::code_at("subtitlesTextColor", &settings_val),
        subtitles_background_color: options::code_at("subtitlesBackgroundColor", &settings_val),
        subtitles_outline_color: options::code_at("subtitlesOutlineColor", &settings_val),
        audio_language: options::code_at("audioLanguage", &settings_val),
        surround_sound,
        audio_passthrough,
        mpv_config_path: extras.mpv_config_path.clone(),
    };
    let mpv_config_path = extras.mpv_config_path.clone().unwrap_or_default();
    let mpv_is_default = mpv_config_path.is_empty();
    let mpv_config_menu = SelectMenu {
        title: if mpv_is_default {
            "Default".to_owned()
        } else {
            mpv_config_path.clone()
        },
        selected: usize::from(mpv_is_default),
        options: vec![
            ("Select...".to_owned(), false),
            ("Default".to_owned(), mpv_is_default),
        ],
    };

    Data {
        user_id,
        user_email,
        logged_in,
        trakt_authenticated,
        streaming_server_url: profile.settings.streaming_server_url.to_string(),
        ui_language: input("interfaceLanguage"),
        interface_scale: options::number_at("interfaceScale", &settings_val),
        quit_on_close: bool_field(&settings_val, "quitOnClose", true),
        esc_exit_fullscreen: bool_field(&settings_val, "escExitFullscreen", true),
        hide_spoilers: bool_field(&settings_val, "hideSpoilers", false),
        gamepad_support: bool_field(&settings_val, "gamepadSupport", false),
        discord_rpc_enabled: bool_field(&settings_val, "discordRpcEnabled", false),
        subtitles_language: input("subtitlesLanguage"),
        subtitles_size: input("subtitlesSize"),
        subtitles_offset: input("subtitlesOffset"),
        subtitles_offset_val,
        subtitles_text_color: input("subtitlesTextColor"),
        subtitles_bg_color: input("subtitlesBackgroundColor"),
        subtitles_outline_color: input("subtitlesOutlineColor"),
        subtitles_bold,
        ass_subtitles_styling,
        audio_language: input("audioLanguage"),
        surround_sound,
        audio_passthrough,
        seek_time: input("seekTimeDuration"),
        seek_short_time: input("seekShortTimeDuration"),
        seek_time_secs: options::number_at("seekTimeDuration", &settings_val) as f32 / 1000.0,
        seek_short_time_secs: options::number_at("seekShortTimeDuration", &settings_val) as f32
            / 1000.0,
        play_in_background: bool_field(&settings_val, "playInBackground", false),
        pause_on_minimize: bool_field(&settings_val, "pauseOnMinimize", false),
        binge_watching: bool_field(&settings_val, "bingeWatching", true),
        next_video_duration: input("nextVideoNotificationDuration"),
        external_player: input("playerType"),
        hardware_decoding: bool_field(&settings_val, "hardwareDecoding", true),
        caching: input("caching"),
        torrent_profile: input("torrentProfile"),
        transcoding_profile: input("transcodingProfile"),
        streaming: StreamingView::default(),
        data_export_loading: false,
        data_export_url: String::new(),
        player_config,
        mpv_config_menu,
        mpv_config_path,
        spotlight: Default::default(),
        content: Default::default(),
        wrap_streams: extras.wrap_streams,
    }
}

#[cfg(test)]
mod content_tests;
#[cfg(test)]
mod tests;
