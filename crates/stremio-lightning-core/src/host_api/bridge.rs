use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::handlers::{
    async_runtime, handshake_response, is_async_command, parse_optional_bool, parse_payload,
    parse_request, response_message, safe_native_player_status, serialize_window_visibility,
    split_invoke_payload,
};
use super::types::{
    DownloadModPayload, FocusChangedPayload, FullscreenIpcPayload, GetLogsPayload, HostApiError,
    HostEvent, HostEventRecord, InterfaceScalePayload, ListenIpcPayload, ListenerRegistry,
    ModFilePayload, ModTypePayload, ParsedRequest, PlatformBridge, RegisterSettingsPayload,
    RpcResponse, SaveSettingPayload, SetExtendedDiagnosticsPayload, SettingKeyPayload,
    ShellPreferenceState, SubmitDiagnosticLogsPayload, UnlistenIpcPayload, ZoomIpcPayload,
    RPC_TYPE_SIGNAL, SHELL_TRANSPORT_EVENT, TRANSPORT_OBJECT,
};
use crate::pip::serialize_picture_in_picture;
use crate::player_api::PlayerEvent;
use crate::{app_update, logging, mods, settings};

enum PlayerStateUpdate {
    Paused(bool),
    Active(bool),
}

#[derive(Deserialize)]
struct WrappedActivity {
    activity: crate::discord_rpc::ActivityPayload,
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

    /// # Errors
    /// Returns an error when the listener registry lock is poisoned.
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

    /// # Errors
    /// Returns an error when the listener registry lock is poisoned or the id is already registered.
    pub fn listen_with_id(&self, id: u64, event: impl Into<String>) -> Result<(), HostApiError> {
        let mut registry = self.lock_listeners()?;
        registry.listen_with_id(id, event);
        Self::flush_pending_transport_messages(&mut registry);
        Ok(())
    }

    /// # Errors
    /// Returns an error when the listener registry lock is poisoned.
    pub fn unlisten(&self, id: u64) -> Result<(), HostApiError> {
        self.lock_listeners()?.unlisten(id);
        Ok(())
    }

    /// # Errors
    /// Returns an error when the host state lock is poisoned.
    pub fn mark_bridge_ready(&self) -> Result<(), HostApiError> {
        let mut registry = self.lock_listeners()?;
        registry.bridge_ready = true;
        Self::flush_pending_transport_messages(&mut registry);
        Ok(())
    }

    /// # Errors
    /// Returns an error when the host state lock is poisoned.
    pub fn mark_transport_ready(&self) -> Result<(), HostApiError> {
        let mut registry = self.lock_listeners()?;
        registry.transport_ready = true;
        Self::flush_pending_transport_messages(&mut registry);
        Ok(())
    }

    /// # Errors
    /// Returns an error when the host state lock is poisoned.
    pub fn queue_transport_message(&self, message: String) -> Result<(), HostApiError> {
        self.update_player_paused_from_transport(&Value::String(message.clone()))?;
        let mut registry = self.lock_listeners()?;
        if registry.pending_transport_messages.len() >= 512 {
            registry.pending_transport_messages.pop_front();
        }
        registry.pending_transport_messages.push_back(message);
        Self::flush_pending_transport_messages(&mut registry);
        Ok(())
    }

    /// # Errors
    /// Returns an error when the host state lock is poisoned.
    pub fn emit_transport_message(&self, message: String) -> Result<(), HostApiError> {
        self.emit_event(SHELL_TRANSPORT_EVENT, Value::String(message))
    }

    /// Emits a native player event as a `shell-transport-message` without
    /// re-parsing the serialized envelope just to track pause state.
    /// # Errors
    /// Returns an error when the host state lock is poisoned.
    pub fn emit_player_event(&self, event: &PlayerEvent) -> Result<(), HostApiError> {
        self.update_player_state_from_player_event(event)?;
        let message = player_event_message(event);
        self.lock_listeners()?
            .emit(SHELL_TRANSPORT_EVENT, Value::String(message));
        Ok(())
    }

