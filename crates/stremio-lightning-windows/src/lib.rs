pub mod host;
pub mod player;
pub mod resources;
pub mod server;
pub mod settings;
pub mod single_instance;
pub mod webview;
pub mod window;

pub use stremio_lightning_identity::{APP_ID, APP_NAME};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum WindowsAppError {
    #[error("Platform error: {0}")]
    Platform(String),
    #[error("Single instance error: {0}")]
    SingleInstance(String),
    #[error("WebView error: {0}")]
    WebView(#[from] crate::webview::WebViewError),
    #[error("Window error: {0}")]
    Window(#[from] crate::window::WindowError),
    #[error("Host error: {0}")]
    Host(#[from] crate::host::WindowsHostError),
    #[error("Server error: {0}")]
    Server(#[from] crate::server::ServerError),
    #[error("stremio-lightning-windows only runs on Windows")]
    UnsupportedPlatform,
}

impl From<String> for WindowsAppError {
    fn from(error: String) -> Self {
        Self::Platform(error)
    }
}

impl From<&str> for WindowsAppError {
    fn from(error: &str) -> Self {
        Self::Platform(error.to_string())
    }
}

impl From<WindowsAppError> for String {
    fn from(error: WindowsAppError) -> Self {
        error.to_string()
    }
}

/// Runs the Windows desktop application shell.
///
/// # Errors
/// Returns [`WindowsAppError`] if shell initialization or runtime loop fails.
pub fn run() -> Result<(), WindowsAppError> {
    platform::run()
}

#[cfg(windows)]
mod platform {
    use super::WindowsAppError;

    pub fn run() -> Result<(), WindowsAppError> {
        let _ = stremio_lightning_core::logging::initialize(
            stremio_lightning_core::logging::LoggingConfig::new(
                stremio_lightning_core::logging::diagnostics_dir_for_platform("windows"),
                stremio_lightning_core::SHELL_VERSION,
                "windows",
                "webview2",
                "WebView2",
            ),
        );
        stremio_lightning_core::logging::info("native.application", "Starting Windows shell");
        if let Err(error) = crate::window::set_app_user_model_id(crate::APP_ID) {
            stremio_lightning_core::logging::warn("native.application", error);
        }
        let args = std::env::args().skip(1).collect::<Vec<_>>();
        let intent = crate::single_instance::launch_intent_from_args(&args);
        let crate::single_instance::SingleInstanceRole::Primary(instance) =
            crate::single_instance::SingleInstanceGuard::acquire(&intent)?
        else {
            return Ok(());
        };

        let ui_notifier = std::sync::Arc::new(std::sync::Mutex::new(None));
        let launch_intents = instance.start_listener(ui_notifier.clone(), intent);
        let settings = crate::settings::ShellSettings::from_args(&args);
        crate::webview::WindowsWebView2Shell::new(settings, launch_intents, ui_notifier)?.run()?;
        Ok(())
    }
}

#[cfg(not(windows))]
mod platform {
    use super::WindowsAppError;

    pub fn run() -> Result<(), WindowsAppError> {
        Err(WindowsAppError::UnsupportedPlatform)
    }
}
