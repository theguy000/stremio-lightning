use crate::host::Host;
use crate::native_window::run_native_window;
use crate::player::MpvPlayerBackend;
use crate::streaming_server::{RealProcessSpawner, StreamingServer};
use crate::webview_runtime::{InjectionBundle, MacosWebviewRuntime};
use std::sync::Arc;

pub const DEFAULT_URL: &str = "http://127.0.0.1:11470/proxy/d=https%3A%2F%2Fweb.stremio.com/";
pub const STREMIO_WEB_URL: &str = "https://web.stremio.com/";

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
            devtools: std::env::var("STREMIO_LIGHTNING_MACOS_DEVTOOLS")
                .ok()
                .as_deref()
                == Some("1"),
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
    let mut args = args.into_iter().map(Into::into).skip(1);

    while let Some(arg) = args.next() {
        if arg == "--url" {
            if let Some(url) = args.next() {
                config.url = normalize_startup_url(&url);
            }
        } else if let Some(url) = arg.strip_prefix("--url=") {
            config.url = normalize_startup_url(url);
        } else if arg == "--devtools" {
            config.devtools = true;
        } else if arg == "--headless-bootstrap" {
            config.headless_bootstrap = true;
        } else if arg == "--no-streaming-server" {
            config.disable_streaming_server = true;
        }
    }

    config
}

pub fn normalize_startup_url(url: &str) -> String {
    if url.trim_end_matches('/') == STREMIO_WEB_URL.trim_end_matches('/') {
        DEFAULT_URL.to_string()
    } else {
        url.to_string()
    }
}

pub fn run(config: AppConfig) -> Result<(), String> {
    let player = MpvPlayerBackend::default();
    let streaming_server = StreamingServer::new(RealProcessSpawner::default())
        .with_disabled(config.disable_streaming_server);
    let host = Arc::new(Host::new(player.clone(), streaming_server));
    if config.disable_streaming_server {
        println!("[StreamingServer] macOS sidecar disabled");
    } else {
        match host.start_streaming_server() {
            Ok(()) => println!("[StreamingServer] macOS sidecar spawned"),
            Err(error) => eprintln!("[StreamingServer] Failed to start macOS sidecar: {error}"),
        }
    }
    let injection = InjectionBundle::load()?;

    println!(
        "[StremioLightning] macOS shell contract bootstrap url={} devtools={} native_player={}",
        config.url,
        config.devtools,
        host.native_player_status().enabled
    );
    println!(
        "[StremioLightning] Injection order: {}",
        injection.script_names().join(" -> ")
    );

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
    fn defaults_to_streaming_server_proxy() {
        let config = parse_args(["stremio-lightning-macos"]);
        assert_eq!(config.url, DEFAULT_URL);
        assert!(!config.devtools);
        assert!(!config.headless_bootstrap);
    }

    #[test]
    fn accepts_developer_url() {
        let config = parse_args(["stremio-lightning-macos", "--url", "file:///tmp/smoke.html"]);
        assert_eq!(config.url, "file:///tmp/smoke.html");
    }

    #[test]
    fn accepts_equals_url_and_devtools() {
        let config = parse_args([
            "stremio-lightning-macos",
            "--url=https://localhost:5173/",
            "--devtools",
        ]);
        assert_eq!(config.url, "https://localhost:5173/");
        assert!(config.devtools);
    }

    #[test]
    fn normalizes_direct_stremio_web_url_to_local_proxy() {
        let config = parse_args([
            "stremio-lightning-macos",
            "--url",
            "https://web.stremio.com/",
        ]);
        assert_eq!(config.url, DEFAULT_URL);
    }

    #[test]
    fn accepts_headless_bootstrap() {
        let config = parse_args(["stremio-lightning-macos", "--headless-bootstrap"]);
        assert!(config.headless_bootstrap);
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
