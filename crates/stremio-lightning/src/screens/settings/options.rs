//! The Settings screen's option lists.
//!
//! One setting used to be described in five places: an option table, a read of its current
//! value, a match arm that wrote it, a field on the view struct, and a property in the Slint
//! file. Only the Slint property is genuinely separate, so everything else now comes from the
//! one `SETTINGS` table below: what the screen shows, and what "option three means".

use serde_json::{json, Value};

use super::super::Input;

/// What a setting's options are: text codes, or numbers.
#[derive(Clone, Copy)]
pub(crate) enum Options {
    Text(&'static [(&'static str, &'static str)]),
    Number(&'static [(u32, &'static str)]),
}

/// What a setting shows when the profile does not carry it.
#[derive(Clone, Copy)]
enum Default {
    Text(&'static str),
    Number(u32),
}

pub(crate) struct Setting {
    pub key: &'static str,
    options: Options,
    default: Default,
    /// The option code that means "not set", written to the profile as null.
    unset: Option<&'static str>,
}

const fn entry(
    key: &'static str,
    options: Options,
    default: Default,
    unset: Option<&'static str>,
) -> Setting {
    Setting {
        key,
        options,
        default,
        unset,
    }
}

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

/// Every setting the screen shows as a menu, with where its options and its default live.
pub(crate) const SETTINGS: &[Setting] = &[
    entry(
        "interfaceLanguage",
        Options::Text(INTERFACE_LANGUAGES),
        Default::Text("eng"),
        None,
    ),
    entry(
        "subtitlesLanguage",
        Options::Text(SUBTITLES_LANGUAGES),
        Default::Text("eng"),
        Some("none"),
    ),
    entry(
        "subtitlesSize",
        Options::Number(SUBTITLES_SIZES),
        Default::Number(100),
        None,
    ),
    entry(
        "subtitlesTextColor",
        Options::Text(SUBTITLES_TEXT_COLORS),
        Default::Text("#FFFFFFFF"),
        None,
    ),
    entry(
        "subtitlesBackgroundColor",
        Options::Text(SUBTITLES_BG_COLORS),
        Default::Text("#00000000"),
        None,
    ),
    entry(
        "subtitlesOutlineColor",
        Options::Text(SUBTITLES_OUTLINE_COLORS),
        Default::Text("#FF000000"),
        None,
    ),
    entry(
        "audioLanguage",
        Options::Text(AUDIO_LANGUAGES),
        Default::Text("eng"),
        None,
    ),
    entry(
        "seekTimeDuration",
        Options::Number(SEEK_DURATIONS),
        Default::Number(10000),
        None,
    ),
    entry(
        "seekShortTimeDuration",
        Options::Number(SEEK_DURATIONS),
        Default::Number(3000),
        None,
    ),
    entry(
        "nextVideoNotificationDuration",
        Options::Number(NEXT_VIDEO_DURATIONS),
        Default::Number(35000),
        None,
    ),
    entry(
        "playerType",
        Options::Text(EXTERNAL_PLAYERS),
        Default::Text(""),
        Some(""),
    ),
    entry(
        "caching",
        Options::Text(CACHING_OPTIONS),
        Default::Text("2147483648"),
        None,
    ),
    entry(
        "torrentProfile",
        Options::Text(TORRENT_PROFILES),
        Default::Text("default"),
        None,
    ),
    entry(
        "transcodingProfile",
        Options::Text(TRANSCODING_PROFILES),
        Default::Text("none"),
        None,
    ),
];

/// The keys of the settings that are on or off rather than a choice.
pub(crate) const TOGGLES: &[&str] = &[
    "quitOnClose",
    "hideSpoilers",
    "escExitFullscreen",
    "surroundSound",
    "playInBackground",
    "bingeWatching",
    "hardwareDecoding",
    "pauseOnMinimize",
];

fn setting(key: &str) -> Option<&'static Setting> {
    SETTINGS.iter().find(|setting| setting.key == key)
}

/// The code a setting currently holds, as the profile's JSON spells it.
fn current(setting: &Setting, value: &Value) -> String {
    let fallback = match setting.default {
        Default::Text(text) => text.to_owned(),
        Default::Number(number) => number.to_string(),
    };
    let unset = setting.unset.unwrap_or_default();
    match value.get(setting.key) {
        Some(Value::Null) => unset.to_owned(),
        Some(Value::String(text)) if text.is_empty() => {
            if unset.is_empty() {
                fallback
            } else {
                unset.to_owned()
            }
        }
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        _ => fallback,
    }
}

