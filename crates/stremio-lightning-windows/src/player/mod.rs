mod backend;
#[cfg(test)]
mod tests;
mod types;

pub use types::{NativePlayerStatus, PlayerError};

use backend::PlayerBackend;
use serde_json::Value;
use std::borrow::Cow;
use stremio_lightning_core::player_api::{
    PlayerCommand, PlayerEnded, PlayerEvent, PlayerPropertyChange,
};

const MAX_MPV_LOG_MESSAGE_LENGTH: usize = 2_048;

#[derive(Debug, Default)]
pub struct WindowsPlayer {
    #[cfg(test)]
    commands: Vec<PlayerCommand>,
    events: Vec<PlayerEvent>,
    backend: PlayerBackend,
}

impl WindowsPlayer {
    /// # Errors
    /// Returns an error when the native player cannot be initialized.
    #[cfg(windows)]
    pub fn initialize(
        &mut self,
        hwnd: windows::Win32::Foundation::HWND,
        notifier: crate::window::UiThreadNotifier,
    ) -> Result<(), PlayerError> {
        self.backend.initialize(hwnd, notifier)
    }

    #[must_use]
    pub fn status(&self) -> NativePlayerStatus {
        self.backend.status()
    }

    /// # Errors
    /// Returns an error when the transport command cannot be applied.
    pub fn handle_transport(
        &mut self,
        method: &str,
        payload: Option<Value>,
    ) -> Result<(), PlayerError> {
        let command = PlayerCommand::from_transport(method, payload).map_err(PlayerError::Other)?;
        log_player_command(&command);
        command.execute_in_order(|command| self.handle_command(command))
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
    #[must_use]
    pub fn commands(&self) -> &[PlayerCommand] {
        &self.commands
    }

    /// Moves buffered events into `out`. `append` empties this player's buffer
    /// without releasing it, so the next tick does not regrow from zero.
    pub fn drain_events_into(&mut self, out: &mut Vec<PlayerEvent>) {
        out.append(&mut self.events);
        self.backend.drain_events_into(out);
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
) -> Result<(&str, Vec<Cow<'_, str>>), PlayerError> {
    let name = values
        .first()
        .and_then(Value::as_str)
        .ok_or(PlayerError::MissingCommandName)?;
    let args = values
        .iter()
        .skip(1)
        .map(|value| match value {
            Value::String(value) => Cow::Borrowed(value.as_str()),
            other => Cow::Owned(other.to_string()),
        })
        .collect();
    Ok((name, args))
}
