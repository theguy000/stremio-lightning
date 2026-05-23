//! Servo web engine runtime for Stremio Lightning Linux.
//!
//! Gated behind `#[cfg(feature = "servo-engine")]`.

use crate::host::Host;
use crate::player::PlayerBackend;
use crate::streaming_server::ProcessSpawner;
use crate::webview_runtime::{InjectionBundle, WebviewLoadState, WebviewShell};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use stremio_lightning_core::pip::PipWindowController;
use winit::raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use servo::RenderingContext;
use url::Url;

/// Configuration for initializing the Servo web engine.
#[derive(Debug, Clone)]
pub struct ServoConfig {
    pub enable_css_grid: bool,
    pub user_agent_suffix: String,
    pub engine_prefs: Vec<(String, String)>,
}

impl Default for ServoConfig {
    fn default() -> Self {
        Self {
            enable_css_grid: true,
            user_agent_suffix: "Servo/StremioLightning".to_string(),
            engine_prefs: vec![(
                "layout.grid.enabled".to_string(),
                "true".to_string(),
            )],
        }
    }
}

/// Simulated event types processed by the Servo background thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServoEvent {
    LoadUrl(String),
    InjectScript(String),
    DispatchIpc { kind: String, payload: Option<Value> },
    Shutdown,
}

/// Servo-powered webview runtime.
#[allow(dead_code)]
pub struct ServoWebviewRuntime<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    url: String,
    devtools: bool,
    injection: InjectionBundle,
    host: Arc<Host<B, P>>,
    loaded: bool,
    servo_config: ServoConfig,
    thread_handle: Mutex<Option<JoinHandle<()>>>,
    event_tx: Mutex<Option<Sender<ServoEvent>>>,
    shutdown_triggered: Arc<AtomicBool>,
}

impl<B, P> ServoWebviewRuntime<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    pub fn new(
        url: impl Into<String>,
        devtools: bool,
        injection: InjectionBundle,
        host: Arc<Host<B, P>>,
    ) -> Self {
        Self {
            url: url.into(),
            devtools,
            injection,
            host,
            loaded: false,
            servo_config: ServoConfig::default(),
            thread_handle: Mutex::new(None),
            event_tx: Mutex::new(None),
            shutdown_triggered: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn with_servo_config(mut self, config: ServoConfig) -> Self {
        self.servo_config = config;
        self
    }

    pub fn servo_config(&self) -> &ServoConfig {
        &self.servo_config
    }

    pub fn is_shutdown(&self) -> bool {
        self.shutdown_triggered.load(Ordering::Relaxed)
    }
}

impl<B, P> WebviewShell for ServoWebviewRuntime<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    fn load(&mut self) -> Result<WebviewLoadState, String> {
        let lower = self.url.to_lowercase();
        if !lower.starts_with("https://")
            && !lower.starts_with("http://")
            && !lower.starts_with("file://")
        {
            return Err("Servo webview URL must use http, https, or file".to_string());
        }

        self.loaded = true;
        Ok(self.load_state())
    }

    fn load_state(&self) -> WebviewLoadState {
        WebviewLoadState {
            url: self.url.clone(),
            devtools: self.devtools,
            document_start_scripts: self.injection.script_names(),
            loaded: self.loaded,
        }
    }

    fn dispatch_ipc(&self, kind: &str, payload: Option<Value>) -> Result<Value, String> {
        self.host.dispatch_ipc(kind, payload)
    }

    fn shutdown(&self) -> Result<(), String> {
        self.shutdown_triggered.store(true, Ordering::Relaxed);
        self.host.shutdown()
    }

    fn script_source(&self, name: &str) -> Option<String> {
        self.injection
            .scripts()
            .iter()
            .find(|script| script.name == name)
            .map(|script| script.source.clone())
    }

    fn drain_event_dispatch_scripts(&self) -> Result<Vec<String>, String> {
        self.host
            .drain_emitted_events()?
            .into_iter()
            .map(|event| {
                let event_name = serde_json::to_string(&event.event)
                    .map_err(|e| format!("Failed to serialize Servo host event name: {e}"))?;
                let payload = serde_json::to_string(&event.payload)
                    .map_err(|e| format!("Failed to serialize Servo host event payload: {e}"))?;
                Ok(format!(
                    "window.__STREMIO_LIGHTNING_LINUX_DISPATCH__({event_name}, {payload});"
                ))
            })
            .collect()
    }

    fn emit_native_player_property_changed(
        &self,
        name: &str,
        data: Value,
    ) -> Result<(), String> {
        self.host.emit_native_player_property_changed(name, data)
    }

    fn emit_native_player_ended(&self, reason: &str) -> Result<(), String> {
        self.host.emit_native_player_ended(reason)
    }

    fn toggle_picture_in_picture(
        &self,
        controller: &mut dyn PipWindowController,
    ) -> Result<bool, String> {
        self.host.toggle_picture_in_picture(controller)
    }

    fn exit_picture_in_picture(
        &self,
        controller: &mut dyn PipWindowController,
    ) -> Result<bool, String> {
        self.host.exit_picture_in_picture(controller)
    }
}

