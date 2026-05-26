use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::pip::serialize_picture_in_picture;
use crate::{app_update, mods, settings};

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
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Eq)]
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

pub const TRANSPORT_OBJECT: &str = "transport";
pub const RPC_TYPE_INIT: u8 = 3;
pub const RPC_TYPE_SIGNAL: u8 = 1;
pub const RPC_TYPE_INVOKE_METHOD: u8 = 6;

pub fn parse_request(message: &str) -> Result<ParsedRequest, String> {
    let request: RpcRequest = serde_json::from_str(message)
        .map_err(|e| format!("Failed to parse shell transport message: {e}"))?;

    if request.id == 0 || request.request_type == Some(RPC_TYPE_INIT) {
        return Ok(ParsedRequest::Handshake);
    }

    match request.request_type {
        Some(RPC_TYPE_INVOKE_METHOD) | None => {}
        Some(request_type) => {
            return Err(format!(
                "Unsupported shell transport request type: {request_type}"
            ));
        }
    }

    let args = request
        .args
        .and_then(|value| value.as_array().cloned())
        .ok_or_else(|| "Missing shell transport args".to_string())?;
    let method = args
        .first()
        .and_then(Value::as_str)
        .ok_or_else(|| "Missing shell transport method".to_string())?
        .to_string();
    let data = args.get(1).cloned();

    Ok(ParsedRequest::Command { method, data })
}

pub fn handshake_response(package_version: &str) -> String {
    serde_json::to_string(&RpcResponse {
        id: 0,
        object: TRANSPORT_OBJECT.to_string(),
        response_type: RPC_TYPE_INIT,
        data: Some(RpcResponseData {
            transport: RpcResponseDataTransport {
                properties: vec![
                    vec![],
                    vec![
                        String::new(),
                        "shellVersion".to_string(),
                        String::new(),
                        package_version.to_string(),
                    ],
                ],
                signals: vec![],
                methods: vec![vec!["onEvent".to_string()]],
            },
        }),
        ..Default::default()
    })
    .expect("failed to serialize handshake response")
}

pub fn response_message(args: Value) -> String {
    serde_json::to_string(&RpcResponse {
        id: 1,
        object: TRANSPORT_OBJECT.to_string(),
        response_type: RPC_TYPE_SIGNAL,
        args: Some(args),
        ..Default::default()
    })
    .expect("failed to serialize transport response")
}

pub fn serialize_window_visibility(visible: bool, is_fullscreen: bool) -> Value {
    serde_json::json!([
        "win-visibility-changed",
        {
            "visible": visible,
            "visibility": u8::from(is_fullscreen),
            "isFullscreen": is_fullscreen
        }
    ])
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostEventRecord {
    pub event: String,
    pub payload: Value,
}

pub const SHELL_TRANSPORT_EVENT: &str = "shell-transport-message";

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
}

pub trait PlatformBridge: Send + Sync {
    fn platform_name(&self) -> &'static str;
    fn shell_name(&self) -> &'static str;
    fn native_player_status(&self) -> Value;
    fn is_streaming_server_running(&self) -> bool;

    // Window methods
    fn minimize_window(&self) -> Result<(), String> {
        Ok(())
    }
    fn focus_window(&self) -> Result<(), String> {
        Ok(())
    }
    fn toggle_window_maximize(&self) -> Result<bool, String> {
        Ok(false)
    }
    fn close_window(&self) -> Result<(), String> {
        Ok(())
    }
    fn start_window_dragging(&self) -> Result<(), String> {
        Ok(())
    }
    fn is_window_maximized(&self) -> Result<bool, String> {
        Ok(false)
    }
    fn is_window_fullscreen(&self) -> Result<bool, String> {
        Ok(false)
    }
    fn set_window_fullscreen(&self, _fullscreen: bool) -> Result<(), String> {
        Ok(())
    }
    fn set_webview_zoom(&self, _level: f64) -> Result<(), String> {
        Ok(())
    }

    // Player/Pip methods
    fn toggle_picture_in_picture(&self) -> Result<bool, String>;
    fn is_pip_enabled(&self) -> Result<bool, String>;

