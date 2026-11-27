//! Live settings search index matching Stremio Web `useSettingsSearch.ts`.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchHit {
    pub label: String,
    pub crumb: String,
    pub page: String,
    pub row: String,
}

/// Pipe-delimited index: `page|row|label|crumb|keywords`.
const INDEX: &str = "interface||Interface|Preferences|ui display
interface|lang|UI Language|Preferences › Interface|language translation locale
interface|zoom|Interface zoom|Preferences › Interface|scale size bigger smaller
interface|blur|Blur unwatched episodes image|Preferences › Interface|spoilers hide
interface|gamepad|Enable gamepad support|Preferences › Interface|controller xbox playstation dualsense
interface|spotlight|Show Spotlight|Preferences › Interface|board hero carousel banner featured
interface|spotlightCatalogs|Spotlight catalogs|Preferences › Interface|board hero pick catalogs
interface|spotlightRows|Show Spotlight rows on the Board|Preferences › Interface|board hide rows
content||Content|Preferences|catalogs streams filter presets hide reorder board discover
content|catalogs|Catalogs|Preferences › Content|organize board discover hide show reorder move addon
content|streamAddons|Stream sources|Preferences › Content|addons disable hide enable providers details
content|streamPresets|Stream Filter Presets|Preferences › Content|include exclude tags resolution 4k 1080p hdr quality language
player||Player|Preferences|video playback
player|subLang|Default Subtitles Language|Preferences › Player|captions
player|subSize|Default Subtitles Size|Preferences › Player|captions font
player|subPos|Vertical Position|Preferences › Player|captions subtitles offset
player|subColor|Subtitles color|Preferences › Player|captions text
player|subBg|Subtitles background color|Preferences › Player|captions bg
player|subOutline|Subtitles outline color|Preferences › Player|captions border
player|subBold|Bold|Preferences › Player|captions subtitles font weight
player|ass|Style ASS subtitles|Preferences › Player|captions override
player|audio|Default Audio Track|Preferences › Player|language dub
player|seek|Arrow keys seek time|Preferences › Player|skip duration
player|seekShift|Arrow keys seek time when Shift key is held|Preferences › Player|skip short
player|auto|Auto-Play Next Episode|Preferences › Player|binge watching
player|popup|Next video popup duration|Preferences › Player|binge notification
player|external|Play in external player|Preferences › Player|vlc mpv m3u
integrations||Integrations|Preferences|connect sync
integrations|trakt|Trakt Scrobbling|Preferences › Integrations|scrobble sync authenticate disconnect
integrations|calendar|Subscribe to calendar|Preferences › Integrations|ics ical episodes
device||Playback and app|This device|device hardware desktop
device|surround|Surround sound|This device › Playback and app|audio 5.1 channels
device|passthrough|Audio passthrough|This device › Playback and app|audio spdif dolby dts truehd receiver
device|hwdec|Hardware-accelerated decoding|This device › Playback and app|gpu hwdec
device|pause|Pause playback when minimized|This device › Playback and app|background window
device|mpvConfig|Custom mpv config|This device › Playback and app|mpv conf file player
device|quit|Quit on close|This device › Playback and app|tray window
device|esc|Escape key exit full screen|This device › Playback and app|esc keyboard fullscreen
device|discord|Show watching activity on Discord|This device › Playback and app|discord rpc rich presence activity status
streaming||Streaming|This device|server network
streaming|urls|Streaming server|This device › Streaming|url address reload
streaming|remoteUrl|Remote URL|This device › Streaming|https link copy
streaming|https|Streaming HTTPS endpoint|This device › Streaming|ssl
streaming|cache|Cache size|This device › Streaming|disk storage caching
streaming|drive|Caching drive|This device › Streaming|cache folder location disk directory
streaming|torrent|Torrent profile|This device › Streaming|peers speed
streaming|transcode|Transcode profile|This device › Streaming|video convert
account||Account|Account|profile email login logout sign out
account|password|Password|Account|change reset security
account|export|Export user data|Account|download gdpr backup
account|deletion|Delete account|Account|remove close request
shortcuts||Shortcuts|Page|keyboard hotkeys keys
help||Help and about|Page|support version build legal
help|vApp|About Stremio|Help and about|app build server version copy details
help|support|Contact support|Help and about|zendesk help
help|legal|Terms of Service|Help and about|privacy policy source code";

pub fn search(query: &str, logged_in: bool, server_online: bool) -> Vec<SearchHit> {
    let tokens: Vec<String> = query
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '%')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    if tokens.is_empty() {
        return Vec::new();
    }
    INDEX
        .lines()
        .filter_map(|line| {
            let mut parts = line.splitn(5, '|');
            Some((
                parts.next()?,
                parts.next()?,
                parts.next()?,
                parts.next()?,
                parts.next()?,
            ))
        })
        .filter(|&(page, row, label, crumb, kw)| {
            let _ = page;
            if !logged_in && matches!(row, "password" | "export" | "deletion" | "https") {
                return false;
            }
            if !server_online
                && matches!(
                    row,
                    "https" | "cache" | "drive" | "torrent" | "transcode" | "remoteUrl"
                )
            {
                return false;
            }
            let hay = format!("{label} {crumb} {kw}").to_lowercase();
            let words: Vec<&str> = hay
                .split(|c: char| !c.is_alphanumeric() && c != '%')
                .filter(|s| !s.is_empty())
                .collect();
            tokens
                .iter()
                .all(|t| words.iter().any(|w| w.starts_with(t.as_str())))
        })
        .map(|(page, row, label, crumb, _)| SearchHit {
            label: label.to_owned(),
            crumb: crumb.to_owned(),
            page: page.to_owned(),
            row: row.to_owned(),
        })
        .collect()
}
