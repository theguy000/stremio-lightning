use std::sync::{Mutex, MutexGuard};
use serde_json::Value;
use stremio_lightning_core::host_api::PlatformBridge;
use stremio_lightning_core::pip::PipState;
use stremio_lightning_core::streaming_logs::StreamingLogTails;

use super::types::WindowRuntimeState;
use super::url_policy::{open_external_url, validate_external_url};
use crate::player::WindowsPlayer;
use crate::server::{RealProcessSpawner, WindowsStreamingServer};

#[cfg(windows)]
use crate::window::NativeWindowController;

pub struct WindowsShellBridge {
    pub player: Mutex<WindowsPlayer>,
    pub streaming_server: WindowsStreamingServer<RealProcessSpawner>,
    pub window_state: Mutex<WindowRuntimeState>,
    pub pip_state: PipState,
    #[cfg(windows)]
    pub window_controller: Mutex<Option<NativeWindowController>>,
}

pub type WindowsBridge = WindowsShellBridge;

impl WindowsShellBridge {
    pub fn lock_player(&self) -> Result<MutexGuard<'_, WindowsPlayer>, String> {
        self.player
            .lock()
            .map_err(|e| format!("Windows player lock poisoned: {e}"))
    }

    pub fn lock_window_state(&self) -> Result<MutexGuard<'_, WindowRuntimeState>, String> {
        self.window_state
            .lock()
            .map_err(|e| format!("Windows window state lock poisoned: {e}"))
    }

    #[cfg(windows)]
    pub fn lock_window_controller(
        &self,
    ) -> Result<MutexGuard<'_, Option<NativeWindowController>>, String> {
        self.window_controller
            .lock()
            .map_err(|e| format!("Windows window controller lock poisoned: {e}"))
    }
}

impl PlatformBridge for WindowsShellBridge {
    fn platform_name(&self) -> &'static str {
        "windows"
    }

    fn shell_name(&self) -> &'static str {
        "webview2"
    }

    fn native_player_status(&self) -> Value {
        let status = self.player.lock().map_or_else(
            |poisoned| poisoned.into_inner().status(),
            |guard| guard.status(),
        );
        serde_json::to_value(status).unwrap_or(Value::Null)
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
            if let Some(controller) = controller.as_mut() {
                self.pip_state.toggle_window_pip(controller)
            } else {
                let enabled = !self.pip_state.is_enabled()?;
                self.pip_state.set_mode(enabled, None)?;
                Ok(enabled)
            }
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

    fn diagnostics_webview_engine(&self) -> &'static str {
        "WebView2"
    }

    fn native_http_diagnostics(&self) -> bool {
        crate::webview::native_http_capture_available()
    }

    fn native_network_failure_diagnostics(&self) -> bool {
        false
    }

    fn streaming_log_tails(
        &self,
        max_bytes_per_stream: usize,
    ) -> Result<Option<StreamingLogTails>, String> {
        self.streaming_server
            .log_tails(max_bytes_per_stream)
            .map(Some)
    }

    fn clear_streaming_logs(&self) -> Result<(), String> {
        self.streaming_server.clear_logs()
    }

    fn handle_custom_transport(&self, method: &str, data: Option<Value>) -> Result<(), String> {
        match method {
            "mpv-observe-prop" | "mpv-set-prop" | "mpv-command" | "native-player-stop" => {
                self.lock_player()?
                    .handle_transport(method, data)
                    .map_err(|error| error.to_string())?;
                Ok(())
            }
            other => Err(format!("Unsupported shell transport method: {other}")),
        }
    }
}