#[cfg(feature = "servo-engine")]
fn servo_ipc_adapter() -> String {
    r#"(function () {
  "use strict";
  if (window.__STREMIO_LIGHTNING_LINUX_IPC__) return;

  var nextId = 1;
  var pending = new Map();

  window.__STREMIO_LIGHTNING_LINUX_IPC__ = function (kind, payload) {
    return new Promise(function (resolve, reject) {
      var id = nextId++;
      pending.set(id, { resolve: resolve, reject: reject });
      console.log("stremio-ipc:" + JSON.stringify({
        id: id,
        kind: kind,
        payload: payload
      }));
    });
  };

  window.__STREMIO_LIGHTNING_LINUX_IPC_RESOLVE__ = function (id, ok, value) {
    var callbacks = pending.get(id);
    if (!callbacks) return;
    pending.delete(id);
    if (ok) callbacks.resolve(value);
    else callbacks.reject(new Error(String(value)));
  };
})();"#
        .to_string()
}

#[cfg(feature = "servo-engine")]
struct AppState<B: PlayerBackend + 'static, P: ProcessSpawner + 'static> {
    window: winit::window::Window,
    servo: servo::Servo,
    rendering_context: std::rc::Rc<servo::WindowRenderingContext>,
    webviews: std::cell::RefCell<Vec<servo::WebView>>,
    host: Arc<Host<B, P>>,
    last_cursor_pos: std::cell::Cell<servo::DevicePoint>,
    modifiers: std::cell::Cell<winit::keyboard::ModifiersState>,
}

#[cfg(feature = "servo-engine")]
impl<B: PlayerBackend + 'static, P: ProcessSpawner + 'static> servo::WebViewDelegate for AppState<B, P> {
    fn notify_new_frame_ready(&self, _: servo::WebView) {
        self.window.request_redraw();
    }

    fn show_console_message(&self, webview: servo::WebView, _level: servo::ConsoleLogLevel, message: String) {
        if let Some(json_str) = message.strip_prefix("stremio-ipc:") {
            let host = self.host.clone();
            let webview_clone = webview.clone();

            #[derive(serde::Deserialize)]
            struct IpcRequest {
                id: u64,
                kind: String,
                payload: Option<serde_json::Value>,
            }

            if let Ok(req) = serde_json::from_str::<IpcRequest>(json_str) {
                let id = req.id;
                let kind = req.kind;
                let payload = req.payload;

                match host.dispatch_ipc(&kind, payload) {
                    Ok(val) => {
                        let val_str = serde_json::to_string(&val).unwrap_or_default();
                        let code = format!("window.__STREMIO_LIGHTNING_LINUX_IPC_RESOLVE__({}, true, {});", id, val_str);
                        webview_clone.evaluate_javascript(code, Box::new(|_| {}));
                    }
                    Err(err) => {
                        let err_str = serde_json::to_string(&err).unwrap_or_default();
                        let code = format!("window.__STREMIO_LIGHTNING_LINUX_IPC_RESOLVE__({}, false, {});", id, err_str);
                        webview_clone.evaluate_javascript(code, Box::new(|_| {}));
                    }
                }
            }
        } else {
            eprintln!("[Servo Console] {}", message);
        }
    }
}

