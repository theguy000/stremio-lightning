use crate::app::AppConfig;
use crate::player::MpvPlayerBackend;
use crate::streaming_server::RealProcessSpawner;
use crate::webview_runtime::MacosWebviewRuntime;

pub const IPC_HANDLER_NAME: &str = "ipc";
pub const DEFAULT_WINDOW_WIDTH: f64 = 1500.0;
pub const DEFAULT_WINDOW_HEIGHT: f64 = 850.0;

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
    _config: AppConfig,
    mut runtime: MacosWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>,
    _player: MpvPlayerBackend,
) -> Result<(), String> {
    let _state = runtime.load()?;
    eprintln!(
        "native macOS AppKit/WKWebView window is not implemented yet; runtime bootstrap completed"
    );
    Ok(())
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
}
