use crate::app::AppConfig;
use crate::player::{MpvPlayerBackend, PlayerBackend};
use crate::streaming_server::RealProcessSpawner;
use crate::webview_runtime::{MacosWebviewRuntime, WebviewLoadState};
#[cfg(any(target_os = "macos", test))]
use serde_json::{json, Value};
#[cfg(any(target_os = "macos", test))]
use stremio_lightning_core::host_api::{self, IpcRequest, ParsedRequest};

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
fn parse_ipc_message(raw: &str) -> Result<IpcRequest, String> {
    serde_json::from_str(raw)
        .map_err(|error| format!("Invalid macOS WKWebView IPC message: {error}"))
}

#[cfg(any(target_os = "macos", test))]
fn invoke_command(payload: Option<&Value>) -> Option<&str> {
    payload
        .and_then(|value| value.get("command"))
        .and_then(Value::as_str)
}

#[cfg(any(target_os = "macos", test))]
fn shell_transport_fullscreen_request(payload: Option<&Value>) -> Result<Option<bool>, String> {
    if invoke_command(payload) != Some("shell_transport_send") {
        return Ok(None);
    }
    let message = payload
        .and_then(|value| value.get("payload"))
        .and_then(|value| value.get("message"))
        .and_then(Value::as_str)
        .ok_or_else(|| "Missing shell_transport_send message".to_string())?;
    let ParsedRequest::Command { method, data } = host_api::parse_request(message)? else {
        return Ok(None);
    };
    if method != "win-set-visibility" {
        return Ok(None);
    }
    data.as_ref()
        .and_then(|value| value.get("fullscreen"))
        .and_then(Value::as_bool)
        .map(Some)
        .ok_or_else(|| "Invalid win-set-visibility payload".to_string())
}

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PendingPipTransition {
    Enter { width: i32, height: i32 },
    RestoreFullscreen,
}

