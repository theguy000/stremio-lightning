//! What the Settings screen shows and updates: profile settings from stremio-core's `Profile`
//! (`useProfileSettingsInputs.js` in stremio-web) plus local shell/streaming preferences.
//! Licence basis: ../../../THIRD_PARTY_NOTICES.md.
//!
//! `options` holds the tables: what each setting's options are and what each option means.
//! This file holds the screen's data and the actions that change a setting.

use std::collections::HashMap;
use std::path::Path;

use serde_json::{json, Value};
use stremio_core::runtime::msg::{Action, ActionCtx, ActionStreamingServer};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::profile::{Profile, Settings as ProfileSettings};

use super::{AppModel, Input};
use crate::AppRuntime;

mod options;

pub(crate) use options::TOGGLES;

/// The settings the player itself needs, pulled out of the profile so mpv does not have to read
/// JSON.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlayerConfig {
    pub(crate) hardware_decoding: bool,
    pub(crate) subtitles_size: u32,
    pub(crate) subtitles_language: String,
    pub(crate) subtitles_text_color: String,
    pub(crate) subtitles_background_color: String,
    pub(crate) subtitles_outline_color: String,
    pub(crate) audio_language: String,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            hardware_decoding: true,
            subtitles_size: 100,
            subtitles_language: "eng".to_owned(),
            subtitles_text_color: "#FFFFFFFF".to_owned(),
            subtitles_background_color: "#00000000".to_owned(),
            subtitles_outline_color: "#FF000000".to_owned(),
            audio_language: "eng".to_owned(),
        }
    }
}

/// Settings this shell keeps beside the profile: values that stremio-web stores but the core's
/// `Profile` does not carry. They are written next to the app's data, not into the profile, and
/// merged over it when the screen is built.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(crate) struct LocalExtras {
    pub(crate) overrides: HashMap<String, Value>,
}

impl LocalExtras {
    pub(crate) fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("native_settings.json");
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub(crate) fn save(&self, data_dir: &Path) {
        let path = data_dir.join("native_settings.json");
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::create_dir_all(data_dir);
            let _ = std::fs::write(path, json);
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct Data {
    pub(crate) user_email: String,
    pub(crate) logged_in: bool,
    pub(crate) trakt_authenticated: bool,
    pub(crate) streaming_server_url: String,
    pub(crate) ui_language: Input,
    pub(crate) quit_on_close: bool,
    pub(crate) esc_exit_fullscreen: bool,
    pub(crate) hide_spoilers: bool,
    pub(crate) subtitles_language: Input,
    pub(crate) subtitles_size: Input,
    pub(crate) subtitles_text_color: Input,
    pub(crate) subtitles_bg_color: Input,
    pub(crate) subtitles_outline_color: Input,
    pub(crate) audio_language: Input,
    pub(crate) surround_sound: bool,
    pub(crate) seek_time: Input,
    pub(crate) seek_short_time: Input,
    pub(crate) play_in_background: bool,
    pub(crate) binge_watching: bool,
    pub(crate) next_video_duration: Input,
    pub(crate) external_player: Input,
    pub(crate) hardware_decoding: bool,
    pub(crate) pause_on_minimize: bool,
    pub(crate) caching: Input,
    pub(crate) torrent_profile: Input,
    pub(crate) transcoding_profile: Input,
    pub(crate) player_config: PlayerConfig,
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

pub(crate) fn data(model: &AppModel, extras: &LocalExtras) -> Data {
    data_from_profile(&model.ctx.profile, extras)
}

pub(crate) fn data_from_profile(profile: &Profile, extras: &LocalExtras) -> Data {
    let profile_json = serde_json::to_value(profile).unwrap_or_default();
    let settings_val = merged_settings_value(&profile.settings, extras);
    let input = |key: &str| options::input(key, &settings_val);

    let auth = profile_json.get("auth").filter(|v| !v.is_null());
    let logged_in = auth.is_some();
    let user_email = auth
        .and_then(|a| a.get("user"))
        .and_then(|u| u.get("email"))
        .and_then(Value::as_str)
        .map_or_else(|| "Anonymous user".to_owned(), str::to_owned);
    let trakt_authenticated = auth
        .and_then(|a| a.get("user"))
        .and_then(|u| u.get("trakt"))
        .is_some_and(|t| !t.is_null());

    let player_config = PlayerConfig {
        hardware_decoding: bool_field(&settings_val, "hardwareDecoding", true),
        subtitles_size: options::number_at("subtitlesSize", &settings_val),
        subtitles_language: options::code_at("subtitlesLanguage", &settings_val),
        subtitles_text_color: options::code_at("subtitlesTextColor", &settings_val),
        subtitles_background_color: options::code_at("subtitlesBackgroundColor", &settings_val),
        subtitles_outline_color: options::code_at("subtitlesOutlineColor", &settings_val),
        audio_language: options::code_at("audioLanguage", &settings_val),
    };

    Data {
        user_email,
        logged_in,
        trakt_authenticated,
        streaming_server_url: profile.settings.streaming_server_url.to_string(),
        ui_language: input("interfaceLanguage"),
        quit_on_close: bool_field(&settings_val, "quitOnClose", false),
        esc_exit_fullscreen: bool_field(&settings_val, "escExitFullscreen", true),
        hide_spoilers: bool_field(&settings_val, "hideSpoilers", false),
        subtitles_language: input("subtitlesLanguage"),
        subtitles_size: input("subtitlesSize"),
        subtitles_text_color: input("subtitlesTextColor"),
        subtitles_bg_color: input("subtitlesBackgroundColor"),
        subtitles_outline_color: input("subtitlesOutlineColor"),
        audio_language: input("audioLanguage"),
        surround_sound: bool_field(&settings_val, "surroundSound", false),
        seek_time: input("seekTimeDuration"),
        seek_short_time: input("seekShortTimeDuration"),
        play_in_background: bool_field(&settings_val, "playInBackground", false),
        binge_watching: bool_field(&settings_val, "bingeWatching", true),
        next_video_duration: input("nextVideoNotificationDuration"),
        external_player: input("playerType"),
        hardware_decoding: bool_field(&settings_val, "hardwareDecoding", true),
        pause_on_minimize: bool_field(&settings_val, "pauseOnMinimize", false),
        caching: input("caching"),
        torrent_profile: input("torrentProfile"),
        transcoding_profile: input("transcodingProfile"),
        player_config,
    }
}

fn update_profile_and_extras(
    current: &ProfileSettings,
    extras: &mut LocalExtras,
    key: &str,
    new_val: Value,
) -> Option<ProfileSettings> {
    extras.overrides.insert(key.to_owned(), new_val.clone());
    let mut val = serde_json::to_value(current).ok()?;
    let obj = val.as_object_mut()?;
    if obj.contains_key(key) {
        obj.insert(key.to_owned(), new_val);
        serde_json::from_value(val).ok()
    } else {
        None
    }
}

/// What "option `index` of `key`" means for the profile, already written into `extras`.
pub(crate) fn apply_select_option(
    current: &ProfileSettings,
    extras: &mut LocalExtras,
    key: &str,
    index: usize,
) -> Option<ProfileSettings> {
    let new_val = options::value_at(key, index)?;
    update_profile_and_extras(current, extras, key, new_val)
}

/// The same for a setting that is on or off.
pub(crate) fn apply_toggle_option(
    current: &ProfileSettings,
    extras: &mut LocalExtras,
    key: &str,
    enabled: bool,
) -> Option<ProfileSettings> {
    if !TOGGLES.contains(&key) {
        return None;
    }
    update_profile_and_extras(current, extras, key, json!(enabled))
}

pub(crate) fn dispatch_settings(runtime: &AppRuntime, settings: ProfileSettings) {
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Ctx(ActionCtx::UpdateSettings(settings)),
    });
}

pub(crate) fn logout(runtime: &AppRuntime) {
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Ctx(ActionCtx::Logout),
    });
}

