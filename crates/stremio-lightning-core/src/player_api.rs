use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "method", content = "data")]
pub enum PlayerCommand {
    #[serde(rename = "mpv-observe-prop")]
    ObserveProperty(String),
    #[serde(rename = "mpv-set-prop")]
    SetProperty(String, Value),
    #[serde(rename = "mpv-command")]
    Command(Vec<Value>),
    #[serde(rename = "native-player-stop")]
    Stop,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct PlayerPropertyChange {
    pub name: String,
    pub data: Value,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PlayerEndedError {
    pub message: String,
    pub critical: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct PlayerEnded {
    pub reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<PlayerEndedError>,
}

/// Why MPV stopped playing. Each shell maps its own `libmpv2::EndFileReason` onto
/// this so the reason strings and the error payload stay identical everywhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndFileCause {
    Eof,
    Error,
    Quit,
    Other,
}

impl PlayerEnded {
    #[must_use]
    pub fn from_cause(cause: EndFileCause) -> Self {
        Self {
            reason: match cause {
                EndFileCause::Eof => "eof",
                EndFileCause::Error => "error",
                EndFileCause::Quit => "quit",
                EndFileCause::Other => "other",
            }
            .to_string(),
            error: matches!(cause, EndFileCause::Error).then(|| PlayerEndedError {
                message: "MPV playback error".to_string(),
                critical: true,
            }),
        }
    }
}

/// The format an MPV property must be observed with. Both shells resolve formats
/// here; keeping one table is what stops `mute` from arriving as `0`/`1` on one
/// platform and `"yes"`/`"no"` on the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MpvPropertyFormat {
    Flag,
    Int,
    Double,
    String,
}

const MPV_FLAG_PROPERTIES: &[&str] = &[
    "pause",
    "mute",
    "buffering",
    "seeking",
    "eof-reached",
    "paused-for-cache",
    "keepaspect",
    "osc",
    "input-default-bindings",
    "input-vo-keyboard",
];

const MPV_INT_PROPERTIES: &[&str] = &["aid", "vid", "sid", "secondary-sid"];

const MPV_DOUBLE_PROPERTIES: &[&str] = &[
    "time-pos",
    "volume",
    "duration",
    "sub-delay",
    "sub-scale",
    "cache-buffering-state",
    "demuxer-cache-time",
    "sub-pos",
    "speed",
    "panscan",
];

/// Properties whose MPV string value is JSON rather than plain text. Reparsing
/// every string property would turn a numeric-looking path into a number.
#[must_use]
pub fn is_json_string_property(name: &str) -> bool {
    matches!(name, "track-list" | "video-params" | "metadata")
}

#[must_use]
pub fn mpv_property_format(name: &str) -> MpvPropertyFormat {
    if MPV_FLAG_PROPERTIES.contains(&name) {
        MpvPropertyFormat::Flag
    } else if MPV_INT_PROPERTIES.contains(&name) {
        MpvPropertyFormat::Int
    } else if MPV_DOUBLE_PROPERTIES.contains(&name) {
        MpvPropertyFormat::Double
    } else {
        MpvPropertyFormat::String
    }
}

/// An observed MPV value, mirroring `libmpv2::PropertyData` without pulling
/// libmpv into core.

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum PlayerEvent {
    #[serde(rename = "mpv-prop-change")]
    PropertyChange(PlayerPropertyChange),
    #[serde(rename = "mpv-event-ended")]
    Ended(PlayerEnded),
    #[serde(rename = "showPictureInPicture")]
    ShowPictureInPicture(Value),
    #[serde(rename = "hidePictureInPicture")]
    HidePictureInPicture(Value),
}

impl PlayerEvent {
    #[must_use]
    pub fn transport_args(&self) -> Value {
        match self {
            Self::PropertyChange(payload) => json!(["mpv-prop-change", payload]),
            Self::Ended(payload) => json!(["mpv-event-ended", payload]),
            Self::ShowPictureInPicture(payload) => json!(["showPictureInPicture", payload]),
            Self::HidePictureInPicture(payload) => json!(["hidePictureInPicture", payload]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_player_command_names() {
        assert_eq!(
            serde_json::to_value(PlayerCommand::ObserveProperty("pause".to_string())).unwrap(),
            json!({"method": "mpv-observe-prop", "data": "pause"})
        );
        assert_eq!(
            serde_json::to_value(PlayerCommand::SetProperty("pause".to_string(), json!(true)))
                .unwrap(),
            json!({"method": "mpv-set-prop", "data": ["pause", true]})
        );
    }

    #[test]
    fn player_events_use_shell_transport_args_shape() {
        assert_eq!(
            PlayerEvent::PropertyChange(PlayerPropertyChange {
                name: "pause".to_string(),
                data: json!(true),
            })
            .transport_args(),
            json!(["mpv-prop-change", {"name": "pause", "data": true}])
        );
        assert_eq!(
            PlayerEvent::Ended(PlayerEnded {
                reason: "eof".to_string(),
                error: None,
            })
            .transport_args(),
            json!(["mpv-event-ended", {"reason": "eof"}])
        );
        assert_eq!(
            PlayerEvent::ShowPictureInPicture(json!({})).transport_args(),
            json!(["showPictureInPicture", {}])
        );
        assert_eq!(
            PlayerEvent::HidePictureInPicture(json!({})).transport_args(),
            json!(["hidePictureInPicture", {}])
        );
    }

    #[test]
    fn end_of_file_causes_use_one_reason_vocabulary() {
        assert_eq!(PlayerEnded::from_cause(EndFileCause::Eof).reason, "eof");
        assert_eq!(PlayerEnded::from_cause(EndFileCause::Other).reason, "other");

        let ended = PlayerEnded::from_cause(EndFileCause::Error);
        assert_eq!(ended.reason, "error");
        assert!(ended.error.as_ref().is_some_and(|error| error.critical));
        assert_eq!(
            PlayerEvent::Ended(ended).transport_args(),
            json!(["mpv-event-ended", {
                "reason": "error",
                "error": { "message": "MPV playback error", "critical": true },
            }])
        );
    }

    #[test]
    fn property_formats_match_the_shared_table() {
        assert_eq!(mpv_property_format("pause"), MpvPropertyFormat::Flag);
        assert_eq!(mpv_property_format("mute"), MpvPropertyFormat::Flag);
        assert_eq!(mpv_property_format("buffering"), MpvPropertyFormat::Flag);
        assert_eq!(mpv_property_format("sid"), MpvPropertyFormat::Int);
        assert_eq!(mpv_property_format("secondary-sid"), MpvPropertyFormat::Int);
        assert_eq!(mpv_property_format("time-pos"), MpvPropertyFormat::Double);
        assert_eq!(mpv_property_format("volume"), MpvPropertyFormat::Double);
        assert_eq!(mpv_property_format("track-list"), MpvPropertyFormat::String);
    }

    #[test]
    fn only_json_properties_are_reparsed() {
        for name in ["track-list", "video-params", "metadata"] {
            assert!(is_json_string_property(name), "{name}");
        }
        // A plain string stays a string even when it looks numeric, so the web UI
        // cannot confuse a text property for a number.
        for name in ["path", "media-title", "mute", "time-pos"] {
            assert!(!is_json_string_property(name), "{name}");
        }
    }
}
