use crate::cef::InjectionBundle;
use crate::host::LinuxHost;
use crate::native_window::run_native_window;
use crate::player::MpvPlayerBackend;
use crate::render::RenderLoopPlan;
use crate::streaming_server::{RealProcessSpawner, StreamingServer};

pub const DEFAULT_URL: &str = "https://web.stremio.com/";

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
            devtools: std::env::var("STREMIO_LIGHTNING_LINUX_DEVTOOLS")
                .ok()
                .as_deref()
                == Some("1"),
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
                config.url = url;
            }
        } else if let Some(url) = arg.strip_prefix("--url=") {
            config.url = url.to_string();
        } else if arg == "--devtools" {
            config.devtools = true;
        } else if arg == "--headless-bootstrap" {
            config.headless_bootstrap = true;
        }
    }

    config
}

pub fn run(config: AppConfig) -> Result<(), String> {
    let host = LinuxHost::new(
        MpvPlayerBackend::default(),
        StreamingServer::new(RealProcessSpawner::default()),
    );
    let injection = InjectionBundle::load()?;
    let render_plan = RenderLoopPlan::default();

    println!(
        "[StremioLightning] Linux shell prototype url={} devtools={} native_player={}",
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
        host.invoke("start_streaming_server", None)?;
        run_native_window(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_stremio_web() {
        let config = parse_args(["stremio-lightning-linux"]);
        assert_eq!(config.url, DEFAULT_URL);
    }

    #[test]
    fn accepts_developer_url() {
        let config = parse_args(["stremio-lightning-linux", "--url", "file:///tmp/smoke.html"]);
        assert_eq!(config.url, "file:///tmp/smoke.html");
    }
}
