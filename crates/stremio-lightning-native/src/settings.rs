// What the Settings screen shows and updates: profile settings from stremio-core's `Profile`
// (`useProfileSettingsInputs.js` in stremio-web) plus local shell/streaming preferences.
// Licence basis: ../THIRD_PARTY_NOTICES.md.
use std::collections::HashMap;
use std::path::Path;

use serde_json::{json, Value};
use stremio_core::runtime::msg::{Action, ActionCtx, ActionStreamingServer};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::profile::{Profile, Settings as ProfileSettings};

use super::{AppModel, AppRuntime, Input};

const INTERFACE_LANGUAGES: &[(&str, &str)] = &[
    ("eng", "English"),
    ("spa", "Español"),
    ("fra", "Français"),
    ("deu", "Deutsch"),
    ("por", "Português"),
    ("ita", "Italiano"),
    ("rus", "Русский"),
    ("pol", "Polski"),
    ("nld", "Nederlands"),
    ("tur", "Türkçe"),
    ("ara", "العربية"),
    ("hin", "हिन्दी"),
    ("ben", "বাংলা"),
    ("jpn", "日本語"),
    ("kor", "한국어"),
    ("zho", "中文"),
];

const SUBTITLES_LANGUAGES: &[(&str, &str)] = &[
    ("none", "None"),
    ("eng", "English"),
    ("spa", "Spanish"),
    ("fra", "French"),
    ("deu", "German"),
    ("por", "Portuguese"),
    ("ita", "Italian"),
    ("rus", "Russian"),
    ("pol", "Polish"),
    ("nld", "Dutch"),
    ("tur", "Turkish"),
    ("ara", "Arabic"),
    ("hin", "Hindi"),
    ("ben", "Bengali"),
    ("jpn", "Japanese"),
    ("kor", "Korean"),
    ("zho", "Chinese"),
];

const AUDIO_LANGUAGES: &[(&str, &str)] = &[
    ("eng", "English"),
    ("spa", "Spanish"),
    ("fra", "French"),
    ("deu", "German"),
    ("por", "Portuguese"),
    ("ita", "Italian"),
    ("rus", "Russian"),
    ("pol", "Polish"),
    ("nld", "Dutch"),
    ("tur", "Turkish"),
    ("ara", "Arabic"),
    ("hin", "Hindi"),
    ("ben", "Bengali"),
    ("jpn", "Japanese"),
    ("kor", "Korean"),
    ("zho", "Chinese"),
];

const SUBTITLES_SIZES: &[(u32, &str)] = &[
    (75, "75%"),
    (100, "100%"),
    (125, "125%"),
    (150, "150%"),
    (175, "175%"),
    (200, "200%"),
    (250, "250%"),
];

const SUBTITLES_TEXT_COLORS: &[(&str, &str)] = &[
    ("#FFFFFFFF", "White"),
    ("#FFFFEB3B", "Yellow"),
    ("#FF00BCD4", "Cyan"),
    ("#FF4CAF50", "Green"),
    ("#FFE91E63", "Magenta"),
];

const SUBTITLES_BG_COLORS: &[(&str, &str)] = &[
    ("#00000000", "None"),
    ("#80000000", "Semi-transparent"),
    ("#FF000000", "Black"),
    ("#CC1E1E1E", "Dark Grey"),
];

const SUBTITLES_OUTLINE_COLORS: &[(&str, &str)] = &[
    ("#FF000000", "Black"),
    ("#00000000", "None"),
    ("#FF333333", "Dark Grey"),
    ("#FFFFFFFF", "White"),
];

const SEEK_DURATIONS: &[(u32, &str)] = &[
    (3000, "3 seconds"),
    (5000, "5 seconds"),
    (10000, "10 seconds"),
    (15000, "15 seconds"),
    (20000, "20 seconds"),
    (30000, "30 seconds"),
];

const NEXT_VIDEO_DURATIONS: &[(u32, &str)] = &[
    (0, "Disabled"),
    (5000, "5 seconds"),
    (10000, "10 seconds"),
    (15000, "15 seconds"),
    (20000, "20 seconds"),
    (25000, "25 seconds"),
    (30000, "30 seconds"),
    (35000, "35 seconds"),
    (40000, "40 seconds"),
    (45000, "45 seconds"),
    (50000, "50 seconds"),
    (55000, "55 seconds"),
    (60000, "60 seconds"),
];

