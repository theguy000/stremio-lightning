pub mod adapter;
pub mod navigation;
pub mod platform;
#[cfg(test)]
mod tests;
pub mod types;

pub use adapter::{host_adapter, windows_host_adapter};
pub use types::{InjectionBundle, WebViewError, HOST_ADAPTER_NAME, WINDOWS_HOST_ADAPTER_NAME};

use crate::host::Host;
use crate::settings::ShellSettings;
use crate::single_instance::LaunchIntent;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use std::sync::Mutex;
use std::sync::{mpsc, Arc};

static NATIVE_HTTP_CAPTURE_AVAILABLE: AtomicBool = AtomicBool::new(false);

pub fn native_http_capture_available() -> bool {
    NATIVE_HTTP_CAPTURE_AVAILABLE.load(Ordering::Relaxed)
}

pub struct WindowsWebView2Shell {
    url: String,
    devtools: bool,
    injection: InjectionBundle,
    #[allow(dead_code)]
    host: Arc<Host>,
    launch_intents: mpsc::Receiver<LaunchIntent>,
    #[cfg(windows)]
    ui_notifier: Arc<Mutex<Option<crate::window::UiThreadNotifier>>>,
}

impl WindowsWebView2Shell {
    /// # Errors
    /// Returns an error when the shell settings are invalid or the shell cannot be constructed.
    #[cfg(windows)]
    pub fn new(
        settings: ShellSettings,
        launch_intents: mpsc::Receiver<LaunchIntent>,
        ui_notifier: Arc<Mutex<Option<crate::window::UiThreadNotifier>>>,
    ) -> Result<Self, WebViewError> {
        Self::build(settings, launch_intents, ui_notifier)
    }

    #[cfg(not(windows))]
    pub fn new(
        settings: ShellSettings,
        launch_intents: mpsc::Receiver<LaunchIntent>,
    ) -> Result<Self, WebViewError> {
        Self::build(settings, launch_intents)
    }

    #[cfg(windows)]
    fn build(
        settings: ShellSettings,
        launch_intents: mpsc::Receiver<LaunchIntent>,
        ui_notifier: Arc<Mutex<Option<crate::window::UiThreadNotifier>>>,
    ) -> Result<Self, WebViewError> {
        let url = settings.webui_url;
        let devtools = settings.devtools;
        if !(url.starts_with("https://") || url.starts_with("http://127.0.0.1:")) {
            return Err(WebViewError::UnsupportedUrl(url));
        }

        Ok(Self {
            url,
            devtools,
            injection: InjectionBundle::load()?,
            host: Arc::new(Host::with_streaming_server_disabled(
                stremio_lightning_core::SHELL_VERSION,
                settings.streaming_server_disabled,
            )),
            launch_intents,
            ui_notifier,
        })
    }

    #[cfg(not(windows))]
    fn build(
        settings: ShellSettings,
        launch_intents: mpsc::Receiver<LaunchIntent>,
    ) -> Result<Self, WebViewError> {
        let url = settings.webui_url;
        let devtools = settings.devtools;
        if !(url.starts_with("https://") || url.starts_with("http://127.0.0.1:")) {
            return Err(WebViewError::UnsupportedUrl(url));
        }

        Ok(Self {
            url,
            devtools,
            injection: InjectionBundle::load()?,
            host: Arc::new(Host::with_streaming_server_disabled(
                stremio_lightning_core::SHELL_VERSION,
                settings.streaming_server_disabled,
            )),
            launch_intents,
        })
    }

    #[must_use]
    pub fn document_start_script_names(&self) -> Vec<&'static str> {
        self.injection
            .scripts()
            .iter()
            .map(|script| script.name)
            .collect()
    }

    /// # Errors
    /// Returns an error when the `WebView2` environment or controller cannot be created, or the run loop fails.
    pub fn run(self) -> Result<(), WebViewError> {
        platform::run_webview2_shell(
            &self.url,
            self.devtools,
            &self.injection,
            self.host,
            self.launch_intents,
            #[cfg(windows)]
            self.ui_notifier,
        )
    }
}
