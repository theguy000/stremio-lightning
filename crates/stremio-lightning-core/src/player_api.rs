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

impl PlayerCommand {
    /// Decodes an `mpv-*` or `native-player-stop` shell transport message. Every shell
    /// decodes through here so payload rules and error wording cannot drift apart.
    ///
    /// # Errors
    /// Returns a message naming the method when it is not a player method or its payload
    /// has the wrong shape.
    pub fn from_transport(method: &str, data: Option<Value>) -> Result<Self, String> {
        match method {
            "mpv-observe-prop" => match data {
                Some(Value::String(name)) => Ok(Self::ObserveProperty(name)),
                _ => Err("Invalid mpv-observe-prop payload".to_string()),
            },
            "mpv-set-prop" => {
                let Some(Value::Array(mut values)) = data else {
                    return Err("Invalid mpv-set-prop payload".to_string());
                };
                let name = values
                    .first()
                    .and_then(Value::as_str)
                    .ok_or("Missing mpv-set-prop name")?
                    .to_string();
                let value = values
                    .get_mut(1)
                    .map(std::mem::take)
                    .ok_or("Missing mpv-set-prop value")?;
                Ok(Self::SetProperty(name, value))
            }
            "mpv-command" => match data {
                Some(Value::Array(values)) if values.first().is_some_and(Value::is_string) => {
                    Ok(Self::Command(values))
                }
                Some(Value::Array(_)) => Err("Missing mpv-command name".to_string()),
                _ => Err("Invalid mpv-command payload".to_string()),
            },
            "native-player-stop" => Ok(Self::Stop),
            other => Err(format!("Unsupported MPV transport method: {other}")),
        }
    }

    /// Runs this command through `execute`, adding the secondary-subtitle rule: mpv's
    /// secondary subtitle is switched off whenever the primary track changes, before
    /// `sid` is set and after `sub-add`, so two tracks never render at once.
    ///
    /// # Errors
    /// Returns the first error `execute` reports; later steps are not run.
    pub fn execute_in_order<E>(
        self,
        mut execute: impl FnMut(Self) -> Result<(), E>,
    ) -> Result<(), E> {
        let disable_secondary =
            || Self::SetProperty("secondary-sid".to_string(), Value::from("no"));
        if matches!(&self, Self::SetProperty(name, _) if name == "sid") {
            execute(disable_secondary())?;
        }
        let after = matches!(
            &self,
            Self::Command(values) if values.first().and_then(Value::as_str) == Some("sub-add")
        );
        execute(self)?;
        if after {
            execute(disable_secondary())?;
        }
        Ok(())
    }
}

/// What a shell's native player reports about itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativePlayerStatus {
    pub enabled: bool,
    pub initialized: bool,
    pub backend: String,
}

/// A player request as a shell backend receives it, after transport decoding.
#[derive(Debug, Clone, PartialEq)]
pub enum PlayerAction {
    ObserveProperty(String),
    SetProperty { name: String, value: Value },
    Command { name: String, args: Vec<String> },
    Stop,
}

/// The player operations a shell must provide. [`handle_transport`] turns shell
/// transport messages into calls on this trait.
pub trait PlayerBackend: Send + Sync + 'static {
    fn status(&self) -> NativePlayerStatus;

    /// # Errors
    /// Returns an error when the backend cannot observe the property.
    fn observe_property(&self, name: String) -> Result<(), String>;

    /// # Errors
    /// Returns an error when the backend cannot set the property.
    fn set_property(&self, name: String, value: Value) -> Result<(), String>;

    /// # Errors
    /// Returns an error when the backend cannot run the command.
    fn command(&self, name: String, args: Vec<String>) -> Result<(), String>;

    /// # Errors
    /// Returns an error when the backend cannot stop playback.
    fn stop(&self) -> Result<(), String>;
}

