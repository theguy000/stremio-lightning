use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use serde::Deserialize;
use serde_json::{json, Value};

use super::handlers::{
    async_runtime, handshake_response, is_async_command, parse_optional_bool, parse_payload,
    parse_request, response_message, safe_native_player_status, serialize_window_visibility,
};
use super::types::{
    DownloadModPayload, FocusChangedPayload, FullscreenIpcPayload, GetLogsPayload, HostApiError,
    HostEvent, HostEventRecord, InvokeIpcPayload, ListenIpcPayload, ListenerRegistry,
    ModFilePayload, ModTypePayload, ParsedRequest, PlatformBridge, RegisterSettingsPayload,
    RpcResponse, SaveSettingPayload, SetExtendedDiagnosticsPayload, SettingKeyPayload,
    ShellPreferenceState, SubmitDiagnosticLogsPayload, UnlistenIpcPayload, ZoomIpcPayload,
    SHELL_TRANSPORT_EVENT,
};
use crate::pip::serialize_picture_in_picture;
use crate::{app_update, logging, mods, settings};

pub struct BaseHost<P: PlatformBridge> {
    pub bridge: P,
    pub listeners: Mutex<ListenerRegistry>,
    pub settings: settings::SettingsState,
    pub app_data_dir: PathBuf,
    pub package_version: &'static str,
    pub shell_preferences: Mutex<ShellPreferenceState>,
    pub discord_rpc: Arc<crate::discord_rpc::DiscordRpcState>,
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