const EXTERNAL_PLAYERS: &[(&str, &str)] = &[
    ("", "Disabled"),
    ("m3u", "M3U Playlist"),
    ("vlc", "VLC"),
    ("mpv", "MPV"),
];

const CACHING_OPTIONS: &[(&str, &str)] = &[
    ("2147483648", "2 GB"),
    ("0", "No caching"),
    ("5368709120", "5 GB"),
    ("10737418240", "10 GB"),
    ("infinite", "Infinite"),
];

const TORRENT_PROFILES: &[(&str, &str)] = &[
    ("default", "Default"),
    ("soft", "Soft"),
    ("fast", "Fast"),
    ("ultra-fast", "Ultra Fast"),
];

const TRANSCODING_PROFILES: &[(&str, &str)] = &[("none", "Disabled"), ("default", "Default")];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerConfig {
    pub hardware_decoding: bool,
    pub subtitles_size: u32,
    pub subtitles_language: String,
    pub subtitles_text_color: String,
    pub subtitles_background_color: String,
    pub subtitles_outline_color: String,
    pub audio_language: String,
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

#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct LocalExtras {
    pub(super) overrides: HashMap<String, Value>,
}

impl LocalExtras {
    pub(super) fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("native_settings.json");
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub(super) fn save(&self, data_dir: &Path) {
        let path = data_dir.join("native_settings.json");
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::create_dir_all(data_dir);
            let _ = std::fs::write(path, json);
        }
    }
}

#[derive(Clone, Default)]
pub(super) struct SettingsData {
    pub(super) user_email: String,
    pub(super) logged_in: bool,
    pub(super) trakt_authenticated: bool,
    pub(super) streaming_server_url: String,
    pub(super) ui_language: Input,
    pub(super) quit_on_close: bool,
    pub(super) esc_exit_fullscreen: bool,
    pub(super) hide_spoilers: bool,
    pub(super) subtitles_language: Input,
    pub(super) subtitles_size: Input,
    pub(super) subtitles_text_color: Input,
    pub(super) subtitles_bg_color: Input,
    pub(super) subtitles_outline_color: Input,
    pub(super) audio_language: Input,
    pub(super) surround_sound: bool,
    pub(super) seek_time: Input,
    pub(super) seek_short_time: Input,
    pub(super) play_in_background: bool,
    pub(super) binge_watching: bool,
    pub(super) next_video_duration: Input,
    pub(super) external_player: Input,
    pub(super) hardware_decoding: bool,
    pub(super) pause_on_minimize: bool,
    pub(super) caching: Input,
    pub(super) torrent_profile: Input,
    pub(super) transcoding_profile: Input,
    pub(super) player_config: PlayerConfig,
}

fn input_from_str_list(pairs: &[(&str, &str)], current: &str) -> Input {
    let selected = pairs
        .iter()
        .position(|(code, _)| code.eq_ignore_ascii_case(current))
        .unwrap_or(0);
    let title = pairs
        .get(selected)
        .map_or_else(String::new, |(_, label)| (*label).to_owned());
    Input {
        title,
        selected,
        options: pairs
            .iter()
            .enumerate()
            .map(|(i, (_, label))| ((*label).to_owned(), i == selected))
            .collect(),
        requests: vec![],
    }
}

fn input_from_u32_list(pairs: &[(u32, &str)], current: u32) -> Input {
    let selected = pairs
        .iter()
        .position(|(val, _)| *val == current)
        .unwrap_or(0);
    let title = pairs
        .get(selected)
        .map_or_else(String::new, |(_, label)| (*label).to_owned());
    Input {
        title,
        selected,
        options: pairs
            .iter()
            .enumerate()
            .map(|(i, (_, label))| ((*label).to_owned(), i == selected))
            .collect(),
        requests: vec![],
    }
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

fn str_field<'a>(value: &'a Value, key: &str, default: &'a str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or(default)
}

fn bool_field(value: &Value, key: &str, default: bool) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn u32_field(value: &Value, key: &str, default: u32) -> u32 {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .unwrap_or(default)
}

