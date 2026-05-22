use crate::host::Host;
use crate::native_window::run_native_window;
use crate::player::MpvPlayerBackend;
use crate::render::RenderLoopPlan;
use crate::streaming_server::{RealProcessSpawner, StreamingServer};
use crate::webview_runtime::{InjectionBundle, WebviewRuntime};
use std::sync::Arc;

pub const DEFAULT_URL: &str = "http://127.0.0.1:11470/proxy/d=https%3A%2F%2Fweb.stremio.com/";
const STREMIO_WEB_URL: &str = "https://web.stremio.com/";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebviewEngine {
    WebKit,
    Servo,
}

impl Default for WebviewEngine {
    fn default() -> Self {
        Self::WebKit
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppConfig {
    pub url: String,
    pub devtools: bool,
    pub headless_bootstrap: bool,
    pub engine: WebviewEngine,
}

pub type ShellSettings = AppConfig;

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            url: DEFAULT_URL.to_string(),
            devtools: true,
            headless_bootstrap: false,
            engine: WebviewEngine::default(),
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
        } else if arg == "--engine" {
            if let Some(eng) = args.next() {
                config.engine = match eng.as_str() {
                    "servo" => WebviewEngine::Servo,
                    _ => WebviewEngine::WebKit,
                };
            }
        } else if let Some(eng) = arg.strip_prefix("--engine=") {
            config.engine = match eng {
                "servo" => WebviewEngine::Servo,
                _ => WebviewEngine::WebKit,
            };
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
        StreamingServer::new(RealProcessSpawner),
    ));
    if let Err(error) = host.start_streaming_server() {
        eprintln!("[StreamingServer] Failed to start Linux sidecar: {error}");
    }

    let _render_plan = RenderLoopPlan::default();

    if config.headless_bootstrap {
        return Ok(());
    }

    match config.engine {
        WebviewEngine::WebKit => {
            let injection = InjectionBundle::load()?;
            let webview =
                WebviewRuntime::new(config.url.clone(), config.devtools, injection, host.clone());
            setup_signal_handler(host);
            run_native_window(config, webview, player)
        }
        WebviewEngine::Servo => run_servo_engine(config, host, player),
    }
}

fn run_servo_engine(
    _config: AppConfig,
    _host: Arc<Host<MpvPlayerBackend, RealProcessSpawner>>,
    _player: MpvPlayerBackend,
) -> Result<(), String> {
    #[cfg(feature = "servo-engine")]
    {
        use crate::servo_runtime::{run_servo_window, ServoWebviewRuntime};

        let injection = InjectionBundle::load_for_servo()?;
        let runtime = ServoWebviewRuntime::new(
            _config.url.clone(),
            _config.devtools,
            injection,
            _host.clone(),
        );
        setup_signal_handler(_host);
        run_servo_window(runtime)
    }

    #[cfg(not(feature = "servo-engine"))]
    {
        Err(
            "Servo engine is not available. Run with: \
             cargo run -p stremio-lightning-linux --features servo-engine -- --engine=servo"
                .to_string(),
        )
    }
}

fn setup_signal_handler(host: Arc<Host<MpvPlayerBackend, RealProcessSpawner>>) {
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to create signal runtime");
        runtime.block_on(async {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigint = signal(SignalKind::interrupt()).expect("Failed to register SIGINT");
            let mut sigterm = signal(SignalKind::terminate()).expect("Failed to register SIGTERM");
            tokio::select! {
                _ = sigint.recv() => {
                    eprintln!("[StremioLightning] Received SIGINT (Ctrl+C), shutting down sidecar...");
                }
                _ = sigterm.recv() => {
                    eprintln!("[StremioLightning] Received SIGTERM (Taskbar Close), shutting down sidecar...");
                }
            }
            if let Err(error) = host.shutdown() {
                eprintln!("[StremioLightning] Failed to shut down Linux runtime: {error}");
            }
            std::process::exit(0);
        });
    });
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

    #[test]
    fn parses_engine_selection() {
        let config = parse_args(["stremio-lightning-linux", "--engine", "servo"]);
        assert_eq!(config.engine, WebviewEngine::Servo);

        let config2 = parse_args(["stremio-lightning-linux", "--engine=webkit"]);
        assert_eq!(config2.engine, WebviewEngine::WebKit);

        let config3 = parse_args(["stremio-lightning-linux"]);
        assert_eq!(config3.engine, WebviewEngine::WebKit);
    }
}
