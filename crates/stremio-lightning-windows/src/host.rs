use crate::player::WindowsPlayer;
use crate::resources::WindowsResourceLayout;
use crate::server::{RealProcessSpawner, WindowsStreamingServer};
use crate::single_instance::LaunchIntent;
use serde::Serialize;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Mutex;
use stremio_lightning_core::host_api::{
    self, BaseHost, HostEvent, HostEventRecord, PlatformBridge,
};
use stremio_lightning_core::pip::{serialize_picture_in_picture, PipState};
use stremio_lightning_core::player_api::PlayerEvent;

#[cfg(windows)]
use crate::window::NativeWindowController;

#[derive(Debug, Default)]
pub struct WindowRuntimeState {
    pub fullscreen: bool,
    pub maximized: bool,
    pub focused: bool,
    pub visible: bool,
}

pub struct WindowsBridge {
    pub player: Mutex<WindowsPlayer>,
    pub streaming_server: WindowsStreamingServer<RealProcessSpawner>,
    pub window_state: Mutex<WindowRuntimeState>,
    pub pip_state: PipState,
    #[cfg(windows)]
    pub window_controller: Mutex<Option<NativeWindowController>>,
}

impl WindowsBridge {
    fn lock_player(&self) -> Result<std::sync::MutexGuard<'_, WindowsPlayer>, String> {
        self.player
            .lock()
            .map_err(|e| format!("Windows player lock poisoned: {e}"))
    }

    fn lock_window_state(&self) -> Result<std::sync::MutexGuard<'_, WindowRuntimeState>, String> {
        self.window_state
            .lock()
            .map_err(|e| format!("Windows window state lock poisoned: {e}"))
    }

    #[cfg(windows)]
    fn lock_window_controller(
        &self,
    ) -> Result<std::sync::MutexGuard<'_, Option<NativeWindowController>>, String> {
        self.window_controller
            .lock()
            .map_err(|e| format!("Windows window controller lock poisoned: {e}"))
    }
}

impl PlatformBridge for WindowsBridge {
    fn platform_name(&self) -> &'static str {
        "windows"
    }

    fn shell_name(&self) -> &'static str {
        "webview2"
    }

    fn native_player_status(&self) -> Value {
        serde_json::to_value(self.player.lock().unwrap().status()).unwrap_or(Value::Null)
    }

    fn is_streaming_server_running(&self) -> bool {
        self.streaming_server.is_running()
    }

    fn minimize_window(&self) -> Result<(), String> {
        #[cfg(windows)]
        if let Some(controller) = self.lock_window_controller()?.as_ref() {
            controller.minimize();
        }
        self.lock_window_state()?.visible = false;
        Ok(())
    }

    fn focus_window(&self) -> Result<(), String> {
        #[cfg(windows)]
        if let Some(controller) = self.lock_window_controller()?.as_ref() {
            controller.focus();
        }
        self.lock_window_state()?.focused = true;
        Ok(())
    }

    fn toggle_window_maximize(&self) -> Result<bool, String> {
        #[cfg(windows)]
        if let Some(controller) = self.lock_window_controller()?.as_ref() {
            return Ok(controller.toggle_maximize());
        }

        let mut state = self.lock_window_state()?;
        state.maximized = !state.maximized;
        state.visible = true;
        Ok(state.maximized)
    }

    fn close_window(&self) -> Result<(), String> {
        #[cfg(windows)]
        if let Some(controller) = self.lock_window_controller()?.as_ref() {
            controller.close();
        }
        Ok(())
    }

    fn start_window_dragging(&self) -> Result<(), String> {
        #[cfg(windows)]
        if let Some(controller) = self.lock_window_controller()?.as_ref() {
            controller.start_dragging();
        }
        Ok(())
    }

    fn is_window_maximized(&self) -> Result<bool, String> {
        #[cfg(windows)]
        if let Some(controller) = self.lock_window_controller()?.as_ref() {
            return Ok(controller.is_maximized());
        }

        Ok(self.lock_window_state()?.maximized)
    }

    fn is_window_fullscreen(&self) -> Result<bool, String> {
        #[cfg(windows)]
        if let Some(controller) = self.lock_window_controller()?.as_ref() {
            return Ok(controller.is_fullscreen());
        }

        Ok(self.lock_window_state()?.fullscreen)
    }

    fn set_window_fullscreen(&self, fullscreen: bool) -> Result<(), String> {
        #[cfg(windows)]
        {
            if let Some(controller) = self.lock_window_controller()?.as_mut() {
                controller.set_fullscreen(fullscreen)?;
            }
        }
        self.lock_window_state()?.fullscreen = fullscreen;
        Ok(())
    }

    fn toggle_picture_in_picture(&self) -> Result<bool, String> {
        #[cfg(windows)]
        {
            let mut controller = self.lock_window_controller()?;
            let controller = controller
                .as_mut()
                .ok_or_else(|| "Windows window controller is not initialized".to_string())?;
            self.pip_state.toggle_window_pip(controller)
        }
        #[cfg(not(windows))]
        {
            let enabled = !self.pip_state.is_enabled()?;
            self.pip_state.set_mode(enabled, None)?;
            Ok(enabled)
        }
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

    fn start_streaming_server(&self) -> Result<(), String> {
        self.streaming_server.start()
    }

    fn stop_streaming_server(&self) -> Result<(), String> {
        self.streaming_server.stop()
    }

    fn restart_streaming_server(&self) -> Result<(), String> {
        self.streaming_server.restart()
    }

    fn handle_custom_transport(&self, method: &str, data: Option<Value>) -> Result<(), String> {
        match method {
            "mpv-observe-prop" | "mpv-set-prop" | "mpv-command" | "native-player-stop" => {
                self.lock_player()?.handle_transport(method, data)?;
                Ok(())
            }
            other => Err(format!("Unsupported shell transport method: {other}")),
        }
    }
}