pub(super) fn settings_data(model: &AppModel, extras: &LocalExtras) -> SettingsData {
    settings_data_from_profile(&model.ctx.profile, extras)
}

pub(super) fn settings_data_from_profile(profile: &Profile, extras: &LocalExtras) -> SettingsData {
    let profile_json = serde_json::to_value(profile).unwrap_or_default();
    let settings_val = merged_settings_value(&profile.settings, extras);

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

    let interface_lang = str_field(&settings_val, "interfaceLanguage", "eng");
    let subtitles_lang = match settings_val.get("subtitlesLanguage") {
        Some(Value::Null) => "none",
        Some(Value::String(s)) if s.is_empty() => "none",
        Some(Value::String(s)) => s.as_str(),
        _ => "eng",
    };
    let subtitles_size = u32_field(&settings_val, "subtitlesSize", 100);
    let subtitles_text_color = str_field(&settings_val, "subtitlesTextColor", "#FFFFFFFF");
    let subtitles_bg_color = str_field(&settings_val, "subtitlesBackgroundColor", "#00000000");
    let subtitles_outline_color = str_field(&settings_val, "subtitlesOutlineColor", "#FF000000");
    let audio_lang = str_field(&settings_val, "audioLanguage", "eng");
    let surround_sound = bool_field(&settings_val, "surroundSound", false);
    let seek_time = u32_field(&settings_val, "seekTimeDuration", 10000);
    let seek_short_time = u32_field(&settings_val, "seekShortTimeDuration", 3000);
    let play_in_background = bool_field(&settings_val, "playInBackground", false);
    let binge_watching = bool_field(&settings_val, "bingeWatching", true);
    let next_video_duration = u32_field(&settings_val, "nextVideoNotificationDuration", 35000);
    let external_player = str_field(&settings_val, "playerType", "");
    let hardware_decoding = bool_field(&settings_val, "hardwareDecoding", true);
    let pause_on_minimize = bool_field(&settings_val, "pauseOnMinimize", false);
    let esc_exit_fullscreen = bool_field(&settings_val, "escExitFullscreen", true);
    let quit_on_close = bool_field(&settings_val, "quitOnClose", false);
    let hide_spoilers = bool_field(&settings_val, "hideSpoilers", false);
    let caching = str_field(&settings_val, "caching", "2147483648");
    let torrent_profile = str_field(&settings_val, "torrentProfile", "default");
    let transcoding_profile = str_field(&settings_val, "transcodingProfile", "none");

    let player_config = PlayerConfig {
        hardware_decoding,
        subtitles_size,
        subtitles_language: subtitles_lang.to_owned(),
        subtitles_text_color: subtitles_text_color.to_owned(),
        subtitles_background_color: subtitles_bg_color.to_owned(),
        subtitles_outline_color: subtitles_outline_color.to_owned(),
        audio_language: audio_lang.to_owned(),
    };

    SettingsData {
        user_email,
        logged_in,
        trakt_authenticated,
        streaming_server_url: profile.settings.streaming_server_url.to_string(),
        ui_language: input_from_str_list(INTERFACE_LANGUAGES, interface_lang),
        quit_on_close,
        esc_exit_fullscreen,
        hide_spoilers,
        subtitles_language: input_from_str_list(SUBTITLES_LANGUAGES, subtitles_lang),
        subtitles_size: input_from_u32_list(SUBTITLES_SIZES, subtitles_size),
        subtitles_text_color: input_from_str_list(SUBTITLES_TEXT_COLORS, subtitles_text_color),
        subtitles_bg_color: input_from_str_list(SUBTITLES_BG_COLORS, subtitles_bg_color),
        subtitles_outline_color: input_from_str_list(
            SUBTITLES_OUTLINE_COLORS,
            subtitles_outline_color,
        ),
        audio_language: input_from_str_list(AUDIO_LANGUAGES, audio_lang),
        surround_sound,
        seek_time: input_from_u32_list(SEEK_DURATIONS, seek_time),
        seek_short_time: input_from_u32_list(SEEK_DURATIONS, seek_short_time),
        play_in_background,
        binge_watching,
        next_video_duration: input_from_u32_list(NEXT_VIDEO_DURATIONS, next_video_duration),
        external_player: input_from_str_list(EXTERNAL_PLAYERS, external_player),
        hardware_decoding,
        pause_on_minimize,
        caching: input_from_str_list(CACHING_OPTIONS, caching),
        torrent_profile: input_from_str_list(TORRENT_PROFILES, torrent_profile),
        transcoding_profile: input_from_str_list(TRANSCODING_PROFILES, transcoding_profile),
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

pub(super) fn apply_select_option(
    current: &ProfileSettings,
    extras: &mut LocalExtras,
    key: &str,
    index: usize,
) -> Option<ProfileSettings> {
    let new_val = match key {
        "interfaceLanguage" => json!(INTERFACE_LANGUAGES.get(index)?.0),
        "subtitlesLanguage" => {
            let (code, _) = *SUBTITLES_LANGUAGES.get(index)?;
            if code == "none" {
                Value::Null
            } else {
                json!(code)
            }
        }
        "subtitlesSize" => json!(SUBTITLES_SIZES.get(index)?.0),
        "subtitlesTextColor" => json!(SUBTITLES_TEXT_COLORS.get(index)?.0),
        "subtitlesBackgroundColor" => json!(SUBTITLES_BG_COLORS.get(index)?.0),
        "subtitlesOutlineColor" => json!(SUBTITLES_OUTLINE_COLORS.get(index)?.0),
        "audioLanguage" => json!(AUDIO_LANGUAGES.get(index)?.0),
        "seekTimeDuration" => json!(SEEK_DURATIONS.get(index)?.0),
        "seekShortTimeDuration" => json!(SEEK_DURATIONS.get(index)?.0),
        "nextVideoNotificationDuration" => json!(NEXT_VIDEO_DURATIONS.get(index)?.0),
        "playerType" => {
            let (player, _) = *EXTERNAL_PLAYERS.get(index)?;
            if player.is_empty() {
                Value::Null
            } else {
                json!(player)
            }
        }
        "caching" => json!(CACHING_OPTIONS.get(index)?.0),
        "torrentProfile" => json!(TORRENT_PROFILES.get(index)?.0),
        "transcodingProfile" => json!(TRANSCODING_PROFILES.get(index)?.0),
        _ => return None,
    };
    update_profile_and_extras(current, extras, key, new_val)
}

pub(super) fn apply_toggle_option(
    current: &ProfileSettings,
    extras: &mut LocalExtras,
    key: &str,
    enabled: bool,
) -> Option<ProfileSettings> {
    match key {
        "quitOnClose" | "hideSpoilers" | "escExitFullscreen" | "surroundSound"
        | "playInBackground" | "bingeWatching" | "hardwareDecoding" | "pauseOnMinimize" => {
            update_profile_and_extras(current, extras, key, json!(enabled))
        }
        _ => None,
    }
}

pub(super) fn dispatch_settings(runtime: &AppRuntime, settings: ProfileSettings) {
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Ctx(ActionCtx::UpdateSettings(settings)),
    });
}

