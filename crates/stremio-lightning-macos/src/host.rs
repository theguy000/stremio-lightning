use crate::app_integration::{lifecycle_event_payload, AppLifecycleEvent, LaunchIntent};
use crate::player::{self, NativePlayerStatus, PlayerBackend};
use crate::streaming_server::{ProcessSpawner, StreamingServer};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use stremio_lightning_core::{
    host_api::{self, ParsedRequest},
    mods, settings,
};

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
    transport_ready: bool,
    pending_open_media: Vec<String>,
}

impl ListenerRegistry {
    fn listen(&mut self, event: impl Into<String>) -> u64 {
        self.next_id += 1;
        self.listeners.insert(self.next_id, event.into());
        self.next_id
    }

    fn listen_with_id(&mut self, id: u64, event: impl Into<String>) {
        self.next_id = self.next_id.max(id);
        self.listeners.insert(id, event.into());
        self.flush_queued_open_media();
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

    fn drain_emitted(&mut self) -> Vec<HostEventRecord> {
        std::mem::take(&mut self.emitted)
    }

    fn mark_bridge_ready(&mut self) {
        self.bridge_ready = true;
        self.flush_queued_open_media();
    }

    fn mark_transport_ready(&mut self) {
        self.transport_ready = true;
        self.flush_queued_open_media();
    }

    fn queue_open_media(&mut self, value: String) {
        self.pending_open_media.push(value);
        self.flush_queued_open_media();
    }

    fn flush_queued_open_media(&mut self) {
        if !(self.bridge_ready && self.transport_ready) {
            return;
        }
        if !self
            .listeners
            .values()
            .any(|listener| listener == SHELL_TRANSPORT_EVENT)
        {
            return;
        }

        let pending = std::mem::take(&mut self.pending_open_media);
        for value in pending {
            self.emitted.push(HostEventRecord {
                event: SHELL_TRANSPORT_EVENT.to_string(),
                payload: json!(host_api::response_message(json!(["open-media", value]))),
            });
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WindowRuntimeState {
    pub fullscreen: bool,
    pub maximized: bool,
    pub focused: bool,
    pub visible: bool,
    pub close_to_hide: bool,
}

pub struct MacosHost<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    player: B,
    streaming_server: StreamingServer<P>,
    app_data_dir: PathBuf,
    settings: settings::SettingsState,
    listeners: Mutex<ListenerRegistry>,
    window_state: Mutex<WindowRuntimeState>,
}

pub type Host<B, P> = MacosHost<B, P>;

impl<B, P> MacosHost<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    pub fn new(player: B, streaming_server: StreamingServer<P>) -> Self {
        Self::with_app_data_dir(player, streaming_server, default_app_data_dir())
    }

    pub fn with_app_data_dir(
        player: B,
        streaming_server: StreamingServer<P>,
        app_data_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            player,
            streaming_server,
            app_data_dir: app_data_dir.into(),
            settings: settings::SettingsState::default(),
            listeners: Mutex::default(),
            window_state: Mutex::new(WindowRuntimeState {
                visible: true,
                close_to_hide: true,
                ..WindowRuntimeState::default()
            }),
        }
    }

    pub fn start_streaming_server(&self) -> Result<(), String> {
        self.streaming_server.start()
    }

    pub fn stop_streaming_server(&self) -> Result<(), String> {
        self.streaming_server.stop()?;
        self.emit_server_stopped()
    }

    pub fn restart_streaming_server(&self) -> Result<(), String> {
        let was_running = self.streaming_server.is_running();
        self.streaming_server.restart()?;
        if was_running {
            self.emit_server_stopped()?;
        }
        if self.streaming_server.is_running() {
            self.emit_server_started()?;
        }
        Ok(())
    }

    pub fn shutdown(&self) -> Result<(), String> {
        self.emit_lifecycle_event(AppLifecycleEvent::Shutdown).ok();
        self.player.stop().ok();
        self.streaming_server.stop()
    }

    pub fn emit_launch_intent(&self, intent: LaunchIntent) -> Result<(), String> {
        self.focus_window()?;
        let Some(value) = intent.open_media_value() else {
            return Ok(());
        };
        self.listeners
            .lock()
            .map_err(|e| e.to_string())?
            .queue_open_media(value);
        Ok(())
    }

    pub fn native_player_status(&self) -> NativePlayerStatus {
        self.player.status()
    }

    pub fn invoke(&self, command: &str, payload: Option<Value>) -> Result<Value, String> {
        match command {
            "download_mod" | "get_registry" | "check_mod_updates" => {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|e| format!("Failed to create async runtime: {e}"))?;
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
                let payload: ModFilePayload = parse_payload(command, payload)?;
                let mod_type = payload.mod_type.parse()?;
                Ok(serde_json::to_value(
                    mods::check_mod_updates(&self.app_data_dir, &payload.filename, mod_type)
                        .await?,
                )
                .map_err(|e| format!("Failed to serialize update info: {e}"))?)
            }
            _ => self.invoke_sync(command, payload),
        }
    }

