use crate::app::AppConfig;
use crate::player::{MpvBackendCommand, MpvPlayerBackend};
use crate::streaming_server::RealProcessSpawner;
use crate::webview_runtime::LinuxWebviewRuntime;
use gtk::gdk::{Display, GLContext, RGBA};
use gtk::glib::{self, Propagation};
use gtk::prelude::*;
use libc::{setlocale, LC_NUMERIC};
use libmpv2::events::{Event, PropertyData};
use libmpv2::render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType};
use libmpv2::{mpv_end_file_reason, Format, Mpv};
use serde::Deserialize;
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::os::raw::c_void;
use std::path::PathBuf;
use std::ptr;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::Duration;
use stremio_lightning_core::pip::{PipRestoreSnapshot, PipWindowController};
use stremio_lightning_core::player_api::{
    is_json_string_property, mpv_property_format, EndFileCause, MpvPropertyFormat, PlayerEnded,
};
use stremio_lightning_identity::{APP_ID, APP_NAME};
use webkit::prelude::*;
use webkit::{
    NavigationPolicyDecision, PolicyDecisionType, UserContentInjectedFrames, UserScript,
    UserScriptInjectionTime, WebView as WebKitWebView,
};

pub(crate) mod mpris;
mod x11;

use self::x11::{install_source_tree_window_icon, request_window_above};
use stremio_lightning_core::launch_intent::classify_launch_argument;

const IPC_HANDLER_NAME: &str = "ipc";
const DEV_ICON_NAME: &str = "128x128";
const DEFAULT_WINDOW_WIDTH: i32 = 1500;
const DEFAULT_WINDOW_HEIGHT: i32 = 850;
const MIN_WINDOW_WIDTH: i32 = 800;
const MIN_WINDOW_HEIGHT: i32 = 600;

thread_local! {
    static LAST_NORMAL_SIZE: RefCell<Option<(i32, i32)>> = const { RefCell::new(None) };
}
#[derive(Debug, Deserialize)]
struct IpcRequest {
    id: u64,
    kind: String,
    payload: Option<Value>,
}

type WebkitIpcRequest = IpcRequest;

#[derive(Debug, Deserialize)]
struct ShellTransportMessage {
    #[serde(rename = "type")]
    message_type: u8,
    args: Option<Value>,
}

/// Claims the application's D-Bus name. Returns `None` when another instance
/// already owns it: the launch argument has been handed over and this process
/// should exit. Must run before anything expensive (sidecar, mpv) starts.
pub fn claim_instance(launch_arg: Option<&str>) -> Option<gtk::Application> {
    glib::set_application_name(APP_NAME);
    glib::set_prgname(Some(APP_ID));
    let app = gtk::Application::new(Some(APP_ID), gtk::gio::ApplicationFlags::HANDLES_OPEN);
    if let Err(error) = app.register(None::<&gtk::gio::Cancellable>) {
        // No session bus etc.: behave as a stand-alone primary instance.
        stremio_lightning_core::logging::warn(
            "native.application",
            format!("[StremioLightning] Single-instance registration failed: {error}"),
        );
        return Some(app);
    }
    if !app.is_remote() {
        return Some(app);
    }
    // `run` on a remote instance forwards to the primary (`open` with an
    // argument, `activate` without) and returns once it has been delivered.
    let args: Vec<&str> = std::iter::once(APP_ID).chain(launch_arg).collect();
    app.run_with_args(&args);
    None
}

pub fn run_native_window(
    app: gtk::Application,
    config: AppConfig,
    mut runtime: LinuxWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>,
    player: MpvPlayerBackend,
) -> Result<(), String> {
    load_epoxy()?;

    let _state = runtime.load()?;
    let runtime = Rc::new(runtime);
    let startup_error: Rc<RefCell<Option<String>>> = Rc::default();

    {
        let runtime = runtime.clone();
        let player = player.clone();
        let startup_error = startup_error.clone();
        app.connect_activate(move |app| {
            if let Some(window) = app.active_window() {
                window.present();
                return;
            }
            let icon_name = configure_application_icon_name();
            gtk::Window::set_default_icon_name(icon_name);
            if let Err(error) =
                build_window(app, &config, runtime.clone(), player.clone(), icon_name)
            {
                *startup_error.borrow_mut() = Some(error);
                app.quit();
            }
        });
    }

    let exit_code = app.run_with_args(&[APP_ID]);
    if let Some(error) = startup_error.borrow_mut().take() {
        return Err(error);
    }
    if exit_code.get() == 0 {
        Ok(())
    } else {
        Err(format!(
            "Linux shell exited with status {}",
            exit_code.get()
        ))
    }
}

