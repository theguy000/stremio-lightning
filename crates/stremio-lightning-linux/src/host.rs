use crate::player::{
    handle_transport, serialize_ended, serialize_property_change, NativePlayerStatus, PlayerBackend,
};
use crate::streaming_server::{ProcessSpawner, StreamingServer};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use stremio_lightning_core::host_api::{self, HostEvent, ParsedRequest};
use stremio_lightning_core::pip::{serialize_picture_in_picture, PipRestoreSnapshot, PipState};
use stremio_lightning_core::{mods, settings};

pub const SHELL_TRANSPORT_EVENT: &str = "shell-transport-message";
const MAX_PENDING_MESSAGES: usize = 512;

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
}

#[derive(Debug)]
struct TransportQueue {
    bridge_ready: bool,
    transport_ready: bool,
    pending: VecDeque<String>,
}

impl Default for TransportQueue {
    fn default() -> Self {
        Self {
            bridge_ready: false,
            transport_ready: false,
            pending: VecDeque::new(),
        }
    }
}

pub struct LinuxHost<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    player: B,
    streaming_server: StreamingServer<P>,
    app_data_dir: PathBuf,
    settings: settings::SettingsState,
    listeners: Mutex<ListenerRegistry>,
    transport: Mutex<TransportQueue>,
    pip_state: PipState,
}

