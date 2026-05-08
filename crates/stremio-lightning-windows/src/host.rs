use crate::player::WindowsPlayer;
use crate::resources::WindowsResourceLayout;
use crate::server::{RealProcessSpawner, WindowsStreamingServer};
use crate::single_instance::LaunchIntent;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Mutex;
use stremio_lightning_core::host_api::{self, HostEvent, ParsedRequest};
use stremio_lightning_core::pip::{serialize_picture_in_picture, PipState};
use stremio_lightning_core::player_api::PlayerEvent;

#[cfg(windows)]
use crate::window::NativeWindowController;

pub const SHELL_TRANSPORT_EVENT: &str = "shell-transport-message";

#[derive(Debug, Clone, PartialEq)]
pub struct HostEventRecord {
    pub event: String,
    pub payload: Value,
}

#[derive(Debug, Default)]
struct ListenerRegistry {
    next_id: u64,
    listeners: HashMap<u64, String>,
    emitted: Vec<HostEventRecord>,
    bridge_ready: bool,
    shell_transport_ready: bool,
    pending_shell_transport_messages: Vec<String>,
}

impl ListenerRegistry {
    fn listen_with_id(&mut self, id: u64, event: impl Into<String>) {
        self.next_id = self.next_id.max(id);
        let event = event.into();
        let is_shell_transport = event == SHELL_TRANSPORT_EVENT;
        self.listeners.insert(id, event);
        if is_shell_transport {
            self.flush_pending_open_media();
        }
    }

    fn unlisten(&mut self, id: u64) {
        self.listeners.remove(&id);
    }

    fn emit(&mut self, event: impl Into<String>, payload: Value) {
        let event = event.into();
        if self.listeners.values().any(|listener| listener == &event) {
            self.emitted.push(HostEventRecord { event, payload });
        }
    }

    fn mark_bridge_ready(&mut self) {
        self.bridge_ready = true;
        self.flush_pending_transport_messages();
    }

    fn mark_transport_ready(&mut self) {
        self.shell_transport_ready = true;
        self.flush_pending_transport_messages();
    }

    fn queue_open_media(&mut self, value: String) {
        self.queue_transport_message(host_api::response_message(json!(["open-media", value])));
    }

    fn queue_media_key(&mut self, action: &str) {
        self.queue_transport_message(host_api::response_message(json!(["media-key", action])));
    }

    fn queue_transport_message(&mut self, message: String) {
        self.pending_shell_transport_messages.push(message);
        self.flush_pending_transport_messages();
    }

    fn flush_pending_open_media(&mut self) {
        self.flush_pending_transport_messages();
    }

    fn flush_pending_transport_messages(&mut self) {
        if !self.bridge_ready
            || !self.shell_transport_ready
            || !self
                .listeners
                .values()
                .any(|listener| listener == SHELL_TRANSPORT_EVENT)
        {
            return;
        }

        for message in std::mem::take(&mut self.pending_shell_transport_messages) {
            self.emitted.push(HostEventRecord {
                event: SHELL_TRANSPORT_EVENT.to_string(),
                payload: json!(message),
            });
        }
    }

    fn drain_emitted(&mut self) -> Vec<HostEventRecord> {
        std::mem::take(&mut self.emitted)
    }
}

#[derive(Debug, Deserialize)]
pub struct WindowsIpcRequest {
    pub id: u64,
    pub kind: String,
    pub payload: Option<Value>,
}

pub type IpcRequest = WindowsIpcRequest;

#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "kind")]
pub enum WindowsIpcOutbound {
    #[serde(rename = "response")]
    Response { id: u64, ok: bool, value: Value },
    #[serde(rename = "event")]
    Event { event: String, payload: Value },
}

pub struct WindowsHost {
    player: Mutex<WindowsPlayer>,
    streaming_server: WindowsStreamingServer<RealProcessSpawner>,
    listeners: Mutex<ListenerRegistry>,
    window_state: Mutex<WindowRuntimeState>,
    pip_state: PipState,
    #[cfg(windows)]
    window_controller: Mutex<Option<NativeWindowController>>,
    package_version: &'static str,
}

pub type Host = WindowsHost;

#[derive(Debug, Default)]
struct WindowRuntimeState {
    fullscreen: bool,
    maximized: bool,
    focused: bool,
    visible: bool,
}

