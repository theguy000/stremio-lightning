use crate::host::Host;
use crate::native_window::run_native_window;
use crate::player::MpvPlayerBackend;
use crate::streaming_server::{RealProcessSpawner, StreamingServer};
use crate::webview_runtime::{InjectionBundle, MacosWebviewRuntime};
use std::sync::Arc;
use stremio_lightning_core::startup::StartupOptions;

pub use stremio_lightning_core::startup::{normalize_startup_url, DEFAULT_URL, STREMIO_WEB_URL};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub url: String,
    pub devtools: bool,
    pub headless_bootstrap: bool,
    pub disable_streaming_server: bool,
}

pub type ShellSettings = AppConfig;

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            url: DEFAULT_URL.to_string(),
            devtools: true,
            headless_bootstrap: false,
            disable_streaming_server: std::env::var("STREMIO_LIGHTNING_MACOS_NO_SERVER")
                .ok()
                .as_deref()
                == Some("1"),
        }
    }
}

pub fn parse_args<I, S>(args: I) -> AppConfig
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let mut config = AppConfig::default();
    let mut startup = StartupOptions::default();
    let mut args = args.into_iter().map(Into::into).skip(1);

    while let Some(arg) = args.next() {
        if startup.apply_arg(&arg, &mut args) {
            continue;
        }
        if arg == "--no-streaming-server" {
            config.disable_streaming_server = true;
        }
    }

    config.url = startup.url;
    config.devtools = startup.devtools;
    config.headless_bootstrap = startup.headless_bootstrap;
    config
}

pub fn run(config: AppConfig) -> Result<(), String> {
    let _ = stremio_lightning_core::logging::initialize(
        stremio_lightning_core::logging::LoggingConfig::new(
            stremio_lightning_core::logging::diagnostics_dir_for_platform("macos"),
            stremio_lightning_core::SHELL_VERSION,
            "macos",
            "wkwebview",
            "WKWebView (native hooks deferred)",
        ),
    );
    stremio_lightning_core::logging::info("native.application", "Starting macOS shell");
    let player = MpvPlayerBackend::default();
    let streaming_server =
        StreamingServer::new(RealProcessSpawner).with_disabled(config.disable_streaming_server);
    let host = Arc::new(Host::new(player.clone(), streaming_server));
    if !config.disable_streaming_server {
        if let Err(error) = host.start_streaming_server() {
            stremio_lightning_core::logging::error(
                "native.streaming-server",
                format!("[StreamingServer] Failed to start macOS sidecar: {error}"),
            );
        }
    }
    let injection = InjectionBundle::load()?;

    let runtime = MacosWebviewRuntime::new(config.url.clone(), config.devtools, injection, host);
    if config.headless_bootstrap {
        runtime.bootstrap_headless().map(|_| ())
    } else {
        run_native_window(config, runtime, player)
    }
}

pub fn uses_streaming_server_proxy(url: &str) -> bool {
    url.starts_with("http://127.0.0.1:11470/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shares_startup_flags_with_core() {
        let config = parse_args([
            "stremio-lightning-macos",
            "--url=https://localhost:5173/",
            "--headless-bootstrap",
        ]);
        assert_eq!(config.url, "https://localhost:5173/");
        assert!(config.devtools);
        assert!(config.headless_bootstrap);
        assert_eq!(parse_args(["stremio-lightning-macos"]).url, DEFAULT_URL);
    }

    #[test]
    fn detects_streaming_server_proxy_urls() {
        assert!(uses_streaming_server_proxy(DEFAULT_URL));
        assert!(!uses_streaming_server_proxy("https://web.stremio.com/"));
        assert!(!uses_streaming_server_proxy("http://localhost:11470/"));
    }

    #[test]
    fn accepts_no_streaming_server() {
        let config = parse_args(["stremio-lightning-macos", "--no-streaming-server"]);
        assert!(config.disable_streaming_server);
    }
}
