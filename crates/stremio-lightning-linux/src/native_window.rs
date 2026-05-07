use crate::app::AppConfig;
use crate::player::MpvPlayerBackend;
use crate::streaming_server::RealProcessSpawner;
use crate::webview_runtime::LinuxWebviewRuntime;
use gtk::gdk::{GLContext, RGBA};
use gtk::glib::{self, Propagation};
use gtk::prelude::*;
use libc::{setlocale, LC_NUMERIC};
use libmpv2::render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType};
use libmpv2::Mpv;
use serde::Deserialize;
use serde_json::{json, Value};
use std::cell::{Cell, RefCell};
use std::net::{SocketAddr, TcpStream};
use std::os::raw::c_void;
use std::ptr;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use webkit::prelude::*;
use webkit::{
    LoadEvent, UserContentInjectedFrames, UserScript, UserScriptInjectionTime,
    WebView as WebKitWebView,
};

const IPC_HANDLER_NAME: &str = "ipc";
const STREAMING_SERVER_ADDR: ([u8; 4], u16) = ([127, 0, 0, 1], 11470);
const STREAMING_SERVER_READY_TIMEOUT: Duration = Duration::from_secs(30);
const STREAMING_SERVER_POLL_INTERVAL: Duration = Duration::from_millis(250);
const STREAMING_SERVER_RELOAD_SCRIPT: &str = r#"
(function () {
    var attempts = 0;
    var maxAttempts = 120;
    var delayMs = 250;

    function getCoreTransport() {
        var coreService = (typeof core !== 'undefined' && core) ||
            (window.services && window.services.core) ||
            (window.app && window.app.core);
        return coreService && coreService.transport ? coreService.transport : null;
    }

    function dispatchReload() {
        attempts += 1;

        try {
            var transport = getCoreTransport();
            if (transport && typeof transport.dispatch === 'function') {
                transport.dispatch({ action: 'StreamingServer', args: { action: 'Reload' } });
                console.log('[StremioLightning] StreamingServer Reload dispatched');
                return;
            }
        } catch (error) {
            console.error('[StremioLightning] Reload error:', error);
            return;
        }

        if (attempts < maxAttempts) {
            setTimeout(dispatchReload, delayMs);
        } else {
            console.warn('[StremioLightning] core.transport not available for Reload after retrying');
        }
    }

    dispatchReload();
})();
"#;

#[derive(Debug, Deserialize)]
struct WebkitIpcRequest {
    id: u64,
    kind: String,
    payload: Option<Value>,
}

pub fn run_native_window(
    config: AppConfig,
    mut runtime: LinuxWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>,
) -> Result<(), String> {
    load_epoxy()?;

    let state = runtime.load()?;
    let app = gtk::Application::new(Some("com.stremio-lightning.linux"), Default::default());
    let runtime = Rc::new(runtime);
    let startup_error: Rc<RefCell<Option<String>>> = Rc::default();

    {
        let runtime = runtime.clone();
        let startup_error = startup_error.clone();
        app.connect_activate(move |app| {
            if let Err(error) = build_window(app, &config, runtime.clone()) {
                *startup_error.borrow_mut() = Some(error);
                app.quit();
            }
        });
    }

    println!(
        "[StremioLightning] GTK4/WebKitGTK6 webview load url={} document_start={}",
        state.url,
        state.document_start_scripts.join(" -> ")
    );

    let exit_code = app.run_with_args(&["stremio-lightning-linux"]);
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
) -> Result<(), String> {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Stremio Lightning Linux")
        .default_width(1500)
        .default_height(850)
        .build();

    let overlay = gtk::Overlay::new();
    let video = build_native_video();
    video.set_hexpand(true);
    video.set_vexpand(true);
    overlay.set_child(Some(&video));

    let webview = build_webview(config, runtime)?;
    overlay.add_overlay(&webview);
    window.set_child(Some(&overlay));
    window.present();
    Ok(())
}