#[cfg(feature = "servo-engine")]
enum App<B: PlayerBackend + 'static, P: ProcessSpawner + 'static> {
    Initial {
        waker: Waker,
        runtime: Box<ServoWebviewRuntime<B, P>>,
    },
    Running(std::rc::Rc<AppState<B, P>>),
}

#[cfg(feature = "servo-engine")]
impl<B: PlayerBackend + 'static, P: ProcessSpawner + 'static> App<B, P> {
    fn new(event_loop: &winit::event_loop::EventLoop<WakerEvent>, runtime: ServoWebviewRuntime<B, P>) -> Self {
        Self::Initial {
            waker: Waker::new(event_loop),
            runtime: Box::new(runtime),
        }
    }
}

#[cfg(feature = "servo-engine")]
impl<B: PlayerBackend + 'static, P: ProcessSpawner + 'static> winit::application::ApplicationHandler<WakerEvent> for App<B, P> {
    fn resumed(&mut self, event_loop: &winit::event_loop::ActiveEventLoop) {
        if let Self::Initial { waker, runtime } = self {
            let display_handle = event_loop
                .display_handle()
                .expect("Failed to get display handle");

            let window_attrs = winit::window::Window::default_attributes()
                .with_title("Stremio Lightning (Servo)")
                .with_inner_size(winit::dpi::PhysicalSize::new(1280, 720));

            let window = event_loop
                .create_window(window_attrs)
                .expect("Failed to create winit Window");

            let window_handle = window.window_handle().expect("Failed to get window handle");

            let rendering_context = std::rc::Rc::new(
                servo::WindowRenderingContext::new(display_handle, window_handle, window.inner_size())
                    .expect("Could not create RenderingContext for window."),
            );

            let _ = rendering_context.make_current();

            let servo = servo::ServoBuilder::default()
                .event_loop_waker(Box::new(waker.clone()))
                .build();
            servo.setup_logging();

            let user_content_manager = std::rc::Rc::new(servo::UserContentManager::new(&servo));

            for script in runtime.injection.scripts() {
                let user_script = std::rc::Rc::new(servo::UserScript::new(script.source.clone(), None));
                user_content_manager.add_script(user_script);
            }

            let ipc_bridge_src = servo_ipc_adapter();
            let ipc_bridge_script = std::rc::Rc::new(servo::UserScript::new(ipc_bridge_src, None));
            user_content_manager.add_script(ipc_bridge_script);

            let app_state = std::rc::Rc::new(AppState {
                window,
                servo,
                rendering_context,
                webviews: Default::default(),
                host: runtime.host.clone(),
                last_cursor_pos: std::cell::Cell::new(servo::DevicePoint::default()),
                modifiers: std::cell::Cell::new(winit::keyboard::ModifiersState::default()),
            });

            let url = Url::parse(&runtime.url)
                .expect("Valid URL expected");

            let webview =
                servo::WebViewBuilder::new(&app_state.servo, app_state.rendering_context.clone())
                    .url(url)
                    .hidpi_scale_factor(euclid::Scale::new(app_state.window.scale_factor() as f32))
                    .user_content_manager(user_content_manager)
                    .delegate(app_state.clone())
                    .build();

            app_state.webviews.borrow_mut().push(webview);
            *self = Self::Running(app_state);
        }
    }

    fn user_event(&mut self, _event_loop: &winit::event_loop::ActiveEventLoop, _event: WakerEvent) {
        if let Self::Running(state) = self {
            state.servo.spin_event_loop();
        }
    }

