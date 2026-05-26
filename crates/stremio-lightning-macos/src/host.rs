use crate::app_integration::{lifecycle_event_payload, AppLifecycleEvent, LaunchIntent};
use crate::diagnostics::{self, MacosDiagnosticsSnapshot};
use crate::player::{self, NativePlayerStatus, PlayerBackend};
use crate::streaming_server::{ProcessSpawner, StreamingServer};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
pub use stremio_lightning_core::host_api::SHELL_TRANSPORT_EVENT;
use stremio_lightning_core::host_api::{self, BaseHost, HostEventRecord, PlatformBridge};
use stremio_lightning_core::pip::PipState;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WindowRuntimeState {
    pub fullscreen: bool,
    pub maximized: bool,
    pub focused: bool,
    pub visible: bool,
    pub close_to_hide: bool,
}

pub struct MacosBridge<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    pub player: B,
    pub streaming_server: StreamingServer<P>,
    pub window_state: Mutex<WindowRuntimeState>,
    pub pip_state: PipState,
}

impl<B, P> MacosBridge<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    fn lock_window_state(&self) -> Result<std::sync::MutexGuard<'_, WindowRuntimeState>, String> {
        self.window_state
            .lock()
            .map_err(|e| format!("macOS window state lock poisoned: {e}"))
    }
}