pub(crate) fn reload_server(runtime: &AppRuntime) {
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::StreamingServer(ActionStreamingServer::Reload),
    });
}

/// What the Settings screen says about the streaming server.
pub(crate) fn server_status(state: &crate::plumbing::stream_server::State) -> String {
    use crate::plumbing::stream_server::State;
    match state {
        State::NotStarted(error) => format!("Stream server: not started ({error})"),
        State::Ready => "Stream server: ready".to_owned(),
        State::PortTaken => format!(
            "Stream server: using the one already running on {}",
            crate::plumbing::stream_server::ADDRESS
        ),
        State::NotAnswering => "Stream server: started but not answering".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_populate_all_settings_menus() {
        let profile = Profile::default();
        let extras = LocalExtras::default();
        let data = data_from_profile(&profile, &extras);
        assert_eq!(data.user_email, "Anonymous user");
        assert!(!data.logged_in);
        assert_eq!(data.ui_language.title, "English");
        assert_eq!(data.subtitles_size.title, "100%");
        assert_eq!(data.seek_time.title, "10 seconds");
        assert_eq!(data.caching.title, "2 GB");
        assert_eq!(data.torrent_profile.title, "Default");
        assert!(data.binge_watching);
        assert!(data.hardware_decoding);
    }

    #[test]
    fn selecting_and_toggling_options_updates_profile_and_local_settings() {
        let profile = Profile::default();
        let mut extras = LocalExtras::default();
        let maybe_updated = apply_select_option(&profile.settings, &mut extras, "subtitlesSize", 3);
        let updated_profile = Profile {
            settings: maybe_updated.unwrap_or_else(|| profile.settings.clone()),
            ..profile
        };
        let data = data_from_profile(&updated_profile, &extras);
        assert_eq!(data.subtitles_size.title, "150%");
        assert_eq!(data.player_config.subtitles_size, 150);

        let maybe_toggled = apply_toggle_option(
            &updated_profile.settings,
            &mut extras,
            "hardwareDecoding",
            false,
        );
        let updated_profile = Profile {
            settings: maybe_toggled.unwrap_or_else(|| updated_profile.settings.clone()),
            ..updated_profile
        };
        let data = data_from_profile(&updated_profile, &extras);
        assert!(!data.hardware_decoding);
        assert!(!data.player_config.hardware_decoding);

        let _ = apply_select_option(&updated_profile.settings, &mut extras, "caching", 3);
        let _ = apply_toggle_option(&updated_profile.settings, &mut extras, "quitOnClose", true);
        let data = data_from_profile(&updated_profile, &extras);
        assert_eq!(data.caching.title, "10 GB");
        assert!(data.quit_on_close);
    }
}
