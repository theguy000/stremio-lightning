use crate::app::AppConfig;
use crate::player::MpvPlayerBackend;
use crate::streaming_server::RealProcessSpawner;
use crate::webview_runtime::MacosWebviewRuntime;

pub const IPC_HANDLER_NAME: &str = "ipc";
pub const DEFAULT_WINDOW_WIDTH: f64 = 1500.0;
pub const DEFAULT_WINDOW_HEIGHT: f64 = 850.0;

#[cfg(target_os = "macos")]
pub fn run_native_window(
    _config: AppConfig,
    mut runtime: MacosWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>,
    _player: MpvPlayerBackend,
) -> Result<(), String> {
    let _state = runtime.load()?;
    eprintln!(
        "native macOS AppKit/WKWebView window is not implemented yet; runtime bootstrap completed"
    );
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn run_native_window(
    _config: AppConfig,
    _runtime: MacosWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>,
    _player: MpvPlayerBackend,
) -> Result<(), String> {
    Err("stremio-lightning-macos native window only runs on macOS".to_string())
}