    // Custom platform controls
    fn open_external_url(&self, _url: &str) -> Result<(), String> {
        Ok(())
    }
    fn get_streaming_server_status(&self) -> Result<Value, String> {
        Ok(Value::Bool(self.is_streaming_server_running()))
    }

    fn start_streaming_server(&self) -> Result<(), String> {
        Ok(())
    }
    fn stop_streaming_server(&self) -> Result<(), String> {
        Ok(())
    }
    fn restart_streaming_server(&self) -> Result<(), String> {
        Ok(())
    }

    // Transport commands delegator
    fn handle_custom_transport(&self, _method: &str, _data: Option<Value>) -> Result<(), String> {
        Ok(())
    }
}

pub struct BaseHost<P: PlatformBridge> {
    pub bridge: P,
    pub listeners: Mutex<ListenerRegistry>,
    pub settings: settings::SettingsState,
    pub app_data_dir: PathBuf,
    pub package_version: &'static str,
    pub shell_preferences: Mutex<ShellPreferenceState>,
    pub discord_rpc: Arc<crate::discord_rpc::DiscordRpcState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellPreferenceState {
    pub auto_pause: bool,
    pub pip_disables_auto_pause: bool,
    pub auto_paused: bool,
    pub player_paused: bool,
}

impl Default for ShellPreferenceState {
    fn default() -> Self {
        Self {
            auto_pause: true,
            pip_disables_auto_pause: true,
            auto_paused: false,
            player_paused: true,
        }
    }
}

impl<P: PlatformBridge> BaseHost<P> {
    pub fn new(bridge: P, app_data_dir: PathBuf, package_version: &'static str) -> Self {
        Self {
            bridge,
            listeners: Mutex::default(),
            settings: settings::SettingsState::default(),
            app_data_dir,
            package_version,
            shell_preferences: Mutex::default(),
            discord_rpc: Arc::new(crate::discord_rpc::DiscordRpcState::default()),
        }
    }

    pub fn lock_listeners(&self) -> Result<std::sync::MutexGuard<'_, ListenerRegistry>, String> {
        self.listeners
            .lock()
            .map_err(|e| format!("Listeners lock poisoned: {e}"))
    }

    pub fn listen_with_id(&self, id: u64, event: impl Into<String>) -> Result<(), String> {
        let mut registry = self.lock_listeners()?;
        registry.listen_with_id(id, event);
        self.flush_pending_transport_messages(&mut registry);
        Ok(())
    }

    pub fn unlisten(&self, id: u64) -> Result<(), String> {
        self.lock_listeners()?.unlisten(id);
        Ok(())
    }

    pub fn mark_bridge_ready(&self) -> Result<(), String> {
        let mut registry = self.lock_listeners()?;
        registry.bridge_ready = true;
        self.flush_pending_transport_messages(&mut registry);
        Ok(())
    }

    pub fn mark_transport_ready(&self) -> Result<(), String> {
        let mut registry = self.lock_listeners()?;
        registry.transport_ready = true;
        self.flush_pending_transport_messages(&mut registry);
        Ok(())
    }

    pub fn queue_transport_message(&self, message: String) -> Result<(), String> {
        self.update_player_paused_from_transport(&Value::String(message.clone()));
        let mut registry = self.lock_listeners()?;
        if registry.pending_transport_messages.len() >= 512 {
            registry.pending_transport_messages.pop_front();
        }
        registry.pending_transport_messages.push_back(message);
        self.flush_pending_transport_messages(&mut registry);
        Ok(())
    }

    pub fn emit_transport_message(&self, message: String) -> Result<(), String> {
        self.emit_event(SHELL_TRANSPORT_EVENT, json!(message))
    }

    fn flush_pending_transport_messages(&self, registry: &mut ListenerRegistry) {
        if !registry.bridge_ready
            || !registry.transport_ready
            || !registry
                .listeners
                .values()
                .any(|listener| listener == SHELL_TRANSPORT_EVENT)
        {
            return;
        }

        let pending = std::mem::take(&mut registry.pending_transport_messages);
        for message in pending {
            registry.emitted.push(HostEventRecord {
                event: SHELL_TRANSPORT_EVENT.to_string(),
                payload: json!(message),
            });
        }
    }