pub(super) fn logout(runtime: &AppRuntime) {
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Ctx(ActionCtx::Logout),
    });
}

pub(super) fn reload_server(runtime: &AppRuntime) {
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::StreamingServer(ActionStreamingServer::Reload),
    });
}

pub(super) fn open_external_url(kind: &str) {
    let url = match kind {
        "support" => "https://stremio.zendesk.com/hc/en-us",
        "source" => "https://github.com/theguy000/stremio-lightning",
        "tos" => "https://www.stremio.com/tos",
        "privacy" => "https://www.stremio.com/privacy",
        _ => return,
    };
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_populate_all_settings_menus() {
        let profile = Profile::default();
        let extras = LocalExtras::default();
        let data = settings_data_from_profile(&profile, &extras);
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
        let data = settings_data_from_profile(&updated_profile, &extras);
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
        let data = settings_data_from_profile(&updated_profile, &extras);
        assert!(!data.hardware_decoding);
        assert!(!data.player_config.hardware_decoding);

        let _ = apply_select_option(&updated_profile.settings, &mut extras, "caching", 3);
        let _ = apply_toggle_option(&updated_profile.settings, &mut extras, "quitOnClose", true);
        let data = settings_data_from_profile(&updated_profile, &extras);
        assert_eq!(data.caching.title, "10 GB");
        assert!(data.quit_on_close);
    }
}
