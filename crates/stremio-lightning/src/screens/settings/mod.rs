//! Settings screen: pure projection from stremio-core's `Profile` + `LocalExtras` (no disk I/O,
//! no whole-`Profile` JSON round-trip) and settings mutation actions.

use serde_json::{json, Value};
use stremio_core::runtime::msg::{Action, ActionCtx, ActionStreamingServer};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::profile::{Profile, Settings as ProfileSettings};

use super::{AppModel, SelectMenu};
use crate::AppRuntime;

mod options;
mod tables;

pub use crate::plumbing::local_settings::LocalExtras;
pub use crate::plumbing::player::PlayerConfig;
pub(crate) use tables::TOGGLES;

#[derive(Clone, Default)]
pub struct Data {
    pub user_email: String,
    pub logged_in: bool,
    pub trakt_authenticated: bool,
    pub streaming_server_url: String,
    pub ui_language: SelectMenu,
    pub quit_on_close: bool,
    pub esc_exit_fullscreen: bool,
    pub hide_spoilers: bool,
    pub subtitles_language: SelectMenu,
    pub subtitles_size: SelectMenu,
    pub subtitles_text_color: SelectMenu,
    pub subtitles_bg_color: SelectMenu,
    pub subtitles_outline_color: SelectMenu,
    pub audio_language: SelectMenu,
    pub surround_sound: bool,
    pub seek_time: SelectMenu,
    pub seek_short_time: SelectMenu,
    pub play_in_background: bool,
    pub binge_watching: bool,
    pub next_video_duration: SelectMenu,
    pub external_player: SelectMenu,
    pub hardware_decoding: bool,
    pub pause_on_minimize: bool,
    pub caching: SelectMenu,
    pub torrent_profile: SelectMenu,
    pub transcoding_profile: SelectMenu,
    pub player_config: PlayerConfig,
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
    data_from_profile(&model.ctx.profile, extras)
}

fn data_from_profile(profile: &Profile, extras: &LocalExtras) -> Data {
    let settings_val = merged_settings_value(&profile.settings, extras);
    let input = |key: &str| options::input(key, &settings_val);

    let logged_in = profile.auth.is_some();
    let user_email = profile
        .auth
        .as_ref()
        .map_or_else(|| "Anonymous user".to_owned(), |a| a.user.email.clone());
    let trakt_authenticated = profile
        .auth
        .as_ref()
        .is_some_and(|a| a.user.trakt.is_some());

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
        wrap_streams: extras.wrap_streams,
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

pub(crate) fn apply_select_option(
    current: &ProfileSettings,
    extras: &mut LocalExtras,
    key: &str,
    index: usize,
) -> Option<ProfileSettings> {
    let new_val = options::value_at(key, index)?;
    update_profile_and_extras(current, extras, key, new_val)
}

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
    fn defaults_and_mutations_project_to_menus_and_player_config() {
        let profile = Profile::default();
        let mut extras = LocalExtras::default();
        let d = data_from_profile(&profile, &extras);
        assert_eq!(
            (d.user_email.as_str(), d.logged_in),
            ("Anonymous user", false)
        );
        assert_eq!(
            (
                d.ui_language.title.as_str(),
                d.subtitles_size.title.as_str()
            ),
            ("English", "100%")
        );
        assert_eq!(
            (d.seek_time.title.as_str(), d.caching.title.as_str()),
            ("10 seconds", "2 GB")
        );
        assert!(d.binge_watching && d.hardware_decoding);

        let updated = apply_select_option(&profile.settings, &mut extras, "subtitlesSize", 3);
        let p2 = Profile {
            settings: updated.unwrap_or_else(|| profile.settings.clone()),
            ..profile
        };
        let d2 = data_from_profile(&p2, &extras);
        assert_eq!(
            (
                d2.subtitles_size.title.as_str(),
                d2.player_config.subtitles_size
            ),
            ("150%", 150)
        );

        let toggled = apply_toggle_option(&p2.settings, &mut extras, "hardwareDecoding", false);
        let p3 = Profile {
            settings: toggled.unwrap_or_else(|| p2.settings.clone()),
            ..p2
        };
        let d3 = data_from_profile(&p3, &extras);
        assert!(!d3.hardware_decoding && !d3.player_config.hardware_decoding);

        let _ = apply_select_option(&p3.settings, &mut extras, "caching", 3);
        let _ = apply_toggle_option(&p3.settings, &mut extras, "quitOnClose", true);
        let d4 = data_from_profile(&p3, &extras);
        assert_eq!(
            (d4.caching.title.as_str(), d4.quit_on_close),
            ("10 GB", true)
        );
    }
}