    fn update_player_paused_from_transport(&self, payload: &Value) {
        // Case 1: Payload is a serialized JSON string (Linux/Windows queued/emitted message)
        if let Some(msg_str) = payload.as_str() {
            if let Ok(resp) = serde_json::from_str::<RpcResponse>(msg_str) {
                if let Some(arr) = resp.args.as_ref().and_then(|v| v.as_array()) {
                    if let Some(event_type) = arr.first().and_then(|v| v.as_str()) {
                        let (name, data) = match event_type {
                            "mpv-prop-change" => {
                                let prop = arr.get(1);
                                let name =
                                    prop.and_then(|p| p.get("name")).and_then(|v| v.as_str());
                                let data = prop.and_then(|p| p.get("data"));
                                (name, data)
                            }
                            _ => (None, None),
                        };
                        self.handle_player_event(event_type, name, data);
                    }
                }
            }
            return;
        }

        // Case 2: Payload is a raw JSON object (macOS native player event)
        if let Some(obj) = payload.as_object() {
            if let Some(event_type) = obj.get("type").and_then(|v| v.as_str()) {
                let (name, data) = match event_type {
                    "mpv-prop-change" => {
                        let name = obj.get("name").and_then(|v| v.as_str());
                        let data = obj.get("data");
                        (name, data)
                    }
                    _ => (None, None),
                };
                self.handle_player_event(event_type, name, data);
            }
        }
    }

    fn handle_player_event(
        &self,
        event_type: &str,
        prop_name: Option<&str>,
        prop_data: Option<&Value>,
    ) {
        match (event_type, prop_name) {
            ("mpv-prop-change", Some("pause")) => {
                if let Some(paused) = prop_data.and_then(|v| v.as_bool()) {
                    self.shell_preferences.lock().unwrap().player_paused = paused;
                }
            }
            ("mpv-event-ended", _) => {
                let mut prefs = self.shell_preferences.lock().unwrap();
                prefs.player_paused = true;
                prefs.auto_paused = false;
            }
            _ => {}
        }
    }

    fn update_player_paused_from_set_prop(&self, payload: &Option<Value>) {
        let Some(args) = payload.as_ref().and_then(|v| v.as_array()) else {
            return;
        };
        if args.first().and_then(|v| v.as_str()) != Some("pause") {
            return;
        }
        let Some(paused) = args.get(1).and_then(|v| v.as_bool()) else {
            return;
        };

        let mut prefs = self.shell_preferences.lock().unwrap();
        prefs.player_paused = paused;
        prefs.auto_paused = false;
    }

    pub fn emit_event(&self, event: impl Into<String>, payload: Value) -> Result<(), String> {
        let event = event.into();
        if event == SHELL_TRANSPORT_EVENT {
            self.update_player_paused_from_transport(&payload);
        }
        self.lock_listeners()?.emit(event, payload);
        Ok(())
    }

    pub fn emit_host_event(&self, event: HostEvent, payload: Value) -> Result<(), String> {
        let event = serde_json::to_value(event)
            .map_err(|e| format!("Failed to serialize host event: {e}"))?
            .as_str()
            .ok_or_else(|| "Host event did not serialize to a string".to_string())?
            .to_string();
        self.emit_event(event, payload)
    }

    pub fn drain_emitted_events(&self) -> Result<Vec<HostEventRecord>, String> {
        Ok(self.lock_listeners()?.drain_emitted())
    }

