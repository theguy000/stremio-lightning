use crate::app_integration::{launch_intent_from_args, LaunchIntent};
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
    pub launch_intent: LaunchIntent,
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
            launch_intent: LaunchIntent::Focus,
        }
    }
}

pub fn parse_args<I, S>(args: I) -> Result<AppConfig, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    let mut config = AppConfig::default();
    config.launch_intent = launch_intent_from_args(args.iter().skip(1))?;
    let mut args = args.into_iter().skip(1);

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

    Ok(config)
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
    let streaming_server =
        StreamingServer::new(RealProcessSpawner).with_disabled(config.disable_streaming_server);
    let host = Arc::new(Host::new(player.clone(), streaming_server));
    start_streaming_server_for_url(&config.url, config.disable_streaming_server, || {
        host.start_streaming_server()
    })?;
    let injection = InjectionBundle::load()?;

    let runtime = MacosWebviewRuntime::new(config.url.clone(), config.devtools, injection, host);
    if config.headless_bootstrap {
        runtime.bootstrap_headless().map(|_| ())
    } else {
        run_native_window(config, runtime, player)
    }
}

pub fn uses_streaming_server_proxy(url: &str) -> bool {
    url == "http://127.0.0.1:11470" || url.starts_with("http://127.0.0.1:11470/")
}

fn start_streaming_server_for_url(
    url: &str,
    disabled: bool,
    start: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if disabled {
        return (!uses_streaming_server_proxy(url))
            .then_some(())
            .ok_or_else(|| "macOS local proxy requires the streaming server".to_string());
    }

    if let Err(error) = start() {
        if uses_streaming_server_proxy(url) {
            return Err(format!(
                "Failed to start required macOS streaming server sidecar: {error}"
            ));
        }
        eprintln!("[StreamingServer] Failed to start macOS sidecar: {error}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_streaming_server_proxy() {
        let config = parse_args(["stremio-lightning-macos"]).unwrap();
        assert_eq!(config.url, DEFAULT_URL);
        assert!(config.devtools);
        assert!(!config.headless_bootstrap);
    }

    #[test]
    fn accepts_developer_url() {
        let config =
            parse_args(["stremio-lightning-macos", "--url", "file:///tmp/smoke.html"]).unwrap();
        assert_eq!(config.url, "file:///tmp/smoke.html");
    }

    #[test]
    fn accepts_equals_url_and_devtools() {
        let config = parse_args([
            "stremio-lightning-macos",
            "--url=https://localhost:5173/",
            "--devtools",
        ])
        .unwrap();
        assert_eq!(config.url, "https://localhost:5173/");
        assert!(config.devtools);
    }

    #[test]
    fn normalizes_direct_stremio_web_url_to_local_proxy() {
        let config = parse_args([
            "stremio-lightning-macos",
            "--url",
            "https://web.stremio.com/",
        ])
        .unwrap();
        assert_eq!(config.url, DEFAULT_URL);
    }

    #[test]
    fn accepts_headless_bootstrap() {
        let config = parse_args(["stremio-lightning-macos", "--headless-bootstrap"]).unwrap();
        assert!(config.headless_bootstrap);
    }

    #[test]
    fn detects_streaming_server_proxy_urls() {
        assert!(uses_streaming_server_proxy(DEFAULT_URL));
        assert!(uses_streaming_server_proxy("http://127.0.0.1:11470"));
        assert!(!uses_streaming_server_proxy("https://web.stremio.com/"));
        assert!(!uses_streaming_server_proxy("http://localhost:11470/"));
    }

    #[test]
    fn requires_sidecar_only_for_local_proxy() {
        assert!(
            start_streaming_server_for_url(DEFAULT_URL, false, || Err("boom".to_string())).is_err()
        );
        assert!(start_streaming_server_for_url(STREMIO_WEB_URL, false, || {
            Err("boom".to_string())
        })
        .is_ok());
        assert!(start_streaming_server_for_url(DEFAULT_URL, true, || {
            panic!("disabled server must not start")
        })
        .is_err());
    }

    #[test]
    fn accepts_no_streaming_server() {
        let config = parse_args(["stremio-lightning-macos", "--no-streaming-server"]).unwrap();
        assert!(config.disable_streaming_server);
    }

    #[test]
    fn captures_initial_launch_intent_without_treating_url_options_as_intents() {
        let config = parse_args([
            "stremio-lightning-macos",
            "--url",
            "https://localhost:5173/",
            "magnet:?xt=urn:btih:test",
        ])
        .unwrap();

        assert_eq!(
            config.launch_intent,
            LaunchIntent::Magnet("magnet:?xt=urn:btih:test".to_string())
        );
    }
}
