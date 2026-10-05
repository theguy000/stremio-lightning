use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use thiserror::Error;

use crate::streaming_logs::StreamingLogTails;

pub const TRANSPORT_OBJECT: &str = "transport";
pub const RPC_TYPE_INIT: u8 = 3;
pub const RPC_TYPE_SIGNAL: u8 = 1;
pub const RPC_TYPE_INVOKE_METHOD: u8 = 6;
pub const SHELL_TRANSPORT_EVENT: &str = "shell-transport-message";

#[derive(Debug, Error)]
pub enum HostApiError {
    #[error("Lock poisoned: {0}")]
    LockPoisoned(String),
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    #[error("Invalid payload for {command}: {error}")]
    InvalidPayload { command: String, error: String },
    #[error("Unsupported host command: {0}")]
    UnsupportedCommand(String),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Validation error: {0}")]
    Validation(#[from] crate::validation::ValidationError),
    #[error("Platform bridge error: {0}")]
    Platform(String),
}

impl From<String> for HostApiError {
    fn from(error: String) -> Self {
        Self::Platform(error)
    }
}

impl From<&str> for HostApiError {
    fn from(error: &str) -> Self {
        Self::Platform(error.to_string())
    }
}

impl From<HostApiError> for String {
    fn from(error: HostApiError) -> Self {
        error.to_string()
    }
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HostCommand {
    Init,
    ToggleDevtools,
    OpenExternalUrl,
    ShellTransportSend,
    ShellBridgeReady,
    GetNativePlayerStatus,
    StartStreamingServer,
    StopStreamingServer,
    RestartStreamingServer,
    GetStreamingServerStatus,
    GetPlugins,
    GetThemes,
    DownloadMod,
    DeleteMod,
    GetModContent,
    GetRegistry,
    CheckModUpdates,
    GetSetting,
    SaveSetting,
    RegisterSettings,
    GetRegisteredSettings,
    StartDiscordRpc,
    StopDiscordRpc,
    UpdateDiscordActivity,
    CheckAppUpdate,
    SetAutoPause,
    GetAutoPause,
    SetPipDisablesAutoPause,
    GetPipDisablesAutoPause,
    TogglePip,
    GetPipMode,
    SetPipSize,
    GetLogs,
    SubmitDiagnosticLogs,
    SetExtendedDiagnostics,
    GetDiagnosticReport,
    ClearDiagnostics,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
pub enum HostEvent {
    #[serde(rename = "window-maximized-changed")]
    WindowMaximizedChanged,
    #[serde(rename = "window-fullscreen-changed")]
    WindowFullscreenChanged,
    #[serde(rename = "server-started")]
    ServerStarted,
    #[serde(rename = "server-stopped")]
    ServerStopped,
    #[serde(rename = "shell-transport-message")]
    ShellTransportMessage,
}

impl HostEvent {
    /// The exact string serde emits for this variant. Keeping it beside the
    /// `rename` attributes is what stops an emitted event name from drifting
    /// away from its serialized form.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::WindowMaximizedChanged => "window-maximized-changed",
            Self::WindowFullscreenChanged => "window-fullscreen-changed",
            Self::ServerStarted => "server-started",
            Self::ServerStopped => "server-stopped",
            Self::ShellTransportMessage => "shell-transport-message",
        }
    }
}

#[derive(Deserialize, Debug, PartialEq)]
pub struct RpcRequest {
    pub id: u64,
    #[serde(rename = "type")]
    pub request_type: Option<u8>,
    pub args: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RpcResponseDataTransport {
    pub properties: Vec<Vec<String>>,
    pub signals: Vec<String>,
    pub methods: Vec<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RpcResponseData {
    pub transport: RpcResponseDataTransport,
}

#[derive(Default, Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct RpcResponse {
    pub id: u64,
    pub object: String,
    #[serde(rename = "type")]
    pub response_type: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<RpcResponseData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Value>,
}

#[derive(Debug, PartialEq)]
pub enum ParsedRequest {
    Handshake,
    Command { method: String, data: Option<Value> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostEventRecord {
    pub event: String,
    pub payload: Value,
}

#[derive(Debug, Default)]
pub struct ListenerRegistry {
    pub next_id: u64,
    pub listeners: HashMap<u64, String>,
    pub emitted: Vec<HostEventRecord>,
    pub bridge_ready: bool,
    pub transport_ready: bool,
    pub pending_transport_messages: VecDeque<String>,
}

impl ListenerRegistry {
    pub fn listen(&mut self, event: impl Into<String>) -> u64 {
        self.next_id += 1;
        self.listeners.insert(self.next_id, event.into());
        self.next_id
    }

    pub fn listen_with_id(&mut self, id: u64, event: impl Into<String>) {
        self.next_id = self.next_id.max(id);
        self.listeners.insert(id, event.into());
    }

    pub fn unlisten(&mut self, id: u64) {
        self.listeners.remove(&id);
    }

    pub fn emit(&mut self, event: impl Into<String>, payload: Value) {
        let event = event.into();
        if self.listeners.values().any(|listener| listener == &event) {
            self.emitted.push(HostEventRecord { event, payload });
        }
    }

    pub fn drain_emitted(&mut self) -> Vec<HostEventRecord> {
        std::mem::take(&mut self.emitted)
    }

    /// Moves buffered events into `out`. `append` empties this queue without
    /// releasing its allocation, so the next emit does not regrow from zero.
    pub fn drain_emitted_into(&mut self, out: &mut Vec<HostEventRecord>) {
        out.append(&mut self.emitted);
    }
}

pub trait PlatformBridge: Send + Sync {
    fn platform_name(&self) -> &'static str;
    fn shell_name(&self) -> &'static str;
    fn native_player_status(&self) -> Value;
    fn is_streaming_server_running(&self) -> bool;

    // Window methods
    /// # Errors
    /// Returns an error when the platform cannot minimize the window.
    fn minimize_window(&self) -> Result<(), String> {
        Ok(())
    }
    /// # Errors
    /// Returns an error when the platform cannot focus the window.
    fn focus_window(&self) -> Result<(), String> {
        Ok(())
    }
    /// # Errors
    /// Returns an error when the platform cannot toggle maximization, together with the resulting state.
    fn toggle_window_maximize(&self) -> Result<bool, String> {
        Ok(false)
    }
    /// # Errors
    /// Returns an error when the platform cannot close the window.
    fn close_window(&self) -> Result<(), String> {
        Ok(())
    }
    /// # Errors
    /// Returns an error when the platform cannot start a window drag.
    fn start_window_dragging(&self) -> Result<(), String> {
        Ok(())
    }
    /// # Errors
    /// Returns an error when the window state cannot be read.
    fn is_window_maximized(&self) -> Result<bool, String> {
        Ok(false)
    }
    /// # Errors
    /// Returns an error when the window state cannot be read.
    fn is_window_fullscreen(&self) -> Result<bool, String> {
        Ok(false)
    }
    /// # Errors
    /// Returns an error when the platform cannot change the fullscreen state.
    fn set_window_fullscreen(&self, _fullscreen: bool) -> Result<(), String> {
        Ok(())
    }
    /// # Errors
    /// Returns an error when the platform cannot change the webview zoom level.
    fn set_webview_zoom(&self, _level: f64) -> Result<(), String> {
        Ok(())
    }

    // Player/Pip methods
    /// # Errors
    /// Returns an error when the platform cannot toggle picture-in-picture, together with the resulting state.
    fn toggle_picture_in_picture(&self) -> Result<bool, String>;
    /// # Errors
    /// Returns an error when the picture-in-picture state cannot be read.
    fn is_pip_enabled(&self) -> Result<bool, String>;
    /// # Errors
    /// Returns an error when the platform cannot resize the picture-in-picture window.
    fn set_pip_size(&self, _width: i32, _height: i32) -> Result<(), String> {
        Ok(())
    }

    // Custom platform controls
    /// # Errors
    /// Returns an error when the platform cannot open the URL.
    fn open_external_url(&self, _url: &str) -> Result<(), String> {
        Ok(())
    }
    /// # Errors
    /// Returns an error when the streaming server state cannot be read.
    fn streaming_server_status(&self) -> Result<Value, String> {
        Ok(Value::Bool(self.is_streaming_server_running()))
    }
    /// # Errors
    /// Returns an error when the streaming server state cannot be read.
    #[deprecated(note = "use `streaming_server_status` instead")]
    fn get_streaming_server_status(&self) -> Result<Value, String> {
        self.streaming_server_status()
    }

    /// # Errors
    /// Returns an error when the streaming server cannot be started.
    fn start_streaming_server(&self) -> Result<(), String> {
        Ok(())
    }
    /// # Errors
    /// Returns an error when the streaming server cannot be stopped.
    fn stop_streaming_server(&self) -> Result<(), String> {
        Ok(())
    }
    /// # Errors
    /// Returns an error when the streaming server cannot be restarted.
    fn restart_streaming_server(&self) -> Result<(), String> {
        Ok(())
    }

    fn diagnostics_webview_engine(&self) -> &'static str {
        self.shell_name()
    }

    fn diagnostics_webview_version(&self) -> Option<String> {
        None
    }

    fn native_http_diagnostics(&self) -> bool {
        false
    }

    fn native_network_failure_diagnostics(&self) -> bool {
        false
    }

    /// # Errors
    /// Returns an error when the streaming server output cannot be read.
    fn streaming_log_tails(
        &self,
        _max_bytes_per_stream: usize,
    ) -> Result<Option<StreamingLogTails>, String> {
        Ok(None)
    }

    /// # Errors
    /// Returns an error when the streaming server output cannot be cleared.
    fn clear_streaming_logs(&self) -> Result<(), String> {
        Ok(())
    }

    // Transport commands delegator
    /// # Errors
    /// Returns an error when the platform rejects the custom transport message.
    fn handle_custom_transport(&self, _method: &str, _data: Option<Value>) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct ShellPreferenceState {
    pub auto_pause: bool,
    pub pip_disables_auto_pause: bool,
    pub auto_paused: bool,
    pub player_active: bool,
    pub player_paused: bool,
}

impl Default for ShellPreferenceState {
    fn default() -> Self {
        Self {
            auto_pause: true,
            pip_disables_auto_pause: true,
            auto_paused: false,
            player_active: false,
            player_paused: true,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadModPayload {
    pub url: String,
    pub mod_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetLogsPayload {
    #[serde(default)]
    pub after_id: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmitDiagnosticLogsPayload {
    pub entries: Vec<crate::logging::ExternalLogEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetExtendedDiagnosticsPayload {
    pub enabled: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModFilePayload {
    pub filename: String,
    pub mod_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModTypePayload {
    pub mod_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingKeyPayload {
    pub plugin_name: String,
    pub key: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSettingPayload {
    pub plugin_name: String,
    pub key: String,
    pub value: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterSettingsPayload {
    pub plugin_name: String,
    pub schema: String,
}

#[derive(Debug, Deserialize)]
pub struct ListenIpcPayload {
    pub id: u64,
    pub event: String,
}

#[derive(Debug, Deserialize)]
pub struct UnlistenIpcPayload {
    pub id: u64,
}

#[derive(Debug, Deserialize)]
pub struct FullscreenIpcPayload {
    pub fullscreen: bool,
}

#[derive(Debug, Deserialize)]
pub struct FocusChangedPayload {
    pub focused: bool,
}

#[derive(Debug, Deserialize)]
pub struct ZoomIpcPayload {
    pub level: f64,
}

#[derive(Debug, Deserialize)]
pub struct IpcRequest {
    pub id: u64,
    pub kind: String,
    pub payload: Option<Value>,
}

#[cfg(test)]
mod tests {
    use super::HostEvent;

    const ALL: [HostEvent; 5] = [
        HostEvent::WindowMaximizedChanged,
        HostEvent::WindowFullscreenChanged,
        HostEvent::ServerStarted,
        HostEvent::ServerStopped,
        HostEvent::ShellTransportMessage,
    ];

    #[test]
    fn as_str_matches_the_serialized_form() {
        for event in ALL {
            assert_eq!(
                serde_json::to_string(&event).unwrap(),
                serde_json::to_string(event.as_str()).unwrap(),
                "{event:?}"
            );
        }
    }
}
