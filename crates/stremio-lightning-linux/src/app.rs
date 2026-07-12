use crate::host::Host;
use crate::native_window::run_native_window;
use crate::player::MpvPlayerBackend;
use crate::render::RenderLoopPlan;
use crate::streaming_server::{RealProcessSpawner, StreamingServer};
use crate::webview_runtime::{InjectionBundle, WebviewRuntime};
use gtk::glib;
use gtk::prelude::*;
use std::sync::Arc;

pub const DEFAULT_URL: &str = "http://127.0.0.1:11470/proxy/d=https%3A%2F%2Fweb.stremio.com/";
const STREMIO_WEB_URL: &str = "https://web.stremio.com/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub url: String,
    pub devtools: bool,
    pub headless_bootstrap: bool,
}

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
    stremio_lightning_core::logging::info("native.application", "Starting Linux shell");
    let player = MpvPlayerBackend::default();
    let host = Arc::new(Host::new(
        player.clone(),
        StreamingServer::new(RealProcessSpawner),
    ));
    if let Err(error) = host.start_streaming_server() {
        stremio_lightning_core::logging::error(
            "native.streaming-server",
            format!("[StreamingServer] Failed to start Linux sidecar: {error}"),
        );
    }

    let injection = InjectionBundle::load()?;
    let _render_plan = RenderLoopPlan::default();

    if config.headless_bootstrap {
        Ok(())
    } else {
        let webview =
            WebviewRuntime::new(config.url.clone(), config.devtools, injection, host.clone());
        setup_signal_handler();
        run_native_window(config, webview, player)
    }
}

fn setup_signal_handler() {
    std::thread::spawn(|| {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create signal runtime");
        runtime.block_on(async {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigint = signal(SignalKind::interrupt()).expect("Failed to register SIGINT");
            let mut sigterm = signal(SignalKind::terminate()).expect("Failed to register SIGTERM");
            let signal_name = tokio::select! {
                _ = sigint.recv() => "SIGINT (Ctrl+C)",
                _ = sigterm.recv() => "SIGTERM",
            };
            stremio_lightning_core::logging::info(
                "native.application",
                format!("[StremioLightning] Received {signal_name}, shutting down..."),
            );
            glib::idle_add_once(request_gtk_shutdown);
        });
    });
}

fn request_gtk_shutdown() {
    let Some(app) = gtk::gio::Application::default() else {
        return;
    };
    let closed = app
        .downcast_ref::<gtk::Application>()
        .and_then(|gtk_app| gtk_app.active_window())
        .map(|window| {
            window.close();
            true
        })
        .unwrap_or(false);
    if !closed {
        app.quit();
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