    fn flush_pending_transport_messages(registry: &mut ListenerRegistry) {
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
                payload: Value::String(message),
            });
        }
    }

    fn update_player_paused_from_transport(&self, payload: &Value) -> Result<(), String> {
        if let Some(msg_str) = payload.as_str() {
            if let Ok(resp) = serde_json::from_str::<RpcResponse>(msg_str) {
                if let Some(args) = resp.args.as_ref() {
                    self.update_player_state_from_transport_args(args)?;
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

    fn update_player_state_from_transport_args(&self, args: &Value) -> Result<(), String> {
        let Some(values) = args.as_array() else {
            return Ok(());
        };
        let Some(event_type) = values.first().and_then(Value::as_str) else {
            return Ok(());
        };
        let (name, data) = match event_type {
            "mpv-prop-change" => {
                let prop = values.get(1);
                let name = prop.and_then(|p| p.get("name")).and_then(Value::as_str);
                let data = prop.and_then(|p| p.get("data"));
                (name, data)
            }
            _ => (None, None),
        };
        self.handle_player_event(event_type, name, data)
    }

    fn update_player_state_from_player_event(&self, event: &PlayerEvent) -> Result<(), String> {
        match event {
            PlayerEvent::PropertyChange(change) => self.handle_player_event(
                "mpv-prop-change",
                Some(change.name.as_str()),
                Some(&change.data),
            ),
            PlayerEvent::Ended(_) => self.handle_player_event("mpv-event-ended", None, None),
            _ => Ok(()),
        }
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

    fn player_state_update_from_command(
        command: &str,
        payload: Option<&Value>,
    ) -> Option<PlayerStateUpdate> {
        if command == "mpv-set-prop" {
            let args = payload?.as_array()?;
            if args.first().and_then(Value::as_str) != Some("pause") {
                return None;
            }
            return args
                .get(1)
                .and_then(Value::as_bool)
                .map(PlayerStateUpdate::Paused);
        }

        let active = match command {
            "native-player-stop" => false,
            "mpv-command" => match payload
                .and_then(Value::as_array)
                .and_then(|args| args.first())
                .and_then(Value::as_str)
            {
                Some("loadfile") => true,
                Some("stop" | "quit") => false,
                _ => return None,
            },
            _ => return None,
        };

        Some(PlayerStateUpdate::Active(active))
    }

    fn apply_player_state_update(&self, update: &PlayerStateUpdate) -> Result<(), String> {
        let mut prefs = self.lock_shell_preferences()?;
        prefs.auto_paused = false;
        match *update {
            PlayerStateUpdate::Paused(paused) => prefs.player_paused = paused,
            PlayerStateUpdate::Active(active) => {
                prefs.player_active = active;
                if !active {
                    prefs.player_paused = true;
                }
            }
        }
        Ok(())
    }

    /// # Errors
    /// Returns an error when the host state lock is poisoned.
    pub fn emit_event(&self, event: impl Into<String>, payload: Value) -> Result<(), HostApiError> {
        let event = event.into();
        if event == SHELL_TRANSPORT_EVENT {
            self.update_player_paused_from_transport(&payload)?;
        }
        self.lock_listeners()?.emit(event, payload);
        Ok(())
    }

    /// # Errors
    /// Returns an error when the host state lock is poisoned.
    pub fn emit_host_event(&self, event: HostEvent, payload: Value) -> Result<(), HostApiError> {
        self.emit_event(event.as_str(), payload)
    }

    /// # Errors
    /// Returns an error when the host state lock is poisoned.
    pub fn drain_emitted_events(&self) -> Result<Vec<HostEventRecord>, HostApiError> {
        Ok(self.lock_listeners()?.drain_emitted())
    }

    /// Moves buffered events into `out`, keeping the registry's buffer for the
    /// next emit. Callers that drain on every tick should reuse `out` for the
    /// same reason.
    /// # Errors
    /// Returns an error when the host state lock is poisoned.
    pub fn drain_emitted_events_into(
        &self,
        out: &mut Vec<HostEventRecord>,
    ) -> Result<(), HostApiError> {
        self.lock_listeners()?.drain_emitted_into(out);
        Ok(())
    }

    /// # Errors
    /// Returns the error reported by the IPC handler, or a parse error when the payload is malformed.
    pub fn dispatch_ipc(&self, kind: &str, payload: Option<Value>) -> Result<Value, String> {
        match kind {
            "invoke" => {
                let (command, payload) = split_invoke_payload(payload)?;
                self.invoke(&command, payload)
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
                if self.bridge.platform_name() == "macos" {
                    self.emit_event(
                        "window-fullscreen-changed",
                        json!({ "fullscreen": payload.fullscreen }),
                    )?;
                    self.emit_transport_message(response_message(serialize_window_visibility(
                        true,
                        payload.fullscreen,
                    )))?;
                } else {
                    self.emit_host_event(
                        HostEvent::WindowFullscreenChanged,
                        json!(payload.fullscreen),
                    )?;
                    self.emit_transport_message(response_message(serialize_window_visibility(
                        true,
                        payload.fullscreen,
                    )))?;
                }
                Ok(Value::Null)
            }
            "webview.setZoom" => {
                let payload: ZoomIpcPayload = parse_payload(kind, payload)?;
                // `level` is a zoom factor where 1.0 is 100%; the range keeps a
                // hostile or buggy page from asking for an unusable scale.
                // WebView2 takes the factor, WebKitGTK wants `log2` of it.
                if !payload.level.is_finite() || !(0.25..=4.0).contains(&payload.level) {
                    return Err("Invalid webview zoom level".to_string());
                }
                self.bridge.set_webview_zoom(payload.level)?;
                Ok(Value::Null)
            }
            other => Err(format!("Unsupported IPC kind: {other}")),
        }
    }

    /// # Errors
    /// Returns the error reported by the command handler.
    pub fn invoke(&self, command: &str, payload: Option<Value>) -> Result<Value, String> {
        if is_async_command(command) {
            let runtime = async_runtime();
            runtime.block_on(self.invoke_async(command, payload))
        } else {
            self.invoke_sync(command, payload)
        }
    }

    /// # Errors
    /// Returns the error reported by the command handler.
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

    /// # Errors
    /// Returns the error reported by the command handler.
    #[allow(clippy::too_many_lines)]
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
                    server_stdout: tails.as_ref().map_or_else(
                        || Err("unavailable".to_string()),
                        |tails| Ok(tails.stdout.clone()),
                    ),
                    server_stderr: tails
                        .map_or_else(|| Err("unavailable".to_string()), |tails| Ok(tails.stderr)),
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
                let state_update =
                    Self::player_state_update_from_command(command, payload.as_ref());
                self.bridge.handle_custom_transport(command, payload)?;
                if let Some(update) = state_update {
                    self.apply_player_state_update(&update)?;
                }
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
                let parsed: WrappedActivity = parse_payload(command, payload)?;
                self.discord_rpc.update_activity(&parsed.activity)?;
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

    /// # Errors
    /// Returns an error when the message is not valid JSON or the shell rejects it.
    pub fn handle_shell_transport_message(&self, message: &str) -> Result<(), String> {
        match parse_request(message)? {
            ParsedRequest::Handshake => {
                self.emit_transport_message(handshake_response(
                    self.package_version,
                    self.bridge.streaming_server_url(),
                ))?;
                Ok(())
            }
            ParsedRequest::Command { method, data } => {
                match method.as_str() {
                    "app-ready" | "app-error" => {
                        self.mark_transport_ready()?;
                    }
                    "quit" => {
                        self.bridge.close_window()?;
                    }
                    "win-set-visibility" => {
                        let payload: FullscreenIpcPayload = parse_payload(&method, data)?;
                        self.bridge.set_window_fullscreen(payload.fullscreen)?;
                        self.emit_transport_message(response_message(serialize_window_visibility(
                            true,
                            self.bridge.is_window_fullscreen()?,
                        )))?;
                    }
                    "win-set-interface-scale" => {
                        let payload: InterfaceScalePayload = parse_payload(&method, data)?;
                        let level = payload.scale / 100.0;
                        if !level.is_finite() || !(0.25..=4.0).contains(&level) {
                            return Err("Invalid interface scale".to_string());
                        }
                        self.bridge.set_webview_zoom(level)?;
                    }
                    "discord-connect" => {
                        let connected = self.discord_rpc.start().is_ok();
                        self.emit_transport_message(response_message(serde_json::json!([
                            "discord-status",
                            { "connected": connected }
                        ])))?;
                    }
                    "discord-disconnect" => {
                        let _ = self.discord_rpc.stop();
                        self.emit_transport_message(response_message(serde_json::json!([
                            "discord-status",
                            { "connected": false }
                        ])))?;
                    }
                    "discord-set-activity" => {
                        if let Some(payload_value) = data {
                            let activity: crate::discord_rpc::ActivityPayload =
                                parse_payload(&method, Some(payload_value))?;
                            let _ = self.discord_rpc.update_activity(&activity);
                        }
                    }
                    "discord-clear-activity" => {
                        let _ = self.discord_rpc.clear_activity();
                    }
                    _ => {
                        let state_update =
                            Self::player_state_update_from_command(&method, data.as_ref());
                        self.bridge.handle_custom_transport(&method, data)?;
                        if let Some(update) = state_update {
                            self.apply_player_state_update(&update)?;
                        }
                    }
                }
                Ok(())
            }
        }
    }

    /// # Errors
    /// Returns an error when the window state lock is poisoned.
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

/// Serializes a player event straight into the RPC signal envelope, without
/// building intermediate `serde_json::Value`s for the event payload.
fn player_event_message(event: &PlayerEvent) -> String {
    #[derive(Serialize)]
    struct TransportSignal<'a, A: Serialize> {
        id: u64,
        object: &'a str,
        #[serde(rename = "type")]
        response_type: u8,
        args: A,
    }

    fn serialize_signal<A: Serialize>(args: A) -> String {
        serde_json::to_string(&TransportSignal {
            id: 1,
            object: TRANSPORT_OBJECT,
            response_type: RPC_TYPE_SIGNAL,
            args,
        })
        .expect("failed to serialize transport response")
    }

    match event {
        PlayerEvent::PropertyChange(payload) => serialize_signal(("mpv-prop-change", payload)),
        PlayerEvent::Ended(payload) => serialize_signal(("mpv-event-ended", payload)),
        PlayerEvent::ShowPictureInPicture(payload) => {
            serialize_signal(("showPictureInPicture", payload))
        }
        PlayerEvent::HidePictureInPicture(payload) => {
            serialize_signal(("hidePictureInPicture", payload))
        }
    }
}
