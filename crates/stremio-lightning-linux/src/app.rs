use crate::host::Host;
use crate::native_window::run_native_window;
use crate::player::MpvPlayerBackend;
use crate::render::RenderLoopPlan;
use crate::streaming_server::{RealProcessSpawner, StreamingServer};
use crate::webview_runtime::{InjectionBundle, WebviewRuntime};
use std::sync::Arc;

pub const DEFAULT_URL: &str = "http://127.0.0.1:11470/proxy/d=https%3A%2F%2Fweb.stremio.com/";
const STREMIO_WEB_URL: &str = "https://web.stremio.com/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub url: String,
    pub devtools: bool,
    pub headless_bootstrap: bool,
}

pub type ShellSettings = AppConfig;

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            url: DEFAULT_URL.to_string(),
            devtools: true,
            headless_bootstrap: false,
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
        }
    }

    config
}

fn normalize_startup_url(url: &str) -> String {
    if url.trim_end_matches('/') == STREMIO_WEB_URL.trim_end_matches('/') {
        DEFAULT_URL.to_string()
    } else {
        url.to_string()
    }
}

pub fn run(config: AppConfig) -> Result<(), String> {
    let player = MpvPlayerBackend::default();
    let host = Arc::new(Host::new(
        player.clone(),
        StreamingServer::new(RealProcessSpawner::default()),
    ));
    match host.start_streaming_server() {
        Ok(()) => println!("[StreamingServer] Linux sidecar spawned"),
        Err(error) => eprintln!("[StreamingServer] Failed to start Linux sidecar: {error}"),
    }

    let injection = InjectionBundle::load()?;
    let render_plan = RenderLoopPlan::default();

    println!(
        "[StremioLightning] Linux shell contract bootstrap url={} devtools={} native_player={}",
        config.url,
        config.devtools,
        host.native_player_status().enabled
    );
    println!(
        "[StremioLightning] Injection order: {}",
        injection.script_names().join(" -> ")
    );
    println!(
        "[StremioLightning] Render order: {}",
        render_plan.steps.join(" -> ")
    );

    if config.headless_bootstrap {
        Ok(())
    } else {
        let webview = WebviewRuntime::new(config.url.clone(), config.devtools, injection, host);
        run_native_window(config, webview, player)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_streaming_server_proxy() {
        let config = parse_args(["stremio-lightning-linux"]);
        assert_eq!(config.url, DEFAULT_URL);
        assert!(config.devtools);
    }

    #[test]
    fn accepts_devtools_flag_for_compatibility() {
        let config = parse_args(["stremio-lightning-linux", "--devtools"]);
        assert!(config.devtools);
    }

    #[test]
    fn accepts_developer_url() {
        let config = parse_args(["stremio-lightning-linux", "--url", "file:///tmp/smoke.html"]);
        assert_eq!(config.url, "file:///tmp/smoke.html");
    }

    #[test]
    fn normalizes_direct_stremio_web_url_to_local_proxy() {
        let config = parse_args([
            "stremio-lightning-linux",
            "--url",
            "https://web.stremio.com/",
        ]);
        assert_eq!(config.url, DEFAULT_URL);
    }
}
