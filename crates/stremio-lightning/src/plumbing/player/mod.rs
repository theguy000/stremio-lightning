//! Video playback plumbing: plain domain types (`PlayerConfig`, `Track`, `Status`) and `MpvSession`.
//! Zero Slint or screen imports; `shell::video_host` adapts `MpvSession` into the window's GL loop.

use crate::plumbing::logs::{Level, Logs, Source};

mod gl;
mod mpv;

pub(crate) use mpv::MpvSession;

#[derive(Clone, Copy)]
pub(crate) struct DesiredSettings {
    pub(crate) scale: stremio_core::types::player::VideoScale,
    pub(crate) volume: Option<f64>,
    pub(crate) muted: Option<bool>,
    pub(crate) speed: Option<f64>,
}

impl Default for DesiredSettings {
    fn default() -> Self {
        Self {
            scale: stremio_core::types::player::VideoScale::Contain,
            volume: None,
            muted: None,
            speed: None,
        }
    }
}

/// Player settings pulled out of the profile so mpv never parses settings JSON.
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

/// One audio or subtitle track mpv can switch to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub language: String,
    pub selected: bool,
    pub external: bool,
}

/// What mpv is doing, in the app's words. Derived `PartialEq` is the single source of truth for
/// change detection so no field can silently stop propagating.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    pub loaded: bool,
    pub time: Option<f64>,
    pub duration: Option<f64>,
    pub paused: Option<bool>,
    pub volume: Option<f64>,
    pub muted: Option<bool>,
    pub speed: Option<f64>,
    pub buffering: bool,
    pub buffered: Option<f64>,
    pub audio: Vec<Track>,
    pub subtitles: Vec<Track>,
    pub subtitles_scale: f64,
    pub subtitles_delay: f64,
    pub subtitles_position: f64,
    pub error: String,
    pub width: u32,
    pub height: u32,
    pub ended: bool,
}

impl Status {
    /// Combines event-driven fields (`loaded`, `error`, `ended`) from `self` with every live
    /// property read from mpv in `measured` so no property field is ever dropped.
    pub(crate) fn with_measured(self, measured: Status) -> Status {
        Status {
            loaded: self.loaded,
            error: self.error,
            ended: self.ended,
            ..measured
        }
    }
}

/// Translates mpv's numeric error codes into human-readable messages.
pub(crate) fn describe(error: libmpv2::Error) -> String {
    match error {
        libmpv2::Error::Raw(code) => match code {
            -4 => "this stream is not playable".to_owned(),
            -8 | -9 => "the player could not decode this stream".to_owned(),
            -12 => "the stream could not be opened".to_owned(),
            -13 => "the stream was not found".to_owned(),
            -16 => "there is no video output".to_owned(),
            -17 => "the player is not available".to_owned(),
            other => format!("the player reported error {other}"),
        },
        other => other.to_string(),
    }
}

/// Records once that video playback is unavailable on stderr and in `Logs`.
pub(crate) fn player_unavailable(logs: &Logs, error: &str) {
    let text = format!("video playback is unavailable: {error}");
    eprintln!("{text}");
    logs.record(Source::App, "Player", Level::Error, text);
}

/// Builds mpv's `sub-add <url> [<flags> [<title> [<lang>]]]` argument list with `select` first.
pub(crate) fn sub_add_arguments<'a>(
    url: &'a str,
    title: &'a str,
    language: &'a str,
) -> Vec<&'a str> {
    let mut arguments = vec![url, "select"];
    if !title.is_empty() || !language.is_empty() {
        arguments.push(title);
    }
    if !language.is_empty() {
        arguments.push(language);
    }
    arguments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sub_add_arguments_place_flags_directly_after_url() {
        assert_eq!(
            sub_add_arguments("https://example.com/1.srt", "Deutsch", "ger"),
            vec!["https://example.com/1.srt", "select", "Deutsch", "ger"]
        );
        assert_eq!(
            sub_add_arguments("https://example.com/1.srt", "", "ger"),
            vec!["https://example.com/1.srt", "select", "", "ger"]
        );
        let args = sub_add_arguments("https://example.com/1.srt", "Deutsch", "ger");
        assert_eq!(args[1], "select");
        assert!(!args.contains(&"file"));
    }

    #[test]
    fn status_merge_preserves_event_fields_and_propagates_subtitles_position() {
        let events = Status {
            loaded: true,
            error: String::new(),
            ended: false,
            subtitles_position: 0.0,
            ..Status::default()
        };
        let measured = Status {
            subtitles_position: 95.0,
            time: Some(12.5),
            ..Status::default()
        };
        let merged = events.clone().with_measured(measured);
        assert!(merged.loaded);
        assert_eq!(merged.subtitles_position, 95.0);
        assert_eq!(merged.time, Some(12.5));
        assert_ne!(events, merged);
    }
}