impl Default for WindowsHost {
    fn default() -> Self {
        Self::new(env!("CARGO_PKG_VERSION"))
    }
}

impl WindowsHost {
    pub fn new(package_version: &'static str) -> Self {
        Self::with_streaming_server_disabled(package_version, false)
    }

    pub fn with_streaming_server_disabled(package_version: &'static str, disabled: bool) -> Self {
        Self {
            player: Mutex::default(),
            streaming_server: WindowsStreamingServer::from_resources(
                &WindowsResourceLayout::from_runtime(),
                disabled,
            ),
            listeners: Mutex::default(),
            window_state: Mutex::default(),
            pip_state: PipState::new(),
            #[cfg(windows)]
            window_controller: Mutex::default(),
            package_version,
        }
    }

    pub fn start_streaming_server(&self) -> Result<(), String> {
        self.streaming_server.start()?;
        if !self.streaming_server.disabled() {
            self.emit_server_started()?;
        }
        Ok(())
    }

    pub fn emit_launch_intent(&self, intent: LaunchIntent) -> Result<(), String> {
        let Some(value) = intent.open_media_value() else {
            return Ok(());
        };
        self.listeners
            .lock()
            .map_err(|e| e.to_string())?
            .queue_open_media(value);
        Ok(())
    }

    #[cfg(windows)]
    pub fn bind_native_window(&self, hwnd: windows::Win32::Foundation::HWND) -> Result<(), String> {
        *self
            .window_controller
            .lock()
            .map_err(|_| "Windows window controller lock poisoned".to_string())? =
            Some(NativeWindowController::new(hwnd));
        Ok(())
    }

    pub fn dispatch_ipc_message(&self, raw: &str) -> Vec<WindowsIpcOutbound> {
        let response = serde_json::from_str::<WindowsIpcRequest>(raw)
            .map_err(|error| format!("Invalid Windows WebView2 IPC message: {error}"))
            .and_then(|request| {
                let id = request.id;
                self.dispatch_ipc(&request.kind, request.payload)
                    .map(|value| (id, true, value))
                    .or_else(|error| Ok((id, false, json!({ "message": error }))))
            });

        let mut outbound = match response {
            Ok((id, ok, value)) => vec![WindowsIpcOutbound::Response { id, ok, value }],
            Err(error) => vec![WindowsIpcOutbound::Event {
                event: "windows-ipc-error".to_string(),
                payload: json!({ "message": error }),
            }],
        };

        outbound.extend(
            self.drain_all_emitted_events()
                .unwrap_or_default()
                .into_iter()
                .map(WindowsIpcOutbound::from),
        );
        outbound
    }

    #[cfg(windows)]
    pub fn initialize_native_player(
        &self,
        hwnd: windows::Win32::Foundation::HWND,
        notifier: crate::window::UiThreadNotifier,
    ) -> Result<(), String> {
        self.player
            .lock()
            .map_err(|_| "Windows player lock poisoned".to_string())?
            .initialize(hwnd, notifier)
    }

