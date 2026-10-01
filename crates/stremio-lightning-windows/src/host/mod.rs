pub mod bridge;
pub mod types;
pub mod url_policy;

#[cfg(test)]
mod tests;

use serde_json::{json, Value};
use std::path::PathBuf;
#[cfg(windows)]
use std::sync::Arc;
use std::sync::Mutex;
use stremio_lightning_core::host_api::{
    self, BaseHost, HostEvent, HostEventRecord, PlatformBridge,
};
use stremio_lightning_core::pip::{serialize_picture_in_picture, PipState};
use stremio_lightning_core::player_api::PlayerEvent;

#[cfg(windows)]
use crate::window::NativeWindowController;

pub use bridge::{WindowsBridge, WindowsShellBridge};
pub use types::{WindowRuntimeState, WindowsHostError, WindowsIpcOutbound};
pub use url_policy::{open_external_url, validate_external_url};

use crate::player::WindowsPlayer;
use crate::resources::WindowsResourceLayout;
use crate::server::{RealProcessSpawner, WindowsServerConfig, WindowsStreamingServer};
use crate::single_instance::LaunchIntent;

pub struct WindowsHost {
    pub base: BaseHost<WindowsShellBridge>,
    #[cfg(windows)]
    pending_responses: Mutex<Vec<WindowsIpcOutbound>>,
    #[cfg(windows)]
    ui_notifier: Mutex<Option<crate::window::UiThreadNotifier>>,
}

pub type Host = WindowsHost;
pub type IpcRequest = host_api::IpcRequest;

impl Default for WindowsHost {
    fn default() -> Self {
        Self::new(stremio_lightning_core::SHELL_VERSION)
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
        let mut server_config =
            WindowsServerConfig::from_resources(&WindowsResourceLayout::from_runtime())
                .disabled(disabled);
        server_config.log_dir = app_data_dir.join("stremio-lightning").join("logs");
        let bridge = WindowsShellBridge {
            player: Mutex::default(),
            streaming_server: WindowsStreamingServer::new(RealProcessSpawner, server_config),
            window_state: Mutex::default(),
            pip_state: PipState::new(),
            #[cfg(windows)]
            window_controller: Mutex::default(),
        };
        Self {
            base: BaseHost::new(bridge, app_data_dir, package_version),
            #[cfg(windows)]
            pending_responses: Mutex::default(),
            #[cfg(windows)]
            ui_notifier: Mutex::default(),
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
        let args = match intent {
            LaunchIntent::StremioDeepLink(_) => host_api::stremio_deep_link_transport_args(&value),
            _ => json!(["open-media", value]),
        };
        self.base
            .queue_transport_message(host_api::response_message(args))?;
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
    pub fn dispatch_ipc_message_async(self: &Arc<Self>, raw: &str) -> Vec<WindowsIpcOutbound> {
        let request = serde_json::from_str::<host_api::IpcRequest>(raw)
            .ok()
            .filter(|request| request.kind == "invoke")
            .and_then(|request| {
                let payload = host_api::parse_payload::<host_api::InvokeIpcPayload>(
                    "invoke",
                    request.payload,
                )
                .ok()?;
                host_api::is_async_command(&payload.command).then_some((request.id, payload))
            });

        let Some((id, payload)) = request else {
            return self.dispatch_ipc_message(raw);
        };

        let host = Arc::clone(self);
        host_api::async_runtime().spawn(async move {
            let result = host
                .base
                .invoke_async(&payload.command, payload.payload)
                .await;
            let outbound = match result {
                Ok(value) => WindowsIpcOutbound::Response {
                    id,
                    ok: true,
                    value,
                },
                Err(error) => WindowsIpcOutbound::Response {
                    id,
                    ok: false,
                    value: json!({ "message": error }),
                },
            };
            if let Ok(mut queue) = host.pending_responses.lock() {
                queue.push(outbound);
            }
            if let Ok(notifier) = host.ui_notifier.lock() {
                if let Some(notifier) = notifier.as_ref() {
                    let _ = notifier.notify();
                }
            }
        });

        self.drain_ipc_events()
    }

    #[cfg(windows)]
    pub fn bind_ui_notifier(
        &self,
        notifier: crate::window::UiThreadNotifier,
    ) -> Result<(), String> {
        *self.ui_notifier.lock().map_err(|e| e.to_string())? = Some(notifier);
        Ok(())
    }

    #[cfg(windows)]
    pub fn drain_pending_responses(&self) -> Vec<WindowsIpcOutbound> {
        self.pending_responses
            .lock()
            .map(|mut queue| std::mem::take(&mut *queue))
            .unwrap_or_default()
    }

    #[cfg(windows)]
    pub fn initialize_native_player(
        &self,
        hwnd: windows::Win32::Foundation::HWND,
        notifier: crate::window::UiThreadNotifier,
    ) -> Result<(), String> {
        self.base
            .bridge
            .lock_player()?
            .initialize(hwnd, notifier)
            .map_err(|error| error.to_string())
    }

    pub fn drain_ipc_events(&self) -> Vec<WindowsIpcOutbound> {
        self.drain_all_emitted_events()
            .unwrap_or_default()
            .into_iter()
            .map(WindowsIpcOutbound::from)
            .collect()
    }

    pub fn drain_emitted_events(&self) -> Result<Vec<HostEventRecord>, String> {
        self.base.drain_emitted_events().map_err(Into::into)
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
        let state_changed = self.is_window_fullscreen()? != fullscreen;
        self.base.bridge.set_window_fullscreen(fullscreen)?;

        if state_changed {
            self.emit_window_fullscreen_changed(fullscreen)?;
        }
        Ok(())
    }

    pub fn emit_window_maximized_changed(&self, maximized: bool) -> Result<(), String> {
        self.base
            .emit_host_event(HostEvent::WindowMaximizedChanged, json!(maximized))
            .map_err(Into::into)
    }

    pub fn emit_window_fullscreen_changed(&self, fullscreen: bool) -> Result<(), String> {
        self.base
            .emit_host_event(HostEvent::WindowFullscreenChanged, json!(fullscreen))?;
        self.base
            .emit_transport_message(host_api::response_message(
                host_api::serialize_window_visibility(true, fullscreen),
            ))
            .map_err(Into::into)
    }

    pub fn emit_server_started(&self) -> Result<(), String> {
        self.base
            .emit_host_event(HostEvent::ServerStarted, Value::Null)
            .map_err(Into::into)
    }

    pub fn emit_server_stopped(&self) -> Result<(), String> {
        self.base
            .emit_host_event(HostEvent::ServerStopped, Value::Null)
            .map_err(Into::into)
    }

    fn drain_all_emitted_events(&self) -> Result<Vec<HostEventRecord>, String> {
        self.emit_player_events()?;
        self.base.drain_emitted_events().map_err(Into::into)
    }

    fn emit_player_events(&self) -> Result<(), String> {
        let events = self.player().lock().map_or_else(
            |poisoned| poisoned.into_inner().drain_events(),
            |mut guard| guard.drain_events(),
        );

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
            .map_err(Into::into)
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
        let changed = self.base.bridge.pip_state.is_enabled()?;
        self.base.bridge.pip_state.set_mode(false, None)?;
        Ok(changed)
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

pub fn default_app_data_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        PathBuf::from(path)
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}