pub struct WindowsHost {
    pub base: BaseHost<WindowsBridge>,
}

pub type Host = WindowsHost;

pub type IpcRequest = host_api::IpcRequest;

#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "kind")]
pub enum WindowsIpcOutbound {
    #[serde(rename = "response")]
    Response { id: u64, ok: bool, value: Value },
    #[serde(rename = "event")]
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

impl Default for WindowsHost {
    fn default() -> Self {
        Self::new(env!("CARGO_PKG_VERSION"))
    }
}

impl WindowsHost {
    pub fn player(&self) -> &Mutex<WindowsPlayer> {
        &self.base.bridge.player
    }

    pub fn streaming_server(&self) -> &WindowsStreamingServer<RealProcessSpawner> {
        &self.base.bridge.streaming_server
    }

    pub fn new(package_version: &'static str) -> Self {
        Self::with_app_data_dir(package_version, default_app_data_dir())
    }

    pub fn with_app_data_dir(package_version: &'static str, app_data_dir: PathBuf) -> Self {
        Self::with_app_data_dir_and_server_disabled(package_version, app_data_dir, false)
    }

    pub fn with_streaming_server_disabled(package_version: &'static str, disabled: bool) -> Self {
        Self::with_app_data_dir_and_server_disabled(
            package_version,
            default_app_data_dir(),
            disabled,
        )
    }

    pub fn with_app_data_dir_and_server_disabled(
        package_version: &'static str,
        app_data_dir: PathBuf,
        disabled: bool,
    ) -> Self {
        let bridge = WindowsBridge {
            player: Mutex::default(),
            streaming_server: WindowsStreamingServer::from_resources(
                &WindowsResourceLayout::from_runtime(),
                disabled,
            ),
            window_state: Mutex::default(),
            pip_state: PipState::new(),
            #[cfg(windows)]
            window_controller: Mutex::default(),
        };
        Self {
            base: BaseHost::new(bridge, app_data_dir, package_version),
        }
    }

    pub fn start_streaming_server(&self) -> Result<(), String> {
        self.streaming_server().start()?;
        if !self.streaming_server().disabled() {
            self.emit_server_started()?;
        }
        Ok(())
    }

    pub fn shutdown(&self) -> Result<(), String> {
        if let Ok(mut player) = self.player().lock() {
            player.shutdown();
        }
        self.streaming_server().stop()
    }

    pub fn emit_launch_intent(&self, intent: LaunchIntent) -> Result<(), String> {
        let Some(value) = intent.open_media_value() else {
            return Ok(());
        };
        self.base
            .queue_transport_message(host_api::response_message(json!(["open-media", value])))?;
        Ok(())
    }

    #[cfg(windows)]
    pub fn bind_native_window(&self, hwnd: windows::Win32::Foundation::HWND) -> Result<(), String> {
        *self.base.bridge.lock_window_controller()? = Some(NativeWindowController::new(hwnd));
        Ok(())
    }