fn build_window(
    app: &gtk::Application,
    config: &AppConfig,
    runtime: Rc<LinuxWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>>,
    player: MpvPlayerBackend,
    icon_name: &str,
) -> Result<(), String> {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title(APP_NAME)
        .icon_name(icon_name)
        .default_width(DEFAULT_WINDOW_WIDTH)
        .default_height(DEFAULT_WINDOW_HEIGHT)
        .build();
    window.set_size_request(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT);
    install_source_tree_window_icon(&window);

    let fullscreen = Rc::new(Cell::new(false));
    let last_visible = Rc::new(Cell::new(None));
    let overlay = gtk::Overlay::new();
    let webview = build_webview(
        config,
        runtime.clone(),
        window.clone(),
        fullscreen.clone(),
        last_visible,
    )?;
    let (video, video_state) = build_native_video(
        player,
        runtime.clone(),
        webview.clone(),
        window.clone(),
        fullscreen.clone(),
    )?;
    video.set_hexpand(true);
    video.set_vexpand(true);
    overlay.set_child(Some(&video));

    overlay.add_overlay(&webview);
    window.set_child(Some(&overlay));

    {
        let app = app.clone();
        let runtime = runtime.clone();
        let video_state = video_state.clone();
        window.connect_close_request(move |_| {
            video_state.shutdown();
            if let Err(error) = runtime.shutdown() {
                stremio_lightning_core::logging::error(
                    "native.window",
                    format!("[StremioLightning] Failed to shut down Linux runtime: {error}"),
                );
            }
            app.quit();
            Propagation::Proceed
        });
    }

    {
        let webview = webview.clone();
        let runtime = runtime.clone();
        let last_active = Rc::new(Cell::new(None::<bool>));
        let current_timeout = Rc::new(RefCell::new(None::<glib::SourceId>));

        window.connect_notify_local(Some("is-active"), move |window, _| {
            if let Some(source_id) = current_timeout.borrow_mut().take() {
                source_id.remove();
            }

            let webview = webview.clone();
            let runtime = runtime.clone();
            let window_clone = window.clone();
            let last_active = last_active.clone();
            let current_timeout_clone = current_timeout.clone();

            let source_id = glib::timeout_add_local(Duration::from_millis(100), move || {
                *current_timeout_clone.borrow_mut() = None;

                let stable_active = window_clone.is_active();
                if last_active.get() != Some(stable_active) {
                    last_active.set(Some(stable_active));

                    let event = if stable_active { "focus" } else { "blur" };
                    let script = format!("window.dispatchEvent(new Event('{event}'));");
                    evaluate_javascript(&webview, &script);

                    runtime
                        .dispatch_ipc(
                            "window.focus_changed",
                            Some(json!({"focused": stable_active})),
                        )
                        .ok();
                }

                glib::ControlFlow::Break
            });

            *current_timeout.borrow_mut() = Some(source_id);
        });
    }

    {
        let webview = webview.clone();
        let runtime = runtime.clone();
        app.connect_open(move |app, files, _| {
            for file in files {
                let arg = file.path().map_or_else(
                    || file.uri().to_string(),
                    |path| path.to_string_lossy().into_owned(),
                );
                let Some(intent) = classify_launch_argument(&arg) else {
                    continue;
                };
                if let Err(error) = runtime.emit_launch_intent(&intent) {
                    stremio_lightning_core::logging::error(
                        "native.window",
                        format!("[StremioLightning] Failed to forward launch intent: {error}"),
                    );
                }
            }
            drain_host_events(&webview, &runtime);
            if let Some(window) = app.active_window() {
                window.present();
            }
        });
    }

    {
        let webview = webview.clone();
        let runtime = runtime.clone();
        mpris::own(app, move |action| {
            if let Err(error) = runtime.emit_media_key(action) {
                stremio_lightning_core::logging::error(
                    "native.window",
                    format!("[StremioLightning] Failed to forward media key: {error}"),
                );
            }
            drain_host_events(&webview, &runtime);
        });
    }

    window.maximize();

    window.present();
    Ok(())
}

/// Emits `window-visible-changed` at most once per transition. The GTK signals
/// and the `window.minimize` / `window.focus` bridge commands can both report the
/// same transition, and the web side expects a change notification, not a repeat.
fn emit_window_visibility_changed(
    webview: &WebKitWebView,
    runtime: &LinuxWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>,
    last_visible: &Cell<Option<bool>>,
    visible: bool,
) {
    if last_visible.replace(Some(visible)) == Some(visible) {
        return;
    }

    if let Err(error) = runtime.emit_window_visible_changed(visible) {
        stremio_lightning_core::logging::error(
            "native.window",
            format!("[StremioLightning] Failed to emit window visibility change: {error}"),
        );
    }
    drain_host_events(webview, runtime);
}

fn configure_application_icon_name() -> &'static str {
    let Some(display) = Display::default() else {
        stremio_lightning_core::logging::warn(
            "native.window",
            "[StremioLightning] Unable to resolve Linux window icon: no GTK display available",
        );
        return APP_ID;
    };

    let icon_theme = gtk::IconTheme::for_display(&display);
    let dev_icon_dir = source_tree_icon_dir();
    if dev_icon_dir.exists() {
        icon_theme.add_search_path(dev_icon_dir);
    }

    if icon_theme.has_icon(APP_ID) {
        APP_ID
    } else if icon_theme.has_icon(DEV_ICON_NAME) {
        DEV_ICON_NAME
    } else {
        stremio_lightning_core::logging::warn("native.window", format!(
            "[StremioLightning] Unable to resolve Linux window icon: missing {APP_ID} or {DEV_ICON_NAME} in GTK icon theme"
        ));
        APP_ID
    }
}

fn source_tree_icon_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/icons")
}