    pub fn drain_ipc_events(&self) -> Vec<WindowsIpcOutbound> {
        self.drain_all_emitted_events()
            .unwrap_or_default()
            .into_iter()
            .map(WindowsIpcOutbound::from)
            .collect()
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
                self.minimize_window()?;
                Ok(Value::Null)
            }
            "window.focus" => {
                self.focus_window()?;
                Ok(Value::Null)
            }
            "window.toggleMaximize" => {
                let maximized = self.toggle_window_maximize()?;
                self.set_window_maximized(maximized)?;
                Ok(Value::Null)
            }
            "window.close" => {
                self.close_window()?;
                Ok(Value::Null)
            }
            "window.startDragging" => {
                self.start_window_dragging()?;
                Ok(Value::Null)
            }
            "window.isMaximized" => Ok(json!(self.is_window_maximized()?)),
            "window.isFullscreen" => Ok(json!(self.is_window_fullscreen()?)),
            "window.setFullscreen" => {
                let payload: FullscreenIpcPayload = parse_payload(kind, payload)?;
                self.set_window_fullscreen(payload.fullscreen)?;
                Ok(Value::Null)
            }
            "webview.setZoom" => {
                let payload: ZoomIpcPayload = parse_payload(kind, payload)?;
                if !payload.level.is_finite() || payload.level <= 0.0 {
                    return Err("Invalid webview zoom level".to_string());
                }
                Ok(Value::Null)
            }
            other => Err(format!("Unsupported Windows IPC kind: {other}")),
        }
    }

    pub fn dispatch_windows_ipc(
        &self,
        kind: &str,
        payload: Option<Value>,
    ) -> Result<Value, String> {
        self.dispatch_ipc(kind, payload)
    }

    pub fn invoke(&self, command: &str, payload: Option<Value>) -> Result<Value, String> {
        match command {
            "init" => Ok(json!({
                "platform": "windows",
                "shell": "webview2",
                "shellVersion": env!("CARGO_PKG_VERSION"),
                "nativePlayer": self.player
                    .lock()
                    .map_err(|_| "Windows player lock poisoned".to_string())?
                    .status(),
                "streamingServerRunning": self.streaming_server.is_running(),
            })),
            "get_native_player_status" => Ok(serde_json::to_value(
                self.player
                    .lock()
                    .map_err(|_| "Windows player lock poisoned".to_string())?
                    .status(),
            )
            .map_err(|error| error.to_string())?),
            "shell_transport_send" => {
                let message = payload
                    .as_ref()
                    .and_then(|value| value.get("message"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Missing shell_transport_send message".to_string())?;
                self.handle_shell_transport_message(message)?;
                Ok(Value::Null)
            }
            "shell_bridge_ready" => {
                self.mark_bridge_ready()?;
                Ok(Value::Null)
            }
            "open_external_url" => {
                let url = payload
                    .as_ref()
                    .and_then(|value| value.get("url"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Missing open_external_url url".to_string())?;
                validate_external_url(url)?;
                open_external_url(url)?;
                Ok(Value::Null)
            }
            "get_streaming_server_status" => Ok(json!(self.streaming_server.is_running())),
            "start_streaming_server" => {
                self.start_streaming_server()?;
                Ok(Value::Null)
            }
            "stop_streaming_server" => {
                self.streaming_server.stop()?;
                if !self.streaming_server.disabled() {
                    self.emit_server_stopped()?;
                }
                Ok(Value::Null)
            }
            "restart_streaming_server" => {
                let was_running = self.streaming_server.is_running();
                self.streaming_server.restart()?;
                if !self.streaming_server.disabled() && was_running {
                    self.emit_server_stopped()?;
                }
                if !self.streaming_server.disabled() {
                    self.emit_server_started()?;
                }
                Ok(Value::Null)
            }
            "get_plugins" | "get_themes" => Ok(json!([])),
            "get_registry" => Ok(json!({ "plugins": [], "themes": [] })),
            "get_registered_settings" => Ok(Value::Null),
            "get_setting" => Ok(Value::Null),
            "save_setting" | "register_settings" => Ok(Value::Null),
            "download_mod" | "delete_mod" | "get_mod_content" | "check_mod_updates" => Err(
                format!("Windows host command is not implemented before mods storage milestone: {command}"),
            ),
            "toggle_devtools" | "start_discord_rpc" | "stop_discord_rpc" | "update_discord_activity" => {
                Ok(Value::Null)
            }
            "check_app_update" => Ok(Value::Null),
            "set_auto_pause" | "set_pip_disables_auto_pause" => Ok(Value::Null),
            "get_auto_pause" | "get_pip_disables_auto_pause" => Ok(json!(false)),
            "toggle_pip" => {
                let enabled = self.toggle_picture_in_picture_window()?;
                self.emit_picture_in_picture(enabled)?;
                Ok(json!(enabled))
            }
            "get_pip_mode" => Ok(json!(self.pip_state.is_enabled()?)),
            other => Err(format!("Unsupported Windows host command: {other}")),
        }
    }

    fn handle_shell_transport_message(&self, message: &str) -> Result<(), String> {
        match host_api::parse_request(message)? {
            ParsedRequest::Handshake => {
                self.emit_transport_message(host_api::handshake_response(self.package_version))
            }
            ParsedRequest::Command { method, data } => match method.as_str() {
                "app-ready" | "app-error" => self.mark_transport_ready(),
                "mpv-observe-prop" | "mpv-set-prop" | "mpv-command" | "native-player-stop" => {
                    self.player
                        .lock()
                        .map_err(|_| "Windows player lock poisoned".to_string())?
                        .handle_transport(&method, data)?;
                    Ok(())
                }
                other => Err(format!("Unsupported shell transport method: {other}")),
            },
        }
    }

    fn listen_with_id(&self, id: u64, event: impl Into<String>) -> Result<(), String> {
        self.listeners
            .lock()
            .map_err(|e| e.to_string())?
            .listen_with_id(id, event);
        Ok(())
    }

    fn unlisten(&self, id: u64) -> Result<(), String> {
        self.listeners
            .lock()
            .map_err(|e| e.to_string())?
            .unlisten(id);
        Ok(())
    }

    fn mark_bridge_ready(&self) -> Result<(), String> {
        self.listeners
            .lock()
            .map_err(|e| e.to_string())?
            .mark_bridge_ready();
        Ok(())
    }

    fn mark_transport_ready(&self) -> Result<(), String> {
        self.listeners
            .lock()
            .map_err(|e| e.to_string())?
            .mark_transport_ready();
        Ok(())
    }

    fn drain_emitted_events(&self) -> Result<Vec<HostEventRecord>, String> {
        Ok(self
            .listeners
            .lock()
            .map_err(|e| e.to_string())?
            .drain_emitted())
    }

    fn drain_all_emitted_events(&self) -> Result<Vec<HostEventRecord>, String> {
        self.emit_player_events()?;
        self.drain_emitted_events()
    }

    fn emit_player_events(&self) -> Result<(), String> {
        let events = self
            .player
            .lock()
            .map_err(|_| "Windows player lock poisoned".to_string())?
            .drain_events();

        for event in events {
            if matches!(event, PlayerEvent::Ended(_)) {
                if self.exit_picture_in_picture_window_for_player_end()? {
                    self.emit_picture_in_picture(false)?;
                }
            }
            self.emit_transport_message(host_api::response_message(event.transport_args()))?;
        }
        Ok(())
    }

    fn emit_transport_message(&self, message: String) -> Result<(), String> {
        self.emit_event(SHELL_TRANSPORT_EVENT, json!(message))
    }

    fn emit_picture_in_picture(&self, enabled: bool) -> Result<(), String> {
        self.emit_transport_message(host_api::response_message(serialize_picture_in_picture(
            enabled,
        )))
    }

    #[cfg(windows)]
    fn toggle_picture_in_picture_window(&self) -> Result<bool, String> {
        let mut controller = self
            .window_controller
            .lock()
            .map_err(|_| "Windows window controller lock poisoned".to_string())?;
        let controller = controller
            .as_mut()
            .ok_or_else(|| "Windows window controller is not initialized".to_string())?;
        self.pip_state.toggle_window_pip(controller)
    }

    #[cfg(not(windows))]
    fn toggle_picture_in_picture_window(&self) -> Result<bool, String> {
        let enabled = !self.pip_state.is_enabled()?;
        self.pip_state.set_mode(enabled, None)?;
        Ok(enabled)
    }

    #[cfg(windows)]
    fn exit_picture_in_picture_window(&self) -> Result<bool, String> {
        self.exit_picture_in_picture_window_with(PipState::exit_window_pip)
    }

    #[cfg(windows)]
    fn exit_picture_in_picture_window_for_player_end(&self) -> Result<bool, String> {
        self.exit_picture_in_picture_window_with(PipState::exit_window_pip_for_player_end)
    }

    #[cfg(windows)]
    fn exit_picture_in_picture_window_with(
        &self,
        exit: impl FnOnce(&PipState, &mut NativeWindowController) -> Result<bool, String>,
    ) -> Result<bool, String> {
        let mut controller = self
            .window_controller
            .lock()
            .map_err(|_| "Windows window controller lock poisoned".to_string())?;
        if let Some(controller) = controller.as_mut() {
            return exit(&self.pip_state, controller);
        }
        Ok(false)
    }

    #[cfg(not(windows))]
    fn exit_picture_in_picture_window(&self) -> Result<bool, String> {
        let changed = self.pip_state.is_enabled()?;
        self.pip_state.set_mode(false, None)?;
        Ok(changed)
    }

    #[cfg(not(windows))]
    fn exit_picture_in_picture_window_for_player_end(&self) -> Result<bool, String> {
        self.exit_picture_in_picture_window()
    }

    pub fn emit_media_key(&self, action: &str) -> Result<(), String> {
        self.listeners
            .lock()
            .map_err(|e| e.to_string())?
            .queue_media_key(action);
        Ok(())
    }

    pub fn update_window_maximized(&self, maximized: bool) -> Result<(), String> {
        self.set_window_maximized(maximized)
    }

    pub fn update_window_focus(&self, focused: bool) -> Result<(), String> {
        let changed = {
            let mut state = self
                .window_state
                .lock()
                .map_err(|_| "Windows window state lock poisoned".to_string())?;
            let changed = state.focused != focused;
            state.focused = focused;
            changed
        };
        if changed {
            self.emit_event("window-focus-changed", json!(focused))?;
        }
        Ok(())
    }

    pub fn update_window_visible(&self, visible: bool) -> Result<(), String> {
        let changed = {
            let mut state = self
                .window_state
                .lock()
                .map_err(|_| "Windows window state lock poisoned".to_string())?;
            let changed = state.visible != visible;
            state.visible = visible;
            changed
        };
        if changed {
            self.emit_event("window-visible-changed", json!(visible))?;
        }
        Ok(())
    }

    fn minimize_window(&self) -> Result<(), String> {
        #[cfg(windows)]
        if let Some(controller) = self
            .window_controller
            .lock()
            .map_err(|_| "Windows window controller lock poisoned".to_string())?
            .as_ref()
        {
            controller.minimize();
        }
        self.update_window_visible(false)
    }

    fn focus_window(&self) -> Result<(), String> {
        #[cfg(windows)]
        if let Some(controller) = self
            .window_controller
            .lock()
            .map_err(|_| "Windows window controller lock poisoned".to_string())?
            .as_ref()
        {
            controller.focus();
        }
        self.update_window_focus(true)
    }

    fn toggle_window_maximize(&self) -> Result<bool, String> {
        #[cfg(windows)]
        if let Some(controller) = self
            .window_controller
            .lock()
            .map_err(|_| "Windows window controller lock poisoned".to_string())?
            .as_ref()
        {
            return Ok(controller.toggle_maximize());
        }

        let state = self
            .window_state
            .lock()
            .map_err(|_| "Windows window state lock poisoned".to_string())?;
        Ok(!state.maximized)
    }

    fn close_window(&self) -> Result<(), String> {
        self.exit_picture_in_picture_window()?;

        #[cfg(windows)]
        if let Some(controller) = self
            .window_controller
            .lock()
            .map_err(|_| "Windows window controller lock poisoned".to_string())?
            .as_ref()
        {
            controller.close();
        }
        Ok(())
    }

    fn start_window_dragging(&self) -> Result<(), String> {
        #[cfg(windows)]
        if let Some(controller) = self
            .window_controller
            .lock()
            .map_err(|_| "Windows window controller lock poisoned".to_string())?
            .as_ref()
        {
            controller.start_dragging();
        }
        Ok(())
    }

    fn is_window_maximized(&self) -> Result<bool, String> {
        #[cfg(windows)]
        if let Some(controller) = self
            .window_controller
            .lock()
            .map_err(|_| "Windows window controller lock poisoned".to_string())?
            .as_ref()
        {
            return Ok(controller.is_maximized());
        }

        Ok(self
            .window_state
            .lock()
            .map_err(|_| "Windows window state lock poisoned".to_string())?
            .maximized)
    }

    fn is_window_fullscreen(&self) -> Result<bool, String> {
        #[cfg(windows)]
        if let Some(controller) = self
            .window_controller
            .lock()
            .map_err(|_| "Windows window controller lock poisoned".to_string())?
            .as_ref()
        {
            return Ok(controller.is_fullscreen());
        }

        Ok(self
            .window_state
            .lock()
            .map_err(|_| "Windows window state lock poisoned".to_string())?
            .fullscreen)
    }

    fn set_window_maximized(&self, maximized: bool) -> Result<(), String> {
        let changed = {
            let mut state = self
                .window_state
                .lock()
                .map_err(|_| "Windows window state lock poisoned".to_string())?;
            let changed = state.maximized != maximized;
            state.maximized = maximized;
            state.visible = true;
            changed
        };
        if changed {
            self.emit_window_maximized_changed(maximized)?;
        }
        Ok(())
    }

    fn set_window_fullscreen(&self, fullscreen: bool) -> Result<(), String> {
        let changed = {
            #[cfg(windows)]
            {
                if let Some(controller) = self
                    .window_controller
                    .lock()
                    .map_err(|_| "Windows window controller lock poisoned".to_string())?
                    .as_mut()
                {
                    controller.set_fullscreen(fullscreen)?
                } else {
                    false
                }
            }

            #[cfg(not(windows))]
            {
                false
            }
        };

        let state_changed = {
            let mut state = self
                .window_state
                .lock()
                .map_err(|_| "Windows window state lock poisoned".to_string())?;
            let state_changed = state.fullscreen != fullscreen;
            state.fullscreen = fullscreen;
            state.visible = true;
            state_changed
        };

        if changed || state_changed {
            self.emit_window_fullscreen_changed(fullscreen)?;
        }
        Ok(())
    }

    fn emit_window_maximized_changed(&self, maximized: bool) -> Result<(), String> {
        self.emit_host_event(HostEvent::WindowMaximizedChanged, json!(maximized))
    }

    fn emit_window_fullscreen_changed(&self, fullscreen: bool) -> Result<(), String> {
        self.emit_host_event(HostEvent::WindowFullscreenChanged, json!(fullscreen))
    }

    fn emit_server_started(&self) -> Result<(), String> {
        self.emit_host_event(HostEvent::ServerStarted, Value::Null)
    }

    fn emit_server_stopped(&self) -> Result<(), String> {
        self.emit_host_event(HostEvent::ServerStopped, Value::Null)
    }

    fn emit_host_event(&self, event: HostEvent, payload: Value) -> Result<(), String> {
        let event = serde_json::to_value(event)
            .map_err(|e| format!("Failed to serialize host event: {e}"))?
            .as_str()
            .ok_or_else(|| "Host event did not serialize to a string".to_string())?
            .to_string();
        self.emit_event(event, payload)
    }

    fn emit_event(&self, event: impl Into<String>, payload: Value) -> Result<(), String> {
        self.listeners
            .lock()
            .map_err(|e| e.to_string())?
            .emit(event, payload);
        Ok(())
    }
}

impl From<HostEventRecord> for WindowsIpcOutbound {
    fn from(record: HostEventRecord) -> Self {
        Self::Event {
            event: record.event,
            payload: record.payload,
        }
    }
}

#[derive(Debug, Deserialize)]
struct InvokeIpcPayload {
    command: String,
    payload: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ListenIpcPayload {
    id: u64,
    event: String,
}

#[derive(Debug, Deserialize)]
struct UnlistenIpcPayload {
    id: u64,
}

#[derive(Debug, Deserialize)]
struct FullscreenIpcPayload {
    fullscreen: bool,
}

#[derive(Debug, Deserialize)]
struct ZoomIpcPayload {
    level: f64,
}

fn parse_payload<T>(command: &str, payload: Option<Value>) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(payload.unwrap_or(Value::Null))
        .map_err(|e| format!("Invalid {command} payload: {e}"))
}

fn validate_external_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.contains(|c: char| c.is_control()) {
        return Err("Rejected non-whitelisted open_external_url URL".to_string());
    }

    let lower = trimmed.to_ascii_lowercase();
    let allowed = ["http://", "https://", "mailto:"]
        .iter()
        .any(|prefix| lower.starts_with(prefix));

    if allowed {
        Ok(())
    } else {
        Err("Rejected non-whitelisted open_external_url URL".to_string())
    }
}

#[cfg(windows)]
fn open_external_url(url: &str) -> Result<(), String> {
    use webview2_com::CoTaskMemPWSTR;
    use windows::core::w;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let url = CoTaskMemPWSTR::from(url.trim());
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            *url.as_ref().as_pcwstr(),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };

    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err("Failed to open external URL".to_string())
    }
}

