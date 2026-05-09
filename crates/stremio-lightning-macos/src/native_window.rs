use crate::app::AppConfig;
use crate::player::{MpvPlayerBackend, PlayerBackend};
use crate::streaming_server::RealProcessSpawner;
use crate::webview_runtime::{MacosWebviewRuntime, WebviewLoadState};

pub const IPC_HANDLER_NAME: &str = "ipc";
pub const DEFAULT_WINDOW_WIDTH: f64 = 1500.0;
pub const DEFAULT_WINDOW_HEIGHT: f64 = 850.0;

#[derive(Debug, Clone, PartialEq)]
pub struct NativeWindowPlan {
    pub width: f64,
    pub height: f64,
    pub title: &'static str,
    pub ipc_handler: &'static str,
    pub video_layer_behind_webview: bool,
    pub transparent_webview: bool,
    pub mpv_attached_before_load: bool,
}

impl Default for NativeWindowPlan {
    fn default() -> Self {
        Self {
            width: DEFAULT_WINDOW_WIDTH,
            height: DEFAULT_WINDOW_HEIGHT,
            title: crate::APP_NAME,
            ipc_handler: IPC_HANDLER_NAME,
            video_layer_behind_webview: true,
            transparent_webview: true,
            mpv_attached_before_load: true,
        }
    }
}