fn build_webview(
    config: &AppConfig,
    runtime: Rc<LinuxWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>>,
    window: gtk::ApplicationWindow,
    fullscreen: Rc<Cell<bool>>,
    last_visible: Rc<Cell<Option<bool>>>,
) -> Result<WebKitWebView, String> {
    let user_content = webkit::UserContentManager::new();
    user_content.register_script_message_handler(IPC_HANDLER_NAME, None);
    user_content.add_script(&document_start_script(webkit_ipc_adapter()));

    for script in runtime.load_state().document_start_scripts {
        let source = runtime
            .script_source(script)
            .ok_or_else(|| format!("Missing document-start script: {script}"))?;
        user_content.add_script(&document_start_script(source));
    }

    let webview = WebKitWebView::builder()
        .user_content_manager(&user_content)
        .build();
    webview.set_hexpand(true);
    webview.set_vexpand(true);
    webview.set_background_color(&RGBA::new(0.0, 0.0, 0.0, 0.0));

    if let Some(settings) = WebViewExt::settings(&webview) {
        settings.set_enable_developer_extras(config.devtools);
        settings.set_enable_media(false);
        settings.set_enable_media_capabilities(false);
        settings.set_enable_media_stream(false);
        settings.set_enable_webaudio(false);
        settings.set_enable_smooth_scrolling(true);
        settings.set_hardware_acceleration_policy(webkit::HardwareAccelerationPolicy::Always);
        // Optimize memory consumption by disabling unused graphics features
        settings.set_enable_webgl(false);
    }

    log_webkit_runtime_version();
    attach_webview_failure_hooks(&webview);

    // Capture phase so the shortcut wins over WebKit's own key handling.
    let shortcuts = gtk::ShortcutController::new();
    shortcuts.set_propagation_phase(gtk::PropagationPhase::Capture);
    for (keys, bypass_cache) in [
        ("F5", false),
        ("<Control>r", false),
        ("<Control>F5", true),
        ("<Control><Shift>r", true),
    ] {
        let webview = webview.clone();
        shortcuts.add_shortcut(gtk::Shortcut::new(
            gtk::ShortcutTrigger::parse_string(keys),
            Some(gtk::CallbackAction::new(move |_, _| {
                if bypass_cache {
                    webview.reload_from_origin();
                } else {
                    webview.reload();
                }
                glib::Propagation::Stop
            })),
        ));
    }
    webview.add_controller(shortcuts);

    {
        let webview = webview.clone();
        let runtime = runtime.clone();
        let window = window.clone();
        let fullscreen = fullscreen.clone();
        let last_visible = last_visible.clone();
        user_content.connect_script_message_received(Some(IPC_HANDLER_NAME), move |_, value| {
            handle_ipc_message(
                &webview,
                &runtime,
                &window,
                &fullscreen,
                &last_visible,
                &value.to_string(),
            );
        });
    }

    // GTK4 exposes no `minimized` notify on `GtkWindow`, so visibility is tracked
    // from the map/unmap pair instead; it also covers taskbar minimize, fullscreen
    // and PiP, which all map or unmap the same window.
    {
        let webview = webview.clone();
        let runtime = runtime.clone();
        let last_visible = last_visible.clone();
        window.clone().connect_map(move |_| {
            emit_window_visibility_changed(&webview, &runtime, &last_visible, true);
        });
    }

    {
        let webview = webview.clone();
        let runtime = runtime.clone();
        let last_visible = last_visible.clone();
        window.clone().connect_unmap(move |_| {
            emit_window_visibility_changed(&webview, &runtime, &last_visible, false);
        });
    }

    {
        let webview = webview.clone();
        let runtime = runtime.clone();
        window.clone().connect_maximized_notify(move |window| {
            if let Err(error) = runtime.emit_window_maximized_changed(window.is_maximized()) {
                stremio_lightning_core::logging::error(
                    "native.window",
                    format!("[StremioLightning] Failed to emit maximize change: {error}"),
                );
            }
            drain_host_events(&webview, &runtime);
        });
    }

    {
        let runtime = runtime.clone();
        let window = window.clone();
        let fullscreen = fullscreen.clone();
        webview.connect_enter_fullscreen(move |webview| {
            set_window_fullscreen(webview, &runtime, &window, &fullscreen, true);
            true
        });
    }

    {
        let runtime = runtime.clone();
        let window = window.clone();
        let fullscreen = fullscreen.clone();
        webview.connect_leave_fullscreen(move |webview| {
            set_window_fullscreen(webview, &runtime, &window, &fullscreen, false);
            true
        });
    }

    let runtime_for_navigation = runtime.clone();
    let app_url = config.url.clone();
    // A `file://` developer smoke page has no http(s) origin to compare against,
    // and the allowlist would then reject every target, so navigation is only
    // restricted once the app really runs on an origin.
    let restrict_navigation =
        !stremio_lightning_core::navigation::has_unrestricted_app_url(&app_url);
    webview.connect_decide_policy(move |webview, decision, decision_type| {
        let uri = decision
            .downcast_ref::<NavigationPolicyDecision>()
            .and_then(|decision| decision.navigation_action())
            .and_then(|action| action.request())
            .and_then(|request| request.uri());

        let is_new_window = decision_type == PolicyDecisionType::NewWindowAction;

        if let Some(uri) = uri {
            if uri
                .get(.."stremio://".len())
                .is_some_and(|value| value.eq_ignore_ascii_case("stremio://"))
            {
                decision.ignore();
                if let Err(error) = runtime_for_navigation.emit_stremio_deep_link(uri.as_str()) {
                    stremio_lightning_core::logging::error(
                        "native.window",
                        format!("[StremioLightning] Failed to open Stremio deep link: {error}"),
                    );
                }
                drain_host_events(webview, &runtime_for_navigation);
                return true;
            }

            // Anything outside the app origin is cancelled and handed to the OS,
            // so a remote page can never render inside the shell with the native
            // bridge injected into it. New-window targets always take that path.
            if !is_new_window
                && (!restrict_navigation
                    || stremio_lightning_core::navigation::is_allowed_webview_navigation(
                        &app_url, &uri,
                    ))
            {
                return false;
            }

            decision.ignore();
            stremio_lightning_core::logging::info(
                "native.webview.linux",
                format!("Leaving the app origin for an external target: {uri}"),
            );
            if let Err(error) = open_allowed_external_uri(&uri) {
                stremio_lightning_core::logging::error(
                    "native.webview.linux",
                    format!("[StremioLightning] Failed to open external URL: {error}"),
                );
            }
            return true;
        }

        if is_new_window {
            decision.ignore();
            return true;
        }

        false
    });

    webview.load_uri(&config.url);
    Ok(webview)
}

fn log_webkit_runtime_version() {
    let version = format!(
        "{}.{}.{}",
        webkit::functions::major_version(),
        webkit::functions::minor_version(),
        webkit::functions::micro_version()
    );
    stremio_lightning_core::logging::update_webview_metadata("WebKitGTK", Some(&version));
    stremio_lightning_core::logging::info(
        "native.webview.linux",
        format!("WebKitGTK runtime version: {version}"),
    );
}

fn attach_webview_failure_hooks(webview: &WebKitWebView) {
    webview.connect_load_failed(|_, event, uri, error| {
        stremio_lightning_core::logging::error(
            "native.webview.linux",
            format!(
                "WebKitGTK navigation failed during {} ({}, error code {})",
                webkit_load_event_name(event),
                safe_webview_resource_descriptor(Some(uri)),
                error.code()
            ),
        );
        // Returning false preserves WebKit's existing failure handling.
        false
    });

    webview.connect_web_process_terminated(|_, reason| {
        stremio_lightning_core::logging::error(
            "native.webview.linux",
            format!(
                "WebKitGTK web process terminated: {}",
                webkit_process_termination_reason_name(reason)
            ),
        );
    });

    webview.connect_resource_load_started(|_, resource, request| {
        let request_descriptor = safe_webview_resource_descriptor(request.uri().as_deref());
        resource.connect_finished(move |resource| {
            let Some(response) = resource.response() else {
                return;
            };
            let status = response.status_code();
            if status >= 400 {
                let descriptor = safe_webview_resource_descriptor(resource.uri().as_deref());
                stremio_lightning_core::logging::error(
                    "native.webview.linux",
                    format!("WebKitGTK resource response failed: HTTP {status} ({descriptor})"),
                );
            }
        });
        resource.connect_failed(move |resource, error| {
            let descriptor = resource
                .uri()
                .as_deref()
                .map(|uri| safe_webview_resource_descriptor(Some(uri)))
                .unwrap_or(request_descriptor);
            stremio_lightning_core::logging::error(
                "native.webview.linux",
                format!(
                    "WebKitGTK resource load failed: {descriptor} (error code {})",
                    error.code()
                ),
            );
        });
    });
}