impl<B, P> LinuxHost<B, P>
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
            transport: Mutex::default(),
            pip_state: PipState::new(),
        }
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

    pub fn start_streaming_server(&self) -> Result<(), String> {
        self.streaming_server.start()?;
        self.emit_server_started()?;
        Ok(())
    }

    pub fn dispatch_linux_ipc(&self, kind: &str, payload: Option<Value>) -> Result<Value, String> {
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
            "window.minimize"
            | "window.toggleMaximize"
            | "window.close"
            | "window.startDragging" => Ok(Value::Null),
            "window.isMaximized" | "window.isFullscreen" => Ok(json!(false)),
            "window.setFullscreen" => {
                let payload: FullscreenIpcPayload = parse_payload(kind, payload)?;
                self.emit_window_fullscreen_changed(payload.fullscreen)?;
                Ok(Value::Null)
            }
            "webview.setZoom" => {
                let payload: ZoomIpcPayload = parse_payload(kind, payload)?;
                if !payload.level.is_finite() || payload.level <= 0.0 {
                    return Err("Invalid webview zoom level".to_string());
                }
                Ok(Value::Null)
            }
            other => Err(format!("Unsupported Linux IPC kind: {other}")),
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
                let mod_type = mods::parse_mod_type(&payload.mod_type)?;
                let filename =
                    mods::download_mod(&self.app_data_dir, &payload.url, mod_type).await?;
                Ok(json!(filename))
            }
            "get_registry" => Ok(serde_json::to_value(mods::fetch_registry().await?)
                .map_err(|e| format!("Failed to serialize registry: {e}"))?),
            "check_mod_updates" => {
                let payload: ModFilePayload = parse_payload(command, payload)?;
                let mod_type = mods::parse_mod_type(&payload.mod_type)?;
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
                "platform": "linux",
                "shellVersion": env!("CARGO_PKG_VERSION"),
                "nativePlayer": self.native_player_status(),
                "streamingServerRunning": self.streaming_server.is_running(),
            })),
            "open_external_url" => {
                let url = payload
                    .as_ref()
                    .and_then(|value| value.get("url"))
                    .and_then(Value::as_str)
                    .ok_or_else(|| "Missing open_external_url url".to_string())?;
                validate_external_url(url)?;
                Ok(Value::Null)
            }
            "start_streaming_server" => {
                self.start_streaming_server()?;
                Ok(Value::Null)
            }
            "stop_streaming_server" => {
                self.streaming_server.stop()?;
                self.emit_server_stopped()?;
                Ok(Value::Null)
            }
            "restart_streaming_server" => {
                let was_running = self.streaming_server.is_running();
                self.streaming_server.restart()?;
                if was_running {
                    self.emit_server_stopped()?;
                }
                self.emit_server_started()?;
                Ok(Value::Null)
            }
            "get_streaming_server_status" => Ok(json!(self.streaming_server.is_running())),
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
            "get_native_player_status" => Ok(serde_json::to_value(self.native_player_status())
                .map_err(|e| format!("Failed to serialize player status: {e}"))?),
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
                let mod_type = mods::parse_mod_type(&payload.mod_type)?;
                mods::delete_mod(&self.app_data_dir, &payload.filename, mod_type)?;
                Ok(Value::Null)
            }
            "get_mod_content" => {
                let payload: ModFilePayload = parse_payload(command, payload)?;
                let mod_type = mods::parse_mod_type(&payload.mod_type)?;
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
                let payload: PluginNamePayload = parse_payload(command, payload)?;
                mods::validate_filename(&payload.plugin_name)?;
                settings::get_registered_settings(
                    &self.settings.registered_schemas,
                    &payload.plugin_name,
                )
            }
            "toggle_pip" => {
                let enabled = !self.pip_state.is_enabled()?;
                self.set_picture_in_picture(enabled, None)?;
                Ok(json!(enabled))
            }
            "get_pip_mode" => Ok(json!(self.pip_state.is_enabled()?)),
            other => Err(format!("Unsupported Linux host command: {other}")),
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

    pub fn emitted_events(&self) -> Result<Vec<HostEventRecord>, String> {
        Ok(self
            .listeners
            .lock()
            .map_err(|e| e.to_string())?
            .emitted
            .clone())
    }

    pub fn drain_emitted_events(&self) -> Result<Vec<HostEventRecord>, String> {
        Ok(self
            .listeners
            .lock()
            .map_err(|e| e.to_string())?
            .drain_emitted())
    }

    pub fn native_player_status(&self) -> NativePlayerStatus {
        self.player.status()
    }

    pub fn emit_transport_event(&self, args: Value) -> Result<(), String> {
        self.emit_or_queue_transport_message(host_api::response_message(args))
    }

    pub fn emit_native_player_property_changed(
        &self,
        name: impl Into<String>,
        data: Value,
    ) -> Result<(), String> {
        self.emit_transport_event(serialize_property_change(name, data))
    }

    pub fn emit_native_player_ended(&self, reason: impl Into<String>) -> Result<(), String> {
        self.emit_transport_event(serialize_ended(reason))
    }

    pub fn pip_snapshot(&self) -> Result<Option<PipRestoreSnapshot>, String> {
        self.pip_state.snapshot()
    }

    pub fn set_picture_in_picture(
        &self,
        enabled: bool,
        snapshot: Option<PipRestoreSnapshot>,
    ) -> Result<(), String> {
        self.pip_state.set_mode(enabled, snapshot)?;
        self.emit_transport_event(serialize_picture_in_picture(enabled))
    }

    pub fn emit_window_maximized_changed(&self, maximized: bool) -> Result<(), String> {
        self.emit_host_event(HostEvent::WindowMaximizedChanged, json!(maximized))
    }

    pub fn emit_window_fullscreen_changed(&self, fullscreen: bool) -> Result<(), String> {
        self.emit_host_event(HostEvent::WindowFullscreenChanged, json!(fullscreen))
    }

    fn emit_server_started(&self) -> Result<(), String> {
        self.emit_host_event(HostEvent::ServerStarted, Value::Null)
    }

    fn emit_server_stopped(&self) -> Result<(), String> {
        self.emit_host_event(HostEvent::ServerStopped, Value::Null)
    }

    fn handle_shell_transport_message(&self, message: &str) -> Result<(), String> {
        match host_api::parse_request(message)? {
            ParsedRequest::Handshake => self.emit_transport_message_now(
                host_api::handshake_response(env!("CARGO_PKG_VERSION")),
            ),
            ParsedRequest::Command { method, data } => match method.as_str() {
                "app-ready" => {
                    self.mark_transport_ready()?;
                    self.flush_pending_transport_messages()
                }
                "app-error" => {
                    self.mark_transport_ready()?;
                    Ok(())
                }
                "mpv-observe-prop" | "mpv-set-prop" | "mpv-command" | "native-player-stop" => {
                    handle_transport(&self.player, &method, data)
                }
                other => Err(format!("Unsupported shell transport method: {other}")),
            },
        }
    }

    fn mark_bridge_ready(&self) -> Result<(), String> {
        self.transport
            .lock()
            .map_err(|e| e.to_string())?
            .bridge_ready = true;
        Ok(())
    }

    fn mark_transport_ready(&self) -> Result<(), String> {
        self.transport
            .lock()
            .map_err(|e| e.to_string())?
            .transport_ready = true;
        Ok(())
    }

    fn emit_or_queue_transport_message(&self, message: String) -> Result<(), String> {
        let mut transport = self.transport.lock().map_err(|e| e.to_string())?;
        if transport.bridge_ready && transport.transport_ready {
            drop(transport);
            self.emit_transport_message_now(message)
        } else {
            if transport.pending.len() >= MAX_PENDING_MESSAGES {
                transport.pending.pop_front();
            }
            transport.pending.push_back(message);
            Ok(())
        }
    }

    fn flush_pending_transport_messages(&self) -> Result<(), String> {
        let mut transport = self.transport.lock().map_err(|e| e.to_string())?;
        if !transport.bridge_ready || !transport.transport_ready {
            return Ok(());
        }
        let messages: Vec<String> = transport.pending.drain(..).collect();
        drop(transport);

        for message in messages {
            self.emit_transport_message_now(message)?;
        }
        Ok(())
    }

    fn emit_transport_message_now(&self, message: String) -> Result<(), String> {
        self.emit_event(SHELL_TRANSPORT_EVENT, json!(message))
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DownloadModPayload {
    url: String,
    mod_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModFilePayload {
    filename: String,
    mod_type: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginNamePayload {
    plugin_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingKeyPayload {
    plugin_name: String,
    key: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveSettingPayload {
    plugin_name: String,
    key: String,
    value: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RegisterSettingsPayload {
    plugin_name: String,
    schema: String,
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

fn default_app_data_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(path);
    }

    if let Some(home) = std::env::var_os("HOME") {
        return Path::new(&home).join(".local").join("share");
    }

    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn validate_external_url(url: &str) -> Result<(), String> {
    let lower = url.to_lowercase();
    let allowed = [
        "http://", "https://", "rtp://", "rtsp://", "ftp://", "ipfs://",
    ]
    .iter()
    .any(|prefix| lower.starts_with(prefix));

    if allowed {
        Ok(())
    } else {
        Err("Rejected non-whitelisted open_external_url URL".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{FakePlayerBackend, PlayerAction};
    use crate::streaming_server::FakeProcessSpawner;
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEMP_ID: AtomicUsize = AtomicUsize::new(0);

    fn host() -> (
        LinuxHost<FakePlayerBackend, FakeProcessSpawner>,
        FakePlayerBackend,
        FakeProcessSpawner,
    ) {
        host_with_app_data(temp_dir("default"))
    }

    fn host_with_app_data(
        app_data_dir: PathBuf,
    ) -> (
        LinuxHost<FakePlayerBackend, FakeProcessSpawner>,
        FakePlayerBackend,
        FakeProcessSpawner,
    ) {
        let player = FakePlayerBackend::initialized();
        let spawner = FakeProcessSpawner::default();
        let server = StreamingServer::with_project_root(spawner.clone(), PathBuf::from("/repo"));
        (
            LinuxHost::with_app_data_dir(player.clone(), server, app_data_dir),
            player,
            spawner,
        )
    }

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "stremio-lightning-linux-host-test-{}-{}-{}",
            std::process::id(),
            name,
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    #[test]
    fn dispatches_phase_three_host_commands() {
        let (host, _player, spawner) = host();
        host.listen("server-started").unwrap();
        host.listen("server-stopped").unwrap();

        assert_eq!(
            host.invoke("get_streaming_server_status", None).unwrap(),
            json!(false)
        );
        host.invoke("start_streaming_server", None).unwrap();
        assert_eq!(
            host.invoke("get_streaming_server_status", None).unwrap(),
            json!(true)
        );
        host.invoke("stop_streaming_server", None).unwrap();
        assert_eq!(
            host.invoke("get_streaming_server_status", None).unwrap(),
            json!(false)
        );
        assert_eq!(spawner.calls().len(), 1);

        let init = host.invoke("init", None).unwrap();
        assert_eq!(init["platform"], "linux");
        assert_eq!(init["nativePlayer"]["enabled"], true);
        assert_eq!(init["streamingServerRunning"], false);

        let status = host.invoke("get_native_player_status", None).unwrap();
        assert_eq!(status["enabled"], true);
        assert_eq!(status["initialized"], true);

        host.invoke(
            "open_external_url",
            Some(json!({"url": "https://web.stremio.com/"})),
        )
        .unwrap();
    }

    #[test]
    fn lists_reads_and_deletes_plugin_and_theme_mods() {
        let root = temp_dir("mods-contract");
        let (host, _player, _spawner) = host_with_app_data(root.clone());

        assert_eq!(host.invoke("get_plugins", None).unwrap(), json!([]));
        assert_eq!(host.invoke("get_themes", None).unwrap(), json!([]));

        mods::write_mod_content(
            &root,
            "sample.plugin.js",
            mods::ModType::Plugin,
            br#"/**
 * @name Sample Plugin
 * @description Demo plugin
 * @author Tester
 * @version 1.0.0
 */
console.log("sample");"#,
        )
        .unwrap();
        mods::write_mod_content(
            &root,
            "sample.theme.css",
            mods::ModType::Theme,
            br#"/**
 * @name Sample Theme
 * @description Demo theme
 * @author Tester
 * @version 1.0.0
 */
:root { --sl-test-color: red; }"#,
        )
        .unwrap();
        host.invoke(
            "save_setting",
            Some(json!({"pluginName": "sample", "key": "enabled", "value": "true"})),
        )
        .unwrap();

        let plugins = host.invoke("get_plugins", None).unwrap();
        assert_eq!(plugins[0]["filename"], "sample.plugin.js");
        assert_eq!(plugins[0]["mod_type"], "plugin");
        assert_eq!(plugins[0]["metadata"]["name"], "Sample Plugin");

        let themes = host.invoke("get_themes", None).unwrap();
        assert_eq!(themes[0]["filename"], "sample.theme.css");
        assert_eq!(themes[0]["mod_type"], "theme");

        let content = host
            .invoke(
                "get_mod_content",
                Some(json!({"filename": "sample.plugin.js", "modType": "plugin"})),
            )
            .unwrap();
        assert!(content.as_str().unwrap().contains("console.log"));

        host.invoke(
            "delete_mod",
            Some(json!({"filename": "sample.plugin.js", "modType": "plugin"})),
        )
        .unwrap();
        assert_eq!(host.invoke("get_plugins", None).unwrap(), json!([]));
        assert!(!mods::mods_dir(&root, mods::ModType::Plugin)
            .join("sample.plugin.json")
            .exists());
    }

    #[test]
    fn rejects_invalid_mod_payloads() {
        let (host, _player, _spawner) = host();
        let traversal = host
            .invoke(
                "get_mod_content",
                Some(json!({"filename": "../evil.plugin.js", "modType": "plugin"})),
            )
            .unwrap_err();
        assert!(traversal.contains("Invalid filename"));

        let invalid_type = host
            .invoke(
                "delete_mod",
                Some(json!({"filename": "sample.plugin.js", "modType": "script"})),
            )
            .unwrap_err();
        assert!(invalid_type.contains("Unknown mod type"));

        let download_error = block_on(host.invoke_async(
            "download_mod",
            Some(json!({"url": "https://example.test/evil.theme.css", "modType": "plugin"})),
        ))
        .unwrap_err();
        assert!(download_error.contains("Invalid plugin filename extension"));
    }

    #[test]
    fn plugin_settings_round_trip_and_validate() {
        let (host, _player, _spawner) = host();

        host.invoke(
            "register_settings",
            Some(json!({
                "pluginName": "sample",
                "schema": r#"[{"key":"enabled","type":"toggle"}]"#
            })),
        )
        .unwrap();
        assert_eq!(
            host.invoke(
                "get_registered_settings",
                Some(json!({"pluginName": "sample"}))
            )
            .unwrap(),
            json!([{"key": "enabled", "type": "toggle"}])
        );

        host.invoke(
            "save_setting",
            Some(json!({"pluginName": "sample", "key": "enabled", "value": "true"})),
        )
        .unwrap();
        assert_eq!(
            host.invoke(
                "get_setting",
                Some(json!({"pluginName": "sample", "key": "enabled"}))
            )
            .unwrap(),
            json!(true)
        );

        host.invoke(
            "save_setting",
            Some(json!({"pluginName": "sample", "key": "mode", "value": "plain text"})),
        )
        .unwrap();
        assert_eq!(
            host.invoke(
                "get_setting",
                Some(json!({"pluginName": "sample", "key": "mode"}))
            )
            .unwrap(),
            json!("plain text")
        );

        let invalid_schema = host
            .invoke(
                "register_settings",
                Some(json!({"pluginName": "sample", "schema": "{"})),
            )
            .unwrap_err();
        assert!(invalid_schema.contains("Failed to parse settings schema"));

        let invalid_plugin = host
            .invoke(
                "get_registered_settings",
                Some(json!({"pluginName": "../sample"})),
            )
            .unwrap_err();
        assert!(invalid_plugin.contains("Invalid filename"));
    }

    #[test]
    fn rejects_unsupported_command() {
        let (host, _player, _spawner) = host();
        let error = host.invoke("unknown_command", None).unwrap_err();
        assert!(error.contains("Unsupported Linux host command"));
    }

    #[test]
    fn queues_shell_transport_events_until_bridge_and_app_ready() {
        let (host, _player, _spawner) = host();
        host.listen(SHELL_TRANSPORT_EVENT).unwrap();

        host.emit_native_player_property_changed("pause", json!(true))
            .unwrap();
        assert!(host.emitted_events().unwrap().is_empty());

        host.invoke("shell_bridge_ready", None).unwrap();
        assert!(host.emitted_events().unwrap().is_empty());

        host.invoke(
            "shell_transport_send",
            Some(json!({"message": r#"{"id":1,"type":6,"args":["app-ready"]}"#})),
        )
        .unwrap();

        let events = host.emitted_events().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, SHELL_TRANSPORT_EVENT);
        assert!(events[0]
            .payload
            .as_str()
            .unwrap()
            .contains("mpv-prop-change"));
    }

    #[test]
    fn dispatches_linux_js_ipc_roundtrip() {
        let (host, _player, _spawner) = host();

        assert_eq!(
            host.dispatch_linux_ipc(
                "invoke",
                Some(json!({"command": "get_streaming_server_status"})),
            )
            .unwrap(),
            json!(false)
        );

        host.dispatch_linux_ipc(
            "listen",
            Some(json!({"id": 77, "event": SHELL_TRANSPORT_EVENT})),
        )
        .unwrap();
        host.dispatch_linux_ipc("invoke", Some(json!({"command": "shell_bridge_ready"})))
            .unwrap();
        host.emit_native_player_property_changed("pause", json!(true))
            .unwrap();
        host.dispatch_linux_ipc(
            "invoke",
            Some(json!({
                "command": "shell_transport_send",
                "payload": {"message": r#"{"id":1,"type":6,"args":["app-ready"]}"#}
            })),
        )
        .unwrap();

        assert_eq!(host.emitted_events().unwrap().len(), 1);
        host.dispatch_linux_ipc("unlisten", Some(json!({"id": 77})))
            .unwrap();
        let _ = host.drain_emitted_events().unwrap();
        host.emit_native_player_property_changed("pause", json!(false))
            .unwrap();
        assert!(host.emitted_events().unwrap().is_empty());
    }

    #[test]
    fn emits_window_and_native_player_lifecycle_events() {
        let (host, _player, _spawner) = host();
        host.listen("window-maximized-changed").unwrap();
        host.listen("window-fullscreen-changed").unwrap();
        host.listen(SHELL_TRANSPORT_EVENT).unwrap();

        host.emit_window_maximized_changed(true).unwrap();
        host.emit_window_fullscreen_changed(false).unwrap();
        host.invoke("shell_bridge_ready", None).unwrap();
        host.invoke(
            "shell_transport_send",
            Some(json!({"message": r#"{"id":1,"type":6,"args":["app-ready"]}"#})),
        )
        .unwrap();
        host.emit_native_player_ended("eof").unwrap();

        let events = host.emitted_events().unwrap();
        assert_eq!(events[0].event, "window-maximized-changed");
        assert_eq!(events[0].payload, json!(true));
        assert_eq!(events[1].event, "window-fullscreen-changed");
        assert_eq!(events[1].payload, json!(false));
        assert!(events[2]
            .payload
            .as_str()
            .unwrap()
            .contains("mpv-event-ended"));
    }

    #[test]
    fn shell_transport_maps_player_commands() {
        let (host, player, _spawner) = host();
        host.invoke(
            "shell_transport_send",
            Some(json!({"message": r#"{"id":7,"type":6,"args":["mpv-command",["loadfile","file:///tmp/a.mp4","replace"]]}"#})),
        )
        .unwrap();

        assert_eq!(
            player.actions(),
            vec![PlayerAction::Command {
                name: "loadfile".to_string(),
                args: vec!["file:///tmp/a.mp4".to_string(), "replace".to_string()],
            }]
        );
    }

    #[test]
    fn toggle_pip_emits_picture_in_picture_events() {
        let (host, _player, _spawner) = host();
        host.listen(SHELL_TRANSPORT_EVENT).unwrap();
        host.invoke("shell_bridge_ready", None).unwrap();
        host.invoke(
            "shell_transport_send",
            Some(json!({"message": r#"{"id":1,"type":6,"args":["app-ready"]}"#})),
        )
        .unwrap();

        host.invoke("toggle_pip", None).unwrap();
        assert_eq!(host.invoke("get_pip_mode", None).unwrap(), json!(true));
        host.invoke("toggle_pip", None).unwrap();
        assert_eq!(host.invoke("get_pip_mode", None).unwrap(), json!(false));

        let events = host.emitted_events().unwrap();
        assert!(events[0]
            .payload
            .as_str()
            .unwrap()
            .contains("showPictureInPicture"));
        assert!(events[1]
            .payload
            .as_str()
            .unwrap()
            .contains("hidePictureInPicture"));
    }

    #[test]
    fn shell_transport_reports_unsupported_methods() {
        let (host, _player, _spawner) = host();
        let error = host
            .invoke(
                "shell_transport_send",
                Some(json!({"message": r#"{"id":7,"type":6,"args":["unknown-method"]}"#})),
            )
            .unwrap_err();
        assert!(error.contains("Unsupported shell transport method"));
    }
}