    fn window_event(
        &mut self,
        event_loop: &winit::event_loop::ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        if let Self::Running(state) = self {
            state.servo.spin_event_loop();
        }

        match event {
            winit::event::WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            winit::event::WindowEvent::RedrawRequested => {
                if let Self::Running(state) = self {
                    if let Some(webview) = state.webviews.borrow().last() {
                        webview.paint();
                    }
                    state.rendering_context.present();
                }
            }
            winit::event::WindowEvent::MouseWheel { delta, .. } => {
                if let Self::Running(state) = self {
                    if let Some(webview) = state.webviews.borrow().last() {
                        let (delta_x, delta_y, mode) = match delta {
                            winit::event::MouseScrollDelta::LineDelta(dx, dy) => {
                                ((dx * 76.0) as f64, (dy * 76.0) as f64, servo::WheelMode::DeltaLine)
                            }
                            winit::event::MouseScrollDelta::PixelDelta(delta) => {
                                (delta.x, delta.y, servo::WheelMode::DeltaPixel)
                            }
                        };

                        let pos = state.last_cursor_pos.get();
                        webview.notify_input_event(servo::InputEvent::Wheel(servo::WheelEvent::new(
                            servo::WheelDelta {
                                x: delta_x,
                                y: delta_y,
                                z: 0.0,
                                mode,
                            },
                            pos.into(),
                        )));
                    }
                }
            }
            winit::event::WindowEvent::Resized(new_size) => {
                if let Self::Running(state) = self {
                    if let Some(webview) = state.webviews.borrow().last() {
                        webview.resize(new_size);
                    }
                }
            }
            winit::event::WindowEvent::ModifiersChanged(new_mods) => {
                if let Self::Running(state) = self {
                    state.modifiers.set(new_mods.state());
                }
            }
            winit::event::WindowEvent::CursorMoved { position, .. } => {
                if let Self::Running(state) = self {
                    if let Some(webview) = state.webviews.borrow().last() {
                        let pos = servo::DevicePoint::new(position.x as f32, position.y as f32);
                        state.last_cursor_pos.set(pos);
                        webview.notify_input_event(servo::InputEvent::MouseMove(
                            servo::MouseMoveEvent::new(servo::WebViewPoint::Device(pos))
                        ));
                    }
                }
            }
            winit::event::WindowEvent::MouseInput { state: element_state, button: winit_button, .. } => {
                if let Self::Running(state) = self {
                    if let Some(webview) = state.webviews.borrow().last() {
                        let action = match element_state {
                            winit::event::ElementState::Pressed => servo::MouseButtonAction::Down,
                            winit::event::ElementState::Released => servo::MouseButtonAction::Up,
                        };

                        let button = match winit_button {
                            winit::event::MouseButton::Left => servo::MouseButton::Left,
                            winit::event::MouseButton::Right => servo::MouseButton::Right,
                            winit::event::MouseButton::Middle => servo::MouseButton::Middle,
                            winit::event::MouseButton::Back => servo::MouseButton::Back,
                            winit::event::MouseButton::Forward => servo::MouseButton::Forward,
                            winit::event::MouseButton::Other(n) => servo::MouseButton::Other(n),
                        };

                        let pos = state.last_cursor_pos.get();
                        webview.notify_input_event(servo::InputEvent::MouseButton(
                            servo::MouseButtonEvent::new(action, button, servo::WebViewPoint::Device(pos))
                        ));
                    }
                }
            }
            winit::event::WindowEvent::KeyboardInput { event, .. } => {
                if let Self::Running(state) = self {
                    if let Some(webview) = state.webviews.borrow().last() {
                        let key_state = match event.state {
                            winit::event::ElementState::Pressed => keyboard_types::KeyState::Down,
                            winit::event::ElementState::Released => keyboard_types::KeyState::Up,
                        };

                        let key = map_key(&event.logical_key);
                        let code = map_code(&event.physical_key);
                        let location = map_location(event.location);
                        let modifiers = map_modifiers(&state.modifiers.get());

                        let kb_event = keyboard_types::KeyboardEvent {
                            state: key_state,
                            key,
                            code,
                            location,
                            modifiers,
                            repeat: event.repeat,
                            is_composing: false,
                        };

                        webview.notify_input_event(servo::InputEvent::Keyboard(
                            servo::KeyboardEvent::new(kb_event)
                        ));
                    }
                }
            }
            _ => (),
        }
    }
}

