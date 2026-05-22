//! Servo web engine runtime stub for Stremio Lightning Windows.
//!
//! Gated behind `#[cfg(feature = "servo-engine")]`.

use crate::webview::{InjectionBundle, WebviewShell};
use crate::single_instance::LaunchIntent;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

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

/// Stub Servo-powered webview runtime.
#[allow(dead_code)]
pub struct ServoWebviewRuntime {
    url: String,
    devtools: bool,
    injection: InjectionBundle,
    launch_intents: Mutex<Option<Receiver<LaunchIntent>>>,
    #[cfg(windows)]
    ui_notifier: Arc<Mutex<Option<crate::window::UiThreadNotifier>>>,
    loaded: bool,
    servo_config: ServoConfig,
    thread_handle: Mutex<Option<JoinHandle<()>>>,
    event_tx: Mutex<Option<Sender<ServoEvent>>>,
    shutdown_triggered: Arc<AtomicBool>,
}

impl ServoWebviewRuntime {
    #[cfg(windows)]
    pub fn new(
        url: impl Into<String>,
        devtools: bool,
        injection: InjectionBundle,
        launch_intents: Receiver<LaunchIntent>,
        ui_notifier: Arc<Mutex<Option<crate::window::UiThreadNotifier>>>,
    ) -> Self {
        Self {
            url: url.into(),
            devtools,
            injection,
            launch_intents: Mutex::new(Some(launch_intents)),
            ui_notifier,
            loaded: false,
            servo_config: ServoConfig::default(),
            thread_handle: Mutex::new(None),
            event_tx: Mutex::new(None),
            shutdown_triggered: Arc::new(AtomicBool::new(false)),
        }
    }

    #[cfg(not(windows))]
    pub fn new(
        url: impl Into<String>,
        devtools: bool,
        injection: InjectionBundle,
        launch_intents: Receiver<LaunchIntent>,
    ) -> Self {
        Self {
            url: url.into(),
            devtools,
            injection,
            launch_intents: Mutex::new(Some(launch_intents)),
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

    pub fn is_shutdown(&self) -> bool {
        self.shutdown_triggered.load(Ordering::Relaxed)
    }
}

impl WebviewShell for ServoWebviewRuntime {
    fn document_start_script_names(&self) -> Vec<&'static str> {
        self.injection.scripts().iter().map(|script| script.name).collect()
    }

    fn run(self) -> Result<(), String> {
        run_servo_window(self)
    }
}

pub fn run_servo_window(runtime: ServoWebviewRuntime) -> Result<(), String> {
    eprintln!("[StremioLightning] [Servo Window] Initializing unified wgpu compositing context on Windows...");
    
    // Simulate web page loading and startup script injection
    let (tx, rx) = channel();
    let url_clone = runtime.url.clone();
    let injection_clone = runtime.injection.clone();
    
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
                    eprintln!("[StremioLightning] [Servo Thread] Processing IPC ({}): {:?}", kind, payload);
                }
                ServoEvent::Shutdown => {
                    eprintln!("[StremioLightning] [Servo Thread] Shutting down Servo loop.");
                    break;
                }
            }
        }
    });

    let _ = tx.send(ServoEvent::LoadUrl(url_clone));
    for script in injection_clone.scripts() {
        let _ = tx.send(ServoEvent::InjectScript(script.name.to_string()));
    }

    *runtime.event_tx.lock().unwrap() = Some(tx);
    *runtime.thread_handle.lock().unwrap() = Some(handle);

    eprintln!("[StremioLightning] [Servo Window] Running event loop simulation. Press Ctrl+C to exit.");

    // Signal early shutdown for testing/simulated execution
    runtime.shutdown_triggered.store(true, Ordering::Relaxed);

    while !runtime.is_shutdown() {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    if let Some(tx) = runtime.event_tx.lock().unwrap().as_ref() {
        let _ = tx.send(ServoEvent::Shutdown);
    }
    if let Some(handle) = runtime.thread_handle.lock().unwrap().take() {
        handle.join().ok();
    }

    eprintln!("[StremioLightning] [Servo Window] Event loop terminated cleanly.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_servo_runtime_creation() {
        let (_tx, rx) = std::sync::mpsc::channel();
        let injection = InjectionBundle::load_for_servo().unwrap();
        
        #[cfg(windows)]
        let runtime = ServoWebviewRuntime::new(
            "https://web.stremio.com/",
            true,
            injection,
            rx,
            Arc::new(Mutex::new(None)),
        );

        #[cfg(not(windows))]
        let runtime = ServoWebviewRuntime::new(
            "https://web.stremio.com/",
            true,
            injection,
            rx,
        );

        assert_eq!(runtime.url, "https://web.stremio.com/");
        assert!(runtime.devtools);
        assert!(!runtime.is_shutdown());
    }
}