fn safe_webview_resource_descriptor(uri: Option<&str>) -> &'static str {
    let scheme = uri.and_then(|uri| uri.split_once(':').map(|(scheme, _)| scheme));
    match scheme {
        Some(scheme) if scheme.eq_ignore_ascii_case("https") => "https resource",
        Some(scheme) if scheme.eq_ignore_ascii_case("http") => "http resource",
        Some(scheme) if scheme.eq_ignore_ascii_case("file") => "file resource",
        Some(_) => "other resource",
        None => "unknown resource",
    }
}

fn webkit_load_event_name(event: webkit::LoadEvent) -> &'static str {
    match event {
        webkit::LoadEvent::Started => "started",
        webkit::LoadEvent::Redirected => "redirected",
        webkit::LoadEvent::Committed => "committed",
        webkit::LoadEvent::Finished => "finished",
        _ => "unknown",
    }
}

fn webkit_process_termination_reason_name(
    reason: webkit::WebProcessTerminationReason,
) -> &'static str {
    match reason {
        webkit::WebProcessTerminationReason::Crashed => "crashed",
        webkit::WebProcessTerminationReason::ExceededMemoryLimit => "exceeded-memory-limit",
        webkit::WebProcessTerminationReason::TerminatedByApi => "terminated-by-api",
        _ => "unknown",
    }
}

fn document_start_script(source: impl Into<String>) -> UserScript {
    UserScript::new(
        &source.into(),
        UserContentInjectedFrames::TopFrame,
        UserScriptInjectionTime::Start,
        &[],
        &[],
    )
}

fn webkit_ipc_adapter() -> String {
    format!(
        r#"(function () {{
  "use strict";
  if (window.__STREMIO_LIGHTNING_LINUX_IPC__) return;

  var nextId = 1;
  var pending = new Map();

  window.__STREMIO_LIGHTNING_LINUX_IPC__ = function (kind, payload) {{
    return new Promise(function (resolve, reject) {{
      var id = nextId++;
      pending.set(id, {{ resolve: resolve, reject: reject }});
      window.webkit.messageHandlers.{handler}.postMessage(JSON.stringify({{
        id: id,
        kind: kind,
        payload: payload
      }}));
    }});
  }};

  window.__STREMIO_LIGHTNING_LINUX_IPC_RESOLVE__ = function (id, ok, value) {{
    var callbacks = pending.get(id);
    if (!callbacks) return;
    pending.delete(id);
    if (ok) callbacks.resolve(value);
    else callbacks.reject(new Error(String(value)));
  }};
}})();"#,
        handler = IPC_HANDLER_NAME
    )
}

fn handle_ipc_message(
    webview: &WebKitWebView,
    runtime: &LinuxWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>,
    window: &gtk::ApplicationWindow,
    fullscreen: &Rc<Cell<bool>>,
    last_visible: &Rc<Cell<Option<bool>>>,
    raw: &str,
) {
    let response = serde_json::from_str::<WebkitIpcRequest>(raw)
        .map_err(|error| format!("Invalid Linux WebKit IPC message: {error}"))
        .and_then(|request| {
            let external_url = external_url_from_ipc_request(&request);
            let id = request.id;
            runtime
                .dispatch_native_window_ipc(
                    &request.kind,
                    request.payload,
                    window,
                    fullscreen,
                    last_visible,
                    webview,
                )
                .and_then(|value| {
                    if let Some(url) = external_url {
                        open_external_uri(&url)?;
                    }
                    Ok(value)
                })
                .map(|value| (id, Ok(value)))
                .or_else(|error| Ok((id, Err(error))))
        });

    match response {
        Ok((id, Ok(value))) => evaluate_javascript(webview, &resolve_ipc_script(id, true, value)),
        Ok((id, Err(error))) => {
            evaluate_javascript(webview, &resolve_ipc_script(id, false, json!(error)))
        }
        Err(error) => stremio_lightning_core::logging::error(
            "native.ipc",
            format!("[StremioLightning] {error}"),
        ),
    }

    drain_host_events(webview, runtime);
}

trait NativeWindowIpc {
    fn dispatch_native_window_ipc(
        &self,
        kind: &str,
        payload: Option<Value>,
        window: &gtk::ApplicationWindow,
        fullscreen: &Rc<Cell<bool>>,
        last_visible: &Rc<Cell<Option<bool>>>,
        webview: &WebKitWebView,
    ) -> Result<Value, String>;
}

impl NativeWindowIpc for LinuxWebviewRuntime<MpvPlayerBackend, RealProcessSpawner> {
    fn dispatch_native_window_ipc(
        &self,
        kind: &str,
        payload: Option<Value>,
        window: &gtk::ApplicationWindow,
        fullscreen: &Rc<Cell<bool>>,
        last_visible: &Rc<Cell<Option<bool>>>,
        webview: &WebKitWebView,
    ) -> Result<Value, String> {
        match kind {
            "invoke" => {
                if let Some(fullscreen_value) =
                    shell_transport_fullscreen_request(payload.as_ref())?
                {
                    set_window_fullscreen(webview, self, window, fullscreen, fullscreen_value);
                    return Ok(Value::Null);
                }

                if invoke_command(payload.as_ref()) == Some("toggle_pip") {
                    let mut controller = NativeWindowController {
                        webview,
                        runtime: self,
                        window,
                        fullscreen,
                    };
                    let enabled = self.toggle_picture_in_picture(&mut controller)?;
                    return Ok(json!(enabled));
                }

                if invoke_command(payload.as_ref()) == Some("toggle_devtools") {
                    if let Some(inspector) = webview.inspector() {
                        if inspector.property::<bool>("is-visible") {
                            inspector.close();
                        } else {
                            inspector.show();
                        }
                    }
                    return Ok(Value::Null);
                }

                if invoke_command(payload.as_ref()) == Some("webview.setZoom") {
                    let level = payload
                        .as_ref()
                        .and_then(|value| value.get("payload"))
                        .and_then(|value| value.get("level"))
                        .and_then(Value::as_f64)
                        .and_then(stremio_lightning_core::host_api::valid_zoom_level)
                        .ok_or_else(|| "Invalid webview.setZoom payload".to_string())?;
                    // WebKitGTK's zoom level is a logarithmic scale where 0.0 is
                    // 100%, while the bridge sends a plain factor.
                    webview.set_zoom_level(level.log2());
                    return Ok(Value::Null);
                }

                LinuxWebviewRuntime::dispatch_ipc(self, kind, payload)
            }
            "window.isFullscreen" => Ok(json!(fullscreen.get())),
            "window.setFullscreen" => {
                let fullscreen_value = payload
                    .as_ref()
                    .and_then(|value| value.get("fullscreen"))
                    .and_then(Value::as_bool)
                    .ok_or_else(|| "Invalid window.setFullscreen payload".to_string())?;
                set_window_fullscreen(webview, self, window, fullscreen, fullscreen_value);
                Ok(Value::Null)
            }
            "window.close" => {
                let mut controller = NativeWindowController {
                    webview,
                    runtime: self,
                    window,
                    fullscreen,
                };
                self.exit_picture_in_picture(&mut controller)?;
                window.close();
                Ok(Value::Null)
            }
            "window.isMaximized" => Ok(json!(window.is_maximized())),
            "window.minimize" => {
                window.minimize();
                emit_window_visibility_changed(webview, self, last_visible, false);
                Ok(Value::Null)
            }
            "window.focus" => {
                window.present();
                emit_window_visibility_changed(webview, self, last_visible, true);
                Ok(Value::Null)
            }
            "window.toggleMaximize" => {
                if window.is_maximized() {
                    window.unmaximize();
                } else {
                    window.maximize();
                }
                Ok(Value::Null)
            }
            "window.startDragging" => {
                start_window_dragging(window)?;
                Ok(Value::Null)
            }
            _ => LinuxWebviewRuntime::dispatch_ipc(self, kind, payload),
        }
    }
}

