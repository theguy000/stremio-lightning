use serde::Serialize;
use stremio_lightning_core::player_api::PlayerCommand;
#[cfg(windows)]
use stremio_lightning_core::player_api::PlayerEvent;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlayerError {
    #[error("Missing mpv-command name")]
    MissingCommandName,

    #[error("Windows MPV backend is not initialized")]
    NotInitialized,

    #[error("Failed to send command to Windows MPV backend: {0}")]
    SendCommand(String),

    #[error("Failed to initialize Windows MPV backend: {0}")]
    Initialization(String),

    #[error("Failed to observe MPV property '{0}': {1}")]
    ObserveProperty(String, String),

    #[error("Failed to set MPV property '{0}': {1}")]
    SetProperty(String, String),

    #[error("Failed to execute MPV command '{0}': {1}")]
    ExecuteCommand(String, String),

    #[error("Failed to stop MPV playback: {0}")]
    Stop(String),

    #[error("Failed to enable Windows MPV diagnostics: {0}")]
    Diagnostics(String),

    #[error("Invalid numeric MPV property value for '{0}'")]
    InvalidNumericPropertyValue(String),

    #[error("{0}")]
    Other(String),
}

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

#[cfg(windows)]
#[derive(Debug)]
pub(crate) enum BackendCommand {
    Player(PlayerCommand),
    Shutdown,
}

#[cfg(windows)]
pub(crate) enum MpvEventAction {
    Emit(PlayerEvent),
    Continue,
    Shutdown,
}
