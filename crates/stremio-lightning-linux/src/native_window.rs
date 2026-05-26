use crate::app::AppConfig;
use crate::player::{MpvBackendCommand, MpvPlayerBackend};
use crate::streaming_server::RealProcessSpawner;
use crate::webview_runtime::LinuxWebviewRuntime;
use gdk_pixbuf::Pixbuf;
use gtk::gdk::{Display, GLContext, RGBA};
use gtk::glib::{self, Propagation};
use gtk::prelude::*;
use libc::{c_char, c_int, c_long, c_uchar, c_ulong, setlocale, LC_NUMERIC};
use libmpv2::events::{Event, PropertyData};
use libmpv2::render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType};
use libmpv2::{Format, Mpv};
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
use stremio_lightning_core::pip::{
    PipRestoreSnapshot, PipWindowController, PIP_WINDOW_HEIGHT, PIP_WINDOW_WIDTH,
};
use webkit::prelude::*;
use webkit::{
    NavigationPolicyDecision, PolicyDecisionType, UserContentInjectedFrames, UserScript,
    UserScriptInjectionTime, WebView as WebKitWebView,
};

const IPC_HANDLER_NAME: &str = "ipc";
const APP_ID: &str = "io.github.theguy000.StremioLightning";
const APP_NAME: &str = "Stremio Lightning";
const DEV_ICON_NAME: &str = "128x128";
const DEFAULT_WINDOW_WIDTH: i32 = 1500;
const DEFAULT_WINDOW_HEIGHT: i32 = 850;
const MIN_WINDOW_WIDTH: i32 = 800;
const MIN_WINDOW_HEIGHT: i32 = 600;

thread_local! {
    static LAST_NORMAL_SIZE: RefCell<Option<(i32, i32)>> = const { RefCell::new(None) };
}
const X11_CLIENT_MESSAGE: c_int = 33;
const X11_PROP_MODE_REPLACE: c_int = 0;
const X11_PROP_MODE_REMOVE: c_long = 0;
const X11_PROP_MODE_ADD: c_long = 1;
const X11_SUBSTRUCTURE_NOTIFY_MASK: c_long = 1 << 19;
const X11_SUBSTRUCTURE_REDIRECT_MASK: c_long = 1 << 20;
const MPV_FLOAT_PROPERTIES: &[&str] = &[
    "time-pos",
    "duration",
    "volume",
    "speed",
    "sub-pos",
    "sub-scale",
    "sub-delay",
    "cache-buffering-state",
    "demuxer-cache-time",
    "panscan",
];
const MPV_BOOL_PROPERTIES: &[&str] = &[
    "pause",
    "buffering",
    "seeking",
    "osc",
    "input-default-bindings",
    "input-vo-keyboard",
    "eof-reached",
    "paused-for-cache",
    "keepaspect",
];
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

#[repr(C)]
union XClientMessageData {
    b: [c_char; 20],
    s: [i16; 10],
    l: [c_long; 5],
}

#[repr(C)]
struct XClientMessageEvent {
    type_: c_int,
    serial: c_ulong,
    send_event: c_int,
    display: *mut c_void,
    window: c_ulong,
    message_type: c_ulong,
    format: c_int,
    data: XClientMessageData,
}

unsafe extern "C" {
    fn gdk_x11_display_get_xdisplay(display: *mut c_void) -> *mut c_void;
    fn gdk_x11_surface_get_xid(surface: *mut c_void) -> c_ulong;
}

#[link(name = "X11")]
unsafe extern "C" {
    fn XDefaultRootWindow(display: *mut c_void) -> c_ulong;
    fn XFlush(display: *mut c_void) -> c_int;
    fn XInternAtom(
        display: *mut c_void,
        atom_name: *const c_char,
        only_if_exists: c_int,
    ) -> c_ulong;
    fn XChangeProperty(
        display: *mut c_void,
        window: c_ulong,
        property: c_ulong,
        type_: c_ulong,
        format: c_int,
        mode: c_int,
        data: *const c_uchar,
        nelements: c_int,
    ) -> c_int;
    fn XSendEvent(
        display: *mut c_void,
        window: c_ulong,
        propagate: c_int,
        event_mask: c_long,
        event_send: *mut XClientMessageEvent,
    ) -> c_int;
}