fn start_window_dragging(window: &gtk::ApplicationWindow) -> Result<(), String> {
    let Some(surface) = window.surface() else {
        return Err("Cannot drag window before it has a surface".to_string());
    };
    let Ok(toplevel) = surface.clone().downcast::<gtk::gdk::Toplevel>() else {
        return Err("Window surface is not a draggable toplevel".to_string());
    };
    let Some(pointer) = surface
        .display()
        .default_seat()
        .and_then(|seat| seat.pointer())
    else {
        return Err("No pointer device available for window dragging".to_string());
    };

    let (x, y) = if let Some((px, py, _)) = surface.device_position(&pointer) {
        (px, py)
    } else {
        (0.0, 0.0)
    };

    toplevel.begin_move(&pointer, 1, x, y, 0);
    Ok(())
}

fn invoke_command(payload: Option<&Value>) -> Option<&str> {
    payload
        .and_then(|value| value.get("command"))
        .and_then(Value::as_str)
}

fn shell_transport_fullscreen_request(payload: Option<&Value>) -> Result<Option<bool>, String> {
    let Some(payload) = payload else {
        return Ok(None);
    };
    if payload.get("command").and_then(Value::as_str) != Some("shell_transport_send") {
        return Ok(None);
    }

    let Some(message) = payload
        .get("payload")
        .and_then(|payload| payload.get("message"))
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };

    let request: ShellTransportMessage = serde_json::from_str(message)
        .map_err(|error| format!("Invalid shell transport message: {error}"))?;
    if request.message_type != 6 {
        return Ok(None);
    }

    let args: Vec<Value> = serde_json::from_value(request.args.unwrap_or(Value::Null))
        .map_err(|error| format!("Invalid shell transport arguments: {error}"))?;
    if args.first().and_then(Value::as_str) != Some("win-set-visibility") {
        return Ok(None);
    }

    let fullscreen = args
        .get(1)
        .and_then(|value| value.get("fullscreen"))
        .and_then(Value::as_bool)
        .ok_or_else(|| "Invalid win-set-visibility payload".to_string())?;
    Ok(Some(fullscreen))
}

struct NativeWindowController<'a> {
    webview: &'a WebKitWebView,
    runtime: &'a LinuxWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>,
    window: &'a gtk::ApplicationWindow,
    fullscreen: &'a Rc<Cell<bool>>,
}

impl PipWindowController for NativeWindowController<'_> {
    fn enter_pip(&mut self, width: i32, height: i32) -> Result<PipRestoreSnapshot, String> {
        let was_fullscreen = self.fullscreen.get();
        let saved_size = if was_fullscreen {
            LAST_NORMAL_SIZE
                .with(|cell| *cell.borrow())
                .or(Some((DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT)))
        } else {
            let curr_w = self.window.width();
            let curr_h = self.window.height();
            (curr_w > 0 && curr_h > 0).then_some((curr_w, curr_h))
        };

        if was_fullscreen {
            set_window_fullscreen(
                self.webview,
                self.runtime,
                self.window,
                self.fullscreen,
                false,
            );
        }
        self.window.unmaximize();
        self.window.set_modal(true);
        self.window.set_resizable(true);
        self.window.set_size_request(240, 135);
        self.window.set_decorated(false);
        request_window_above(self.window, true)?;
        self.window.set_default_size(width, height);
        self.window.present();

        Ok(PipRestoreSnapshot {
            was_fullscreen,
            saved_size,
        })
    }

    fn exit_pip(&mut self, snapshot: PipRestoreSnapshot) -> Result<(), String> {
        request_window_above(self.window, false)?;
        self.window.set_decorated(true);
        self.window.set_modal(false);
        self.window
            .set_size_request(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT);
        self.window.set_resizable(true);

        if snapshot.was_fullscreen {
            set_window_fullscreen(
                self.webview,
                self.runtime,
                self.window,
                self.fullscreen,
                true,
            );
        } else if let Some((width, height)) = snapshot.saved_size {
            self.window.set_default_size(width, height);
        }

        self.window.present();
        Ok(())
    }
}

fn set_window_fullscreen(
    webview: &WebKitWebView,
    runtime: &LinuxWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>,
    window: &gtk::ApplicationWindow,
    fullscreen: &Rc<Cell<bool>>,
    fullscreen_value: bool,
) {
    if fullscreen_value {
        let width = window.width();
        let height = window.height();
        if width > 0 && height > 0 {
            LAST_NORMAL_SIZE.with(|cell| {
                *cell.borrow_mut() = Some((width, height));
            });
        }
        window.fullscreen();
    } else {
        window.unfullscreen();
    }

    if fullscreen.replace(fullscreen_value) != fullscreen_value {
        if let Err(error) = runtime.dispatch_ipc(
            "window.setFullscreen",
            Some(json!({ "fullscreen": fullscreen_value })),
        ) {
            stremio_lightning_core::logging::error(
                "native.window",
                format!("[StremioLightning] Failed to emit fullscreen state: {error}"),
            );
        }
        drain_host_events(webview, runtime);
    }
}