    pub fn lock_listeners(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, ListenerRegistry>, HostApiError> {
        self.listeners
            .lock()
            .map_err(|e| HostApiError::LockPoisoned(format!("Listeners lock poisoned: {e}")))
    }

    fn lock_shell_preferences(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, ShellPreferenceState>, HostApiError> {
        self.shell_preferences.lock().map_err(|e| {
            HostApiError::LockPoisoned(format!("Shell preferences lock poisoned: {e}"))
        })
    }

    pub fn listen_with_id(&self, id: u64, event: impl Into<String>) -> Result<(), HostApiError> {
        let mut registry = self.lock_listeners()?;
        registry.listen_with_id(id, event);
        self.flush_pending_transport_messages(&mut registry);
        Ok(())
    }

    pub fn unlisten(&self, id: u64) -> Result<(), HostApiError> {
        self.lock_listeners()?.unlisten(id);
        Ok(())
    }

    pub fn mark_bridge_ready(&self) -> Result<(), HostApiError> {
        let mut registry = self.lock_listeners()?;
        registry.bridge_ready = true;
        self.flush_pending_transport_messages(&mut registry);
        Ok(())
    }

    pub fn mark_transport_ready(&self) -> Result<(), HostApiError> {
        let mut registry = self.lock_listeners()?;
        registry.transport_ready = true;
        self.flush_pending_transport_messages(&mut registry);
        Ok(())
    }

    pub fn queue_transport_message(&self, message: String) -> Result<(), HostApiError> {
        self.update_player_paused_from_transport(&Value::String(message.clone()))?;
        let mut registry = self.lock_listeners()?;
        if registry.pending_transport_messages.len() >= 512 {
            registry.pending_transport_messages.pop_front();
        }
        registry.pending_transport_messages.push_back(message);
        self.flush_pending_transport_messages(&mut registry);
        Ok(())
    }

    pub fn emit_transport_message(&self, message: String) -> Result<(), HostApiError> {
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

    fn update_player_paused_from_transport(&self, payload: &Value) -> Result<(), String> {
        if let Some(msg_str) = payload.as_str() {
            if let Ok(resp) = serde_json::from_str::<RpcResponse>(msg_str) {
                if let Some(arr) = resp.args.as_ref().and_then(Value::as_array) {
                    if let Some(event_type) = arr.first().and_then(Value::as_str) {
                        let (name, data) = match event_type {
                            "mpv-prop-change" => {
                                let prop = arr.get(1);
                                let name =
                                    prop.and_then(|p| p.get("name")).and_then(Value::as_str);
                                let data = prop.and_then(|p| p.get("data"));
                                (name, data)
                            }
                            _ => (None, None),
                        };
                        self.handle_player_event(event_type, name, data)?;
                    }
                }
            }
            return Ok(());
        }

        if let Some(obj) = payload.as_object() {
            if let Some(event_type) = obj.get("type").and_then(Value::as_str) {
                let (name, data) = match event_type {
                    "mpv-prop-change" => {
                        let name = obj.get("name").and_then(Value::as_str);
                        let data = obj.get("data");
                        (name, data)
                    }
                    _ => (None, None),
                };
                self.handle_player_event(event_type, name, data)?;
            }
        }

        Ok(())
    }

    fn handle_player_event(
        &self,
        event_type: &str,
        prop_name: Option<&str>,
        prop_data: Option<&Value>,
    ) -> Result<(), String> {
        match (event_type, prop_name) {
            ("mpv-prop-change", Some("pause")) => {
                if let Some(paused) = prop_data.and_then(Value::as_bool) {
                    let mut prefs = self.lock_shell_preferences()?;
                    prefs.player_paused = paused;
                }
            }
            ("mpv-event-ended", _) => {
                let mut prefs = self.lock_shell_preferences()?;
                prefs.player_active = false;
                prefs.player_paused = true;
                prefs.auto_paused = false;
            }
            _ => {}
        }
        Ok(())
    }

    fn update_player_state_from_command(
        &self,
        command: &str,
        payload: &Option<Value>,
    ) -> Result<(), String> {
        if command == "mpv-set-prop" {
            return self.update_player_paused_from_set_prop(payload);
        }

        let player_active = match command {
            "native-player-stop" => Some(false),
            "mpv-command" => match payload
                .as_ref()
                .and_then(Value::as_array)
                .and_then(|args| args.first())
                .and_then(Value::as_str)
            {
                Some("loadfile") => Some(true),
                Some("stop" | "quit") => Some(false),
                _ => None,
            },
            _ => None,
        };

        if let Some(player_active) = player_active {
            let mut prefs = self.lock_shell_preferences()?;
            prefs.player_active = player_active;
            prefs.auto_paused = false;
            if !player_active {
                prefs.player_paused = true;
            }
        }

        Ok(())
    }

    fn update_player_paused_from_set_prop(&self, payload: &Option<Value>) -> Result<(), String> {
        let Some(args) = payload.as_ref().and_then(Value::as_array) else {
            return Ok(());
        };
        if args.first().and_then(Value::as_str) != Some("pause") {
            return Ok(());
        }
        let Some(paused) = args.get(1).and_then(Value::as_bool) else {
            return Ok(());
        };

        let mut prefs = self.lock_shell_preferences()?;
        prefs.player_paused = paused;
        prefs.auto_paused = false;
        Ok(())
    }

    pub fn emit_event(&self, event: impl Into<String>, payload: Value) -> Result<(), HostApiError> {
        let event = event.into();
        if event == SHELL_TRANSPORT_EVENT {
            self.update_player_paused_from_transport(&payload)?;
        }
        self.lock_listeners()?.emit(event, payload);
        Ok(())
    }

    pub fn emit_host_event(&self, event: HostEvent, payload: Value) -> Result<(), HostApiError> {
        let event = serde_json::to_value(event)?
            .as_str()
            .ok_or_else(|| {
                HostApiError::InvalidRequest("Host event is not a string".to_string())
            })?
            .to_string();
        self.emit_event(event, payload)
    }

    pub fn drain_emitted_events(&self) -> Result<Vec<HostEventRecord>, HostApiError> {
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
        if is_async_command(command) {
            let runtime = async_runtime();
            runtime.block_on(self.invoke_async(command, payload))
        } else {
            self.invoke_sync(command, payload)
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
                logging::info(
                    "native.mods",
                    format!("Downloaded {} mod {filename}", mod_type.as_str()),
                );
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
            "get_logs" => {
                let payload: GetLogsPayload = parse_payload(command, payload)?;
                serde_json::to_value(logging::snapshot_after(payload.after_id))
                    .map_err(|error| format!("Failed to serialize logs: {error}"))
            }
            "submit_diagnostic_logs" => {
                if payload
                    .as_ref()
                    .and_then(|value| serde_json::to_vec(value).ok())
                    .is_some_and(|bytes| bytes.len() > logging::MAX_EXTERNAL_BATCH_BYTES)
                {
                    return Err("Diagnostic batch payload is too large".to_string());
                }
                let payload: SubmitDiagnosticLogsPayload = parse_payload(command, payload)?;
                logging::submit_external(payload.entries)?;
                Ok(Value::Null)
            }
            "set_extended_diagnostics" => {
                let payload: SetExtendedDiagnosticsPayload = parse_payload(command, payload)?;
                logging::set_extended(payload.enabled);
                Ok(Value::Null)
            }
            "get_diagnostic_report" => {
                const SERVER_REPORT_TAIL_BYTES: usize = 2 * 1024 * 1024;
                let tails = match self.bridge.streaming_log_tails(SERVER_REPORT_TAIL_BYTES) {
                    Ok(Some(tails)) => Some(tails),
                    Ok(None) | Err(_) => None,
                };
                let report = logging::diagnostic_report(logging::DiagnosticReportRuntime {
                    native_player_status: safe_native_player_status(
                        &self.bridge.native_player_status(),
                    ),
                    streaming_server_running: self.bridge.is_streaming_server_running(),
                    server_stdout: tails
                        .as_ref()
                        .map(|tails| Ok(tails.stdout.clone()))
                        .unwrap_or_else(|| Err("unavailable".to_string())),
                    server_stderr: tails
                        .map(|tails| Ok(tails.stderr))
                        .unwrap_or_else(|| Err("unavailable".to_string())),
                });
                Ok(json!(report))
            }
            "clear_diagnostics" => {
                self.bridge.clear_streaming_logs()?;
                logging::clear_diagnostics()?;
                Ok(Value::Null)
            }
            "init" => {
                let (logged_engine, logged_version) = logging::webview_metadata();
                let webview_engine = if logged_engine == "unavailable" {
                    self.bridge.diagnostics_webview_engine().to_string()
                } else {
                    logged_engine
                };
                let webview_version =
                    logged_version.or_else(|| self.bridge.diagnostics_webview_version());
                Ok(json!({
                    "platform": self.bridge.platform_name(),
                    "shell": self.bridge.shell_name(),
                    "shellVersion": self.package_version,
                    "nativePlayer": self.bridge.native_player_status(),
                    "streamingServerRunning": self.bridge.is_streaming_server_running(),
                    "diagnostics": {
                        "persistent": logging::is_persistent_available(),
                        "nativeHttpCapture": self.bridge.native_http_diagnostics(),
                        "nativeNetworkFailureCapture":
                            self.bridge.native_network_failure_diagnostics(),
                        "webviewEngine": webview_engine,
                        "webviewVersion": webview_version,
                    },
                }))
            }
            "get_native_player_status" => Ok(self.bridge.native_player_status()),
            "get_streaming_server_status" => self.bridge.streaming_server_status(),
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
                self.bridge.start_streaming_server().map_err(|error| {
                    logging::error(
                        "native.streaming-server",
                        format!("Failed to start streaming server: {error}"),
                    );
                    error
                })?;
                logging::info("native.streaming-server", "Streaming server started");
                if self.bridge.is_streaming_server_running() {
                    self.emit_host_event(HostEvent::ServerStarted, Value::Null)?;
                }
                Ok(Value::Null)
            }
            "stop_streaming_server" => {
                self.bridge.stop_streaming_server().map_err(|error| {
                    logging::error(
                        "native.streaming-server",
                        format!("Failed to stop streaming server: {error}"),
                    );
                    error
                })?;
                logging::info("native.streaming-server", "Streaming server stopped");
                self.emit_host_event(HostEvent::ServerStopped, Value::Null)?;
                Ok(Value::Null)
            }
            "restart_streaming_server" => {
                let was_running = self.bridge.is_streaming_server_running();
                self.bridge.restart_streaming_server().map_err(|error| {
                    logging::error(
                        "native.streaming-server",
                        format!("Failed to restart streaming server: {error}"),
                    );
                    error
                })?;
                logging::info("native.streaming-server", "Streaming server restarted");
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
                logging::info(
                    "native.mods",
                    format!("Deleted {} mod {}", mod_type.as_str(), payload.filename),
                );
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
                Ok(settings::setting(
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
                settings::registered_settings(&self.settings.registered_schemas)
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
            "set_pip_size" => {
                #[derive(Deserialize)]
                struct SizePayload {
                    width: i32,
                    height: i32,
                }
                let payload: SizePayload = parse_payload(command, payload)?;
                self.bridge.set_pip_size(payload.width, payload.height)?;
                Ok(Value::Null)
            }
            "set_auto_pause" => {
                let enabled = parse_optional_bool(payload).unwrap_or(true);
                let mut prefs = self.lock_shell_preferences()?;
                prefs.auto_pause = enabled;
                Ok(Value::Null)
            }
            "get_auto_pause" => Ok(json!(self.lock_shell_preferences()?.auto_pause)),
            "set_pip_disables_auto_pause" => {
                let enabled = parse_optional_bool(payload).unwrap_or(true);
                let mut prefs = self.lock_shell_preferences()?;
                prefs.pip_disables_auto_pause = enabled;
                Ok(Value::Null)
            }
            "get_pip_disables_auto_pause" => Ok(json!(
                self.lock_shell_preferences()?.pip_disables_auto_pause
            )),
            "mpv-observe-prop" | "mpv-set-prop" | "mpv-command" | "native-player-stop" => {
                self.bridge
                    .handle_custom_transport(command, payload.clone())?;
                self.update_player_state_from_command(command, &payload)?;
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

    pub fn handle_shell_transport_message(&self, message: &str) -> Result<(), String> {
        match parse_request(message)? {
            ParsedRequest::Handshake => {
                self.emit_transport_message(handshake_response(self.package_version))?;
                Ok(())
            }
            ParsedRequest::Command { method, data } => {
                if method == "app-ready" || method == "app-error" {
                    self.mark_transport_ready()?;
                } else if method == "win-set-visibility" {
                    let payload: FullscreenIpcPayload = parse_payload(&method, data)?;
                    self.bridge.set_window_fullscreen(payload.fullscreen)?;
                    self.emit_transport_message(response_message(serialize_window_visibility(
                        true,
                        self.bridge.is_window_fullscreen()?,
                    )))?;
                } else {
                    self.bridge.handle_custom_transport(&method, data.clone())?;
                    self.update_player_state_from_command(&method, &data)?;
                }
                Ok(())
            }
        }
    }

    pub fn update_window_focus(&self, focused: bool) -> Result<(), String> {
        self.emit_event("window-focus-changed", json!(focused))?;

        let is_pip = self.bridge.is_pip_enabled().unwrap_or(false);

        let pause_action = {
            let prefs = self.lock_shell_preferences()?;
            if !prefs.player_active
                || !prefs.auto_pause
                || (is_pip && prefs.pip_disables_auto_pause)
            {
                return Ok(());
            }
            if focused && prefs.auto_paused {
                Some(false)
            } else if !focused && !prefs.player_paused {
                Some(true)
            } else {
                None
            }
        };

        if let Some(paused) = pause_action {
            self.bridge
                .handle_custom_transport("mpv-set-prop", Some(json!(["pause", paused])))?;
            self.lock_shell_preferences()?.auto_paused = paused;
        }

        Ok(())
    }
}
