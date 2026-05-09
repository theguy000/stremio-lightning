use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use stremio_lightning_core::player_api::{
    PlayerEnded, PlayerEndedError, PlayerEvent, PlayerPropertyChange,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NativePlayerStatus {
    pub enabled: bool,
    pub initialized: bool,
    pub backend: String,
}

pub trait PlayerBackend: Clone + Send + Sync + 'static {
    fn status(&self) -> NativePlayerStatus;
    fn observe_property(&self, name: String) -> Result<(), String>;
    fn set_property(&self, name: String, value: Value) -> Result<(), String>;
    fn command(&self, name: String, args: Vec<String>) -> Result<(), String>;
    fn stop(&self) -> Result<(), String>;
    fn drain_events(&self) -> Result<Vec<PlayerEvent>, String>;
}

#[derive(Debug, Clone, PartialEq)]
pub enum PlayerAction {
    ObserveProperty(String),
    SetProperty { name: String, value: Value },
    Command { name: String, args: Vec<String> },
    Stop,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MpvBackendCommand {
    ObserveProperty(String),
    SetProperty { name: String, value: Value },
    Command { name: String, args: Vec<String> },
    Stop,
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MpvOption {
    pub name: &'static str,
    pub value: String,
}

pub fn default_mpv_options(app_name: &str, debug: bool) -> Vec<MpvOption> {
    vec![
        MpvOption {
            name: "title",
            value: app_name.to_string(),
        },
        MpvOption {
            name: "audio-client-name",
            value: app_name.to_string(),
        },
        MpvOption {
            name: "terminal",
            value: "yes".to_string(),
        },
        MpvOption {
            name: "msg-level",
            value: if debug {
                "all=no,cplayer=debug"
            } else {
                "all=no"
            }
            .to_string(),
        },
        MpvOption {
            name: "quiet",
            value: "yes".to_string(),
        },
        MpvOption {
            name: "hwdec",
            value: "auto".to_string(),
        },
        MpvOption {
            name: "audio-fallback-to-null",
            value: "yes".to_string(),
        },
        MpvOption {
            name: "cache",
            value: "yes".to_string(),
        },
    ]
}

#[derive(Debug, Default, Clone)]
pub struct FakePlayerBackend {
    actions: Arc<Mutex<Vec<PlayerAction>>>,
    events: Arc<Mutex<Vec<PlayerEvent>>>,
    initialized: bool,
}

impl FakePlayerBackend {
    pub fn initialized() -> Self {
        Self {
            actions: Arc::default(),
            events: Arc::default(),
            initialized: true,
        }
    }

    pub fn stopped(&self) -> bool {
        self.actions()
            .iter()
            .any(|action| matches!(action, PlayerAction::Stop))
    }

    pub fn actions(&self) -> Vec<PlayerAction> {
        self.actions
            .lock()
            .expect("fake macOS player actions poisoned")
            .clone()
    }

    pub fn push_event(&self, event: PlayerEvent) -> Result<(), String> {
        self.events.lock().map_err(|e| e.to_string())?.push(event);
        Ok(())
    }
}

impl PlayerBackend for FakePlayerBackend {
    fn status(&self) -> NativePlayerStatus {
        NativePlayerStatus {
            enabled: true,
            initialized: self.initialized,
            backend: "fake".to_string(),
        }
    }

    fn observe_property(&self, name: String) -> Result<(), String> {
        self.actions
            .lock()
            .map_err(|e| e.to_string())?
            .push(PlayerAction::ObserveProperty(name));
        Ok(())
    }

    fn set_property(&self, name: String, value: Value) -> Result<(), String> {
        self.actions
            .lock()
            .map_err(|e| e.to_string())?
            .push(PlayerAction::SetProperty { name, value });
        Ok(())
    }

    fn command(&self, name: String, args: Vec<String>) -> Result<(), String> {
        self.actions
            .lock()
            .map_err(|e| e.to_string())?
            .push(PlayerAction::Command { name, args });
        Ok(())
    }

    fn stop(&self) -> Result<(), String> {
        self.actions
            .lock()
            .map_err(|e| e.to_string())?
            .push(PlayerAction::Stop);
        Ok(())
    }

    fn drain_events(&self) -> Result<Vec<PlayerEvent>, String> {
        Ok(std::mem::take(
            &mut *self.events.lock().map_err(|e| e.to_string())?,
        ))
    }
}

#[derive(Debug, Default, Clone)]
pub struct MpvPlayerBackend {
    initialized: Arc<Mutex<bool>>,
    sender: Arc<Mutex<Option<Sender<MpvBackendCommand>>>>,
    events: Arc<Mutex<Vec<PlayerEvent>>>,
}

impl MpvPlayerBackend {
    pub fn attach(&self, sender: Sender<MpvBackendCommand>) -> Result<(), String> {
        *self.sender.lock().map_err(|e| e.to_string())? = Some(sender);
        self.mark_initialized()
    }

    pub fn mark_initialized(&self) -> Result<(), String> {
        *self.initialized.lock().map_err(|e| e.to_string())? = true;
        Ok(())
    }

    pub fn push_event(&self, event: PlayerEvent) -> Result<(), String> {
        self.events.lock().map_err(|e| e.to_string())?.push(event);
        Ok(())
    }

    fn send(&self, command: MpvBackendCommand) -> Result<(), String> {
        let sender = self
            .sender
            .lock()
            .map_err(|e| e.to_string())?
            .clone()
            .ok_or_else(|| {
                "macOS MPV backend is not attached to a native video layer".to_string()
            })?;
        sender
            .send(command)
            .map_err(|e| format!("Failed to send command to macOS MPV backend: {e}"))
    }
}

impl PlayerBackend for MpvPlayerBackend {
    fn status(&self) -> NativePlayerStatus {
        NativePlayerStatus {
            enabled: true,
            initialized: self.initialized.lock().map(|guard| *guard).unwrap_or(false),
            backend: "libmpv-macos".to_string(),
        }
    }

    fn observe_property(&self, name: String) -> Result<(), String> {
        self.send(MpvBackendCommand::ObserveProperty(name))
    }

    fn set_property(&self, name: String, value: Value) -> Result<(), String> {
        self.send(MpvBackendCommand::SetProperty { name, value })
    }

    fn command(&self, name: String, args: Vec<String>) -> Result<(), String> {
        self.send(MpvBackendCommand::Command { name, args })
    }

    fn stop(&self) -> Result<(), String> {
        self.send(MpvBackendCommand::Stop)
    }

    fn drain_events(&self) -> Result<Vec<PlayerEvent>, String> {
        Ok(std::mem::take(
            &mut *self.events.lock().map_err(|e| e.to_string())?,
        ))
    }
}

pub fn handle_transport<B: PlayerBackend>(
    backend: &B,
    method: &str,
    data: Option<Value>,
) -> Result<(), String> {
    match method {
        "mpv-observe-prop" => {
            let name = data
                .as_ref()
                .and_then(Value::as_str)
                .ok_or_else(|| "Invalid mpv-observe-prop payload".to_string())?;
            backend.observe_property(name.to_string())
        }
        "mpv-set-prop" => {
            let pair = data
                .as_ref()
                .and_then(Value::as_array)
                .ok_or_else(|| "Invalid mpv-set-prop payload".to_string())?;
            let name = pair
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| "Missing mpv-set-prop name".to_string())?;
            let value = pair
                .get(1)
                .cloned()
                .ok_or_else(|| "Missing mpv-set-prop value".to_string())?;
            backend.set_property(name.to_string(), value)
        }
        "mpv-command" => {
            let args = data
                .as_ref()
                .and_then(Value::as_array)
                .ok_or_else(|| "Invalid mpv-command payload".to_string())?;
            let name = args
                .first()
                .and_then(Value::as_str)
                .ok_or_else(|| "Missing mpv-command name".to_string())?;
            let values = args
                .iter()
                .skip(1)
                .map(|value| match value {
                    Value::String(value) => value.clone(),
                    other => other.to_string(),
                })
                .collect();
            backend.command(name.to_string(), values)
        }
        "native-player-stop" => backend.stop(),
        other => Err(format!(
            "Unsupported macOS player transport method: {other}"
        )),
    }
}

pub fn serialize_property_change(name: impl Into<String>, data: Value) -> Value {
    PlayerEvent::PropertyChange(PlayerPropertyChange {
        name: name.into(),
        data,
    })
    .transport_args()
}

pub fn serialize_ended(reason: impl Into<String>) -> Value {
    PlayerEvent::Ended(PlayerEnded {
        reason: reason.into(),
        error: None,
    })
    .transport_args()
}

pub fn serialize_error(message: impl Into<String>) -> Value {
    PlayerEvent::Ended(PlayerEnded {
        reason: "error".to_string(),
        error: Some(PlayerEndedError {
            message: message.into(),
            critical: true,
        }),
    })
    .transport_args()
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct VideoVisibilityState {
    visible: bool,
}

impl VideoVisibilityState {
    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn apply_command(&mut self, command: &MpvBackendCommand) {
        match command {
            MpvBackendCommand::Command { name, args } if name == "loadfile" && !args.is_empty() => {
                self.visible = true;
            }
            MpvBackendCommand::Stop | MpvBackendCommand::Shutdown => {
                self.visible = false;
            }
            _ => {}
        }
    }

    pub fn apply_event(&mut self, event: &PlayerEvent) {
        match event {
            PlayerEvent::PropertyChange(change) if change.name == "video-params" => {
                self.visible = !change.data.is_null() && change.data != json!(false);
            }
            PlayerEvent::Ended(_) => {
                self.visible = false;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_observe_property() {
        let backend = FakePlayerBackend::initialized();
        handle_transport(&backend, "mpv-observe-prop", Some(json!("pause"))).unwrap();
        assert_eq!(
            backend.actions(),
            vec![PlayerAction::ObserveProperty("pause".to_string())]
        );
    }

    #[test]
    fn maps_set_property() {
        let backend = FakePlayerBackend::initialized();
        handle_transport(&backend, "mpv-set-prop", Some(json!(["pause", true]))).unwrap();
        assert_eq!(
            backend.actions(),
            vec![PlayerAction::SetProperty {
                name: "pause".to_string(),
                value: json!(true),
            }]
        );
    }

    #[test]
    fn maps_loadfile_command() {
        let backend = FakePlayerBackend::initialized();
        handle_transport(
            &backend,
            "mpv-command",
            Some(json!(["loadfile", "file:///tmp/sample.mp4", "replace"])),
        )
        .unwrap();
        assert_eq!(
            backend.actions(),
            vec![PlayerAction::Command {
                name: "loadfile".to_string(),
                args: vec!["file:///tmp/sample.mp4".to_string(), "replace".to_string()],
            }]
        );
    }

    #[test]
    fn maps_stop() {
        let backend = FakePlayerBackend::initialized();
        handle_transport(&backend, "native-player-stop", None).unwrap();
        assert_eq!(backend.actions(), vec![PlayerAction::Stop]);
        assert!(backend.stopped());
    }

    #[test]
    fn serializes_player_events() {
        assert_eq!(
            serialize_property_change("pause", json!(true)),
            json!(["mpv-prop-change", {"name": "pause", "data": true}])
        );
        assert_eq!(
            serialize_ended("eof"),
            json!(["mpv-event-ended", {"reason": "eof"}])
        );
        assert_eq!(
            serialize_error("MPV playback error"),
            json!([
                "mpv-event-ended",
                {"reason": "error", "error": {"message": "MPV playback error", "critical": true}}
            ])
        );
    }

    #[test]
    fn mpv_backend_forwards_to_attached_video_layer() {
        let backend = MpvPlayerBackend::default();
        let (sender, receiver) = std::sync::mpsc::channel();
        backend.attach(sender).unwrap();

        backend
            .command(
                "loadfile".to_string(),
                vec!["file:///tmp/a.mp4".to_string()],
            )
            .unwrap();

        assert_eq!(
            receiver.recv().unwrap(),
            MpvBackendCommand::Command {
                name: "loadfile".to_string(),
                args: vec!["file:///tmp/a.mp4".to_string()],
            }
        );
        assert!(backend.status().initialized);
    }

    #[test]
    fn unattached_mpv_backend_rejects_commands() {
        let backend = MpvPlayerBackend::default();
        assert_eq!(
            backend.stop().unwrap_err(),
            "macOS MPV backend is not attached to a native video layer"
        );
    }

    #[test]
    fn video_visibility_tracks_start_video_detection_end_and_error() {
        let mut state = VideoVisibilityState::default();
        assert!(!state.visible());

        state.apply_command(&MpvBackendCommand::Command {
            name: "loadfile".to_string(),
            args: vec!["file:///tmp/sample.mp4".to_string()],
        });
        assert!(state.visible());

        state.apply_event(&PlayerEvent::PropertyChange(PlayerPropertyChange {
            name: "video-params".to_string(),
            data: Value::Null,
        }));
        assert!(!state.visible());

        state.apply_event(&PlayerEvent::PropertyChange(PlayerPropertyChange {
            name: "video-params".to_string(),
            data: json!({"w": 1920, "h": 1080}),
        }));
        assert!(state.visible());

        state.apply_event(&PlayerEvent::Ended(PlayerEnded {
            reason: "eof".to_string(),
            error: None,
        }));
        assert!(!state.visible());

        state.apply_command(&MpvBackendCommand::Command {
            name: "loadfile".to_string(),
            args: vec!["https://example.com/stream.mkv".to_string()],
        });
        state.apply_event(&PlayerEvent::Ended(PlayerEnded {
            reason: "error".to_string(),
            error: Some(PlayerEndedError {
                message: "MPV playback error".to_string(),
                critical: true,
            }),
        }));
        assert!(!state.visible());
    }

    #[test]
    fn default_mpv_options_match_macos_shell_defaults() {
        let options = default_mpv_options("Stremio Lightning", true);
        assert!(options.contains(&MpvOption {
            name: "audio-client-name",
            value: "Stremio Lightning".to_string(),
        }));
        assert!(options.contains(&MpvOption {
            name: "hwdec",
            value: "auto".to_string(),
        }));
        assert!(options.contains(&MpvOption {
            name: "audio-fallback-to-null",
            value: "yes".to_string(),
        }));
        assert!(options.contains(&MpvOption {
            name: "msg-level",
            value: "all=no,cplayer=debug".to_string(),
        }));
    }
}