impl<B, P> PlatformBridge for MacosBridge<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    fn platform_name(&self) -> &'static str {
        "macos"
    }

    fn shell_name(&self) -> &'static str {
        ""
    }

    fn native_player_status(&self) -> Value {
        serde_json::to_value(self.player.status()).unwrap_or(Value::Null)
    }

    fn is_streaming_server_running(&self) -> bool {
        self.streaming_server.is_running()
    }

    fn get_streaming_server_status(&self) -> Result<Value, String> {
        serde_json::to_value(self.streaming_server.status())
            .map_err(|e| format!("Failed to serialize macOS streaming server status: {e}"))
    }

    fn toggle_picture_in_picture(&self) -> Result<bool, String> {
        self.pip_state.toggle()
    }

    fn is_pip_enabled(&self) -> Result<bool, String> {
        self.pip_state.is_enabled()
    }

    fn set_pip_size(&self, width: i32, height: i32) -> Result<(), String> {
        self.pip_state.set_size(width, height)
    }

    fn open_external_url(&self, url: &str) -> Result<(), String> {
        validate_external_url(url)?;
        open_external_url(url)?;
        Ok(())
    }

    fn minimize_window(&self) -> Result<(), String> {
        self.lock_window_state()?.visible = false;
        Ok(())
    }

    fn toggle_window_maximize(&self) -> Result<bool, String> {
        let mut state = self.lock_window_state()?;
        state.maximized = !state.maximized;
        state.visible = true;
        Ok(state.maximized)
    }

    fn close_window(&self) -> Result<(), String> {
        let close_to_hide = self.lock_window_state()?.close_to_hide;
        if close_to_hide {
            self.lock_window_state()?.visible = false;
        }
        Ok(())
    }

    fn is_window_maximized(&self) -> Result<bool, String> {
        Ok(self.lock_window_state()?.maximized)
    }

    fn is_window_fullscreen(&self) -> Result<bool, String> {
        Ok(self.lock_window_state()?.fullscreen)
    }

    fn set_window_fullscreen(&self, fullscreen: bool) -> Result<(), String> {
        self.lock_window_state()?.fullscreen = fullscreen;
        Ok(())
    }

    fn handle_custom_transport(&self, method: &str, data: Option<Value>) -> Result<(), String> {
        match method {
            "mpv-observe-prop" | "mpv-set-prop" | "mpv-command" | "native-player-stop" => {
                player::handle_transport(&self.player, method, data)?;
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn start_streaming_server(&self) -> Result<(), String> {
        self.streaming_server.start()
    }

    fn stop_streaming_server(&self) -> Result<(), String> {
        self.streaming_server.stop()
    }

    fn restart_streaming_server(&self) -> Result<(), String> {
        self.streaming_server.restart()
    }
}

pub struct MacosHost<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    pub base: BaseHost<MacosBridge<B, P>>,
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
        let bridge = MacosBridge {
            player,
            streaming_server,
            window_state: Mutex::new(WindowRuntimeState {
                visible: true,
                close_to_hide: true,
                ..WindowRuntimeState::default()
            }),
            pip_state: PipState::new(),
        };
        Self {
            base: BaseHost::new(bridge, app_data_dir.into(), env!("CARGO_PKG_VERSION")),
        }
    }

    pub fn player(&self) -> &B {
        &self.base.bridge.player
    }

    pub fn streaming_server(&self) -> &StreamingServer<P> {
        &self.base.bridge.streaming_server
    }

    pub fn window_state(&self) -> Result<WindowRuntimeState, String> {
        Ok(self.base.bridge.lock_window_state()?.clone())
    }

    pub fn start_streaming_server(&self) -> Result<(), String> {
        self.streaming_server().start()
    }

    pub fn stop_streaming_server(&self) -> Result<(), String> {
        self.streaming_server().stop()?;
        self.emit_server_stopped()
    }

    pub fn restart_streaming_server(&self) -> Result<(), String> {
        let was_running = self.streaming_server().is_running();
        self.streaming_server().restart()?;
        if was_running {
            self.emit_server_stopped()?;
        }
        if self.streaming_server().is_running() {
            self.emit_server_started()?;
        }
        Ok(())
    }

    pub fn shutdown(&self) -> Result<(), String> {
        self.emit_lifecycle_event(AppLifecycleEvent::Shutdown).ok();
        self.player().stop().ok();
        self.streaming_server().stop()
    }

    pub fn emit_launch_intent(&self, intent: LaunchIntent) -> Result<(), String> {
        self.focus_window()?;
        let Some(value) = intent.open_media_value() else {
            return Ok(());
        };
        self.base
            .queue_transport_message(host_api::response_message(json!(["open-media", value])))?;
        Ok(())
    }

    pub fn native_player_status(&self) -> NativePlayerStatus {
        self.player().status()
    }

    pub fn diagnostics_snapshot(
        &self,
        load_state: &crate::webview_runtime::WebviewLoadState,
        ipc_errors: Vec<String>,
        first_frame_timing: Option<std::time::Duration>,
    ) -> MacosDiagnosticsSnapshot {
        let server = self.streaming_server().diagnostics();
        diagnostics::diagnostics_snapshot(
            load_state,
            ipc_errors,
            self.native_player_status(),
            &player::default_mpv_options(crate::APP_NAME, cfg!(debug_assertions)),
            first_frame_timing,
            server.status,
            server.stdout_log,
            server.stderr_log,
        )
    }

    pub fn invoke(&self, command: &str, payload: Option<Value>) -> Result<Value, String> {
        let res = self.base.invoke(command, payload);
        if matches!(
            command,
            "mpv-observe-prop" | "mpv-set-prop" | "mpv-command" | "native-player-stop"
        ) {
            self.emit_drained_player_events().ok();
        }
        res
    }

    pub fn dispatch_ipc(&self, kind: &str, payload: Option<Value>) -> Result<Value, String> {
        match kind {
            "window.close" => {
                self.close_window()?;
                Ok(Value::Null)
            }
            other => self.base.dispatch_ipc(other, payload),
        }
    }

    pub fn listen(&self, event: impl Into<String>) -> Result<u64, String> {
        Ok(self.base.lock_listeners()?.listen(event))
    }

    pub fn listen_with_id(&self, id: u64, event: impl Into<String>) -> Result<(), String> {
        self.base.listen_with_id(id, event)
    }

    pub fn unlisten(&self, id: u64) -> Result<(), String> {
        self.base.unlisten(id)
    }

    pub fn emit_window_fullscreen_changed(&self, fullscreen: bool) -> Result<(), String> {
        self.base.emit_event(
            "window-fullscreen-changed",
            json!({ "fullscreen": fullscreen }),
        )?;
        self.emit_transport_response(host_api::response_message(
            host_api::serialize_window_visibility(true, fullscreen),
        ))
    }

    pub fn emit_window_maximized_changed(&self, maximized: bool) -> Result<(), String> {
        self.base.emit_event(
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
        self.base.emit_event(name, payload)
    }

    pub fn emit_server_started(&self) -> Result<(), String> {
        self.base.emit_event(
            "server-started",
            json!({ "url": self.streaming_server().url() }),
        )
    }

    pub fn emit_server_stopped(&self) -> Result<(), String> {
        self.base.emit_event(
            "server-stopped",
            json!({ "url": self.streaming_server().url() }),
        )
    }

    pub fn emit_native_player_property_changed(
        &self,
        name: impl Into<String>,
        data: Value,
    ) -> Result<(), String> {
        self.base.emit_event(
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
        self.base.emit_event(
            SHELL_TRANSPORT_EVENT,
            json!({
                "type": event_type,
                "payload": payload,
            }),
        )
    }

    fn emit_transport_response(&self, message: String) -> Result<(), String> {
        self.base.emit_event(SHELL_TRANSPORT_EVENT, json!(message))
    }

    pub fn minimize_window(&self) -> Result<(), String> {
        self.set_window_visible(false)
    }

    pub fn toggle_window_maximize(&self) -> Result<(), String> {
        let maximized = {
            let mut state = self.base.bridge.lock_window_state()?;
            state.maximized = !state.maximized;
            state.visible = true;
            state.maximized
        };
        self.emit_window_maximized_changed(maximized)
    }

    pub fn close_window(&self) -> Result<(), String> {
        let close_to_hide = self.window_state()?.close_to_hide;
        if close_to_hide {
            self.set_window_visible(false)
        } else {
            self.emit_lifecycle_event(AppLifecycleEvent::Shutdown)
        }
    }

    pub fn focus_window(&self) -> Result<(), String> {
        self.set_window_focus(true)?;
        self.set_window_visible(true)
    }

    pub fn update_window_focus(&self, focused: bool) -> Result<(), String> {
        self.set_window_focus(focused)?;
        self.base.update_window_focus(focused)
    }

    pub fn set_window_focus(&self, focused: bool) -> Result<(), String> {
        self.base.bridge.lock_window_state()?.focused = focused;
        Ok(())
    }

    pub fn set_window_visible(&self, visible: bool) -> Result<(), String> {
        self.base.bridge.lock_window_state()?.visible = visible;
        Ok(())
    }

    pub fn set_window_fullscreen(&self, fullscreen: bool) -> Result<(), String> {
        let changed = {
            let mut state = self.base.bridge.lock_window_state()?;
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
        for event in self.player().drain_events()? {
            self.emit_native_player_transport_args(event.transport_args())?;
        }
        Ok(())
    }

    pub fn drain_emitted_events(&self) -> Result<Vec<HostEventRecord>, String> {
        self.emit_drained_player_events()?;
        self.base.drain_emitted_events()
    }
}

fn default_app_data_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(|home| Path::new(&home).join("Library").join("Application Support"))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
}

fn validate_external_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    let allowed = [
        "http://", "https://", "rtp://", "rtsp://", "ftp://", "ipfs://",
    ]
    .iter()
    .any(|prefix| {
        trimmed
            .get(..prefix.len())
            .is_some_and(|s| s.eq_ignore_ascii_case(prefix))
    });

    if allowed {
        Ok(())
    } else {
        Err("Rejected non-whitelisted open_external_url URL".to_string())
    }
}

#[cfg(target_os = "macos")]
fn open_external_url(url: &str) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(url)
        .spawn()
        .map_err(|e| format!("Failed to open external URL: {e}"))?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn open_external_url(_url: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::FakePlayerBackend;
    use crate::streaming_server::{FakeProcessSpawner, StreamingServer};
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use stremio_lightning_core::mods;

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
        assert!(error.contains("Invalid listen payload:"));
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
            "Unsupported IPC kind: unknown.kind"
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
            .base
            .invoke(
                "get_mod_content",
                Some(json!({"filename": "cinema.plugin.js", "modType": "plugin"})),
            )
            .unwrap();
        assert!(content.as_str().unwrap().contains("window.__cinema"));

        host.invoke(
            "delete_mod",
            Some(json!({"filename": "cinema.plugin.js", "modType": "plugin"})),
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
                Some(json!({"pluginName": "cinema", "key": "enabled"}))
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
            host.invoke("get_registered_settings", None,).unwrap()["cinema"]["type"],
            "object"
        );
        let _ = fs::remove_dir_all(app_data_dir);
    }

    #[test]
    fn shared_host_command_fixture_covers_macos_supported_commands() {
        let host = test_host();
        for command in [
            "init",
            "get_native_player_status",
            "get_streaming_server_status",
            "toggle_devtools",
            "start_discord_rpc",
            "stop_discord_rpc",
            "update_discord_activity",
            "check_app_update",
        ] {
            host.invoke(command, None)
                .unwrap_or_else(|error| panic!("{command} failed shared fixture: {error}"));
        }

        host.invoke("set_auto_pause", Some(json!({"enabled": true})))
            .unwrap();
        assert_eq!(host.invoke("get_auto_pause", None).unwrap(), json!(true));
        host.invoke("set_pip_disables_auto_pause", Some(json!(false)))
            .unwrap();
        assert_eq!(
            host.invoke("get_pip_disables_auto_pause", None).unwrap(),
            json!(false)
        );
    }

    #[test]
    fn shared_plugin_api_fixture_covers_plugins_themes_and_settings() {
        let app_data_dir = temp_app_data_dir("shared-plugin-fixture");
        let plugins_dir = mods::mods_dir(&app_data_dir, mods::ModType::Plugin);
        let themes_dir = mods::mods_dir(&app_data_dir, mods::ModType::Theme);
        fs::create_dir_all(&plugins_dir).unwrap();
        fs::create_dir_all(&themes_dir).unwrap();
        fs::write(
            plugins_dir.join("cinema.plugin.js"),
            "/**\n * @name Cinema\n * @version 1.0.0\n */\nwindow.__cinema = true;",
        )
        .unwrap();
        fs::write(
            themes_dir.join("night.theme.css"),
            "/**\n * @name Night\n * @version 1.0.0\n */\nbody { color: white; }",
        )
        .unwrap();

        let host = test_host_with_app_data_dir(app_data_dir.clone());
        assert_eq!(
            host.invoke("get_plugins", None).unwrap()[0]["filename"],
            "cinema.plugin.js"
        );
        assert_eq!(
            host.invoke("get_themes", None).unwrap()[0]["filename"],
            "night.theme.css"
        );
        assert!(host
            .base
            .invoke(
                "get_mod_content",
                Some(json!({"filename": "night.theme.css", "modType": "theme"})),
            )
            .unwrap()
            .as_str()
            .unwrap()
            .contains("color: white"));
        host.invoke(
            "save_setting",
            Some(json!({"pluginName": "cinema", "key": "quality", "value": "\"1080p\""})),
        )
        .unwrap();
        assert_eq!(
            host.invoke(
                "get_setting",
                Some(json!({"pluginName": "cinema", "key": "quality"})),
            )
            .unwrap(),
            json!("1080p")
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
    fn pip_commands_track_mode_and_emit_shared_player_event() {
        let host = test_host();
        host.dispatch_ipc(
            "listen",
            Some(json!({"id": 21, "event": SHELL_TRANSPORT_EVENT})),
        )
        .unwrap();

        assert_eq!(host.invoke("get_pip_mode", None).unwrap(), json!(false));
        assert_eq!(host.invoke("toggle_pip", None).unwrap(), json!(true));
        assert_eq!(host.invoke("get_pip_mode", None).unwrap(), json!(true));

        let events = host.drain_emitted_events().unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, SHELL_TRANSPORT_EVENT);
        assert_eq!(events[0].payload["type"], "showPictureInPicture");
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
