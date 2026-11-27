//! Static option tables for the Settings screen (`SETTINGS` and `TOGGLES`).

#[derive(Clone, Copy)]
pub(super) enum Options {
    Text(&'static [(&'static str, &'static str)]),
    Number(&'static [(u32, &'static str)]),
}

#[derive(Clone, Copy)]
pub(super) enum DefaultVal {
    Text(&'static str),
    Number(u32),
}

pub(super) struct Setting {
    pub key: &'static str,
    pub options: Options,
    pub default: DefaultVal,
    pub unset: Option<&'static str>,
}

const fn text(
    key: &'static str,
    options: &'static [(&'static str, &'static str)],
    default: &'static str,
    unset: Option<&'static str>,
) -> Setting {
    Setting {
        key,
        options: Options::Text(options),
        default: DefaultVal::Text(default),
        unset,
    }
}

const fn num(key: &'static str, options: &'static [(u32, &'static str)], default: u32) -> Setting {
    Setting {
        key,
        options: Options::Number(options),
        default: DefaultVal::Number(default),
        unset: None,
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

const AUDIO_LANGUAGES: &[(&str, &str)] = SUBTITLES_LANGUAGES.split_at(1).1;

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

const INTERFACE_SCALES: &[(u32, &str)] = &[
    (75, "75%"),
    (100, "100%"),
    (125, "125%"),
    (150, "150%"),
    (175, "175%"),
];

const SUBTITLES_OFFSETS: &[(u32, &str)] = &[
    (0, "0%"),
    (5, "5%"),
    (10, "10%"),
    (15, "15%"),
    (20, "20%"),
    (25, "25%"),
    (30, "30%"),
    (35, "35%"),
    (40, "40%"),
    (45, "45%"),
    (50, "50%"),
    (55, "55%"),
    (60, "60%"),
    (65, "65%"),
    (70, "70%"),
    (75, "75%"),
    (80, "80%"),
    (85, "85%"),
    (90, "90%"),
    (95, "95%"),
    (100, "100%"),
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
    (65000, "65 seconds"),
    (70000, "70 seconds"),
    (75000, "75 seconds"),
    (80000, "80 seconds"),
    (85000, "85 seconds"),
    (90000, "90 seconds"),
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

pub(super) const SETTINGS: &[Setting] = &[
    text("interfaceLanguage", INTERFACE_LANGUAGES, "eng", None),
    num("interfaceScale", INTERFACE_SCALES, 100),
    num("subtitlesOffset", SUBTITLES_OFFSETS, 5),
    text(
        "subtitlesLanguage",
        SUBTITLES_LANGUAGES,
        "eng",
        Some("none"),
    ),
    num("subtitlesSize", SUBTITLES_SIZES, 100),
    text(
        "subtitlesTextColor",
        SUBTITLES_TEXT_COLORS,
        "#FFFFFFFF",
        None,
    ),
    text(
        "subtitlesBackgroundColor",
        SUBTITLES_BG_COLORS,
        "#00000000",
        None,
    ),
    text(
        "subtitlesOutlineColor",
        SUBTITLES_OUTLINE_COLORS,
        "#FF000000",
        None,
    ),
    text("audioLanguage", AUDIO_LANGUAGES, "eng", None),
    num("seekTimeDuration", SEEK_DURATIONS, 10000),
    num("seekShortTimeDuration", SEEK_DURATIONS, 3000),
    num("nextVideoNotificationDuration", NEXT_VIDEO_DURATIONS, 35000),
    text("playerType", EXTERNAL_PLAYERS, "", Some("")),
    text("caching", CACHING_OPTIONS, "2147483648", None),
    text("torrentProfile", TORRENT_PROFILES, "default", None),
    text("transcodingProfile", TRANSCODING_PROFILES, "none", None),
];

pub(crate) const TOGGLES: &[&str] = &[
    "quitOnClose",
    "hideSpoilers",
    "gamepadSupport",
    "discordRpcEnabled",
    "escExitFullscreen",
    "subtitlesBold",
    "assSubtitlesStyling",
    "surroundSound",
    "audioPassthrough",
    "playInBackground",
    "bingeWatching",
    "hardwareDecoding",
    "pauseOnMinimize",
];
