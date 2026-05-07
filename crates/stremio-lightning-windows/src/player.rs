use serde::Serialize;
use serde_json::Value;
use stremio_lightning_core::player_api::{
    PlayerCommand, PlayerEnded, PlayerEvent, PlayerPropertyChange,
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NativePlayerStatus {
    pub enabled: bool,
    pub initialized: bool,
    pub backend: &'static str,
}

impl Default for NativePlayerStatus {
    fn default() -> Self {
        Self {
            enabled: cfg!(windows),
            initialized: false,
            backend: "webview2-libmpv",
        }
    }
}

#[derive(Debug, Default)]
pub struct WindowsPlayer {
    commands: Vec<PlayerCommand>,
    events: Vec<PlayerEvent>,
}

impl WindowsPlayer {
    pub fn status(&self) -> NativePlayerStatus {
        NativePlayerStatus {
            initialized: cfg!(windows),
            ..NativePlayerStatus::default()
        }
    }

    pub fn handle_transport(&mut self, method: &str, payload: Option<Value>) -> Result<(), String> {
        let command = match method {
            "mpv-observe-prop" => PlayerCommand::ObserveProperty(
                payload
                    .and_then(|value| value.as_str().map(ToOwned::to_owned))
                    .ok_or_else(|| "Missing mpv-observe-prop payload".to_string())?,
            ),
            "mpv-set-prop" => {
                let values = payload
                    .and_then(|value| value.as_array().cloned())
                    .ok_or_else(|| "Invalid mpv-set-prop payload".to_string())?;
                let name = values
                    .first()
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Missing mpv-set-prop name".to_string())?
                    .to_string();
                let value = values
                    .get(1)
                    .cloned()
                    .ok_or_else(|| "Missing mpv-set-prop value".to_string())?;
                PlayerCommand::SetProperty(name, value)
            }
            "mpv-command" => PlayerCommand::Command(
                payload
                    .and_then(|value| value.as_array().cloned())
                    .ok_or_else(|| "Invalid mpv-command payload".to_string())?,
            ),
            "native-player-stop" => PlayerCommand::Stop,
            other => return Err(format!("Unsupported Windows player command: {other}")),
        };

        self.commands.push(command);
        Ok(())
    }

    pub fn emit_property_change(&mut self, name: impl Into<String>, data: Value) {
        self.events
            .push(PlayerEvent::PropertyChange(PlayerPropertyChange {
                name: name.into(),
                data,
            }));
    }

    pub fn emit_ended(&mut self, reason: impl Into<String>) {
        self.events.push(PlayerEvent::Ended(PlayerEnded {
            reason: reason.into(),
            error: None,
        }));
    }

    pub fn commands(&self) -> &[PlayerCommand] {
        &self.commands
    }

    pub fn drain_events(&mut self) -> Vec<PlayerEvent> {
        std::mem::take(&mut self.events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_transport_commands_to_shared_player_commands() {
        let mut player = WindowsPlayer::default();
        player
            .handle_transport("mpv-observe-prop", Some(json!("pause")))
            .unwrap();
        player
            .handle_transport("mpv-set-prop", Some(json!(["pause", true])))
            .unwrap();
        player
            .handle_transport(
                "mpv-command",
                Some(json!(["loadfile", "file:///video.mp4"])),
            )
            .unwrap();

        assert_eq!(
            player.commands(),
            &[
                PlayerCommand::ObserveProperty("pause".to_string()),
                PlayerCommand::SetProperty("pause".to_string(), json!(true)),
                PlayerCommand::Command(vec![json!("loadfile"), json!("file:///video.mp4")]),
            ]
        );
    }
}