    pub fn dispatch_ipc_message(&self, raw: &str) -> Vec<WindowsIpcOutbound> {
        let response = serde_json::from_str::<host_api::IpcRequest>(raw)
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
        self.base.bridge.lock_player()?.initialize(hwnd, notifier)
    }

    pub fn drain_ipc_events(&self) -> Vec<WindowsIpcOutbound> {
        self.drain_all_emitted_events()
            .unwrap_or_default()
            .into_iter()
            .map(WindowsIpcOutbound::from)
            .collect()
    }

    pub fn drain_emitted_events(&self) -> Result<Vec<HostEventRecord>, String> {
        self.base.drain_emitted_events()
    }

    pub fn dispatch_ipc(&self, kind: &str, payload: Option<Value>) -> Result<Value, String> {
        self.base.dispatch_ipc(kind, payload)
    }

    pub fn dispatch_windows_ipc(
        &self,
        kind: &str,
        payload: Option<Value>,
    ) -> Result<Value, String> {
        self.dispatch_ipc(kind, payload)
    }

    pub fn invoke(&self, command: &str, payload: Option<Value>) -> Result<Value, String> {
        self.base.invoke(command, payload)
    }

    pub fn emit_media_key(&self, action: &str) -> Result<(), String> {
        self.base
            .queue_transport_message(host_api::response_message(json!(["media-key", action])))?;
        Ok(())
    }

    pub fn update_window_maximized(&self, maximized: bool) -> Result<(), String> {
        self.set_window_maximized(maximized)
    }

    pub fn update_window_focus(&self, focused: bool) -> Result<(), String> {
        let changed = {
            let mut state = self.base.bridge.lock_window_state()?;
            let changed = state.focused != focused;
            state.focused = focused;
            changed
        };
        if changed {
            self.base.update_window_focus(focused)?;
        }
        Ok(())
    }

    pub fn update_window_visible(&self, visible: bool) -> Result<(), String> {
        let changed = {
            let mut state = self.base.bridge.lock_window_state()?;
            let changed = state.visible != visible;
            state.visible = visible;
            changed
        };
        if changed {
            self.base
                .emit_event("window-visible-changed", json!(visible))?;
        }
        Ok(())
    }

    pub fn minimize_window(&self) -> Result<(), String> {
        self.base.bridge.minimize_window()?;
        self.update_window_visible(false)
    }

    pub fn focus_window(&self) -> Result<(), String> {
        self.base.bridge.focus_window()?;
        self.update_window_focus(true)
    }

    pub fn toggle_window_maximize(&self) -> Result<bool, String> {
        let maximized = self.base.bridge.toggle_window_maximize()?;
        self.set_window_maximized(maximized)?;
        Ok(maximized)
    }

    pub fn close_window(&self) -> Result<(), String> {
        self.exit_picture_in_picture_window()?;
        self.base.bridge.close_window()?;
        Ok(())
    }

    pub fn start_window_dragging(&self) -> Result<(), String> {
        self.base.bridge.start_window_dragging()?;
        Ok(())
    }

    pub fn is_window_maximized(&self) -> Result<bool, String> {
        self.base.bridge.is_window_maximized()
    }

    pub fn is_window_fullscreen(&self) -> Result<bool, String> {
        self.base.bridge.is_window_fullscreen()
    }

