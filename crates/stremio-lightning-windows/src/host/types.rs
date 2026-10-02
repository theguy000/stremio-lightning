use serde::{Deserialize, Serialize};
use serde_json::Value;
use stremio_lightning_core::host_api::HostEventRecord;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WindowsHostError {
    #[error("Lock poisoned: {0}")]
    LockPoisoned(String),
    #[error("Invalid IPC message: {0}")]
    InvalidIpc(String),
    #[error("Controller uninitialized: {0}")]
    ControllerUninitialized(String),
    #[error("External URL error: {0}")]
    ExternalUrl(String),
    #[error("Player error: {0}")]
    Player(#[from] crate::player::PlayerError),
    #[error("Window error: {0}")]
    Window(#[from] crate::window::WindowError),
    #[error("Core API error: {0}")]
    Core(String),
    #[error("Host API error: {0}")]
    HostApi(#[from] stremio_lightning_core::host_api::HostApiError),
}

impl From<String> for WindowsHostError {
    fn from(error: String) -> Self {
        Self::Core(error)
    }
}

impl From<&str> for WindowsHostError {
    fn from(error: &str) -> Self {
        Self::Core(error.to_string())
    }
}

impl From<WindowsHostError> for String {
    fn from(error: WindowsHostError) -> Self {
        error.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum WindowsIpcOutbound {
    Response { id: u64, ok: bool, value: Value },
    Event { event: String, payload: Value },
}

impl From<HostEventRecord> for WindowsIpcOutbound {
    fn from(record: HostEventRecord) -> Self {
        Self::Event {
            event: record.event,
            payload: record.payload,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct WindowRuntimeState {
    pub visible: bool,
    pub maximized: bool,
    pub fullscreen: bool,
    pub focused: bool,
}

impl Default for WindowRuntimeState {
    fn default() -> Self {
        Self {
            visible: true,
            maximized: false,
            fullscreen: false,
            focused: true,
        }
    }
}