/// Decodes a shell transport message and runs it on `backend`, in the order
/// [`PlayerCommand::execute_in_order`] defines.
///
/// # Errors
/// Returns the decoding error, or the first error the backend reports.
pub fn handle_transport<B: PlayerBackend>(
    backend: &B,
    method: &str,
    data: Option<Value>,
) -> Result<(), String> {
    PlayerCommand::from_transport(method, data)?.execute_in_order(|command| match command {
        PlayerCommand::ObserveProperty(name) => backend.observe_property(name),
        PlayerCommand::SetProperty(name, value) => backend.set_property(name, value),
        PlayerCommand::Command(values) => {
            let name = values
                .first()
                .and_then(Value::as_str)
                .ok_or("Missing mpv-command name")?;
            let args = values
                .iter()
                .skip(1)
                .map(|value| match value {
                    Value::String(string) => string.clone(),
                    other => other.to_string(),
                })
                .collect();
            backend.command(name.to_string(), args)
        }
        PlayerCommand::Stop => backend.stop(),
    })
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
    Stop,
    Redirect,
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
                EndFileCause::Stop => "stop",
                EndFileCause::Redirect => "redirect",
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
    fn transport_messages_decode_into_player_commands() {
        let ok = [
            (
                "mpv-observe-prop",
                json!("pause"),
                PlayerCommand::ObserveProperty("pause".into()),
            ),
            (
                "mpv-set-prop",
                json!(["pause", true]),
                PlayerCommand::SetProperty("pause".into(), json!(true)),
            ),
            (
                "mpv-command",
                json!(["loadfile", "x"]),
                PlayerCommand::Command(vec![json!("loadfile"), json!("x")]),
            ),
        ];
        for (method, data, expected) in ok {
            assert_eq!(
                PlayerCommand::from_transport(method, Some(data)),
                Ok(expected)
            );
        }
        assert_eq!(
            PlayerCommand::from_transport("native-player-stop", None),
            Ok(PlayerCommand::Stop)
        );

        let bad = [
            ("mpv-observe-prop", None, "Invalid mpv-observe-prop payload"),
            (
                "mpv-set-prop",
                Some(json!("pause")),
                "Invalid mpv-set-prop payload",
            ),
            (
                "mpv-set-prop",
                Some(json!([1, true])),
                "Missing mpv-set-prop name",
            ),
            (
                "mpv-set-prop",
                Some(json!(["pause"])),
                "Missing mpv-set-prop value",
            ),
            (
                "mpv-command",
                Some(json!({})),
                "Invalid mpv-command payload",
            ),
            ("mpv-command", Some(json!([])), "Missing mpv-command name"),
            (
                "mpv-nope",
                None,
                "Unsupported MPV transport method: mpv-nope",
            ),
        ];
        for (method, data, message) in bad {
            assert_eq!(
                PlayerCommand::from_transport(method, data),
                Err(message.to_string())
            );
        }
    }

    #[test]
    fn secondary_subtitle_is_disabled_around_primary_track_changes() {
        let off = PlayerCommand::SetProperty("secondary-sid".into(), json!("no"));
        let run = |command: PlayerCommand| {
            let mut seen = Vec::new();
            command
                .execute_in_order(|step| {
                    seen.push(step);
                    Ok::<(), String>(())
                })
                .unwrap();
            seen
        };

        let set_sid = PlayerCommand::SetProperty("sid".into(), json!(3));
        assert_eq!(run(set_sid.clone()), [off.clone(), set_sid]);

        let sub_add = PlayerCommand::Command(vec![json!("sub-add"), json!("a.srt")]);
        assert_eq!(run(sub_add.clone()), [sub_add, off]);

        assert_eq!(run(PlayerCommand::Stop), [PlayerCommand::Stop]);
    }

    #[test]
    fn a_failed_step_stops_the_sequence() {
        let mut calls = 0;
        let result = PlayerCommand::SetProperty("sid".into(), json!(3)).execute_in_order(|_| {
            calls += 1;
            Err::<(), _>("boom")
        });
        assert_eq!(result, Err("boom"));
        assert_eq!(calls, 1);
    }

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
        assert_eq!(PlayerEnded::from_cause(EndFileCause::Stop).reason, "stop");
        assert_eq!(
            PlayerEnded::from_cause(EndFileCause::Redirect).reason,
            "redirect"
        );
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

    #[derive(Default)]
    struct RecordingBackend {
        actions: std::sync::Mutex<Vec<PlayerAction>>,
    }

    impl RecordingBackend {
        fn actions(&self) -> Vec<PlayerAction> {
            self.actions.lock().unwrap().clone()
        }

        fn record(&self, action: PlayerAction) {
            self.actions.lock().unwrap().push(action);
        }
    }

    impl PlayerBackend for RecordingBackend {
        fn status(&self) -> NativePlayerStatus {
            NativePlayerStatus {
                enabled: true,
                initialized: true,
                backend: "recording".to_string(),
            }
        }

        fn observe_property(&self, name: String) -> Result<(), String> {
            self.record(PlayerAction::ObserveProperty(name));
            Ok(())
        }

        fn set_property(&self, name: String, value: Value) -> Result<(), String> {
            self.record(PlayerAction::SetProperty { name, value });
            Ok(())
        }

        fn command(&self, name: String, args: Vec<String>) -> Result<(), String> {
            self.record(PlayerAction::Command { name, args });
            Ok(())
        }

        fn stop(&self) -> Result<(), String> {
            self.record(PlayerAction::Stop);
            Ok(())
        }
    }

    fn secondary_off() -> PlayerAction {
        PlayerAction::SetProperty {
            name: "secondary-sid".to_string(),
            value: json!("no"),
        }
    }

    #[test]
    fn handle_transport_maps_each_method_to_a_backend_call() {
        let backend = RecordingBackend::default();

        handle_transport(&backend, "mpv-observe-prop", Some(json!("pause"))).unwrap();
        handle_transport(&backend, "mpv-set-prop", Some(json!(["pause", true]))).unwrap();
        handle_transport(
            &backend,
            "mpv-command",
            Some(json!(["loadfile", "file:///tmp/sample.mp4", "replace", 1])),
        )
        .unwrap();
        handle_transport(&backend, "native-player-stop", None).unwrap();

        assert_eq!(
            backend.actions(),
            vec![
                PlayerAction::ObserveProperty("pause".to_string()),
                PlayerAction::SetProperty {
                    name: "pause".to_string(),
                    value: json!(true),
                },
                PlayerAction::Command {
                    name: "loadfile".to_string(),
                    args: vec![
                        "file:///tmp/sample.mp4".to_string(),
                        "replace".to_string(),
                        "1".to_string(),
                    ],
                },
                PlayerAction::Stop,
            ]
        );
    }

    #[test]
    fn handle_transport_clears_secondary_subtitle_around_track_changes() {
        let backend = RecordingBackend::default();

        handle_transport(&backend, "mpv-set-prop", Some(json!(["sid", 3]))).unwrap();
        handle_transport(
            &backend,
            "mpv-command",
            Some(json!(["sub-add", "file:///tmp/sub.srt", "select"])),
        )
        .unwrap();

        assert_eq!(
            backend.actions(),
            vec![
                secondary_off(),
                PlayerAction::SetProperty {
                    name: "sid".to_string(),
                    value: json!(3),
                },
                PlayerAction::Command {
                    name: "sub-add".to_string(),
                    args: vec!["file:///tmp/sub.srt".to_string(), "select".to_string()],
                },
                secondary_off(),
            ]
        );
    }

    #[test]
    fn handle_transport_rejects_bad_messages_without_calling_the_backend() {
        let backend = RecordingBackend::default();

        assert_eq!(
            handle_transport(&backend, "mpv-nope", None),
            Err("Unsupported MPV transport method: mpv-nope".to_string())
        );
        assert!(backend.actions().is_empty());
    }
}