    pub fn set_window_maximized(&self, maximized: bool) -> Result<(), String> {
        let changed = {
            let mut state = self.base.bridge.lock_window_state()?;
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

    pub fn set_window_fullscreen(&self, fullscreen: bool) -> Result<(), String> {
        self.base.bridge.set_window_fullscreen(fullscreen)?;
        let state_changed = {
            let mut state = self.base.bridge.lock_window_state()?;
            let state_changed = state.fullscreen != fullscreen;
            state.fullscreen = fullscreen;
            state.visible = true;
            state_changed
        };

        if state_changed {
            self.emit_window_fullscreen_changed(fullscreen)?;
        }
        Ok(())
    }

    pub fn emit_window_maximized_changed(&self, maximized: bool) -> Result<(), String> {
        self.base
            .emit_host_event(HostEvent::WindowMaximizedChanged, json!(maximized))
    }

    pub fn emit_window_fullscreen_changed(&self, fullscreen: bool) -> Result<(), String> {
        self.base
            .emit_host_event(HostEvent::WindowFullscreenChanged, json!(fullscreen))?;
        self.base.emit_transport_message(host_api::response_message(
            host_api::serialize_window_visibility(true, fullscreen),
        ))
    }

    pub fn emit_server_started(&self) -> Result<(), String> {
        self.base
            .emit_host_event(HostEvent::ServerStarted, Value::Null)
    }

    pub fn emit_server_stopped(&self) -> Result<(), String> {
        self.base
            .emit_host_event(HostEvent::ServerStopped, Value::Null)
    }

    fn drain_all_emitted_events(&self) -> Result<Vec<HostEventRecord>, String> {
        self.emit_player_events()?;
        self.base.drain_emitted_events()
    }

    fn emit_player_events(&self) -> Result<(), String> {
        let events = self.player().lock().unwrap().drain_events();

        for event in events {
            if matches!(event, PlayerEvent::Ended(_))
                && self.exit_picture_in_picture_window_for_player_end()?
            {
                self.emit_picture_in_picture(false)?;
            }
            self.base
                .emit_transport_message(host_api::response_message(event.transport_args()))?;
        }
        Ok(())
    }

    fn emit_picture_in_picture(&self, enabled: bool) -> Result<(), String> {
        self.base
            .emit_transport_message(host_api::response_message(serialize_picture_in_picture(
                enabled,
            )))
    }

    pub fn toggle_picture_in_picture_window(&self) -> Result<bool, String> {
        self.base.bridge.toggle_picture_in_picture()
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
        let mut controller = self.base.bridge.lock_window_controller()?;
        if let Some(controller) = controller.as_mut() {
            return exit(&self.base.bridge.pip_state, controller);
        }
        Ok(false)
    }

    #[cfg(not(windows))]
    fn exit_picture_in_picture_window(&self) -> Result<bool, String> {
        let changed = self.base.bridge.pip_state.is_enabled()?;
        self.base.bridge.pip_state.set_mode(false, None)?;
        Ok(changed)
    }

    #[cfg(not(windows))]
    fn exit_picture_in_picture_window_for_player_end(&self) -> Result<bool, String> {
        self.exit_picture_in_picture_window()
    }
}

fn default_app_data_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        PathBuf::from(path)
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

fn validate_external_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.contains(|c: char| c.is_control()) {
        return Err("Rejected non-whitelisted open_external_url URL".to_string());
    }

    let allowed = ["http://", "https://", "mailto:"].iter().any(|prefix| {
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
    use stremio_lightning_core::mods;

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
        let host = WindowsHost::new("0.1.6");
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

        host.player().lock().unwrap().emit_ended("eof");

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

    use std::sync::atomic::{AtomicUsize, Ordering};

    static TEMP_ID: AtomicUsize = AtomicUsize::new(0);

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "stremio-lightning-windows-host-test-{}-{}-{}",
            std::process::id(),
            name,
            TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    fn host_with_app_data(app_data_dir: PathBuf) -> WindowsHost {
        WindowsHost::with_app_data_dir_and_server_disabled(
            env!("CARGO_PKG_VERSION"),
            app_data_dir,
            true,
        )
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(future)
    }

    #[test]
    fn lists_reads_and_deletes_plugin_and_theme_mods() {
        let root = temp_dir("mods-contract");
        let host = host_with_app_data(root.clone());

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
            .base
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
        assert_eq!(host.base.invoke("get_plugins", None).unwrap(), json!([]));
        assert!(!mods::mods_dir(&root, mods::ModType::Plugin)
            .join("sample.plugin.json")
            .exists());
    }

    #[test]
    fn rejects_invalid_mod_payloads() {
        let host = WindowsHost::default();
        let traversal = host
            .base
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

        let download_error = block_on(host.base.invoke_async(
            "download_mod",
            Some(json!({"url": "https://example.test/evil.theme.css", "modType": "plugin"})),
        ))
        .unwrap_err();
        assert!(download_error.contains("Invalid plugin filename extension"));
    }

    #[test]
    fn plugin_settings_round_trip_and_validate() {
        let root = temp_dir("settings-contract");
        let host = host_with_app_data(root.clone());

        host.invoke(
            "register_settings",
            Some(json!({
                "pluginName": "sample",
                "schema": r#"[{"key":"enabled","type":"toggle"}]"#
            })),
        )
        .unwrap();
        assert_eq!(
            host.invoke("get_registered_settings", None).unwrap(),
            json!({"sample": [{"key": "enabled", "type": "toggle"}]})
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

        host.base
            .invoke(
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
    }
}
