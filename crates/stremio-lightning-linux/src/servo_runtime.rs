//! Servo web engine runtime stub for Stremio Lightning Linux.
//!
//! This module provides the `ServoWebviewRuntime` — a stub implementation of
//! the [`WebviewShell`] trait that will eventually drive a Servo-powered
//! webview with a unified wgpu compositing pipeline.
//!
//! Gated behind `#[cfg(feature = "servo-engine")]`.

use crate::host::Host;
use crate::player::PlayerBackend;
use crate::render::RenderLoopPlan;
use crate::streaming_server::ProcessSpawner;
use crate::webview_runtime::{InjectionBundle, WebviewLoadState, WebviewShell};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use stremio_lightning_core::pip::PipWindowController;

/// Configuration for initializing the Servo web engine.
///
/// Mirrors the subset of Servo startup preferences relevant to Stremio Lightning,
/// including CSS Grid force-enable and custom User-Agent.
#[derive(Debug, Clone)]
pub struct ServoConfig {
    /// Force-enable CSS Grid layout support (Taffy backend).
    pub enable_css_grid: bool,
    /// Custom User-Agent string appended to identify Servo/StremioLightning.
    pub user_agent_suffix: String,
    /// Servo engine preferences passed at initialization time.
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
    /// Request to load a specific web page URL.
    LoadUrl(String),
    /// Request to inject a named JavaScript snippet.
    InjectScript(String),
    /// IPC dispatch call from the web page.
    DispatchIpc { kind: String, payload: Option<Value> },
    /// Shutdown the background thread.
    Shutdown,
}

/// Stub Servo-powered webview runtime.
///
/// This runtime will eventually:
/// 1. Initialize a Servo instance in a background Rust thread.
/// 2. Manage a winit event loop for window input.
/// 3. Composite MPV video + Servo web UI via a shared wgpu device.
///
/// Currently all rendering operations are simulated or delegate to stubs.
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

    /// Configure Servo engine preferences before loading.
    pub fn with_servo_config(mut self, config: ServoConfig) -> Self {
        self.servo_config = config;
        self
    }

    /// Returns the active Servo engine configuration.
    pub fn servo_config(&self) -> &ServoConfig {
        &self.servo_config
    }

    /// Returns true if the shutdown sequence was initiated.
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

        let (tx, rx) = channel();

        // Spawn background thread simulating Servo engine thread initialization (Phase 2.2)
        let url_clone = self.url.clone();
        let handle = std::thread::spawn(move || {
            eprintln!("[StremioLightning] [Servo Thread] Initializing Servo engine instance...");
            while let Ok(event) = rx.recv() {
                eprintln!("[StremioLightning] [Servo Thread] Processing event: {:?}", event);
                match event {
                    ServoEvent::LoadUrl(url) => {
                        eprintln!("[StremioLightning] [Servo Thread] Loading page: {}", url);
                    }
                    ServoEvent::InjectScript(name) => {
                        eprintln!("[StremioLightning] [Servo Thread] Injected script: {}", name);
                    }
                    ServoEvent::DispatchIpc { kind, payload } => {
                        eprintln!(
                            "[StremioLightning] [Servo Thread] Processing IPC ({}): {:?}",
                            kind, payload
                        );
                    }
                    ServoEvent::Shutdown => {
                        eprintln!("[StremioLightning] [Servo Thread] Shutting down Servo loop.");
                        break;
                    }
                }
            }
        });

        // Queue initial load and script injection events
        let _ = tx.send(ServoEvent::LoadUrl(url_clone));
        for script in self.injection.scripts() {
            let _ = tx.send(ServoEvent::InjectScript(script.name.to_string()));
        }

        *self.event_tx.lock().unwrap() = Some(tx);
        *self.thread_handle.lock().unwrap() = Some(handle);

        eprintln!(
            "[StremioLightning] Servo engine stub: load({}) with config {:?}",
            self.url, self.servo_config
        );
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
        // Log/route to background thread (Phase 2.3)
        if let Some(tx) = self.event_tx.lock().unwrap().as_ref() {
            let _ = tx.send(ServoEvent::DispatchIpc {
                kind: kind.to_string(),
                payload: payload.clone(),
            });
        }
        self.host.dispatch_ipc(kind, payload)
    }

    fn shutdown(&self) -> Result<(), String> {
        // Send shutdown signal to Servo thread
        self.shutdown_triggered.store(true, Ordering::Relaxed);
        if let Some(tx) = self.event_tx.lock().unwrap().as_ref() {
            let _ = tx.send(ServoEvent::Shutdown);
        }
        if let Some(handle) = self.thread_handle.lock().unwrap().take() {
            handle.join().ok();
        }
        eprintln!("[StremioLightning] Servo engine stub: shutdown");
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
        // Phase 2 will route events through the Servo JS context.
        // For now, use the same host event drain as WebKit.
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

/// Stub entry point for the Servo-powered native window loop.
///
/// Phase 3.2: Stubs a mock winit event loop driving the compositing pipeline.
/// Composites: `[clear-wgpu-frame] → [render-mpv-texture-layer] → [render-servo-webrender-overlay] → [wgpu-present-frame]`.
pub fn run_servo_window<B, P>(
    runtime: ServoWebviewRuntime<B, P>,
) -> Result<(), String>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    eprintln!("[StremioLightning] [Servo Window] Initializing unified wgpu compositing context...");
    let plan = RenderLoopPlan::servo();
    eprintln!(
        "[StremioLightning] [Servo Window] Configured render plan: {:?}",
        plan.steps
    );
    eprintln!("[StremioLightning] [Servo Window] Running winit event loop simulation. Press Ctrl+C to exit.");

    // Loop and sleep, checking if we've received a shutdown event
    while !runtime.is_shutdown() {
        std::thread::sleep(std::time::Duration::from_millis(50));
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

        // dispatch_ipc delegates to the host, which handles known commands
        let result = runtime.dispatch_ipc(
            "invoke",
            Some(serde_json::json!({"command": "shell_bridge_ready"})),
        );
        assert!(result.is_ok());
    }
}
