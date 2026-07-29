use crate::app::AppConfig;
use crate::player::{MpvPlayerBackend, PlayerBackend};
use crate::streaming_server::RealProcessSpawner;
use crate::webview_runtime::{MacosWebviewRuntime, WebviewLoadState};
#[cfg(any(target_os = "macos", test))]
use serde_json::Value;
#[cfg(any(target_os = "macos", test))]
use stremio_lightning_core::host_api::IpcRequest;

pub const IPC_HANDLER_NAME: &str = "ipc";
pub const DEFAULT_WINDOW_WIDTH: f64 = 1500.0;
pub const DEFAULT_WINDOW_HEIGHT: f64 = 850.0;

#[derive(Debug, Clone, PartialEq)]
pub struct NativeWindowPlan {
    pub width: f64,
    pub height: f64,
    pub min_width: f64,
    pub min_height: f64,
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
            min_width: 800.0,
            min_height: 600.0,
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
        if self.min_width <= 0.0 || self.min_height <= 0.0 {
            return Err("macOS native window minimum dimensions must be positive".to_string());
        }
        if self.min_width > self.width || self.min_height > self.height {
            return Err(
                "macOS native window minimum dimensions cannot exceed initial dimensions"
                    .to_string(),
            );
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

#[cfg(any(target_os = "macos", test))]
fn dispatch_ipc_message(
    runtime: &MacosWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>,
    raw: &str,
) -> Result<(u64, Result<Value, String>), String> {
    let request = serde_json::from_str::<IpcRequest>(raw)
        .map_err(|error| format!("Invalid macOS WKWebView IPC message: {error}"))?;
    Ok((
        request.id,
        runtime.dispatch_ipc(&request.kind, request.payload),
    ))
}

#[cfg(any(target_os = "macos", test))]
fn resolve_ipc_script(id: u64, response: Result<Value, String>) -> String {
    match response {
        Ok(value) => {
            format!("window.__STREMIO_LIGHTNING_MACOS_RESOLVE__({id}, {value}, null);")
        }
        Err(error) => format!(
            "window.__STREMIO_LIGHTNING_MACOS_RESOLVE__({id}, null, {});",
            Value::String(error)
        ),
    }
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
    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2::{define_class, msg_send, DefinedClass, MainThreadOnly};
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSBackingStoreType,
        NSColor, NSWindow, NSWindowStyleMask,
    };
    use objc2_foundation::{
        MainThreadMarker, NSJSONSerialization, NSJSONWritingOptions, NSNotification, NSObject,
        NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSURLRequest, NSURL,
    };
    use objc2_web_kit::{
        WKScriptMessage, WKScriptMessageHandler, WKUserContentController, WKUserScript,
        WKUserScriptInjectionTime, WKWebView, WKWebViewConfiguration,
    };
    use std::cell::OnceCell;

    struct AppDelegateIvars {
        runtime: MacosWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>,
        _player: MpvPlayerBackend,
        url: Retained<NSURL>,
        window: OnceCell<Retained<NSWindow>>,
        webview: OnceCell<Retained<WKWebView>>,
    }

    define_class!(
        #[unsafe(super = NSObject)]
        #[thread_kind = MainThreadOnly]
        #[ivars = AppDelegateIvars]
        struct AppDelegate;

        unsafe impl NSObjectProtocol for AppDelegate {}

        unsafe impl WKScriptMessageHandler for AppDelegate {
            #[unsafe(method(userContentController:didReceiveScriptMessage:))]
            fn did_receive_script_message(
                &self,
                _user_content_controller: &WKUserContentController,
                message: &WKScriptMessage,
            ) {
                if let Err(error) = self.handle_script_message(message) {
                    eprintln!("[WKWebView IPC] {error}");
                }
            }
        }

        unsafe impl NSApplicationDelegate for AppDelegate {
            #[unsafe(method(applicationDidFinishLaunching:))]
            fn did_finish_launching(&self, _notification: &NSNotification) {
                let plan = NativeWindowPlan::default();
                let user_content_controller = unsafe { WKUserContentController::new(self.mtm()) };
                for name in self.ivars().runtime.load_state().document_start_scripts {
                    let source = self
                        .ivars()
                        .runtime
                        .script_source(name)
                        .expect("configured bridge script must exist");
                    let script = unsafe {
                        WKUserScript::initWithSource_injectionTime_forMainFrameOnly(
                            WKUserScript::alloc(self.mtm()),
                            &NSString::from_str(&source),
                            WKUserScriptInjectionTime::AtDocumentStart,
                            false,
                        )
                    };
                    unsafe { user_content_controller.addUserScript(&script) };
                }

                let handler: &ProtocolObject<dyn WKScriptMessageHandler> =
                    ProtocolObject::from_ref(self);
                unsafe {
                    user_content_controller.addScriptMessageHandler_name(
                        handler,
                        &NSString::from_str(plan.ipc_handler),
                    );
                }

                let webview_configuration = unsafe { WKWebViewConfiguration::new(self.mtm()) };
                unsafe { webview_configuration.setUserContentController(&user_content_controller) };
                let webview = unsafe {
                    WKWebView::initWithFrame_configuration(
                        WKWebView::alloc(self.mtm()),
                        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(plan.width, plan.height)),
                        &webview_configuration,
                    )
                };
                unsafe {
                    webview.setUnderPageBackgroundColor(Some(&NSColor::clearColor()));
                }

                let window = unsafe {
                    NSWindow::initWithContentRect_styleMask_backing_defer(
                        NSWindow::alloc(self.mtm()),
                        NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(plan.width, plan.height)),
                        NSWindowStyleMask::Titled
                            | NSWindowStyleMask::Closable
                            | NSWindowStyleMask::Miniaturizable
                            | NSWindowStyleMask::Resizable,
                        NSBackingStoreType::Buffered,
                        false,
                    )
                };
                unsafe { window.setReleasedWhenClosed(false) };
                window.setTitle(&NSString::from_str(plan.title));
                window.setContentMinSize(NSSize::new(plan.min_width, plan.min_height));
                window.setContentView(Some(&webview));
                window.center();
                self.ivars()
                    .window
                    .set(window)
                    .expect("application window must only be created once");
                self.ivars()
                    .webview
                    .set(webview)
                    .expect("application webview must only be created once");

                let webview = self.ivars().webview.get().expect("webview must exist");
                let navigation = if self.ivars().url.isFileURL() {
                    let read_access = self
                        .ivars()
                        .url
                        .URLByDeletingLastPathComponent()
                        .unwrap_or_else(|| self.ivars().url.clone());
                    unsafe {
                        webview.loadFileURL_allowingReadAccessToURL(&self.ivars().url, &read_access)
                    }
                } else {
                    let request = NSURLRequest::requestWithURL(&self.ivars().url);
                    unsafe { webview.loadRequest(&request) }
                };
                if navigation.is_none() {
                    eprintln!("[WKWebView] Failed to start loading the configured web UI");
                }
                self.ivars()
                    .window
                    .get()
                    .expect("window must exist")
                    .makeKeyAndOrderFront(None);

                let app = NSApplication::sharedApplication(self.mtm());
                let _ = app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
                #[allow(deprecated)]
                app.activateIgnoringOtherApps(true);
            }

            #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
            fn should_terminate_after_last_window_closed(&self, _app: &NSApplication) -> bool {
                true
            }
        }
    );

    impl AppDelegate {
        fn new(
            mtm: MainThreadMarker,
            runtime: MacosWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>,
            player: MpvPlayerBackend,
        ) -> Result<Retained<Self>, String> {
            let url_string = runtime.load_state().url;
            let url = NSURL::URLWithString(&NSString::from_str(&url_string))
                .ok_or_else(|| format!("Invalid macOS webview URL: {url_string}"))?;
            let this = Self::alloc(mtm).set_ivars(AppDelegateIvars {
                runtime,
                _player: player,
                url,
                window: OnceCell::new(),
                webview: OnceCell::new(),
            });
            Ok(unsafe { msg_send![super(this), init] })
        }

        fn handle_script_message(&self, message: &WKScriptMessage) -> Result<(), String> {
            let frame = unsafe { message.frameInfo() };
            if !unsafe { frame.isMainFrame() } {
                return Err("Rejected macOS WKWebView IPC from a subframe".to_string());
            }
            let url = unsafe { frame.request() }
                .URL()
                .and_then(|url| url.absoluteString())
                .map(|url| url.to_string())
                .ok_or_else(|| "Rejected macOS WKWebView IPC without a frame URL".to_string())?;
            if decide_navigation_policy(&url, true) != NavigationDecision::Allow {
                return Err(format!(
                    "Rejected macOS WKWebView IPC from untrusted URL: {url}"
                ));
            }

            let body = unsafe { message.body() };
            let data = unsafe {
                NSJSONSerialization::dataWithJSONObject_options_error(
                    &body,
                    NSJSONWritingOptions::FragmentsAllowed,
                )
            }
            .map_err(|error| format!("Invalid macOS WKWebView IPC body: {error}"))?;
            let raw = String::from_utf8(data.to_vec())
                .map_err(|error| format!("Invalid macOS WKWebView IPC UTF-8: {error}"))?;
            let (id, response) = dispatch_ipc_message(&self.ivars().runtime, &raw)?;
            let webview = unsafe { message.webView() }
                .ok_or_else(|| "macOS WKWebView IPC message has no webview".to_string())?;
            evaluate_javascript(&webview, &resolve_ipc_script(id, response));
            for script in self.ivars().runtime.drain_event_dispatch_scripts()? {
                evaluate_javascript(&webview, &script);
            }
            Ok(())
        }

        fn remove_script_message_handler(&self) {
            if let Some(webview) = self.ivars().webview.get() {
                let controller = unsafe { webview.configuration().userContentController() };
                unsafe {
                    controller
                        .removeScriptMessageHandlerForName(&NSString::from_str(IPC_HANDLER_NAME));
                }
            }
        }
    }

    fn evaluate_javascript(webview: &WKWebView, script: &str) {
        unsafe {
            webview.evaluateJavaScript_completionHandler(&NSString::from_str(script), None);
        }
    }

    pub fn run(
        _config: AppConfig,
        runtime: MacosWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>,
        player: MpvPlayerBackend,
    ) -> Result<(), String> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| "macOS AppKit must run on the main thread".to_string())?;
        let app = NSApplication::sharedApplication(mtm);
        let delegate = AppDelegate::new(mtm, runtime, player)?;
        app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));

        app.run();
        delegate.remove_script_message_handler();
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
        assert_eq!(plan.min_width, 800.0);
        assert_eq!(plan.min_height, 600.0);
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
            StreamingServer::new(RealProcessSpawner),
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

    #[test]
    fn ipc_message_routes_to_host_and_formats_javascript_responses() {
        let player = MpvPlayerBackend::default();
        let host = Arc::new(Host::new(player, StreamingServer::new(RealProcessSpawner)));
        let runtime = MacosWebviewRuntime::new(
            "https://web.stremio.com/",
            false,
            InjectionBundle::load().unwrap(),
            host,
        );

        let (id, response) = dispatch_ipc_message(
            &runtime,
            r#"{"id":7,"kind":"invoke","payload":{"command":"init"}}"#,
        )
        .unwrap();
        assert_eq!(id, 7);
        let script = resolve_ipc_script(id, response);
        assert!(script.starts_with("window.__STREMIO_LIGHTNING_MACOS_RESOLVE__(7, {"));
        assert!(script.contains("\"platform\":\"macos\""));

        let (id, response) =
            dispatch_ipc_message(&runtime, r#"{"id":8,"kind":"unknown"}"#).unwrap();
        assert_eq!(
            resolve_ipc_script(id, response),
            "window.__STREMIO_LIGHTNING_MACOS_RESOLVE__(8, null, \"Unsupported IPC kind: unknown\");"
        );
        assert!(dispatch_ipc_message(&runtime, "not json")
            .unwrap_err()
            .starts_with("Invalid macOS WKWebView IPC message:"));
    }
}
