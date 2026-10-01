mod backend;
#[cfg(test)]
mod tests;
mod types;

pub use types::{NativePlayerStatus, PlayerError};

use backend::PlayerBackend;
use serde_json::Value;
use stremio_lightning_core::player_api::{
    PlayerCommand, PlayerEnded, PlayerEvent, PlayerPropertyChange,
};

const PRIMARY_SUBTITLE_PROPERTY: &str = "sid";
const SECONDARY_SUBTITLE_PROPERTY: &str = "secondary-sid";
const SUB_ADD_COMMAND: &str = "sub-add";
const MAX_MPV_LOG_MESSAGE_LENGTH: usize = 2_048;

#[derive(Debug, Default)]
pub struct WindowsPlayer {
    #[cfg(test)]
    commands: Vec<PlayerCommand>,
    events: Vec<PlayerEvent>,
    backend: PlayerBackend,
}

impl WindowsPlayer {
    #[cfg(windows)]
    pub fn initialize(
        &mut self,
        hwnd: windows::Win32::Foundation::HWND,
        notifier: crate::window::UiThreadNotifier,
    ) -> Result<(), PlayerError> {
        self.backend.initialize(hwnd, notifier)
    }

    pub fn status(&self) -> NativePlayerStatus {
        self.backend.status()
    }

    pub fn handle_transport(
        &mut self,
        method: &str,
        payload: Option<Value>,
    ) -> Result<(), PlayerError> {
        let command = match method {
            "mpv-observe-prop" => PlayerCommand::ObserveProperty(
                payload
                    .and_then(|value| value.as_str().map(ToOwned::to_owned))
                    .ok_or(PlayerError::MissingPayload("mpv-observe-prop"))?,
            ),
            "mpv-set-prop" => {
                let values = payload
                    .and_then(|value| value.as_array().cloned())
                    .ok_or(PlayerError::InvalidPayload("mpv-set-prop"))?;
                let name = values
                    .first()
                    .and_then(Value::as_str)
                    .ok_or(PlayerError::MissingPropertyName("mpv-set-prop"))?
                    .to_string();
                let value = values
                    .get(1)
                    .cloned()
                    .ok_or(PlayerError::MissingPropertyValue("mpv-set-prop"))?;
                PlayerCommand::SetProperty(name, value)
            }
            "mpv-command" => PlayerCommand::Command(
                payload
                    .and_then(|value| value.as_array().cloned())
                    .ok_or(PlayerError::InvalidPayload("mpv-command"))?,
            ),
            "native-player-stop" => PlayerCommand::Stop,
            other => return Err(PlayerError::UnsupportedCommand(other.to_string())),
        };

        log_player_command(&command);

        if matches!(
            &command,
            PlayerCommand::SetProperty(name, _) if name == PRIMARY_SUBTITLE_PROPERTY
        ) {
            self.disable_secondary_subtitle()?;
        }
        let is_sub_add = matches!(
            &command,
            PlayerCommand::Command(values)
                if values.first().and_then(Value::as_str) == Some(SUB_ADD_COMMAND)
        );
        self.handle_command(command)?;
        if is_sub_add {
            self.disable_secondary_subtitle()?;
        }
        Ok(())
    }

    fn disable_secondary_subtitle(&mut self) -> Result<(), PlayerError> {
        self.handle_command(PlayerCommand::SetProperty(
            SECONDARY_SUBTITLE_PROPERTY.to_string(),
            Value::from("no"),
        ))
    }

    fn handle_command(&mut self, command: PlayerCommand) -> Result<(), PlayerError> {
        #[cfg(test)]
        self.commands.push(command.clone());
        self.backend.handle_command(command)?;
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

    #[cfg(test)]
    pub fn commands(&self) -> &[PlayerCommand] {
        &self.commands
    }

    pub fn drain_events(&mut self) -> Vec<PlayerEvent> {
        let mut events = std::mem::take(&mut self.events);
        events.extend(self.backend.drain_events());
        events
    }

    pub fn shutdown(&mut self) {
        self.backend.shutdown();
    }
}

fn log_player_command(command: &PlayerCommand) {
    match command {
        PlayerCommand::Command(values) => {
            let Some(name) = values.first().and_then(Value::as_str) else {
                return;
            };
            let message =
                format!("[StremioLightning] MPV command received: {name} (arguments redacted)");
            if name == "loadfile" {
                stremio_lightning_core::logging::info("native.player", message);
            } else {
                stremio_lightning_core::logging::debug("native.player", message);
            }
        }
        PlayerCommand::Stop => stremio_lightning_core::logging::info(
            "native.player",
            "[StremioLightning] MPV stop requested",
        ),
        PlayerCommand::ObserveProperty(_) | PlayerCommand::SetProperty(_, _) => {}
    }
}

pub(crate) fn sanitize_mpv_log_message(message: &str) -> String {
    let mut sanitized = stremio_lightning_core::logging::sanitize_text(message.trim());

    if sanitized.len() > MAX_MPV_LOG_MESSAGE_LENGTH {
        let mut boundary = MAX_MPV_LOG_MESSAGE_LENGTH;
        while !sanitized.is_char_boundary(boundary) {
            boundary -= 1;
        }
        sanitized.truncate(boundary);
        sanitized.push_str("... [truncated]");
    }
    sanitized
}

pub(crate) fn command_name_and_args(
    values: &[Value],
) -> Result<(String, Vec<String>), PlayerError> {
    let name = values
        .first()
        .and_then(Value::as_str)
        .ok_or(PlayerError::MissingCommandName)?
        .to_string();
    let args = values
        .iter()
        .skip(1)
        .map(|value| match value {
            Value::String(value) => value.clone(),
            other => other.to_string(),
        })
        .collect();
    Ok((name, args))
}