    pub fn dispatch_ipc(&self, kind: &str, payload: Option<Value>) -> Result<Value, String> {
        match kind {
            "invoke" => {
                let payload: InvokeIpcPayload = parse_payload(kind, payload)?;
                self.invoke(&payload.command, payload.payload)
            }
            "listen" => {
                let payload: ListenIpcPayload = parse_payload(kind, payload)?;
                self.listen_with_id(payload.id, payload.event)?;
                Ok(Value::Null)
            }
            "unlisten" => {
                let payload: UnlistenIpcPayload = parse_payload(kind, payload)?;
                self.unlisten(payload.id)?;
                Ok(Value::Null)
            }
            "window.minimize" => {
                self.bridge.minimize_window()?;
                Ok(Value::Null)
            }
            "window.focus" => {
                self.bridge.focus_window()?;
                Ok(Value::Null)
            }
            "window.focus_changed" => {
                let payload: FocusChangedPayload = parse_payload(kind, payload)?;
                self.update_window_focus(payload.focused)?;
                Ok(Value::Null)
            }
            "window.toggleMaximize" => {
                let maximized = self.bridge.toggle_window_maximize()?;
                match self.bridge.platform_name() {
                    "macos" => {
                        self.emit_event(
                            "window-maximized-changed",
                            json!({ "maximized": maximized }),
                        )?;
                    }
                    _ => {
                        self.emit_host_event(HostEvent::WindowMaximizedChanged, json!(maximized))?;
                    }
                }
                Ok(Value::Null)
            }
            "window.close" => {
                self.bridge.close_window()?;
                Ok(Value::Null)
            }
            "window.startDragging" => {
                self.bridge.start_window_dragging()?;
                Ok(Value::Null)
            }
            "window.isMaximized" => Ok(json!(self.bridge.is_window_maximized()?)),
            "window.isFullscreen" => Ok(json!(self.bridge.is_window_fullscreen()?)),
            "window.setFullscreen" => {
                let payload: FullscreenIpcPayload = parse_payload(kind, payload)?;
                self.bridge.set_window_fullscreen(payload.fullscreen)?;
                match self.bridge.platform_name() {
                    "macos" => {
                        self.emit_event(
                            "window-fullscreen-changed",
                            json!({ "fullscreen": payload.fullscreen }),
                        )?;
                        self.emit_transport_message(response_message(
                            serialize_window_visibility(true, payload.fullscreen),
                        ))?;
                    }
                    _ => {
                        self.emit_host_event(
                            HostEvent::WindowFullscreenChanged,
                            json!(payload.fullscreen),
                        )?;
                        self.emit_transport_message(response_message(
                            serialize_window_visibility(true, payload.fullscreen),
                        ))?;
                    }
                }
                Ok(Value::Null)
            }
            "webview.setZoom" => {
                let payload: ZoomIpcPayload = parse_payload(kind, payload)?;
                if !payload.level.is_finite() || payload.level <= 0.0 {
                    return Err("Invalid webview zoom level".to_string());
                }
                self.bridge.set_webview_zoom(payload.level)?;
                Ok(Value::Null)
            }
            other => Err(format!("Unsupported IPC kind: {other}")),
        }
    }

    pub fn invoke(&self, command: &str, payload: Option<Value>) -> Result<Value, String> {
        match command {
            "download_mod" | "get_registry" | "check_mod_updates" | "check_app_update" => {
                let runtime = get_async_runtime();
                runtime.block_on(self.invoke_async(command, payload))
            }
            _ => self.invoke_sync(command, payload),
        }
    }

    pub async fn invoke_async(
        &self,
        command: &str,
        payload: Option<Value>,
    ) -> Result<Value, String> {
        match command {
            "download_mod" => {
                let payload: DownloadModPayload = parse_payload(command, payload)?;
                let mod_type = payload.mod_type.parse()?;
                let filename =
                    mods::download_mod(&self.app_data_dir, &payload.url, mod_type).await?;
                Ok(json!(filename))
            }
            "get_registry" => Ok(serde_json::to_value(mods::fetch_registry().await?)
                .map_err(|e| format!("Failed to serialize registry: {e}"))?),
            "check_mod_updates" => {
                let payload: ModTypePayload = parse_payload(command, payload)?;
                let mod_type = payload.mod_type.parse()?;
                Ok(serde_json::to_value(
                    mods::check_mod_updates(&self.app_data_dir, mod_type).await?,
                )
                .map_err(|e| format!("Failed to serialize update info: {e}"))?)
            }
            "check_app_update" => Ok(serde_json::to_value(
                app_update::check_app_update(self.package_version).await?,
            )
            .map_err(|e| format!("Failed to serialize app update info: {e}"))?),
            _ => self.invoke_sync(command, payload),
        }
    }