#[cfg(not(windows))]
fn open_external_url(_url: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn expected_init_contract() -> Value {
        json!({
            "platform": "windows",
            "shell": "webview2",
            "shellVersion": env!("CARGO_PKG_VERSION"),
            "nativePlayer": { "enabled": cfg!(windows), "initialized": false, "backend": "webview2-libmpv" },
            "streamingServerRunning": false,
        })
    }

    #[test]
    fn exposes_webview2_init_contract() {
        assert_eq!(
            WindowsHost::default().invoke("init", None).unwrap(),
            expected_init_contract()
        );
    }

    #[test]
    fn handles_shell_transport_handshake() {
        let host = WindowsHost::new("0.1.0");
        host.dispatch_windows_ipc(
            "listen",
            Some(json!({ "id": 7, "event": "shell-transport-message" })),
        )
        .unwrap();
        host.invoke(
            "shell_transport_send",
            Some(json!({ "message": r#"{"id":0,"type":3}"# })),
        )
        .unwrap();
        let response = host.drain_emitted_events().unwrap().remove(0).payload;
        assert_eq!(
            serde_json::from_str::<Value>(response.as_str().unwrap()).unwrap()["type"],
            json!(3)
        );
    }

    #[test]
    fn dispatches_request_response_ipc() {
        let host = WindowsHost::default();
        let outbound = host.dispatch_ipc_message(
            r#"{"id":42,"kind":"invoke","payload":{"command":"init","payload":null}}"#,
        );

        assert_eq!(
            outbound[0],
            WindowsIpcOutbound::Response {
                id: 42,
                ok: true,
                value: expected_init_contract(),
            }
        );
    }

    #[test]
    fn returns_structured_error_for_invalid_command() {
        let host = WindowsHost::default();
        let outbound = host.dispatch_ipc_message(
            r#"{"id":9,"kind":"invoke","payload":{"command":"missing","payload":null}}"#,
        );

        assert_eq!(
            outbound[0],
            WindowsIpcOutbound::Response {
                id: 9,
                ok: false,
                value: json!({ "message": "Unsupported Windows host command: missing" }),
            }
        );
    }

    #[test]
    fn listener_registration_controls_events() {
        let host = WindowsHost::default();
        host.dispatch_windows_ipc(
            "listen",
            Some(json!({ "id": 2, "event": "window-fullscreen-changed" })),
        )
        .unwrap();

        let outbound = host.dispatch_ipc_message(
            r#"{"id":3,"kind":"window.setFullscreen","payload":{"fullscreen":true}}"#,
        );
        assert_eq!(outbound.len(), 2);
        assert_eq!(
            outbound[1],
            WindowsIpcOutbound::Event {
                event: "window-fullscreen-changed".to_string(),
                payload: json!(true),
            }
        );
    }

    #[test]
    fn queues_open_media_until_shell_transport_is_ready() {
        let host = WindowsHost::default();
        host.emit_launch_intent(LaunchIntent::Magnet("magnet:?xt=urn:btih:test".to_string()))
            .unwrap();

        assert!(host.drain_ipc_events().is_empty());

        host.dispatch_windows_ipc(
            "listen",
            Some(json!({ "id": 8, "event": "shell-transport-message" })),
        )
        .unwrap();
        assert!(host.drain_ipc_events().is_empty());

        host.dispatch_windows_ipc("invoke", Some(json!({"command": "shell_bridge_ready"})))
            .unwrap();
        assert!(host.drain_ipc_events().is_empty());

        host.invoke(
            "shell_transport_send",
            Some(json!({ "message": r#"{"id":1,"type":6,"args":["app-ready"]}"# })),
        )
        .unwrap();

        let events = host.drain_ipc_events();
        assert_eq!(events.len(), 1);
        let WindowsIpcOutbound::Event { event, payload } = &events[0] else {
            panic!("expected shell transport event");
        };
        assert_eq!(event, "shell-transport-message");
        let transport: Value = serde_json::from_str(payload.as_str().unwrap()).unwrap();
        assert_eq!(
            transport["args"],
            json!(["open-media", "magnet:?xt=urn:btih:test"])
        );
    }

    #[test]
    fn queues_media_keys_through_shell_transport() {
        let host = WindowsHost::default();
        host.dispatch_windows_ipc(
            "listen",
            Some(json!({ "id": 8, "event": "shell-transport-message" })),
        )
        .unwrap();
        host.dispatch_windows_ipc("invoke", Some(json!({"command": "shell_bridge_ready"})))
            .unwrap();
        host.invoke(
            "shell_transport_send",
            Some(json!({ "message": r#"{"id":1,"type":6,"args":["app-ready"]}"# })),
        )
        .unwrap();

        host.emit_media_key("play-pause").unwrap();

        let events = host.drain_ipc_events();
        let WindowsIpcOutbound::Event { event, payload } = &events[0] else {
            panic!("expected shell transport event");
        };
        assert_eq!(event, "shell-transport-message");
        let transport: Value = serde_json::from_str(payload.as_str().unwrap()).unwrap();
        assert_eq!(transport["args"], json!(["media-key", "play-pause"]));
    }

    #[test]
    fn handles_pip_toggle_state() {
        let host = WindowsHost::default();
        host.dispatch_windows_ipc(
            "listen",
            Some(json!({ "id": 8, "event": "shell-transport-message" })),
        )
        .unwrap();

        assert_eq!(host.invoke("get_pip_mode", None).unwrap(), json!(false));
        host.invoke("toggle_pip", None).unwrap();
        assert_eq!(host.invoke("get_pip_mode", None).unwrap(), json!(true));
        host.invoke("toggle_pip", None).unwrap();
        assert_eq!(host.invoke("get_pip_mode", None).unwrap(), json!(false));

        let events = host.drain_ipc_events();
        assert_eq!(events.len(), 2);
        let WindowsIpcOutbound::Event { payload, .. } = &events[0] else {
            panic!("expected shell transport event");
        };
        assert!(payload.as_str().unwrap().contains("showPictureInPicture"));
        let WindowsIpcOutbound::Event { payload, .. } = &events[1] else {
            panic!("expected shell transport event");
        };
        assert!(payload.as_str().unwrap().contains("hidePictureInPicture"));
    }

    #[test]
    fn exits_pip_when_native_player_ends() {
        let host = WindowsHost::default();
        host.dispatch_windows_ipc(
            "listen",
            Some(json!({ "id": 8, "event": "shell-transport-message" })),
        )
        .unwrap();

        host.invoke("toggle_pip", None).unwrap();
        assert_eq!(host.invoke("get_pip_mode", None).unwrap(), json!(true));
        host.drain_ipc_events();

        host.player.lock().unwrap().emit_ended("eof");

        let events = host.drain_ipc_events();
        assert_eq!(host.invoke("get_pip_mode", None).unwrap(), json!(false));
        assert_eq!(events.len(), 2);
        let WindowsIpcOutbound::Event { payload, .. } = &events[0] else {
            panic!("expected shell transport event");
        };
        assert!(payload.as_str().unwrap().contains("hidePictureInPicture"));
        let WindowsIpcOutbound::Event { payload, .. } = &events[1] else {
            panic!("expected shell transport event");
        };
        assert!(payload.as_str().unwrap().contains("ended"));
    }

    #[test]
    fn tracks_window_maximized_state() {
        let host = WindowsHost::default();
        host.dispatch_windows_ipc(
            "listen",
            Some(json!({ "id": 2, "event": "window-maximized-changed" })),
        )
        .unwrap();

        assert_eq!(
            host.dispatch_windows_ipc("window.isMaximized", None)
                .unwrap(),
            json!(false)
        );
        let outbound =
            host.dispatch_ipc_message(r#"{"id":3,"kind":"window.toggleMaximize","payload":null}"#);
        assert_eq!(
            outbound[0],
            WindowsIpcOutbound::Response {
                id: 3,
                ok: true,
                value: Value::Null
            }
        );
        assert_eq!(
            outbound[1],
            WindowsIpcOutbound::Event {
                event: "window-maximized-changed".to_string(),
                payload: json!(true)
            }
        );
        assert_eq!(
            host.dispatch_windows_ipc("window.isMaximized", None)
                .unwrap(),
            json!(true)
        );
    }

    #[test]
    fn external_url_policy_rejects_unsafe_schemes() {
        assert!(validate_external_url("https://web.stremio.com/").is_ok());
        assert!(validate_external_url("http://127.0.0.1:11470/").is_ok());
        assert!(validate_external_url("mailto:support@example.com").is_ok());
        assert!(validate_external_url("file:///C:/Windows/notepad.exe").is_err());
        assert!(validate_external_url("javascript:alert(1)").is_err());
        assert!(validate_external_url("ms-settings:privacy").is_err());
        assert!(validate_external_url("https://example.com/\ncalc").is_err());
    }

    #[test]
    fn matches_json_host_contract_fixture() {
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/host_contract.json")).unwrap();
        let host = WindowsHost::default();

        let init = host.dispatch_ipc_message(&fixture["invokeInitRequest"].to_string());
        assert_eq!(
            serde_json::to_value(&init[0]).unwrap(),
            fixture["invokeInitResponse"]
        );

        let invalid = host.dispatch_ipc_message(&fixture["invalidCommandRequest"].to_string());
        assert_eq!(
            serde_json::to_value(&invalid[0]).unwrap(),
            fixture["invalidCommandResponse"]
        );
    }
}