pub fn run_native_window(
    config: AppConfig,
    mut runtime: LinuxWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>,
    player: MpvPlayerBackend,
) -> Result<(), String> {
    load_epoxy()?;

    let _state = runtime.load()?;
    glib::set_application_name(APP_NAME);
    glib::set_prgname(Some(APP_ID));
    let app = gtk::Application::new(Some(APP_ID), gtk::gio::ApplicationFlags::NON_UNIQUE);
    let runtime = Rc::new(runtime);
    let startup_error: Rc<RefCell<Option<String>>> = Rc::default();

    {
        let runtime = runtime.clone();
        let player = player.clone();
        let startup_error = startup_error.clone();
        app.connect_activate(move |app| {
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
    let overlay = gtk::Overlay::new();
    let webview = build_webview(config, runtime.clone(), window.clone(), fullscreen.clone())?;
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
                eprintln!("[StremioLightning] Failed to shut down Linux runtime: {error}");
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

    window.present();
    Ok(())
}

fn configure_application_icon_name() -> &'static str {
    let Some(display) = Display::default() else {
        eprintln!(
            "[StremioLightning] Unable to resolve Linux window icon: no GTK display available"
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
        eprintln!(
            "[StremioLightning] Unable to resolve Linux window icon: missing {APP_ID} or {DEV_ICON_NAME} in GTK icon theme"
        );
        APP_ID
    }
}

fn source_tree_icon_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/icons")
}

fn install_source_tree_window_icon(window: &gtk::ApplicationWindow) {
    let window = window.clone();
    window.connect_realize(move |window| {
        let Some(surface) = window.surface() else {
            return;
        };
        if !is_x11_surface(&surface) {
            return;
        }
        if let Err(error) = set_x11_window_icon(&surface) {
            eprintln!("[StremioLightning] Failed to set Linux taskbar icon: {error}");
        }
    });
}

fn set_x11_window_icon(surface: &gtk::gdk::Surface) -> Result<(), String> {
    const NET_WM_ICON: &[u8] = b"_NET_WM_ICON\0";
    const CARDINAL: &[u8] = b"CARDINAL\0";

    const ICON_BYTES: &[u8] = include_bytes!("../../../assets/icons/128x128.png");

    let bytes = gtk::glib::Bytes::from(ICON_BYTES);
    let stream = gtk::gio::MemoryInputStream::from_bytes(&bytes);
    let pixbuf = Pixbuf::from_stream(&stream, None::<&gtk::gio::Cancellable>)
        .map_err(|error| format!("failed to load icon from bytes: {error}"))?;

    let width = pixbuf.width();
    let height = pixbuf.height();
    let channels = pixbuf.n_channels();
    if width <= 0 || height <= 0 || channels < 3 {
        return Err("invalid icon image".to_string());
    }

    let pixels = pixbuf.read_pixel_bytes();
    let pixels = pixels.as_ref();
    let rowstride = pixbuf.rowstride() as usize;
    let width_usize = width as usize;
    let height_usize = height as usize;
    let channels_usize = channels as usize;

    // Guard against potential out-of-bounds access if pixel data size is insufficient
    let expected_min_len = (height_usize - 1) * rowstride + width_usize * channels_usize;
    if pixels.len() < expected_min_len {
        return Err("pixel buffer size is insufficient for icon dimensions".to_string());
    }

    let mut icon_data = Vec::with_capacity(2 + width_usize * height_usize);
    icon_data.push(width as c_ulong);
    icon_data.push(height as c_ulong);
    for y in 0..height_usize {
        let row_start = y * rowstride;
        for x in 0..width_usize {
            let offset = row_start + x * channels_usize;
            let r = pixels[offset] as c_ulong;
            let g = pixels[offset + 1] as c_ulong;
            let b = pixels[offset + 2] as c_ulong;
            let a = if channels_usize >= 4 {
                pixels[offset + 3] as c_ulong
            } else {
                0xff
            };
            icon_data.push((a << 24) | (r << 16) | (g << 8) | b);
        }
    }

    let display = surface.display();
    // SAFETY: gdk_x11_display_get_xdisplay expects a valid GdkDisplay pointer.
    // display.as_ptr() is guaranteed to be valid by GDK display handles.
    let xdisplay = unsafe { gdk_x11_display_get_xdisplay(display.as_ptr() as *mut c_void) };
    if xdisplay.is_null() {
        return Err("failed to read X11 display".to_string());
    }

    // SAFETY: gdk_x11_surface_get_xid expects a valid GdkSurface pointer.
    // surface.as_ptr() is guaranteed to be valid by GDK surface handles.
    let xid = unsafe { gdk_x11_surface_get_xid(surface.as_ptr() as *mut c_void) };
    if xid == 0 {
        return Err("failed to read X11 window id".to_string());
    }

    // SAFETY: XInternAtom is standard Xlib FFI. xdisplay is verified non-null.
    // String slices are null-terminated byte slices.
    let icon_atom = unsafe { XInternAtom(xdisplay, NET_WM_ICON.as_ptr().cast(), 0) };
    let cardinal_atom = unsafe { XInternAtom(xdisplay, CARDINAL.as_ptr().cast(), 0) };
    if icon_atom == 0 || cardinal_atom == 0 {
        return Err("failed to resolve X11 taskbar icon atoms".to_string());
    }

    // SAFETY: XChangeProperty modifies window properties. Parameters are verified to be aligned
    // and matched. icon_data has format 32 (long array). XFlush synchronizes display server buffers.
    unsafe {
        XChangeProperty(
            xdisplay,
            xid,
            icon_atom,
            cardinal_atom,
            32,
            X11_PROP_MODE_REPLACE,
            icon_data.as_ptr().cast::<c_uchar>(),
            icon_data.len() as c_int,
        );
        XFlush(xdisplay);
    }

    Ok(())
}

fn build_webview(
    config: &AppConfig,
    runtime: Rc<LinuxWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>>,
    window: gtk::ApplicationWindow,
    fullscreen: Rc<Cell<bool>>,
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

    {
        let webview = webview.clone();
        let runtime = runtime.clone();
        let window = window.clone();
        let fullscreen = fullscreen.clone();
        user_content.connect_script_message_received(Some(IPC_HANDLER_NAME), move |_, value| {
            handle_ipc_message(&webview, &runtime, &window, &fullscreen, &value.to_string());
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

    webview.connect_decide_policy(move |_, decision, decision_type| {
        if decision_type == PolicyDecisionType::NewWindowAction {
            if let Some(uri) = decision
                .downcast_ref::<NavigationPolicyDecision>()
                .and_then(|decision| decision.navigation_action())
                .and_then(|action| action.request())
                .and_then(|request| request.uri())
            {
                if let Err(error) = open_external_uri(uri.as_str()) {
                    eprintln!("[StremioLightning] Failed to open external URL {uri}: {error}");
                }
            }
            decision.ignore();
            return true;
        }

        false
    });

    webview.load_uri(&config.url);
    Ok(webview)
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
        Err(error) => eprintln!("[StremioLightning] {error}"),
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
                    let _enabled = self.toggle_picture_in_picture(&mut controller)?;
                    return Ok(Value::Null);
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
    fn enter_pip(&mut self) -> Result<PipRestoreSnapshot, String> {
        let was_fullscreen = self.fullscreen.get();
        let saved_size = if was_fullscreen {
            LAST_NORMAL_SIZE
                .with(|cell| *cell.borrow())
                .or(Some((DEFAULT_WINDOW_WIDTH, DEFAULT_WINDOW_HEIGHT)))
        } else {
            let width = self.window.width();
            let height = self.window.height();
            (width > 0 && height > 0).then_some((width, height))
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
        self.window.set_resizable(false);
        self.window
            .set_size_request(PIP_WINDOW_WIDTH, PIP_WINDOW_HEIGHT);
        self.window.set_decorated(false);
        request_window_above(self.window, true)?;
        self.window
            .set_default_size(PIP_WINDOW_WIDTH, PIP_WINDOW_HEIGHT);
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
        if snapshot.was_fullscreen {
            if let Some((width, height)) = snapshot.saved_size {
                self.window.set_resizable(false);
                self.window
                    .set_size_request(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT);
                self.window.set_size_request(width, height);
                self.window.set_default_size(width, height);
            }
            self.window
                .set_size_request(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT);
            self.window.set_resizable(true);
            set_window_fullscreen(
                self.webview,
                self.runtime,
                self.window,
                self.fullscreen,
                true,
            );
        } else if let Some((width, height)) = snapshot.saved_size {
            self.window.set_resizable(false);
            self.window
                .set_size_request(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT);
            self.window.set_size_request(width, height);
            self.window.set_default_size(width, height);
            let window = self.window.clone();
            glib::timeout_add_local_once(Duration::from_millis(250), move || {
                window.set_size_request(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT);
                window.set_resizable(true);
            });
        } else {
            self.window
                .set_size_request(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT);
            self.window.set_resizable(true);
        }

        self.window.present();
        Ok(())
    }
}

fn request_window_above(window: &gtk::ApplicationWindow, above: bool) -> Result<(), String> {
    let surface = window
        .surface()
        .ok_or_else(|| "Cannot update PiP window stacking before it has a surface".to_string())?;

    if !is_x11_surface(&surface) {
        if above {
            eprintln!(
                "[StremioLightning] PiP always-on-top is only available on Linux X11 sessions"
            );
        }
        return Ok(());
    }

    send_x11_window_state_above(&surface, above)
}

fn is_x11_surface(surface: &gtk::gdk::Surface) -> bool {
    surface.type_().name().contains("X11")
}

fn send_x11_window_state_above(surface: &gtk::gdk::Surface, above: bool) -> Result<(), String> {
    const NET_WM_STATE: &[u8] = b"_NET_WM_STATE\0";
    const NET_WM_STATE_ABOVE: &[u8] = b"_NET_WM_STATE_ABOVE\0";

    let display = surface.display();

    // SAFETY: Standard X11 FFI operations with verified display, window, and atom handles.
    unsafe {
        let xdisplay = gdk_x11_display_get_xdisplay(display.as_ptr() as *mut c_void);
        if xdisplay.is_null() {
            return Err("Failed to read X11 display for PiP window".to_string());
        }

        let xid = gdk_x11_surface_get_xid(surface.as_ptr() as *mut c_void);
        if xid == 0 {
            return Err("Failed to read X11 window id for PiP window".to_string());
        }

        let state_atom = XInternAtom(xdisplay, NET_WM_STATE.as_ptr().cast(), 0);
        let above_atom = XInternAtom(xdisplay, NET_WM_STATE_ABOVE.as_ptr().cast(), 0);
        if state_atom == 0 || above_atom == 0 {
            return Err("Failed to resolve X11 PiP always-on-top atoms".to_string());
        }

        let action = if above {
            X11_PROP_MODE_ADD
        } else {
            X11_PROP_MODE_REMOVE
        };
        let mut event = XClientMessageEvent {
            type_: X11_CLIENT_MESSAGE,
            serial: 0,
            send_event: 1,
            display: xdisplay,
            window: xid,
            message_type: state_atom,
            format: 32,
            data: XClientMessageData {
                l: [action, above_atom as c_long, 0, 1, 0],
            },
        };

        let root = XDefaultRootWindow(xdisplay);

        let sent = XSendEvent(
            xdisplay,
            root,
            0,
            X11_SUBSTRUCTURE_REDIRECT_MASK | X11_SUBSTRUCTURE_NOTIFY_MASK,
            &mut event,
        );
        if sent == 0 {
            return Err("Failed to send X11 PiP always-on-top request".to_string());
        }

        XFlush(xdisplay);
    }

    Ok(())
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
            eprintln!("[StremioLightning] Failed to emit fullscreen state: {error}");
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

fn resolve_ipc_script(id: u64, ok: bool, value: Value) -> String {
    format!(
        "window.__STREMIO_LIGHTNING_LINUX_IPC_RESOLVE__({id}, {ok}, {value});",
        value = value
    )
}

fn evaluate_javascript(webview: &WebKitWebView, script: &str) {
    webview.evaluate_javascript(script, None, None, gtk::gio::Cancellable::NONE, |result| {
        if let Err(error) = result {
            eprintln!("[StremioLightning] Failed to run webview JavaScript: {error}");
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
        let format = if MPV_BOOL_PROPERTIES.contains(&name) {
            Format::Flag
        } else if MPV_FLOAT_PROPERTIES.contains(&name) {
            Format::Double
        } else {
            Format::String
        };

        if let Err(error) = self.mpv.borrow().observe_property(name, format, 0) {
            eprintln!("[StremioLightning] Failed to observe MPV property {name}: {error}");
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
            eprintln!("[StremioLightning] Failed to set MPV property {name}: {error}");
        }
    }

    fn command(&self, name: &str, args: &[String]) {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        if let Err(error) = self.mpv.borrow().command(name, &args) {
            eprintln!("[StremioLightning] Failed to run MPV command {name}: {error}");
        }
    }

    fn poll_event<T: FnOnce(Event)>(&self, callback: T) -> bool {
        let mut mpv = self.mpv.borrow_mut();
        let Some(result) = mpv.wait_event(0.0) else {
            return false;
        };

        match result {
            Ok(event) => callback(event),
            Err(error) => eprintln!("[StremioLightning] Failed to read MPV event: {error}"),
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
                let mut render_context = RenderContext::new(
                    mpv_handle,
                    vec![
                        RenderParam::ApiType(RenderParamApiType::OpenGl),
                        RenderParam::InitParams(OpenGLInitParams {
                            get_proc_address: mpv_get_proc_address,
                            ctx: context,
                        }),
                        RenderParam::BlockForTargetTime(false),
                    ],
                )
                .expect("Failed to create mpv render context");

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
                render_context
                    .render::<GLContext>(
                        state.current_fbo(),
                        area.width() * scale,
                        area.height() * scale,
                        true,
                    )
                    .expect("Failed to render mpv frame");
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
                if let Some(value) = property_data_to_json(change) {
                    if let Err(error) = runtime.emit_native_player_property_changed(name, value) {
                        eprintln!("[StremioLightning] Failed to emit MPV property change: {error}");
                    }
                }
            }
            Event::EndFile(_) => {
                let mut controller = NativeWindowController {
                    webview: &webview,
                    runtime: &runtime,
                    window: &window,
                    fullscreen: &fullscreen,
                };
                if let Err(error) = runtime.exit_picture_in_picture_for_player_end(&mut controller)
                {
                    eprintln!("[StremioLightning] Failed to exit PiP after MPV ended: {error}");
                }
                if let Err(error) = runtime.emit_native_player_ended("eof") {
                    eprintln!("[StremioLightning] Failed to emit MPV ended event: {error}");
                }
            }
            _ => {}
        }) {}

        drain_runtime_events_to_webview(&runtime, &webview);
        glib::ControlFlow::Continue
    });
}

fn property_data_to_json(change: PropertyData) -> Option<Value> {
    match change {
        PropertyData::Str(value) => {
            Some(serde_json::from_str(value).unwrap_or_else(|_| Value::String(value.to_string())))
        }
        PropertyData::Flag(value) => Some(Value::Bool(value)),
        PropertyData::Double(value) => serde_json::Number::from_f64(value).map(Value::Number),
        _ => None,
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
        Err(error) => eprintln!("[StremioLightning] Failed to drain host events: {error}"),
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
    fn mpv_property_type_lists_match_official_loading_properties() {
        assert!(MPV_BOOL_PROPERTIES.contains(&"buffering"));
        assert!(MPV_BOOL_PROPERTIES.contains(&"seeking"));
        assert!(MPV_BOOL_PROPERTIES.contains(&"paused-for-cache"));
        assert!(MPV_BOOL_PROPERTIES.contains(&"eof-reached"));
        assert!(MPV_FLOAT_PROPERTIES.contains(&"cache-buffering-state"));
        assert!(MPV_FLOAT_PROPERTIES.contains(&"demuxer-cache-time"));
    }
}