fn build_webview(
    config: &AppConfig,
    runtime: Rc<LinuxWebviewRuntime<MpvPlayerBackend, RealProcessSpawner>>,
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
    }

    {
        let webview = webview.clone();
        let runtime = runtime.clone();
        user_content.connect_script_message_received(Some(IPC_HANDLER_NAME), move |_, value| {
            handle_ipc_message(&webview, &runtime, &value.to_string());
        });
    }

    {
        let reload_scheduled = Rc::new(Cell::new(false));
        let inspector_shown = Rc::new(Cell::new(false));
        let devtools = config.devtools;
        webview.connect_load_changed(move |webview, event| {
            if event == LoadEvent::Finished {
                if devtools && !inspector_shown.replace(true) {
                    if let Some(inspector) = webview.inspector() {
                        inspector.show();
                    }
                }
                if !reload_scheduled.replace(true) {
                    schedule_streaming_server_reload(webview);
                }
            }
        });
    }

    webview.load_uri(&config.url);
    Ok(webview)
}

fn schedule_streaming_server_reload(webview: &WebKitWebView) {
    let webview = webview.clone();
    let started_at = Instant::now();

    glib::timeout_add_local(STREAMING_SERVER_POLL_INTERVAL, move || {
        if is_streaming_server_ready() {
            eprintln!("[StreamingServer] Server HTTP ready, dispatching Reload");
            evaluate_javascript(&webview, STREAMING_SERVER_RELOAD_SCRIPT);
            return glib::ControlFlow::Break;
        }

        if started_at.elapsed() >= STREAMING_SERVER_READY_TIMEOUT {
            eprintln!("[StreamingServer] Server never became ready, skipping Reload");
            return glib::ControlFlow::Break;
        }

        glib::ControlFlow::Continue
    });
}

fn is_streaming_server_ready() -> bool {
    let addr = SocketAddr::from(STREAMING_SERVER_ADDR);
    TcpStream::connect_timeout(&addr, Duration::from_millis(100)).is_ok()
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
    raw: &str,
) {
    let response = serde_json::from_str::<WebkitIpcRequest>(raw)
        .map_err(|error| format!("Invalid Linux WebKit IPC message: {error}"))
        .and_then(|request| {
            let id = request.id;
            runtime
                .dispatch_ipc(&request.kind, request.payload)
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

    match runtime.drain_event_dispatch_scripts() {
        Ok(scripts) => {
            for script in scripts {
                evaluate_javascript(webview, &script);
            }
        }
        Err(error) => eprintln!("[StremioLightning] Failed to drain host events: {error}"),
    }
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
            let library = unsafe { libloading::os::unix::Library::new("libepoxy.so.0") }
                .map_err(|error| format!("Failed to load libepoxy: {error}"))?;
            let library = Box::leak(Box::new(library));

            epoxy::load_with(|name| {
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
    fbo: Cell<u32>,
}

impl NativeVideoState {
    fn new() -> Result<Self, String> {
        unsafe {
            setlocale(LC_NUMERIC, c"C".as_ptr());
        }

        let mpv = Mpv::with_initializer(|init| {
            init.set_property("vo", "libmpv")?;
            init.set_property("video-timing-offset", "0")?;
            init.set_property("terminal", "yes")?;
            Ok(())
        })
        .map_err(|error| format!("Failed to create mpv: {error}"))?;

        mpv.disable_deprecated_events().ok();

        Ok(Self {
            mpv: RefCell::new(mpv),
            render_context: RefCell::default(),
            fbo: Cell::default(),
        })
    }

    fn current_fbo(&self) -> i32 {
        let mut fbo = self.fbo.get();
        if fbo == 0 {
            let mut current_fbo = 0;
            unsafe {
                epoxy::GetIntegerv(epoxy::FRAMEBUFFER_BINDING, &mut current_fbo);
            }
            fbo = current_fbo as u32;
            self.fbo.set(fbo);
        }
        fbo as i32
    }
}

fn build_native_video() -> gtk::GLArea {
    let area = gtk::GLArea::new();
    let state =
        Rc::new(NativeVideoState::new().expect("Failed to initialize native mpv video backend"));

    {
        let state = state.clone();
        area.connect_realize(move |area| {
            area.make_current();
            if area.error().is_some() {
                return;
            }

            if let Some(context) = area.context() {
                let mut mpv = state.mpv.borrow_mut();
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

                let (sender, receiver) = mpsc::channel::<()>();
                let area_for_idle = area.clone();
                glib::idle_add_local(move || {
                    if receiver.try_recv().is_ok() {
                        area_for_idle.queue_render();
                    }
                    glib::ControlFlow::Continue
                });

                render_context.set_update_callback(move || {
                    sender.send(()).ok();
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

    area
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
}