    fn invoke_sync(&self, command: &str, payload: Option<Value>) -> Result<Value, String> {
        match command {
            "init" => Ok(json!({
                "platform": "macos",
                "shellVersion": env!("CARGO_PKG_VERSION"),
                "nativePlayer": self.native_player_status(),
                "streamingServerRunning": self.streaming_server.is_running(),
                "streamingServer": self.streaming_server.status(),
            })),
            "start_streaming_server" => {
                self.start_streaming_server()?;
                self.emit_server_started()?;
                Ok(Value::Null)
            }
            "stop_streaming_server" => {
                self.stop_streaming_server()?;
                Ok(Value::Null)
            }
            "restart_streaming_server" => {
                self.restart_streaming_server()?;
                Ok(Value::Null)
            }
            "get_streaming_server_status" => Ok(serde_json::to_value(
                self.streaming_server.status(),
            )
            .map_err(|e| format!("Failed to serialize macOS streaming server status: {e}"))?),
            "open_external_url" => {
                let url = payload
                    .as_ref()
                    .and_then(|value| value.get("url"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Missing open_external_url url".to_string())?;
                validate_external_url(url)?;
                Ok(Value::Null)
            }
            "get_native_player_status" => Ok(serde_json::to_value(self.native_player_status())
                .map_err(|e| format!("Failed to serialize macOS player status: {e}"))?),
            "mpv-observe-prop" | "mpv-set-prop" | "mpv-command" | "native-player-stop" => {
                player::handle_transport(&self.player, command, payload)?;
                self.emit_drained_player_events()?;
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
            "get_plugins" => Ok(serde_json::to_value(mods::list_mods(
                &self.app_data_dir,
                mods::ModType::Plugin,
            )?)
            .map_err(|e| format!("Failed to serialize macOS plugins: {e}"))?),
            "get_themes" => Ok(serde_json::to_value(mods::list_mods(
                &self.app_data_dir,
                mods::ModType::Theme,
            )?)
            .map_err(|e| format!("Failed to serialize macOS themes: {e}"))?),
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
                    .unwrap_or_else(|_| Value::String(payload.value));
                let plugins_dir = mods::mods_dir(&self.app_data_dir, mods::ModType::Plugin);
                std::fs::create_dir_all(&plugins_dir)
                    .map_err(|e| format!("Failed to create macOS plugins dir: {e}"))?;
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
                    .map_err(|e| format!("Failed to parse macOS settings schema: {e}"))?;
                settings::register_settings(
                    &self.settings.registered_schemas,
                    payload.plugin_name,
                    schema,
                )?;
                Ok(Value::Null)
            }
            "get_registered_settings" => {
                let payload: PluginNamePayload = parse_payload(command, payload)?;
                mods::validate_filename(&payload.plugin_name)?;
                settings::get_registered_settings(
                    &self.settings.registered_schemas,
                    &payload.plugin_name,
                )
            }
            "shell_bridge_ready" => {
                self.mark_bridge_ready()?;
                Ok(Value::Null)
            }
            other => Err(format!("Unsupported macOS host command: {other}")),
        }
    }

    fn handle_shell_transport_message(&self, message: &str) -> Result<(), String> {
        match host_api::parse_request(message)? {
            ParsedRequest::Handshake => self
                .emit_transport_response(host_api::handshake_response(env!("CARGO_PKG_VERSION"))),
            ParsedRequest::Command { method, data } => match method.as_str() {
                "app-ready" | "app-error" => self.mark_transport_ready(),
                "mpv-observe-prop" | "mpv-set-prop" | "mpv-command" | "native-player-stop" => {
                    player::handle_transport(&self.player, &method, data)?;
                    self.emit_drained_player_events()
                }
                other => Err(format!("Unsupported macOS shell transport method: {other}")),
            },
        }
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
            "window.toggleMaximize" => {
                self.toggle_window_maximize()?;
                Ok(Value::Null)
            }
            "window.close" => {
                self.close_window()?;
                Ok(Value::Null)
            }
            "window.startDragging" => Ok(Value::Null),
            "window.isMaximized" => Ok(json!(self.window_state()?.maximized)),
            "window.isFullscreen" => Ok(json!(self.window_state()?.fullscreen)),
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
            other => Err(format!("Unsupported macOS IPC kind: {other}")),
        }
    }

    pub fn listen(&self, event: impl Into<String>) -> Result<u64, String> {
        Ok(self
            .listeners
            .lock()
            .map_err(|e| e.to_string())?
            .listen(event))
    }

    pub fn listen_with_id(&self, id: u64, event: impl Into<String>) -> Result<(), String> {
        self.listeners
            .lock()
            .map_err(|e| e.to_string())?
            .listen_with_id(id, event);
        Ok(())
    }

    pub fn unlisten(&self, id: u64) -> Result<(), String> {
        self.listeners
            .lock()
            .map_err(|e| e.to_string())?
            .unlisten(id);
        Ok(())
    }

    pub fn emit_window_fullscreen_changed(&self, fullscreen: bool) -> Result<(), String> {
        self.emit_event(
            "window-fullscreen-changed",
            json!({ "fullscreen": fullscreen }),
        )
    }

    pub fn emit_window_maximized_changed(&self, maximized: bool) -> Result<(), String> {
        self.emit_event(
            "window-maximized-changed",
            json!({ "maximized": maximized }),
        )
    }

    pub fn emit_lifecycle_event(&self, event: AppLifecycleEvent) -> Result<(), String> {
        let (name, payload) = lifecycle_event_payload(event);
        match event {
            AppLifecycleEvent::BecameActive => self.update_window_focus(true)?,
            AppLifecycleEvent::ResignedActive => self.update_window_focus(false)?,
            AppLifecycleEvent::WindowFocused(focused) => self.set_window_focus(focused)?,
            AppLifecycleEvent::WindowVisible(visible) => self.set_window_visible(visible)?,
            AppLifecycleEvent::Shutdown => {}
        }
        self.emit_event(name, payload)
    }

    pub fn emit_server_started(&self) -> Result<(), String> {
        self.emit_event(
            "server-started",
            json!({ "url": self.streaming_server.url() }),
        )
    }

    pub fn emit_server_stopped(&self) -> Result<(), String> {
        self.emit_event(
            "server-stopped",
            json!({ "url": self.streaming_server.url() }),
        )
    }

    pub fn emit_native_player_property_changed(
        &self,
        name: impl Into<String>,
        data: Value,
    ) -> Result<(), String> {
        self.emit_event(
            SHELL_TRANSPORT_EVENT,
            json!({
                "type": "mpv-prop-change",
                "name": name.into(),
                "data": data,
            }),
        )
    }

    pub fn emit_native_player_transport_args(&self, args: Value) -> Result<(), String> {
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
        )
    }