#[cfg(any(target_os = "macos", test))]
fn schedule_pending_fullscreen_restore(
    transition: &std::cell::Cell<Option<PendingPipTransition>>,
    was_fullscreen: bool,
) -> bool {
    if was_fullscreen && transition.take().is_some() {
        transition.set(Some(PendingPipTransition::RestoreFullscreen));
        true
    } else {
        false
    }
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
    use crate::app_integration::AppLifecycleEvent;
    use crate::host::open_external_url;
    use crate::player::{MacosMpvRenderer, MpvVideoLayerHandle};
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, ProtocolObject};
    use objc2::{define_class, msg_send, sel, DefinedClass, MainThreadOnly, Message};
    use objc2_app_kit::{
        NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate,
        NSApplicationTerminateReply, NSAutoresizingMaskOptions, NSBackingStoreType, NSColor,
        NSFloatingWindowLevel, NSView, NSWindow, NSWindowDelegate, NSWindowLevel,
        NSWindowStyleMask,
    };
    use objc2_foundation::{
        MainThreadMarker, NSError, NSJSONSerialization, NSJSONWritingOptions, NSNotification,
        NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString, NSTimer, NSURLRequest,
        NSURL,
    };
    use objc2_web_kit::{
        WKNavigationAction, WKNavigationActionPolicy, WKNavigationDelegate, WKScriptMessage,
        WKScriptMessageHandler, WKUserContentController, WKUserScript, WKUserScriptInjectionTime,
        WKWebView, WKWebViewConfiguration,
    };
    use std::cell::{Cell, OnceCell, RefCell};
    use stremio_lightning_core::pip::{PipRestoreSnapshot, PipWindowController};

    #[derive(Clone, Copy)]
    struct PipWindowSnapshot {
        frame: NSRect,
        min_size: NSSize,
        style_mask: NSWindowStyleMask,
        level: NSWindowLevel,
        was_zoomed: bool,
    }

    struct AppDelegateIvars {
        runtime: MacosWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>,
        player: MpvPlayerBackend,
        url: Retained<NSURL>,
        renderer: OnceCell<MacosMpvRenderer>,
        window: OnceCell<Retained<NSWindow>>,
        video_layer: OnceCell<Retained<NSView>>,
        webview: OnceCell<Retained<WKWebView>>,
        player_event_timer: OnceCell<Retained<NSTimer>>,
        pip_window: RefCell<Option<PipWindowSnapshot>>,
        pending_pip_transition: Cell<Option<PendingPipTransition>>,
        termination_pending: Cell<bool>,
    }

    define_class!(
        #[unsafe(super = NSObject)]
        #[thread_kind = MainThreadOnly]
        #[ivars = AppDelegateIvars]
        struct AppDelegate;

        impl AppDelegate {
            #[unsafe(method(drainPlayerEvents:))]
            fn drain_player_events_timer(&self, _timer: &NSTimer) {
                if let Err(error) = self.drain_events_to_webview() {
                    eprintln!("[MPV] Failed to drain macOS player events: {error}");
                }
            }

            #[unsafe(method(finishTermination:))]
            fn finish_termination_timer(&self, _timer: &NSTimer) {
                self.finish_termination();
            }
        }

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

        unsafe impl WKNavigationDelegate for AppDelegate {
            #[unsafe(method(webView:decidePolicyForNavigationAction:decisionHandler:))]
            fn decide_policy_for_navigation_action(
                &self,
                _webview: &WKWebView,
                navigation_action: &WKNavigationAction,
                decision_handler: &block2::DynBlock<dyn Fn(WKNavigationActionPolicy)>,
            ) {
                let request = unsafe { navigation_action.request() };
                let url = request
                    .URL()
                    .and_then(|url| url.absoluteString())
                    .map(|url| url.to_string());
                let is_main_frame = unsafe { navigation_action.targetFrame() }
                    .is_none_or(|frame| unsafe { frame.isMainFrame() });
                let policy = match url
                    .as_deref()
                    .map(|url| (url, decide_navigation_policy(url, is_main_frame)))
                {
                    Some((_, NavigationDecision::Allow)) => WKNavigationActionPolicy::Allow,
                    Some((url, NavigationDecision::OpenExternally)) => {
                        if let Err(error) = open_external_url(url) {
                            eprintln!("[WKWebView] Failed to open external URL {url}: {error}");
                        }
                        WKNavigationActionPolicy::Cancel
                    }
                    Some((url, NavigationDecision::Block)) => {
                        eprintln!("[WKWebView] Blocked navigation to {url}");
                        WKNavigationActionPolicy::Cancel
                    }
                    None => {
                        eprintln!("[WKWebView] Blocked navigation without a URL");
                        WKNavigationActionPolicy::Cancel
                    }
                };
                decision_handler.call((policy,));
            }
        }

        unsafe impl NSWindowDelegate for AppDelegate {
            #[unsafe(method(windowDidBecomeKey:))]
            fn did_become_key(&self, _notification: &NSNotification) {
                self.emit_lifecycle_event(AppLifecycleEvent::WindowFocused(true));
            }

            #[unsafe(method(windowDidResignKey:))]
            fn did_resign_key(&self, _notification: &NSNotification) {
                self.emit_lifecycle_event(AppLifecycleEvent::WindowFocused(false));
            }

            #[unsafe(method(windowDidChangeOcclusionState:))]
            fn did_change_occlusion_state(&self, _notification: &NSNotification) {
                if let Some(window) = self.ivars().window.get() {
                    self.emit_lifecycle_event(AppLifecycleEvent::WindowVisible(window.isVisible()));
                }
            }

            #[unsafe(method(windowDidExitFullScreen:))]
            fn did_exit_fullscreen(&self, _notification: &NSNotification) {
                let Some(transition) = self.ivars().pending_pip_transition.take() else {
                    return;
                };
                if let Some(window) = self.ivars().window.get() {
                    match transition {
                        PendingPipTransition::Enter { width, height } => {
                            enter_pip_window(window, &self.ivars().pip_window, width, height);
                        }
                        PendingPipTransition::RestoreFullscreen => {
                            set_native_fullscreen(window, true);
                        }
                    }
                }
            }
        }

        unsafe impl NSApplicationDelegate for AppDelegate {
            #[unsafe(method(applicationDidBecomeActive:))]
            fn did_become_active(&self, _notification: &NSNotification) {
                self.emit_lifecycle_event(AppLifecycleEvent::BecameActive);
            }

            #[unsafe(method(applicationDidResignActive:))]
            fn did_resign_active(&self, _notification: &NSNotification) {
                self.emit_lifecycle_event(AppLifecycleEvent::ResignedActive);
            }

            #[unsafe(method(applicationShouldTerminate:))]
            fn should_terminate(
                &self,
                _application: &NSApplication,
            ) -> NSApplicationTerminateReply {
                if self.ivars().termination_pending.get() {
                    return NSApplicationTerminateReply::TerminateLater;
                }
                if let Err(error) = self.ivars().runtime.shutdown() {
                    eprintln!("[AppKit Lifecycle] Failed to shut down macOS host: {error}");
                }

                let Some((webview, scripts)) = self.event_dispatch_scripts().unwrap_or_else(|error| {
                    eprintln!("[AppKit Lifecycle] Failed to prepare shutdown event: {error}");
                    None
                }) else {
                    return NSApplicationTerminateReply::TerminateNow;
                };
                if scripts.is_empty() {
                    return NSApplicationTerminateReply::TerminateNow;
                }

                self.ivars().termination_pending.set(true);
                // WebKit may not complete during teardown; never leave AppKit waiting forever.
                let timeout = unsafe {
                    NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                        1.0,
                        self,
                        sel!(finishTermination:),
                        None,
                        false,
                    )
                };
                drop(timeout);

                let delegate = self.retain();
                let completion = block2::RcBlock::new(
                    move |_result: *mut AnyObject, _error: *mut NSError| {
                        delegate.finish_termination();
                    },
                );
                unsafe {
                    webview.evaluateJavaScript_completionHandler(
                        &NSString::from_str(&scripts.join("\n")),
                        Some(&completion),
                    );
                }
                NSApplicationTerminateReply::TerminateLater
            }

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
                let navigation_delegate: &ProtocolObject<dyn WKNavigationDelegate> =
                    ProtocolObject::from_ref(self);
                unsafe { webview.setNavigationDelegate(Some(navigation_delegate)) };
                let autoresizing = NSAutoresizingMaskOptions::ViewWidthSizable
                    | NSAutoresizingMaskOptions::ViewHeightSizable;
                webview.setAutoresizingMask(autoresizing);

                let frame =
                    NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(plan.width, plan.height));
                let content_view = NSView::initWithFrame(NSView::alloc(self.mtm()), frame);
                let video_layer = NSView::initWithFrame(NSView::alloc(self.mtm()), frame);
                video_layer.setAutoresizingMask(autoresizing);
                video_layer.setWantsLayer(true);
                content_view.addSubview(&video_layer);

                let window = unsafe {
                    NSWindow::initWithContentRect_styleMask_backing_defer(
                        NSWindow::alloc(self.mtm()),
                        frame,
                        NSWindowStyleMask::Titled
                            | NSWindowStyleMask::Closable
                            | NSWindowStyleMask::Miniaturizable
                            | NSWindowStyleMask::Resizable,
                        NSBackingStoreType::Buffered,
                        false,
                    )
                };
                unsafe { window.setReleasedWhenClosed(false) };
                let window_delegate: &ProtocolObject<dyn NSWindowDelegate> =
                    ProtocolObject::from_ref(self);
                window.setDelegate(Some(window_delegate));
                window.setTitle(&NSString::from_str(plan.title));
                window.setContentMinSize(NSSize::new(plan.min_width, plan.min_height));
                window.setContentView(Some(&content_view));
                window.center();

                let handle = MpvVideoLayerHandle::new(Retained::as_ptr(&video_layer) as usize)
                    .expect("retained macOS video layer must have a valid pointer");
                let renderer = match self
                    .ivars()
                    .player
                    .attach_to_video_layer(handle, crate::APP_NAME)
                {
                    Ok(renderer) => renderer,
                    Err(error) => {
                        eprintln!("[MPV] Failed to attach macOS video layer: {error}");
                        NSApplication::sharedApplication(self.mtm()).terminate(None);
                        return;
                    }
                };
                assert!(
                    self.ivars().renderer.set(renderer).is_ok(),
                    "MPV renderer must only be attached once"
                );
                assert!(
                    self.ivars().video_layer.set(video_layer).is_ok(),
                    "MPV video layer must only be created once"
                );
                content_view.addSubview(&webview);
                self.ivars()
                    .window
                    .set(window)
                    .expect("application window must only be created once");
                self.ivars()
                    .webview
                    .set(webview)
                    .expect("application webview must only be created once");
                let timer = unsafe {
                    NSTimer::scheduledTimerWithTimeInterval_target_selector_userInfo_repeats(
                        0.016,
                        self,
                        sel!(drainPlayerEvents:),
                        None,
                        true,
                    )
                };
                self.ivars()
                    .player_event_timer
                    .set(timer)
                    .expect("player event timer must only be created once");

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
                player,
                url,
                renderer: OnceCell::new(),
                window: OnceCell::new(),
                video_layer: OnceCell::new(),
                webview: OnceCell::new(),
                player_event_timer: OnceCell::new(),
                pip_window: RefCell::new(None),
                pending_pip_transition: Cell::new(None),
                termination_pending: Cell::new(false),
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
            let request = parse_ipc_message(&raw)?;
            let id = request.id;
            let response = self.dispatch_native_ipc(&request.kind, request.payload);
            let webview = unsafe { message.webView() }
                .ok_or_else(|| "macOS WKWebView IPC message has no webview".to_string())?;
            evaluate_javascript(&webview, &resolve_ipc_script(id, response));
            self.drain_events_to_webview()
        }

        fn emit_lifecycle_event(&self, event: AppLifecycleEvent) {
            let result = self
                .ivars()
                .runtime
                .host()
                .emit_lifecycle_event(event)
                .and_then(|_| self.drain_events_to_webview());
            if let Err(error) = result {
                eprintln!("[AppKit Lifecycle] Failed to emit {event:?}: {error}");
            }
        }

        fn finish_termination(&self) {
            if self.ivars().termination_pending.replace(false) {
                NSApplication::sharedApplication(self.mtm())
                    .replyToApplicationShouldTerminate(true);
            }
        }

        fn event_dispatch_scripts(&self) -> Result<Option<(&WKWebView, Vec<String>)>, String> {
            let Some(window) = self.ivars().window.get() else {
                return Ok(None);
            };
            let Some(webview) = self.ivars().webview.get() else {
                return Ok(None);
            };
            let mut controller = self.window_controller(window);
            let scripts = self
                .ivars()
                .runtime
                .drain_event_dispatch_scripts_with_pip_controller(&mut controller)?;
            Ok(Some((webview, scripts)))
        }

        fn drain_events_to_webview(&self) -> Result<(), String> {
            let Some((webview, scripts)) = self.event_dispatch_scripts()? else {
                return Ok(());
            };
            for script in scripts {
                evaluate_javascript(&webview, &script);
            }
            Ok(())
        }

        fn dispatch_native_ipc(&self, kind: &str, payload: Option<Value>) -> Result<Value, String> {
            let window = self
                .ivars()
                .window
                .get()
                .ok_or_else(|| "macOS native window is not initialized".to_string())?;
            let webview = self
                .ivars()
                .webview
                .get()
                .ok_or_else(|| "macOS WKWebView is not initialized".to_string())?;

            if let Some(fullscreen) = shell_transport_fullscreen_request(payload.as_ref())? {
                self.set_fullscreen(window, fullscreen)?;
                return Ok(Value::Null);
            }

            match kind {
                "invoke" if invoke_command(payload.as_ref()) == Some("toggle_pip") => {
                    let mut controller = self.window_controller(window);
                    Ok(json!(self
                        .ivars()
                        .runtime
                        .host()
                        .toggle_picture_in_picture(&mut controller)?))
                }
                "window.minimize" => {
                    window.miniaturize(None);
                    self.ivars().runtime.dispatch_ipc(kind, payload)
                }
                "window.focus" => {
                    if window.isMiniaturized() {
                        window.deminiaturize(None);
                    }
                    window.makeKeyAndOrderFront(None);
                    let app = NSApplication::sharedApplication(self.mtm());
                    #[allow(deprecated)]
                    app.activateIgnoringOtherApps(true);
                    self.ivars().runtime.dispatch_ipc(kind, payload)
                }
                "window.toggleMaximize" => {
                    if window.isMiniaturized() {
                        window.deminiaturize(None);
                    }
                    window.zoom(None);
                    self.ivars()
                        .runtime
                        .host()
                        .emit_window_maximized_changed(window.isZoomed())?;
                    Ok(Value::Null)
                }
                "window.close" => {
                    let mut controller = self.window_controller(window);
                    self.ivars()
                        .runtime
                        .host()
                        .exit_picture_in_picture(&mut controller)?;
                    window.orderOut(None);
                    self.ivars().runtime.dispatch_ipc(kind, payload)
                }
                "window.isMaximized" => Ok(json!(window.isZoomed())),
                "window.isFullscreen" => Ok(json!(is_window_fullscreen(window))),
                "window.setFullscreen" => {
                    let request: host_api::FullscreenIpcPayload =
                        host_api::parse_payload(kind, payload)?;
                    self.set_fullscreen(window, request.fullscreen)?;
                    Ok(Value::Null)
                }
                "webview.setZoom" => {
                    let request: host_api::ZoomIpcPayload = host_api::parse_payload(kind, payload)?;
                    if !request.level.is_finite() || request.level <= 0.0 {
                        return Err("Invalid webview zoom level".to_string());
                    }
                    unsafe { webview.setPageZoom(request.level) };
                    Ok(Value::Null)
                }
                _ => self.ivars().runtime.dispatch_ipc(kind, payload),
            }
        }

        fn set_fullscreen(&self, window: &NSWindow, fullscreen: bool) -> Result<(), String> {
            let restored_fullscreen = if fullscreen {
                let mut controller = self.window_controller(window);
                self.ivars()
                    .runtime
                    .host()
                    .exit_picture_in_picture(&mut controller)?;
                controller.restored_fullscreen
            } else {
                false
            };
            if !restored_fullscreen {
                set_native_fullscreen(window, fullscreen);
            }
            self.ivars().runtime.dispatch_ipc(
                "window.setFullscreen",
                Some(json!({ "fullscreen": fullscreen })),
            )?;
            Ok(())
        }

        fn window_controller<'a>(&'a self, window: &'a NSWindow) -> NativeWindowController<'a> {
            NativeWindowController {
                window,
                pip_window: &self.ivars().pip_window,
                pending_pip_transition: &self.ivars().pending_pip_transition,
                restored_fullscreen: false,
            }
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

        fn invalidate_player_event_timer(&self) {
            if let Some(timer) = self.ivars().player_event_timer.get() {
                timer.invalidate();
            }
        }
    }

    struct NativeWindowController<'a> {
        window: &'a NSWindow,
        pip_window: &'a RefCell<Option<PipWindowSnapshot>>,
        pending_pip_transition: &'a Cell<Option<PendingPipTransition>>,
        restored_fullscreen: bool,
    }

    impl PipWindowController for NativeWindowController<'_> {
        fn enter_pip(&mut self, width: i32, height: i32) -> Result<PipRestoreSnapshot, String> {
            let was_fullscreen = is_window_fullscreen(self.window);
            let frame = self.window.frame();
            if was_fullscreen {
                self.pending_pip_transition
                    .set(Some(PendingPipTransition::Enter { width, height }));
                self.window.toggleFullScreen(None);
            } else {
                enter_pip_window(self.window, self.pip_window, width, height);
            }
            Ok(PipRestoreSnapshot {
                was_fullscreen,
                saved_size: (!was_fullscreen)
                    .then_some((frame.size.width as i32, frame.size.height as i32)),
            })
        }

        fn exit_pip(&mut self, snapshot: PipRestoreSnapshot) -> Result<(), String> {
            self.restored_fullscreen = snapshot.was_fullscreen;
            let restore_is_pending = schedule_pending_fullscreen_restore(
                self.pending_pip_transition,
                snapshot.was_fullscreen,
            );
            if let Some(window_snapshot) = self.pip_window.borrow_mut().take() {
                self.window.setStyleMask(window_snapshot.style_mask);
                self.window.setContentMinSize(window_snapshot.min_size);
                self.window.setLevel(window_snapshot.level);
                self.window.setFrame_display(window_snapshot.frame, true);
                if window_snapshot.was_zoomed && !self.window.isZoomed() {
                    self.window.zoom(None);
                }
            }
            if snapshot.was_fullscreen && !restore_is_pending && !is_window_fullscreen(self.window)
            {
                self.window.toggleFullScreen(None);
            }
            self.window.makeKeyAndOrderFront(None);
            Ok(())
        }
    }

    fn is_window_fullscreen(window: &NSWindow) -> bool {
        window.styleMask().contains(NSWindowStyleMask::FullScreen)
    }

    fn set_native_fullscreen(window: &NSWindow, fullscreen: bool) {
        if is_window_fullscreen(window) != fullscreen {
            window.toggleFullScreen(None);
        }
    }

    fn enter_pip_window(
        window: &NSWindow,
        state: &RefCell<Option<PipWindowSnapshot>>,
        width: i32,
        height: i32,
    ) {
        if state.borrow().is_some() {
            return;
        }
        let was_zoomed = window.isZoomed();
        if was_zoomed {
            window.zoom(None);
        }
        let frame = window.frame();
        *state.borrow_mut() = Some(PipWindowSnapshot {
            frame,
            min_size: window.contentMinSize(),
            style_mask: window.styleMask(),
            level: window.level(),
            was_zoomed,
        });
        window.setContentMinSize(NSSize::new(240.0, 135.0));
        window.setStyleMask(NSWindowStyleMask::Borderless);
        window.setLevel(NSFloatingWindowLevel);
        window.setFrame_display(
            NSRect::new(
                NSPoint::new(
                    frame.origin.x,
                    frame.origin.y + frame.size.height - height as f64,
                ),
                NSSize::new(width as f64, height as f64),
            ),
            true,
        );
        window.makeKeyAndOrderFront(None);
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
        delegate.invalidate_player_event_timer();
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

        let request =
            parse_ipc_message(r#"{"id":7,"kind":"invoke","payload":{"command":"init"}}"#).unwrap();
        let id = request.id;
        let response = runtime.dispatch_ipc(&request.kind, request.payload);
        assert_eq!(id, 7);
        let script = resolve_ipc_script(id, response);
        assert!(script.starts_with("window.__STREMIO_LIGHTNING_MACOS_RESOLVE__(7, {"));
        assert!(script.contains("\"platform\":\"macos\""));

        let request = parse_ipc_message(r#"{"id":8,"kind":"unknown"}"#).unwrap();
        let id = request.id;
        let response = runtime.dispatch_ipc(&request.kind, request.payload);
        assert_eq!(
            resolve_ipc_script(id, response),
            "window.__STREMIO_LIGHTNING_MACOS_RESOLVE__(8, null, \"Unsupported IPC kind: unknown\");"
        );
        assert!(parse_ipc_message("not json")
            .unwrap_err()
            .starts_with("Invalid macOS WKWebView IPC message:"));
    }

    #[test]
    fn pending_fullscreen_pip_exit_schedules_fullscreen_restore() {
        let transition = std::cell::Cell::new(Some(PendingPipTransition::Enter {
            width: 480,
            height: 270,
        }));
        assert!(schedule_pending_fullscreen_restore(&transition, true));
        assert_eq!(
            transition.get(),
            Some(PendingPipTransition::RestoreFullscreen)
        );
    }

    #[test]
    fn extracts_native_window_requests_from_ipc_payloads() {
        let fullscreen = json!({
            "command": "shell_transport_send",
            "payload": {
                "message": r#"{"id":1,"type":6,"args":["win-set-visibility",{"fullscreen":true}]}"#
            }
        });
        assert_eq!(
            shell_transport_fullscreen_request(Some(&fullscreen)).unwrap(),
            Some(true)
        );
        assert_eq!(
            invoke_command(Some(&json!({ "command": "toggle_pip" }))),
            Some("toggle_pip")
        );
    }
}