    pub fn invoke_sync(&self, command: &str, payload: Option<Value>) -> Result<Value, String> {
        match command {
            "init" => Ok(json!({
                "platform": self.bridge.platform_name(),
                "shell": self.bridge.shell_name(),
                "shellVersion": self.package_version,
                "nativePlayer": self.bridge.native_player_status(),
                "streamingServerRunning": self.bridge.is_streaming_server_running(),
            })),
            "get_native_player_status" => Ok(self.bridge.native_player_status()),
            "get_streaming_server_status" => self.bridge.get_streaming_server_status(),
            "shell_bridge_ready" => {
                self.mark_bridge_ready()?;
                Ok(Value::Null)
            }
            "shell_transport_send" => {
                let message = payload
                    .as_ref()
                    .and_then(|value| value.get("message"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Missing shell_transport_send message".to_string())?;
                self.handle_shell_transport_message(message)?;
                Ok(Value::Null)
            }
            "open_external_url" => {
                let url = payload
                    .as_ref()
                    .and_then(|value| value.get("url"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Missing open_external_url url".to_string())?;
                self.bridge.open_external_url(url)?;
                Ok(Value::Null)
            }
            "start_streaming_server" => {
                self.bridge.start_streaming_server()?;
                if self.bridge.is_streaming_server_running() {
                    self.emit_host_event(HostEvent::ServerStarted, Value::Null)?;
                }
                Ok(Value::Null)
            }
            "stop_streaming_server" => {
                self.bridge.stop_streaming_server()?;
                self.emit_host_event(HostEvent::ServerStopped, Value::Null)?;
                Ok(Value::Null)
            }
            "restart_streaming_server" => {
                let was_running = self.bridge.is_streaming_server_running();
                self.bridge.restart_streaming_server()?;
                if was_running {
                    self.emit_host_event(HostEvent::ServerStopped, Value::Null)?;
                }
                if self.bridge.is_streaming_server_running() {
                    self.emit_host_event(HostEvent::ServerStarted, Value::Null)?;
                }
                Ok(Value::Null)
            }
            "get_plugins" => Ok(serde_json::to_value(mods::list_mods(
                &self.app_data_dir,
                mods::ModType::Plugin,
            )?)
            .map_err(|e| format!("Failed to serialize plugins: {e}"))?),
            "get_themes" => Ok(serde_json::to_value(mods::list_mods(
                &self.app_data_dir,
                mods::ModType::Theme,
            )?)
            .map_err(|e| format!("Failed to serialize themes: {e}"))?),
            "delete_mod" => {
                let payload: ModFilePayload = parse_payload(command, payload)?;
                let mod_type = payload.mod_type.parse()?;
                mods::delete_mod(&self.app_data_dir, &payload.filename, mod_type)?;
                Ok(Value::Null)
            }
            "get_mod_content" => {
                let payload: ModFilePayload = parse_payload(command, payload)?;
                let mod_type = payload.mod_type.parse()?;
                Ok(json!(mods::read_mod_content(
                    &self.app_data_dir,
                    &payload.filename,
                    mod_type
                )?))
            }
            "get_setting" => {
                let payload: SettingKeyPayload = parse_payload(command, payload)?;
                Ok(settings::get_setting(
                    &mods::mods_dir(&self.app_data_dir, mods::ModType::Plugin),
                    &payload.plugin_name,
                    &payload.key,
                )?)
            }
            "save_setting" => {
                let payload: SaveSettingPayload = parse_payload(command, payload)?;
                let value = serde_json::from_str::<Value>(&payload.value)
                    .unwrap_or(Value::String(payload.value));
                let plugins_dir = mods::mods_dir(&self.app_data_dir, mods::ModType::Plugin);
                std::fs::create_dir_all(&plugins_dir)
                    .map_err(|e| format!("Failed to create plugins dir: {e}"))?;
                let _guard = self
                    .settings
                    .settings_lock
                    .lock()
                    .map_err(|e| e.to_string())?;
                settings::save_setting(&plugins_dir, &payload.plugin_name, &payload.key, value)?;
                Ok(Value::Null)
            }
            "register_settings" => {
                let payload: RegisterSettingsPayload = parse_payload(command, payload)?;
                mods::validate_filename(&payload.plugin_name)?;
                let schema = serde_json::from_str::<Value>(&payload.schema)
                    .map_err(|e| format!("Failed to parse settings schema: {e}"))?;
                settings::register_settings(
                    &self.settings.registered_schemas,
                    payload.plugin_name,
                    schema,
                )?;
                Ok(Value::Null)
            }
            "get_registered_settings" => {
                settings::get_registered_settings(&self.settings.registered_schemas)
            }
            "toggle_pip" => {
                let enabled = self.bridge.toggle_picture_in_picture()?;
                match self.bridge.platform_name() {
                    "macos" => {
                        let args = serialize_picture_in_picture(enabled);
                        let values = args
                            .as_array()
                            .ok_or_else(|| "Invalid macOS native player event args".to_string())?;
                        let event_type = values
                            .first()
                            .and_then(Value::as_str)
                            .ok_or_else(|| "Missing macOS native player event type".to_string())?;
                        let payload = values.get(1).cloned().unwrap_or(Value::Null);
                        self.emit_event(
                            SHELL_TRANSPORT_EVENT,
                            json!({
                                "type": event_type,
                                "payload": payload,
                            }),
                        )?;
                    }
                    _ => {
                        self.emit_transport_message(response_message(
                            serialize_picture_in_picture(enabled),
                        ))?;
                    }
                }
                Ok(json!(enabled))
            }
            "get_pip_mode" => Ok(json!(self.bridge.is_pip_enabled()?)),
            "set_auto_pause" => {
                let enabled = parse_optional_bool(payload).unwrap_or(true);
                self.shell_preferences.lock().unwrap().auto_pause = enabled;
                Ok(Value::Null)
            }
            "get_auto_pause" => Ok(json!(self.shell_preferences.lock().unwrap().auto_pause)),
            "set_pip_disables_auto_pause" => {
                let enabled = parse_optional_bool(payload).unwrap_or(true);
                self.shell_preferences
                    .lock()
                    .unwrap()
                    .pip_disables_auto_pause = enabled;
                Ok(Value::Null)
            }
            "get_pip_disables_auto_pause" => Ok(json!(
                self.shell_preferences
                    .lock()
                    .unwrap()
                    .pip_disables_auto_pause
            )),
            "mpv-observe-prop" | "mpv-set-prop" | "mpv-command" | "native-player-stop" => {
                if command == "mpv-set-prop" {
                    self.update_player_paused_from_set_prop(&payload);
                }
                self.bridge.handle_custom_transport(command, payload)?;
                Ok(Value::Null)
            }
            "toggle_devtools" => Ok(Value::Null),
            "start_discord_rpc" => {
                self.discord_rpc.start()?;
                Ok(Value::Null)
            }
            "stop_discord_rpc" => {
                self.discord_rpc.stop()?;
                Ok(Value::Null)
            }
            "update_discord_activity" => {
                if payload.is_none() || payload == Some(Value::Null) {
                    return Ok(Value::Null);
                }
                #[derive(Deserialize)]
                struct WrappedActivity {
                    activity: crate::discord_rpc::ActivityPayload,
                }
                let parsed: WrappedActivity = parse_payload(command, payload)?;
                self.discord_rpc.update_activity(parsed.activity)?;
                Ok(Value::Null)
            }
            other => {
                let capitalized_platform = match self.bridge.platform_name() {
                    "windows" => "Windows",
                    "linux" => "Linux",
                    "macos" => "macOS",
                    other => other,
                };
                Err(format!(
                    "Unsupported {capitalized_platform} host command: {other}"
                ))
            }
        }
    }

    fn handle_shell_transport_message(&self, message: &str) -> Result<(), String> {
        match parse_request(message)? {
            ParsedRequest::Handshake => {
                self.emit_transport_message(handshake_response(self.package_version))
            }
            ParsedRequest::Command { method, data } => {
                if method == "app-ready" || method == "app-error" {
                    self.mark_transport_ready()?;
                } else {
                    self.bridge.handle_custom_transport(&method, data)?;
                }
                Ok(())
            }
        }
    }

    pub fn update_window_focus(&self, focused: bool) -> Result<(), String> {
        self.emit_event("window-focus-changed", json!(focused))?;

        let is_pip = self.bridge.is_pip_enabled().unwrap_or(false);

        // Determine the action while holding the lock, then release before calling bridge
        let pause_action = {
            let mut prefs = self.shell_preferences.lock().unwrap();
            if !prefs.auto_pause || (is_pip && prefs.pip_disables_auto_pause) {
                return Ok(());
            }
            if focused && prefs.auto_paused {
                prefs.auto_paused = false;
                Some(false)
            } else if !focused && !prefs.player_paused {
                prefs.auto_paused = true;
                Some(true)
            } else {
                None
            }
        };

        if let Some(paused) = pause_action {
            self.bridge
                .handle_custom_transport("mpv-set-prop", Some(json!(["pause", paused])))
                .ok();
        }

        Ok(())
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
pub struct InvokeIpcPayload {
    pub command: String,
    pub payload: Option<Value>,
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

pub fn get_async_runtime() -> &'static tokio::runtime::Runtime {
    static TOKIO_RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    TOKIO_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create async runtime")
    })
}

pub fn parse_payload<T>(command: &str, payload: Option<Value>) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(payload.unwrap_or(Value::Null))
        .map_err(|e| format!("Invalid {command} payload: {e}"))
}

pub fn parse_optional_bool(payload: Option<Value>) -> Option<bool> {
    let value = payload?;
    value
        .as_bool()
        .or_else(|| value.get("enabled").and_then(Value::as_bool))
        .or_else(|| value.get("value").and_then(Value::as_bool))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    #[test]
    fn host_command_names_match_frontend() {
        assert_eq!(
            serde_json::to_value(HostCommand::ToggleDevtools).unwrap(),
            json!("toggle_devtools")
        );
        assert_eq!(
            serde_json::to_value(HostCommand::SetPipDisablesAutoPause).unwrap(),
            json!("set_pip_disables_auto_pause")
        );
    }

    #[test]
    fn host_event_names_match_frontend() {
        assert_eq!(
            serde_json::to_value(HostEvent::WindowMaximizedChanged).unwrap(),
            json!("window-maximized-changed")
        );
        assert_eq!(
            serde_json::to_value(HostEvent::ShellTransportMessage).unwrap(),
            json!("shell-transport-message")
        );
    }

    #[test]
    fn parses_handshake_request() {
        assert_eq!(
            parse_request(r#"{"id":0,"type":3}"#).unwrap(),
            ParsedRequest::Handshake
        );
    }

    #[test]
    fn parses_command_request() {
        assert_eq!(
            parse_request(r#"{"id":7,"type":6,"args":["mpv-command",["stop"]]}"#).unwrap(),
            ParsedRequest::Command {
                method: "mpv-command".to_string(),
                data: Some(json!(["stop"])),
            }
        );
    }

    #[test]
    fn serializes_handshake_shape() {
        let payload: Value = serde_json::from_str(&handshake_response("0.0.0")).unwrap();
        assert_eq!(
            payload,
            json!({
                "id": 0,
                "object": "transport",
                "type": 3,
                "data": {
                    "transport": {
                        "properties": [[], ["", "shellVersion", "", "0.0.0"]],
                        "signals": [],
                        "methods": [["onEvent"]]
                    }
                }
            })
        );
    }

    #[test]
    fn serializes_event_shape() {
        let payload: Value =
            serde_json::from_str(&response_message(json!(["open-media", "stremio://foo"])))
                .unwrap();
        assert_eq!(
            payload,
            json!({
                "id": 1,
                "object": "transport",
                "type": 1,
                "args": ["open-media", "stremio://foo"]
            })
        );
    }

    #[test]
    fn serializes_window_visibility_event() {
        assert_eq!(
            serialize_window_visibility(true, true),
            json!(["win-visibility-changed", {
                "visible": true,
                "visibility": 1,
                "isFullscreen": true
            }])
        );
        assert_eq!(
            serialize_window_visibility(true, false),
            json!(["win-visibility-changed", {
                "visible": true,
                "visibility": 0,
                "isFullscreen": false
            }])
        );
    }
}