fn drain_host_events(
    webview: &WebKitWebView,
    runtime: &LinuxWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>,
) {
    drain_runtime_events_to_webview(runtime, webview);
}

fn external_url_from_ipc_request(request: &WebkitIpcRequest) -> Option<String> {
    if request.kind != "invoke" {
        return None;
    }

    let payload = request.payload.as_ref()?;

    if payload.get("command").and_then(Value::as_str) != Some("open_external_url") {
        return None;
    }

    payload
        .get("payload")
        .and_then(|payload| payload.get("url"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn open_external_uri(uri: &str) -> Result<(), String> {
    gtk::gio::AppInfo::launch_default_for_uri(uri, None::<&gtk::gio::AppLaunchContext>)
        .map_err(|error| error.to_string())
}

/// Runs the shared scheme allowlist before touching the OS, so a `file://` or
/// `javascript:` navigation target can never be launched either.
fn open_allowed_external_uri(uri: &str) -> Result<(), String> {
    stremio_lightning_core::navigation::validate_external_url(uri)?;
    open_external_uri(uri)
}

fn resolve_ipc_script(id: u64, ok: bool, value: Value) -> String {
    format!(
        "window.__STREMIO_LIGHTNING_LINUX_IPC_RESOLVE__({id}, {ok}, {value});",
        value = value
    )
}

fn evaluate_javascript(webview: &WebKitWebView, script: &str) {
    webview.evaluate_javascript(script, None, None, gtk::gio::Cancellable::NONE, |result| {
        if let Err(error) = result {
            stremio_lightning_core::logging::error(
                "native.webview",
                format!("[StremioLightning] Failed to run webview JavaScript: {error}"),
            );
        }
    });
}

fn mpv_get_proc_address(_context: &GLContext, name: &str) -> *mut c_void {
    epoxy::get_proc_addr(name) as _
}

fn load_epoxy() -> Result<(), String> {
    static EPOXY_LOADED: OnceLock<Result<(), String>> = OnceLock::new();

    EPOXY_LOADED
        .get_or_init(|| {
            // SAFETY: Loading the libepoxy.so.0 shared library from standard system paths is safe
            // and expected in a GTK Linux desktop environment that runs OpenGL overlays.
            let library = unsafe { libloading::os::unix::Library::new("libepoxy.so.0") }
                .map_err(|error| format!("Failed to load libepoxy: {error}"))?;
            let library = Box::leak(Box::new(library));

            epoxy::load_with(|name| {
                // SAFETY: Retrieving a raw function symbol by its null-terminated name is safe
                // as long as the shared library exists and is alive (guaranteed by Box::leak above).
                unsafe { library.get::<*const c_void>(name.as_bytes()) }
                    .map(|symbol| *symbol)
                    .unwrap_or(ptr::null())
            });

            Ok(())
        })
        .clone()
}

struct NativeVideoState {
    mpv: RefCell<Mpv>,
    render_context: RefCell<Option<RenderContext>>,
    render_error_logged: Cell<bool>,
    shutting_down: Cell<bool>,
}

impl NativeVideoState {
    fn new() -> Result<Self, String> {
        // SAFETY: Setting LC_NUMERIC to "C" is required so that libmpv parses float properties
        // using standard decimals (e.g. `0.5`) regardless of the user's host OS system locale.
        // This must be run during initialization before multiple worker threads run concurrently.
        unsafe {
            setlocale(LC_NUMERIC, c"C".as_ptr());
        }

        let mpv = Mpv::with_initializer(|init| {
            init.set_property("vo", "libmpv")?;
            init.set_property("video-timing-offset", "0")?;
            init.set_property("terminal", "yes")?;
            init.set_property("cache", "yes")?;
            init.set_property("hwdec", "yes")?;
            Ok(())
        })
        .map_err(|error| format!("Failed to create mpv: {error}"))?;

        mpv.disable_deprecated_events().ok();

        Ok(Self {
            mpv: RefCell::new(mpv),
            render_context: RefCell::default(),
            render_error_logged: Cell::default(),
            shutting_down: Cell::default(),
        })
    }

    fn shutdown(&self) {
        if self.shutting_down.replace(true) {
            return;
        }

        self.render_context.borrow_mut().take();
        self.command("stop", &[]);
        self.command("quit", &[]);
    }

    fn current_fbo(&self) -> i32 {
        let mut current_fbo = 0;
        // SAFETY: epoxy::GetIntegerv is safe to invoke when a valid GL Context is active.
        unsafe {
            epoxy::GetIntegerv(epoxy::FRAMEBUFFER_BINDING, &mut current_fbo);
        }
        current_fbo
    }

    fn handle_command(&self, command: MpvBackendCommand) {
        match command {
            MpvBackendCommand::ObserveProperty(name) => self.observe_property(&name),
            MpvBackendCommand::SetProperty { name, value } => self.set_property(&name, value),
            MpvBackendCommand::Command { name, args } => self.command(&name, &args),
            MpvBackendCommand::Stop => self.command("stop", &[]),
        }
    }

    fn observe_property(&self, name: &str) {
        let format = match mpv_property_format(name) {
            MpvPropertyFormat::Flag => Format::Flag,
            MpvPropertyFormat::Int => Format::Int64,
            MpvPropertyFormat::Double => Format::Double,
            MpvPropertyFormat::String => Format::String,
        };

        if let Err(error) = self.mpv.borrow().observe_property(name, format, 0) {
            stremio_lightning_core::logging::error(
                "native.player",
                format!("[StremioLightning] Failed to observe MPV property {name}: {error}"),
            );
        }
    }

    fn set_property(&self, name: &str, value: Value) {
        let result = match value {
            Value::Bool(value) => self.mpv.borrow().set_property(name, value),
            Value::Number(value) => value
                .as_f64()
                .ok_or(libmpv2::Error::Raw(-4))
                .and_then(|value| self.mpv.borrow().set_property(name, value)),
            Value::String(value) => self.mpv.borrow().set_property(name, value.as_str()),
            other => self
                .mpv
                .borrow()
                .set_property(name, other.to_string().as_str()),
        };

        if let Err(error) = result {
            stremio_lightning_core::logging::error(
                "native.player",
                format!("[StremioLightning] Failed to set MPV property {name}: {error}"),
            );
        }
    }

    fn command(&self, name: &str, args: &[String]) {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        if let Err(error) = self.mpv.borrow().command(name, &args) {
            stremio_lightning_core::logging::error(
                "native.player",
                format!("[StremioLightning] Failed to run MPV command {name}: {error}"),
            );
        }
    }

    fn poll_event<T: FnOnce(Event)>(&self, callback: T) -> bool {
        let mut mpv = self.mpv.borrow_mut();
        let Some(result) = mpv.wait_event(0.0) else {
            return false;
        };

        match result {
            Ok(event) => callback(event),
            Err(error) => stremio_lightning_core::logging::error(
                "native.player",
                format!("[StremioLightning] Failed to read MPV event: {error}"),
            ),
        }

        true
    }
}

fn build_native_video(
    player: MpvPlayerBackend,
    runtime: Rc<LinuxWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>>,
    webview: WebKitWebView,
    window: gtk::ApplicationWindow,
    fullscreen: Rc<Cell<bool>>,
) -> Result<(gtk::GLArea, Rc<NativeVideoState>), String> {
    let area = gtk::GLArea::new();
    let state = Rc::new(NativeVideoState::new()?);

    // Bridge standard blocking channel required by the player backend to the single-threaded GLib main loop.
    let (std_sender, std_receiver) = mpsc::channel::<MpvBackendCommand>();
    player.attach(std_sender)?;

    let (glib_sender, mut glib_receiver) =
        tokio::sync::mpsc::unbounded_channel::<MpvBackendCommand>();
    let state_for_command = state.clone();
    glib::MainContext::default().spawn_local(async move {
        while let Some(command) = glib_receiver.recv().await {
            if state_for_command.shutting_down.get() {
                break;
            }
            state_for_command.handle_command(command);
        }
    });

    std::thread::spawn(move || {
        while let Ok(command) = std_receiver.recv() {
            if glib_sender.send(command).is_err() {
                break;
            }
        }
    });

    install_mpv_event_drain(&state, runtime, webview, window, fullscreen);

    {
        let state = state.clone();
        area.connect_realize(move |area| {
            area.make_current();
            if area.error().is_some() {
                return;
            }

            if let Some(context) = area.context() {
                let mut mpv = state.mpv.borrow_mut();
                // SAFETY: mpv.ctx is a valid, non-null raw pointer to the underlying mpv_handle
                // managed securely by the libmpv2 Mpv instance, which remains alive and active.
                let mpv_handle = unsafe { mpv.ctx.as_mut() };
                let mut render_context = match RenderContext::new(
                    mpv_handle,
                    vec![
                        RenderParam::ApiType(RenderParamApiType::OpenGl),
                        RenderParam::InitParams(OpenGLInitParams {
                            get_proc_address: mpv_get_proc_address,
                            ctx: context,
                        }),
                        RenderParam::BlockForTargetTime(false),
                    ],
                ) {
                    Ok(render_context) => render_context,
                    Err(error) => {
                        stremio_lightning_core::logging::error(
                            "native.player",
                            format!(
                                "[StremioLightning] Failed to create MPV render context: {error}"
                            ),
                        );
                        return;
                    }
                };

                // Safely request GLArea redrawing on the main GTK/GLib thread from the background MPV render thread.
                let (glib_sender, mut glib_receiver) = tokio::sync::mpsc::unbounded_channel::<()>();
                let area_for_render = area.clone();
                let state_for_render = state.clone();
                glib::MainContext::default().spawn_local(async move {
                    while glib_receiver.recv().await.is_some() {
                        if state_for_render.shutting_down.get() {
                            break;
                        }
                        area_for_render.queue_render();
                    }
                });

                render_context.set_update_callback(move || {
                    glib_sender.send(()).ok();
                });

                *state.render_context.borrow_mut() = Some(render_context);
            }
        });
    }

    {
        let state = state.clone();
        area.connect_unrealize(move |_| {
            state.render_context.borrow_mut().take();
        });
    }

    {
        let state = state.clone();
        area.connect_render(move |area, _context| {
            if state.shutting_down.get() {
                return Propagation::Stop;
            }

            if let Some(ref render_context) = *state.render_context.borrow() {
                let scale = area.scale_factor();
                if let Err(error) = render_context.render::<GLContext>(
                    state.current_fbo(),
                    area.width() * scale,
                    area.height() * scale,
                    true,
                ) {
                    if !state.render_error_logged.replace(true) {
                        stremio_lightning_core::logging::error(
                            "native.player",
                            format!("[StremioLightning] Failed to render MPV frame: {error}"),
                        );
                    }
                } else {
                    state.render_error_logged.set(false);
                }
            }
            Propagation::Stop
        });
    }

    Ok((area, state))
}

fn install_mpv_event_drain(
    state: &Rc<NativeVideoState>,
    runtime: Rc<LinuxWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>>,
    webview: WebKitWebView,
    window: gtk::ApplicationWindow,
    fullscreen: Rc<Cell<bool>>,
) {
    let state = state.clone();
    glib::timeout_add_local(Duration::from_millis(16), move || {
        if state.shutting_down.get() {
            return glib::ControlFlow::Break;
        }

        while state.poll_event(|event| match event {
            Event::PropertyChange { name, change, .. } => {
                if let Some(value) = property_data_to_json(name, change) {
                    if let Err(error) = runtime.emit_native_player_property_changed(name, value) {
                        stremio_lightning_core::logging::error(
                            "native.player",
                            format!(
                                "[StremioLightning] Failed to emit MPV property change: {error}"
                            ),
                        );
                    }
                }
            }
            Event::EndFile(reason) => {
                let mut controller = NativeWindowController {
                    webview: &webview,
                    runtime: &runtime,
                    window: &window,
                    fullscreen: &fullscreen,
                };
                if let Err(error) = runtime.exit_picture_in_picture_for_player_end(&mut controller)
                {
                    stremio_lightning_core::logging::error(
                        "native.pip",
                        format!("[StremioLightning] Failed to exit PiP after MPV ended: {error}"),
                    );
                }
                let ended = PlayerEnded::from_cause(end_file_cause(reason));
                if let Err(error) = runtime.emit_native_player_ended(ended) {
                    stremio_lightning_core::logging::error(
                        "native.player",
                        format!("[StremioLightning] Failed to emit MPV ended event: {error}"),
                    );
                }
            }
            _ => {}
        }) {}

        drain_runtime_events_to_webview(&runtime, &webview);
        glib::ControlFlow::Continue
    });
}

fn property_data_to_json(name: &str, change: PropertyData) -> Option<Value> {
    match change {
        PropertyData::Str(value) | PropertyData::OsdStr(value) => {
            Some(if is_json_string_property(name) {
                serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string()))
            } else {
                Value::String(value.to_string())
            })
        }
        PropertyData::Flag(value) => Some(Value::Bool(value)),
        PropertyData::Int64(value) => Some(json!(value)),
        PropertyData::Double(value) => serde_json::Number::from_f64(value).map(Value::Number),
        _ => None,
    }
}

fn end_file_cause(reason: libmpv2::EndFileReason) -> EndFileCause {
    match reason {
        mpv_end_file_reason::Eof => EndFileCause::Eof,
        mpv_end_file_reason::Stop => EndFileCause::Stop,
        mpv_end_file_reason::Redirect => EndFileCause::Redirect,
        mpv_end_file_reason::Error => EndFileCause::Error,
        mpv_end_file_reason::Quit => EndFileCause::Quit,
        _ => EndFileCause::Other,
    }
}

fn drain_runtime_events_to_webview(
    runtime: &LinuxWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>,
    webview: &WebKitWebView,
) {
    match runtime.drain_event_dispatch_scripts() {
        Ok(scripts) => {
            for script in scripts {
                evaluate_javascript(webview, &script);
            }
        }
        Err(error) => stremio_lightning_core::logging::error(
            "native.ipc",
            format!("[StremioLightning] Failed to drain host events: {error}"),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webkit_ipc_adapter_installs_expected_global() {
        let script = webkit_ipc_adapter();
        assert!(script.contains("__STREMIO_LIGHTNING_LINUX_IPC__"));
        assert!(script.contains("__STREMIO_LIGHTNING_LINUX_IPC_RESOLVE__"));
        assert!(script.contains("window.webkit.messageHandlers.ipc"));
    }

    #[test]
    fn resolve_ipc_script_embeds_json_value() {
        assert_eq!(
            resolve_ipc_script(7, true, json!({"ok": true})),
            r#"window.__STREMIO_LIGHTNING_LINUX_IPC_RESOLVE__(7, true, {"ok":true});"#
        );
    }

    #[test]
    fn extracts_open_external_url_ipc_request() {
        let request = WebkitIpcRequest {
            id: 1,
            kind: "invoke".to_string(),
            payload: Some(json!({
                "command": "open_external_url",
                "payload": { "url": "https://www.strem.io/login-fb" }
            })),
        };

        assert_eq!(
            external_url_from_ipc_request(&request),
            Some("https://www.strem.io/login-fb".to_string())
        );
    }

    #[test]
    fn ignores_non_external_url_ipc_request() {
        let request = WebkitIpcRequest {
            id: 1,
            kind: "invoke".to_string(),
            payload: Some(json!({
                "command": "get_streaming_server_status",
                "payload": null
            })),
        };

        assert_eq!(external_url_from_ipc_request(&request), None);
    }

    #[test]
    fn extracts_invoke_command() {
        let payload = json!({"command": "toggle_pip", "payload": null});
        assert_eq!(invoke_command(Some(&payload)), Some("toggle_pip"));
        assert_eq!(invoke_command(None), None);
    }

    #[test]
    fn extracts_shell_transport_fullscreen_request() {
        let payload = json!({
            "command": "shell_transport_send",
            "payload": {
                "message": r#"{"id":7,"type":6,"args":["win-set-visibility",{"fullscreen":true}]}"#
            }
        });

        assert_eq!(
            shell_transport_fullscreen_request(Some(&payload)).unwrap(),
            Some(true)
        );
    }

    #[test]
    fn mpv_property_formats_cover_the_official_loading_properties() {
        for name in ["buffering", "seeking", "paused-for-cache", "eof-reached"] {
            assert_eq!(mpv_property_format(name), MpvPropertyFormat::Flag, "{name}");
        }
        for name in ["aid", "vid", "sid", "secondary-sid"] {
            assert_eq!(mpv_property_format(name), MpvPropertyFormat::Int, "{name}");
        }
        for name in ["cache-buffering-state", "demuxer-cache-time"] {
            assert_eq!(
                mpv_property_format(name),
                MpvPropertyFormat::Double,
                "{name}"
            );
        }
        // `mute` is an MPV flag, so it must not arrive as a number or a string.
        assert_eq!(mpv_property_format("mute"), MpvPropertyFormat::Flag);
    }

    #[test]
    fn serializes_integer_property_changes_as_json_numbers() {
        assert_eq!(
            property_data_to_json("aid", PropertyData::Int64(7)),
            Some(json!(7))
        );
        assert_eq!(
            property_data_to_json("mute", PropertyData::Flag(true)),
            Some(json!(true))
        );
    }

    #[test]
    fn maps_end_file_reasons_onto_the_shared_vocabulary() {
        assert_eq!(end_file_cause(mpv_end_file_reason::Eof), EndFileCause::Eof);
        assert_eq!(
            end_file_cause(mpv_end_file_reason::Stop),
            EndFileCause::Stop
        );
        assert_eq!(
            end_file_cause(mpv_end_file_reason::Redirect),
            EndFileCause::Redirect
        );
        assert_eq!(
            end_file_cause(mpv_end_file_reason::Error),
            EndFileCause::Error
        );
        assert_eq!(
            end_file_cause(mpv_end_file_reason::Quit),
            EndFileCause::Quit
        );
    }

    #[test]
    fn webkit_failure_descriptors_never_include_raw_uris() {
        assert_eq!(
            safe_webview_resource_descriptor(Some(
                "https://api.example.test/stream/token?secret=hidden"
            )),
            "https resource"
        );
        assert_eq!(
            safe_webview_resource_descriptor(Some("file:///home/private/video.mkv")),
            "file resource"
        );
        assert_eq!(safe_webview_resource_descriptor(None), "unknown resource");
    }
}