#[cfg(feature = "servo-engine")]
fn map_key(winit_key: &winit::keyboard::Key) -> keyboard_types::Key {
    use std::str::FromStr;
    match winit_key {
        winit::keyboard::Key::Named(named) => {
            let name_str = format!("{:?}", named);
            if let Ok(k) = keyboard_types::NamedKey::from_str(&name_str) {
                keyboard_types::Key::Named(k)
            } else {
                keyboard_types::Key::Named(keyboard_types::NamedKey::Unidentified)
            }
        }
        winit::keyboard::Key::Character(s) => {
            keyboard_types::Key::Character(s.to_string())
        }
        _ => keyboard_types::Key::Named(keyboard_types::NamedKey::Unidentified),
    }
}

#[cfg(feature = "servo-engine")]
fn map_code(winit_code: &winit::keyboard::PhysicalKey) -> keyboard_types::Code {
    use std::str::FromStr;
    match winit_code {
        winit::keyboard::PhysicalKey::Code(code) => {
            let code_str = format!("{:?}", code);
            if let Ok(c) = keyboard_types::Code::from_str(&code_str) {
                c
            } else {
                keyboard_types::Code::Unidentified
            }
        }
        _ => keyboard_types::Code::Unidentified,
    }
}

#[cfg(feature = "servo-engine")]
fn map_location(winit_loc: winit::keyboard::KeyLocation) -> keyboard_types::Location {
    match winit_loc {
        winit::keyboard::KeyLocation::Standard => keyboard_types::Location::Standard,
        winit::keyboard::KeyLocation::Left => keyboard_types::Location::Left,
        winit::keyboard::KeyLocation::Right => keyboard_types::Location::Right,
        winit::keyboard::KeyLocation::Numpad => keyboard_types::Location::Numpad,
    }
}

#[cfg(feature = "servo-engine")]
fn map_modifiers(winit_mods: &winit::keyboard::ModifiersState) -> keyboard_types::Modifiers {
    let mut mods = keyboard_types::Modifiers::empty();
    if winit_mods.shift_key() {
        mods.insert(keyboard_types::Modifiers::SHIFT);
    }
    if winit_mods.control_key() {
        mods.insert(keyboard_types::Modifiers::CONTROL);
    }
    if winit_mods.alt_key() {
        mods.insert(keyboard_types::Modifiers::ALT);
    }
    if winit_mods.super_key() {
        mods.insert(keyboard_types::Modifiers::META);
    }
    mods
}

#[cfg(feature = "servo-engine")]
#[derive(Clone)]
struct Waker(winit::event_loop::EventLoopProxy<WakerEvent>);

#[cfg(feature = "servo-engine")]
#[derive(Debug)]
struct WakerEvent;

#[cfg(feature = "servo-engine")]
impl Waker {
    fn new(event_loop: &winit::event_loop::EventLoop<WakerEvent>) -> Self {
        Self(event_loop.create_proxy())
    }
}

#[cfg(feature = "servo-engine")]
impl servo::EventLoopWaker for Waker {
    fn clone_box(&self) -> Box<dyn servo::EventLoopWaker> {
        Box::new(Self(self.0.clone()))
    }

    fn wake(&self) {
        let _ = self.0.send_event(WakerEvent);
    }
}