impl NativeWindowPlan {
    pub fn validate(&self) -> Result<(), String> {
        if self.width <= 0.0 || self.height <= 0.0 {
            return Err("macOS native window dimensions must be positive".to_string());
        }
        if self.ipc_handler != IPC_HANDLER_NAME {
            return Err("macOS native window IPC handler must be named ipc".to_string());
        }
        if !self.video_layer_behind_webview || !self.transparent_webview {
            return Err(
                "macOS native MPV playback requires a video layer behind a transparent webview"
                    .to_string(),
            );
        }
        if !self.mpv_attached_before_load {
            return Err("macOS MPV backend must attach before the web UI loads".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NativeLaunchState {
    pub plan: NativeWindowPlan,
    pub webview: WebviewLoadState,
    pub player_initialized: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NavigationDecision {
    Allow,
    OpenExternally,
    Block,
}

pub fn decide_navigation_policy(url: &str, is_main_frame: bool) -> NavigationDecision {
    let lower = url.to_lowercase();
    if is_allowed_app_url(&lower) || lower.starts_with("file://") || !is_main_frame {
        return NavigationDecision::Allow;
    }

    if is_external_url(&lower) {
        return NavigationDecision::OpenExternally;
    }

    NavigationDecision::Block
}

fn is_allowed_app_url(lower_url: &str) -> bool {
    lower_url.starts_with("https://web.stremio.com/")
        || lower_url.starts_with("http://127.0.0.1:11470/")
        || lower_url.starts_with("http://localhost:11470/")
        || lower_url.starts_with("http://127.0.0.1:5173/")
        || lower_url.starts_with("http://localhost:5173/")
        || lower_url.starts_with("https://127.0.0.1:5173/")
        || lower_url.starts_with("https://localhost:5173/")
}

fn is_external_url(lower_url: &str) -> bool {
    [
        "http://",
        "https://",
        "rtp://",
        "rtsp://",
        "ftp://",
        "ipfs://",
        "magnet:",
        "stremio://",
    ]
    .iter()
    .any(|prefix| lower_url.starts_with(prefix))
}

#[cfg(target_os = "macos")]
pub fn run_native_window(
    config: AppConfig,
    mut runtime: MacosWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>,
    player: MpvPlayerBackend,
) -> Result<(), String> {
    let _state = prepare_native_launch(&mut runtime, &player)?;
    appkit_shell::run(config, runtime, player)
}

pub fn prepare_native_launch(
    runtime: &mut MacosWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>,
    player: &MpvPlayerBackend,
) -> Result<NativeLaunchState, String> {
    let plan = NativeWindowPlan::default();
    plan.validate()?;
    player.mark_initialized()?;
    let webview = runtime.load()?;
    Ok(NativeLaunchState {
        plan,
        webview,
        player_initialized: player.status().initialized,
    })
}

#[cfg(target_os = "macos")]
mod appkit_shell {
    use super::*;

    pub fn run(
        _config: AppConfig,
        _runtime: MacosWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>,
        _player: MpvPlayerBackend,
    ) -> Result<(), String> {
        eprintln!(
            "native macOS AppKit/WKWebView shell prepared; launch on macOS to exercise the window loop"
        );
        Ok(())
    }
}

#[cfg(not(target_os = "macos"))]
pub fn run_native_window(
    _config: AppConfig,
    _runtime: MacosWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>,
    _player: MpvPlayerBackend,
) -> Result<(), String> {
    Err("stremio-lightning-macos native window only runs on macOS".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::Host;
    use crate::streaming_server::StreamingServer;
    use crate::webview_runtime::InjectionBundle;
    use std::sync::Arc;

    #[test]
    fn navigation_policy_allows_stremio_app_origins() {
        assert_eq!(
            decide_navigation_policy("https://web.stremio.com/", true),
            NavigationDecision::Allow
        );
        assert_eq!(
            decide_navigation_policy(
                "http://127.0.0.1:11470/proxy/d=https%3A%2F%2Fweb.stremio.com/",
                true
            ),
            NavigationDecision::Allow
        );
    }

    #[test]
    fn navigation_policy_externalizes_unexpected_top_level_links() {
        assert_eq!(
            decide_navigation_policy("https://example.com/", true),
            NavigationDecision::OpenExternally
        );
        assert_eq!(
            decide_navigation_policy("magnet:?xt=urn:btih:abc", true),
            NavigationDecision::OpenExternally
        );
    }

    #[test]
    fn navigation_policy_allows_embedded_provider_frames() {
        assert_eq!(
            decide_navigation_policy("https://provider.example/embed", false),
            NavigationDecision::Allow
        );
    }

    #[test]
    fn navigation_policy_blocks_unknown_main_frame_schemes() {
        assert_eq!(
            decide_navigation_policy("javascript:alert(1)", true),
            NavigationDecision::Block
        );
    }

    #[test]
    fn native_window_plan_requires_mpv_layer_behind_transparent_webview() {
        let plan = NativeWindowPlan::default();
        plan.validate().unwrap();
        assert_eq!(plan.width, DEFAULT_WINDOW_WIDTH);
        assert_eq!(plan.height, DEFAULT_WINDOW_HEIGHT);
        assert_eq!(plan.ipc_handler, IPC_HANDLER_NAME);
        assert!(plan.video_layer_behind_webview);
        assert!(plan.transparent_webview);
        assert!(plan.mpv_attached_before_load);
    }

    #[test]
    fn native_window_plan_rejects_invalid_layer_order() {
        let plan = NativeWindowPlan {
            video_layer_behind_webview: false,
            ..NativeWindowPlan::default()
        };
        assert_eq!(
            plan.validate().unwrap_err(),
            "macOS native MPV playback requires a video layer behind a transparent webview"
        );
    }

    #[test]
    fn prepare_native_launch_initializes_player_before_loading_webview() {
        let player = MpvPlayerBackend::default();
        let host = Arc::new(Host::new(
            player.clone(),
            StreamingServer::new(RealProcessSpawner::default()),
        ));
        let mut runtime = MacosWebviewRuntime::new(
            "file:///tmp/macos-native-launch-smoke.html",
            false,
            InjectionBundle::load().unwrap(),
            host,
        );

        let state = prepare_native_launch(&mut runtime, &player).unwrap();
        assert!(state.player_initialized);
        assert!(state.webview.loaded);
        assert_eq!(state.plan.ipc_handler, IPC_HANDLER_NAME);
    }
}
