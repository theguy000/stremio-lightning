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

const fn entry(
    key: &'static str,
    options: Options,
    default: DefaultVal,
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

pub(super) const SETTINGS: &[Setting] = &[
    entry(
        "interfaceLanguage",
        Options::Text(INTERFACE_LANGUAGES),
        DefaultVal::Text("eng"),
        None,
    ),
    entry(
        "subtitlesLanguage",
        Options::Text(SUBTITLES_LANGUAGES),
        DefaultVal::Text("eng"),
        Some("none"),
    ),
    entry(
        "subtitlesSize",
        Options::Number(SUBTITLES_SIZES),
        DefaultVal::Number(100),
        None,
    ),
    entry(
        "subtitlesTextColor",
        Options::Text(SUBTITLES_TEXT_COLORS),
        DefaultVal::Text("#FFFFFFFF"),
        None,
    ),
    entry(
        "subtitlesBackgroundColor",
        Options::Text(SUBTITLES_BG_COLORS),
        DefaultVal::Text("#00000000"),
        None,
    ),
    entry(
        "subtitlesOutlineColor",
        Options::Text(SUBTITLES_OUTLINE_COLORS),
        DefaultVal::Text("#FF000000"),
        None,
    ),
    entry(
        "audioLanguage",
        Options::Text(AUDIO_LANGUAGES),
        DefaultVal::Text("eng"),
        None,
    ),
    entry(
        "seekTimeDuration",
        Options::Number(SEEK_DURATIONS),
        DefaultVal::Number(10000),
        None,
    ),
    entry(
        "seekShortTimeDuration",
        Options::Number(SEEK_DURATIONS),
        DefaultVal::Number(3000),
        None,
    ),
    entry(
        "nextVideoNotificationDuration",
        Options::Number(NEXT_VIDEO_DURATIONS),
        DefaultVal::Number(35000),
        None,
    ),
    entry(
        "playerType",
        Options::Text(EXTERNAL_PLAYERS),
        DefaultVal::Text(""),
        Some(""),
    ),
    entry(
        "caching",
        Options::Text(CACHING_OPTIONS),
        DefaultVal::Text("2147483648"),
        None,
    ),
    entry(
        "torrentProfile",
        Options::Text(TORRENT_PROFILES),
        DefaultVal::Text("default"),
        None,
    ),
    entry(
        "transcodingProfile",
        Options::Text(TRANSCODING_PROFILES),
        DefaultVal::Text("none"),
        None,
    ),
];

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
