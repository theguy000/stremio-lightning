use stremio_lightning_core::bridge_assets::{
    bridge_scripts, load_mod_ui_source, InjectionScript, MOD_UI_NAME,
};
use thiserror::Error;

pub const WINDOWS_HOST_ADAPTER_NAME: &str = "windows-host-adapter";
pub const HOST_ADAPTER_NAME: &str = WINDOWS_HOST_ADAPTER_NAME;

#[derive(Debug, Error)]
pub enum WebViewError {
    #[error("Unsupported WebView2 load URL: {0}")]
    UnsupportedUrl(String),

    #[error("Failed to load injection bundle: {0}")]
    InjectionBundle(String),

    #[error("WebView2 shell can only run on Windows")]
    WindowsOnly,

    #[error("Failed to initialize COM for WebView2: {0}")]
    ComInitialization(String),

    #[error("WebView2 controller is not available")]
    ControllerUnavailable,

    #[error("WebView2 instance is not available")]
    InstanceUnavailable,

    #[error("Failed to read WebView2 host bounds: {0}")]
    HostBounds(String),

    #[error("Failed to resize WebView2 controller: {0}")]
    ResizeController(String),

    #[error("Failed to show WebView2 controller: {0}")]
    ShowController(String),

    #[error("Failed to focus WebView2 controller: {0}")]
    FocusController(String),

    #[error("Failed to get WebView2 instance: {0}")]
    GetInstance(String),

    #[error("Failed to create WebView2 environment: {0}")]
    EnvironmentCreation(String),

    #[error("Failed to create WebView2 user data directory '{0}': {1}")]
    UserDataDirectory(String, String),

    #[error("WebView2 user data directory is not valid Unicode")]
    InvalidUserDataPath,

    #[error("LOCALAPPDATA is not available for WebView2 user data")]
    LocalAppDataUnavailable,

    #[error("Failed to create WebView2 controller: {0}")]
    ControllerCreation(String),

    #[error("Failed to get WebView2 controller2: {0}")]
    GetController2(String),

    #[error("Failed to set transparent WebView2 background: {0}")]
    SetBackgroundColor(String),

    #[error("Failed to attach WebView2 accelerator key handler: {0}")]
    AttachAcceleratorHandler(String),

    #[error("Failed to get WebView2 settings: {0}")]
    GetSettings(String),

    #[error("Failed to inject WebView2 script '{0}': {1}")]
    ScriptInjection(String, String),

    #[error("Failed to attach WebView2 message handler: {0}")]
    AttachMessageHandler(String),

    #[error("Failed to attach WebView2 navigation handler: {0}")]
    AttachNavigationHandler(String),

    #[error("Failed to attach WebView2 new window handler: {0}")]
    AttachNewWindowHandler(String),

    #[error("Failed to attach WebView2 navigation completed handler: {0}")]
    AttachNavigationCompletedHandler(String),

    #[error("Failed to attach WebView2 document title changed handler: {0}")]
    AttachDocumentTitleChangedHandler(String),

    #[error("Failed to attach WebView2 process failure handler: {0}")]
    AttachProcessFailedHandler(String),

    #[error("Failed to serialize Windows IPC response: {0}")]
    SerializeIpcResponse(String),

    #[error("Failed to post WebView2 IPC response: {0}")]
    PostIpcResponse(String),

    #[error("Failed to navigate WebView2: {0}")]
    Navigate(String),

    #[error("Window error: {0}")]
    Window(String),

    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectionBundle {
    scripts: Vec<InjectionScript>,
}

impl InjectionBundle {
    /// # Errors
    /// Returns an error when the bundled web assets cannot be read.
    pub fn load() -> Result<Self, WebViewError> {
        let mut scripts = vec![InjectionScript {
            name: HOST_ADAPTER_NAME,
            source: super::adapter::host_adapter(),
        }];
        scripts.extend(bridge_scripts());
        scripts.push(InjectionScript {
            name: MOD_UI_NAME,
            source: load_mod_ui_source().map_err(WebViewError::InjectionBundle)?,
        });

        Ok(Self { scripts })
    }

    #[must_use]
    pub fn scripts(&self) -> &[InjectionScript] {
        &self.scripts
    }

    /// Concatenates every injection source into a single script so the shell can
    /// register them with one `WebView2` round-trip instead of one per script.
    #[must_use]
    pub fn combined_source(&self) -> String {
        self.scripts
            .iter()
            .map(|script| script.source.as_str())
            .collect::<Vec<_>>()
            .join("\n;\n")
    }

    #[must_use]
    pub fn script_names(&self) -> Vec<&'static str> {
        self.scripts.iter().map(|script| script.name).collect()
    }
}

#[cfg(any(windows, test))]
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct CleanupReport {
    failures: Vec<String>,
}

#[cfg(any(windows, test))]
impl CleanupReport {
    pub(crate) fn record(&mut self, action: &'static str, result: Result<(), String>) {
        if let Err(error) = result {
            self.failures.push(format!("{action}: {error}"));
        }
    }

    #[cfg(windows)]
    pub(crate) fn record_windows(
        &mut self,
        action: &'static str,
        result: windows::core::Result<()>,
    ) {
        self.record(action, result.map_err(|error| error.to_string()));
    }

    #[cfg(test)]
    pub(crate) fn failures(&self) -> &[String] {
        &self.failures
    }

    #[cfg(windows)]
    pub(crate) fn log(self, context: &str) {
        for failure in self.failures {
            stremio_lightning_core::logging::error(
                "native.webview.windows",
                format!("{context}: {failure}"),
            );
        }
    }
}