/// Stub entry point for the Servo-powered native window loop.
pub fn run_servo_window<B, P>(
    runtime: ServoWebviewRuntime<B, P>,
) -> Result<(), String>
where
    B: PlayerBackend + 'static,
    P: ProcessSpawner + 'static,
{
    if runtime.is_shutdown() {
        return Ok(());
    }

    eprintln!("[StremioLightning] [Servo Window] Starting actual Servo winit event loop...");

    #[cfg(feature = "servo-engine")]
    {
        let _ = rustls::crypto::aws_lc_rs::default_provider()
            .install_default();

        let event_loop = winit::event_loop::EventLoop::with_user_event()
            .build()
            .map_err(|e| e.to_string())?;

        let mut app = App::new(&event_loop, runtime);
        event_loop.run_app(&mut app).map_err(|e| e.to_string())?;
    }

    eprintln!("[StremioLightning] [Servo Window] Event loop terminated cleanly.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::Host;
    use crate::player::FakePlayerBackend;
    use crate::streaming_server::{FakeProcessSpawner, StreamingServer};
    use std::path::PathBuf;

    fn test_host() -> Arc<Host<FakePlayerBackend, FakeProcessSpawner>> {
        Arc::new(Host::with_app_data_dir(
            FakePlayerBackend::initialized(),
            StreamingServer::with_project_root(
                FakeProcessSpawner::default(),
                PathBuf::from("/repo"),
            ),
            std::env::temp_dir(),
        ))
    }

    #[test]
    fn servo_runtime_loads_with_valid_url() {
        let host = test_host();
        let injection = InjectionBundle::load_for_servo().unwrap();
        let mut runtime = ServoWebviewRuntime::new(
            "https://web.stremio.com/",
            false,
            injection,
            host,
        );

        let state = runtime.load().unwrap();
        assert!(state.loaded);
        assert_eq!(state.url, "https://web.stremio.com/");
    }

    #[test]
    fn servo_runtime_rejects_invalid_url() {
        let host = test_host();
        let injection = InjectionBundle::load_for_servo().unwrap();
        let mut runtime = ServoWebviewRuntime::new(
            "ftp://invalid.com",
            false,
            injection,
            host,
        );

        assert!(runtime.load().is_err());
    }

    #[test]
    fn servo_config_defaults_enable_css_grid() {
        let config = ServoConfig::default();
        assert!(config.enable_css_grid);
        assert!(config
            .engine_prefs
            .iter()
            .any(|(k, v)| k == "layout.grid.enabled" && v == "true"));
        assert!(config.user_agent_suffix.contains("Servo"));
    }

    #[test]
    fn servo_runtime_injection_includes_polyfills_and_compat() {
        let injection = InjectionBundle::load_for_servo().unwrap();
        let names = injection.script_names();
        assert!(
            names.contains(&"bridge/polyfills.js"),
            "Servo injection bundle must include polyfills.js"
        );
        assert!(
            names.contains(&"bridge/servo-compat-style.js"),
            "Servo injection bundle must include servo-compat-style.js"
        );
    }

    #[test]
    fn servo_window_runs_and_exits_cleanly() {
        let host = test_host();
        let injection = InjectionBundle::load_for_servo().unwrap();
        let mut runtime = ServoWebviewRuntime::new(
            "https://web.stremio.com/",
            false,
            injection,
            host,
        );
        runtime.load().unwrap();
        runtime.shutdown().unwrap();
        let result = run_servo_window(runtime);
        assert!(result.is_ok());
    }

    #[test]
    fn servo_runtime_dispatches_ipc_through_host() {
        let host = test_host();
        let injection = InjectionBundle::load_for_servo().unwrap();
        let mut runtime = ServoWebviewRuntime::new(
            "https://web.stremio.com/",
            false,
            injection,
            host,
        );
        runtime.load().unwrap();

        let result = runtime.dispatch_ipc(
            "invoke",
            Some(serde_json::json!({"command": "shell_bridge_ready"})),
        );
        assert!(result.is_ok());
    }
}