    pub fn window_state(&self) -> Result<WindowRuntimeState, String> {
        Ok(self.window_state.lock().map_err(|e| e.to_string())?.clone())
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

    fn emit_transport_response(&self, message: String) -> Result<(), String> {
        self.emit_event(SHELL_TRANSPORT_EVENT, json!(message))
    }

    fn minimize_window(&self) -> Result<(), String> {
        self.set_window_visible(false)
    }

    fn toggle_window_maximize(&self) -> Result<(), String> {
        let maximized = {
            let mut state = self.window_state.lock().map_err(|e| e.to_string())?;
            state.maximized = !state.maximized;
            state.visible = true;
            state.maximized
        };
        self.emit_window_maximized_changed(maximized)
    }

    fn close_window(&self) -> Result<(), String> {
        let close_to_hide = self.window_state()?.close_to_hide;
        if close_to_hide {
            self.set_window_visible(false)
        } else {
            self.emit_lifecycle_event(AppLifecycleEvent::Shutdown)
        }
    }

    fn focus_window(&self) -> Result<(), String> {
        self.set_window_focus(true)?;
        self.set_window_visible(true)
    }

    fn update_window_focus(&self, focused: bool) -> Result<(), String> {
        self.set_window_focus(focused)
    }

    fn set_window_focus(&self, focused: bool) -> Result<(), String> {
        self.window_state.lock().map_err(|e| e.to_string())?.focused = focused;
        Ok(())
    }

    fn set_window_visible(&self, visible: bool) -> Result<(), String> {
        self.window_state.lock().map_err(|e| e.to_string())?.visible = visible;
        Ok(())
    }

    fn set_window_fullscreen(&self, fullscreen: bool) -> Result<(), String> {
        let changed = {
            let mut state = self.window_state.lock().map_err(|e| e.to_string())?;
            let changed = state.fullscreen != fullscreen;
            state.fullscreen = fullscreen;
            state.visible = true;
            changed
        };
        if changed {
            self.emit_window_fullscreen_changed(fullscreen)?;
        }
        Ok(())
    }

    pub fn emit_drained_player_events(&self) -> Result<(), String> {
        for event in self.player.drain_events()? {
            self.emit_native_player_transport_args(event.transport_args())?;
        }
        Ok(())
    }

    pub fn drain_emitted_events(&self) -> Result<Vec<HostEventRecord>, String> {
        self.emit_drained_player_events()?;
        Ok(self
            .listeners
            .lock()
            .map_err(|e| e.to_string())?
            .drain_emitted())
    }

    fn emit_event(&self, event: impl Into<String>, payload: Value) -> Result<(), String> {
        self.listeners
            .lock()
            .map_err(|e| e.to_string())?
            .emit(event, payload);
        Ok(())
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

#[derive(Debug, Deserialize)]
struct DownloadModPayload {
    url: String,
    #[serde(rename = "type")]
    mod_type: String,
}

#[derive(Debug, Deserialize)]
struct ModFilePayload {
    filename: String,
    #[serde(rename = "type")]
    mod_type: String,
}

#[derive(Debug, Deserialize)]
struct SettingKeyPayload {
    #[serde(rename = "pluginName")]
    plugin_name: String,
    key: String,
}

#[derive(Debug, Deserialize)]
struct SaveSettingPayload {
    #[serde(rename = "pluginName")]
    plugin_name: String,
    key: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct RegisterSettingsPayload {
    #[serde(rename = "pluginName")]
    plugin_name: String,
    schema: String,
}

#[derive(Debug, Deserialize)]
struct PluginNamePayload {
    #[serde(rename = "pluginName")]
    plugin_name: String,
}

fn parse_payload<T>(label: &str, payload: Option<Value>) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(payload.unwrap_or(Value::Null))
        .map_err(|e| format!("Invalid macOS IPC payload for {label}: {e}"))
}

fn default_app_data_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(|home| Path::new(&home).join("Library").join("Application Support"))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn validate_external_url(url: &str) -> Result<(), String> {
    let lower = url.to_lowercase();
    if [
        "http://", "https://", "rtp://", "rtsp://", "ftp://", "ipfs://",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix))
    {
        Ok(())
    } else {
        Err("Rejected non-whitelisted open_external_url URL".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::FakePlayerBackend;
    use crate::streaming_server::{FakeProcessSpawner, StreamingServer};
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEMP_ID: AtomicUsize = AtomicUsize::new(0);

    fn test_host() -> Host<FakePlayerBackend, FakeProcessSpawner> {
        Host::new(
            FakePlayerBackend::initialized(),
            StreamingServer::new(FakeProcessSpawner::default()),
        )
    }

    fn temp_app_data_dir(name: &str) -> PathBuf {
        let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "stremio-lightning-macos-host-test-{}-{name}-{id}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        path
    }

    fn test_host_with_app_data_dir(
        app_data_dir: PathBuf,
    ) -> Host<FakePlayerBackend, FakeProcessSpawner> {
        Host::with_app_data_dir(
            FakePlayerBackend::initialized(),
            StreamingServer::new(FakeProcessSpawner::default()),
            app_data_dir,
        )
    }

    #[test]
    fn dispatch_ipc_routes_invoke_to_host() {
        let host = test_host();
        let value = host
            .dispatch_ipc("invoke", Some(json!({"command": "init"})))
            .unwrap();
        assert_eq!(value["platform"], "macos");
        assert_eq!(value["nativePlayer"]["backend"], "fake");
    }

    #[test]
    fn dispatch_ipc_validates_payload_shape() {
        let host = test_host();
        let error = host
            .dispatch_ipc("listen", Some(json!({"id": 1})))
            .unwrap_err();
        assert!(error.contains("Invalid macOS IPC payload for listen"));
    }

    #[test]
    fn unsupported_commands_return_errors() {
        let host = test_host();
        assert_eq!(
            host.dispatch_ipc("invoke", Some(json!({"command": "missing"})))
                .unwrap_err(),
            "Unsupported macOS host command: missing"
        );
        assert_eq!(
            host.dispatch_ipc("unknown.kind", None).unwrap_err(),
            "Unsupported macOS IPC kind: unknown.kind"
        );
    }

    #[test]
    fn streaming_server_commands_report_status() {
        let host = test_host();
        assert_eq!(
            host.invoke("get_streaming_server_status", None).unwrap()["running"],
            false
        );
        host.invoke("start_streaming_server", None).unwrap();
        assert_eq!(
            host.invoke("get_streaming_server_status", None).unwrap()["running"],
            true
        );
        host.invoke("restart_streaming_server", None).unwrap();
        assert_eq!(
            host.invoke("get_streaming_server_status", None).unwrap()["running"],
            true
        );
        host.invoke("stop_streaming_server", None).unwrap();
        assert_eq!(
            host.invoke("get_streaming_server_status", None).unwrap()["running"],
            false
        );
    }

    #[test]
    fn host_api_contract_supports_mod_listing_and_content() {
        let app_data_dir = temp_app_data_dir("mods");
        let plugins_dir = mods::mods_dir(&app_data_dir, mods::ModType::Plugin);
        fs::create_dir_all(&plugins_dir).unwrap();
        fs::write(
            plugins_dir.join("cinema.plugin.js"),
            "/**\n * @name Cinema\n * @description Test plugin\n * @author Tests\n * @version 1.0.0\n */\nwindow.__cinema = true;",
        )
        .unwrap();

        let host = test_host_with_app_data_dir(app_data_dir.clone());
        let plugins = host.invoke("get_plugins", None).unwrap();
        assert_eq!(plugins[0]["filename"], "cinema.plugin.js");
        assert_eq!(plugins[0]["metadata"]["name"], "Cinema");

        let content = host
            .invoke(
                "get_mod_content",
                Some(json!({"filename": "cinema.plugin.js", "type": "plugin"})),
            )
            .unwrap();
        assert!(content.as_str().unwrap().contains("window.__cinema"));

        host.invoke(
            "delete_mod",
            Some(json!({"filename": "cinema.plugin.js", "type": "plugin"})),
        )
        .unwrap();
        assert!(host
            .invoke("get_plugins", None)
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty());
        let _ = fs::remove_dir_all(app_data_dir);
    }

    #[test]
    fn host_api_contract_supports_plugin_settings() {
        let app_data_dir = temp_app_data_dir("settings");
        let host = test_host_with_app_data_dir(app_data_dir.clone());

        host.invoke(
            "save_setting",
            Some(json!({"pluginName": "cinema", "key": "enabled", "value": "true"})),
        )
        .unwrap();
        assert_eq!(
            host.invoke(
                "get_setting",
                Some(json!({"pluginName": "cinema", "key": "enabled"})),
            )
            .unwrap(),
            json!(true)
        );

        host.invoke(
            "register_settings",
            Some(json!({"pluginName": "cinema", "schema": "{\"type\":\"object\"}"})),
        )
        .unwrap();
        assert_eq!(
            host.invoke(
                "get_registered_settings",
                Some(json!({"pluginName": "cinema"})),
            )
            .unwrap()["type"],
            "object"
        );
        let _ = fs::remove_dir_all(app_data_dir);
    }

    #[test]
    fn open_external_url_rejects_untrusted_schemes() {
        let host = test_host();
        host.invoke(
            "open_external_url",
            Some(json!({"url": "https://example.com/"})),
        )
        .unwrap();
        assert_eq!(
            host.invoke(
                "open_external_url",
                Some(json!({"url": "javascript:alert(1)"})),
            )
            .unwrap_err(),
            "Rejected non-whitelisted open_external_url URL"
        );
    }

    #[test]
    fn host_routes_player_transport_commands() {
        let player = FakePlayerBackend::initialized();
        let host = Host::new(
            player.clone(),
            StreamingServer::new(FakeProcessSpawner::default()),
        );

        host.invoke("mpv-observe-prop", Some(json!("pause")))
            .unwrap();
        host.invoke("mpv-set-prop", Some(json!(["pause", true])))
            .unwrap();
        host.invoke(
            "mpv-command",
            Some(json!(["loadfile", "file:///tmp/sample.mp4", "replace"])),
        )
        .unwrap();
        host.invoke("native-player-stop", None).unwrap();

        assert_eq!(
            player.actions(),
            vec![
                crate::player::PlayerAction::ObserveProperty("pause".to_string()),
                crate::player::PlayerAction::SetProperty {
                    name: "pause".to_string(),
                    value: json!(true),
                },
                crate::player::PlayerAction::Command {
                    name: "loadfile".to_string(),
                    args: vec!["file:///tmp/sample.mp4".to_string(), "replace".to_string()],
                },
                crate::player::PlayerAction::Stop,
            ]
        );
    }

    #[test]
    fn host_drains_player_events_to_shell_transport() {
        let player = FakePlayerBackend::initialized();
        let host = Host::new(
            player.clone(),
            StreamingServer::new(FakeProcessSpawner::default()),
        );
        host.dispatch_ipc(
            "listen",
            Some(json!({"id": 3, "event": SHELL_TRANSPORT_EVENT})),
        )
        .unwrap();
        player
            .push_event(stremio_lightning_core::player_api::PlayerEvent::Ended(
                stremio_lightning_core::player_api::PlayerEnded {
                    reason: "eof".to_string(),
                    error: None,
                },
            ))
            .unwrap();

        let events = host.drain_emitted_events().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, SHELL_TRANSPORT_EVENT);
        assert_eq!(events[0].payload["type"], "mpv-event-ended");
        assert_eq!(events[0].payload["payload"]["reason"], "eof");
    }

    #[test]
    fn shell_transport_send_routes_player_commands() {
        let player = FakePlayerBackend::initialized();
        let host = Host::new(
            player.clone(),
            StreamingServer::new(FakeProcessSpawner::default()),
        );

        host.invoke(
            "shell_transport_send",
            Some(json!({ "message": r#"{"id":9,"type":6,"args":["mpv-command",["loadfile","file:///tmp/sample.mp4","replace"]]}"# })),
        )
        .unwrap();

        assert_eq!(
            player.actions(),
            vec![crate::player::PlayerAction::Command {
                name: "loadfile".to_string(),
                args: vec!["file:///tmp/sample.mp4".to_string(), "replace".to_string()],
            }]
        );
    }

    #[test]
    fn shell_transport_handshake_emits_response() {
        let host = test_host();
        host.dispatch_ipc(
            "listen",
            Some(json!({"id": 11, "event": SHELL_TRANSPORT_EVENT})),
        )
        .unwrap();

        host.invoke(
            "shell_transport_send",
            Some(json!({ "message": r#"{"id":0,"type":3}"# })),
        )
        .unwrap();

        let events = host.drain_emitted_events().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, SHELL_TRANSPORT_EVENT);
        let transport: Value = serde_json::from_str(events[0].payload.as_str().unwrap()).unwrap();
        assert_eq!(transport["type"], 3);
        assert_eq!(transport["object"], "transport");
    }

    #[test]
    fn launch_intents_queue_until_bridge_and_transport_ready() {
        let host = test_host();
        host.emit_launch_intent(LaunchIntent::Magnet("magnet:?xt=urn:btih:test".to_string()))
            .unwrap();
        assert!(host.window_state().unwrap().focused);
        assert!(host.drain_emitted_events().unwrap().is_empty());

        host.dispatch_ipc(
            "listen",
            Some(json!({"id": 12, "event": SHELL_TRANSPORT_EVENT})),
        )
        .unwrap();
        host.invoke("shell_bridge_ready", None).unwrap();
        assert!(host.drain_emitted_events().unwrap().is_empty());

        host.invoke(
            "shell_transport_send",
            Some(json!({ "message": r#"{"id":1,"type":6,"args":["app-ready"]}"# })),
        )
        .unwrap();

        let events = host.drain_emitted_events().unwrap();
        assert_eq!(events.len(), 1);
        let transport: Value = serde_json::from_str(events[0].payload.as_str().unwrap()).unwrap();
        assert_eq!(
            transport["args"],
            json!(["open-media", "magnet:?xt=urn:btih:test"])
        );
    }

    #[test]
    fn window_commands_update_mockable_state_and_emit_events() {
        let host = test_host();
        host.dispatch_ipc(
            "listen",
            Some(json!({"id": 13, "event": "window-fullscreen-changed"})),
        )
        .unwrap();
        host.dispatch_ipc(
            "listen",
            Some(json!({"id": 14, "event": "window-maximized-changed"})),
        )
        .unwrap();

        host.dispatch_ipc("window.toggleMaximize", None).unwrap();
        assert_eq!(
            host.dispatch_ipc("window.isMaximized", None).unwrap(),
            json!(true)
        );
        host.dispatch_ipc("window.setFullscreen", Some(json!({"fullscreen": true})))
            .unwrap();
        assert_eq!(
            host.dispatch_ipc("window.isFullscreen", None).unwrap(),
            json!(true)
        );
        host.dispatch_ipc("window.minimize", None).unwrap();
        assert!(!host.window_state().unwrap().visible);
        host.dispatch_ipc("window.close", None).unwrap();
        assert!(!host.window_state().unwrap().visible);

        let events = host.drain_emitted_events().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event, "window-maximized-changed");
        assert_eq!(events[0].payload, json!({"maximized": true}));
        assert_eq!(events[1].event, "window-fullscreen-changed");
        assert_eq!(events[1].payload, json!({"fullscreen": true}));
    }

    #[test]
    fn lifecycle_events_are_serialized_and_update_state() {
        let host = test_host();
        host.dispatch_ipc(
            "listen",
            Some(json!({"id": 15, "event": "app-became-active"})),
        )
        .unwrap();
        host.dispatch_ipc(
            "listen",
            Some(json!({"id": 16, "event": "window-visible-changed"})),
        )
        .unwrap();

        host.emit_lifecycle_event(AppLifecycleEvent::BecameActive)
            .unwrap();
        host.emit_lifecycle_event(AppLifecycleEvent::WindowVisible(false))
            .unwrap();

        let state = host.window_state().unwrap();
        assert!(state.focused);
        assert!(!state.visible);
        let events = host.drain_emitted_events().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event, "app-became-active");
        assert_eq!(events[0].payload, json!({"active": true}));
        assert_eq!(events[1].event, "window-visible-changed");
        assert_eq!(events[1].payload, json!({"visible": false}));
    }

    #[test]
    fn listeners_gate_drained_events() {
        let host = test_host();
        host.emit_native_player_property_changed("pause", json!(true))
            .unwrap();
        assert!(host.drain_emitted_events().unwrap().is_empty());

        host.dispatch_ipc(
            "listen",
            Some(json!({"id": 7, "event": SHELL_TRANSPORT_EVENT})),
        )
        .unwrap();
        host.emit_native_player_property_changed("pause", json!(false))
            .unwrap();
        assert_eq!(host.drain_emitted_events().unwrap().len(), 1);

        host.dispatch_ipc("unlisten", Some(json!({"id": 7})))
            .unwrap();
        host.emit_native_player_property_changed("pause", json!(true))
            .unwrap();
        assert!(host.drain_emitted_events().unwrap().is_empty());
    }
}