/// The closed label and options of the setting `key`, chosen by the value in `settings`.
pub(crate) fn input(key: &str, settings: &Value) -> Input {
    let Some(setting) = setting(key) else {
        return Input::default();
    };
    let current = current(setting, settings);
    let (labels, selected): (Vec<&str>, usize) = match setting.options {
        Options::Text(pairs) => {
            let selected = pairs
                .iter()
                .position(|(code, _)| code.eq_ignore_ascii_case(&current))
                .unwrap_or(0);
            (pairs.iter().map(|(_, label)| *label).collect(), selected)
        }
        Options::Number(pairs) => {
            let selected = current
                .parse::<u32>()
                .ok()
                .and_then(|value| pairs.iter().position(|(code, _)| *code == value))
                .unwrap_or(0);
            (pairs.iter().map(|(_, label)| *label).collect(), selected)
        }
    };
    Input {
        title: labels.get(selected).copied().unwrap_or_default().to_owned(),
        selected,
        options: labels
            .iter()
            .enumerate()
            .map(|(index, label)| ((*label).to_owned(), index == selected))
            .collect(),
        requests: vec![],
    }
}

/// The numeric code a setting currently holds, or its default.
pub(crate) fn number_at(key: &str, settings: &Value) -> u32 {
    let Some(setting) = setting(key) else {
        return 0;
    };
    let fallback = match setting.default {
        Default::Number(number) => number,
        Default::Text(_) => 0,
    };
    settings
        .get(setting.key)
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .unwrap_or(fallback)
}

/// The text code a setting currently holds, or its default.
pub(crate) fn code_at(key: &str, settings: &Value) -> String {
    let Some(setting) = setting(key) else {
        return String::new();
    };
    current(setting, settings)
}

/// The profile value that option `index` of the setting `key` means.
pub(crate) fn value_at(key: &str, index: usize) -> Option<Value> {
    let setting = setting(key)?;
    let code = match setting.options {
        Options::Text(pairs) => {
            let (code, _) = *pairs.get(index)?;
            if Some(code) == setting.unset {
                return Some(Value::Null);
            }
            code
        }
        Options::Number(pairs) => return pairs.get(index).map(|(number, _)| json!(number)),
    };
    Some(json!(code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_shows_the_profile_value_it_is_given() {
        let settings = json!({ "interfaceLanguage": "ben", "subtitlesSize": 150 });
        let language = input("interfaceLanguage", &settings);
        assert_eq!(language.title, "বাংলা");
        assert_eq!(language.selected, 12);

        let size = input("subtitlesSize", &settings);
        assert_eq!(size.title, "150%");
        assert_eq!(size.selected, 3);
    }

    #[test]
    fn missing_values_fall_back_and_nulls_mean_unset() {
        let settings = json!({ "subtitlesLanguage": null, "playerType": null });
        assert_eq!(input("subtitlesLanguage", &settings).title, "None");
        assert_eq!(input("playerType", &settings).title, "Disabled");
        // A key the profile does not carry at all uses the setting's own default.
        assert_eq!(input("audioLanguage", &json!({})).title, "English");
        assert_eq!(input("seekTimeDuration", &json!({})).title, "10 seconds");
    }

    #[test]
    fn the_language_menus_are_written_in_their_own_scripts() {
        // These labels are the only non-ASCII text in the app outside a Slint file, so they are
        // the only place an encoding mistake would reach a person. Pin several by name: a
        // double-encoded label ("EspaÃ±ol") or a lost one passes every other test.
        let interface: Vec<String> = input("interfaceLanguage", &json!({}))
            .options
            .into_iter()
            .map(|(label, _)| label)
            .collect();
        for expected in [
            "Español",
            "Русский",
            "العربية",
            "বাংলা",
            "हिन्दी",
            "日本語",
            "한국어",
            "中文",
        ] {
            assert!(
                interface.iter().any(|label| label == expected),
                "{expected} is missing from the interface languages"
            );
        }
    }

    #[test]
    fn an_option_maps_back_to_the_value_it_stands_for() {
        assert_eq!(
            value_at("subtitlesLanguage", 0),
            Some(serde_json::Value::Null)
        );
        assert_eq!(value_at("subtitlesLanguage", 1), Some(json!("eng")));
        assert_eq!(value_at("subtitlesSize", 3), Some(json!(150)));
        assert_eq!(value_at("playerType", 0), Some(serde_json::Value::Null));
        assert_eq!(value_at("nonsense", 0), None);
    }
}
