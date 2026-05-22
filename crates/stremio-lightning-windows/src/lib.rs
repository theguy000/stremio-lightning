pub mod host;
pub mod player;
pub mod resources;
pub mod server;
pub mod settings;
pub mod single_instance;
pub mod webview;
pub mod window;
#[cfg(feature = "servo-engine")]
pub mod servo_runtime;

pub const APP_NAME: &str = "Stremio Lightning";

pub fn run() -> Result<(), String> {
    platform::run()
}

#[cfg(windows)]
mod platform {
    pub fn run() -> Result<(), String> {
        let args = std::env::args().skip(1).collect::<Vec<_>>();
        let intent = crate::single_instance::launch_intent_from_args(&args);
        let crate::single_instance::SingleInstanceRole::Primary(instance) =
            crate::single_instance::SingleInstanceGuard::acquire(intent.clone())?
        else {
            return Ok(());
        };

        let ui_notifier = std::sync::Arc::new(std::sync::Mutex::new(None));
        let launch_intents = instance.start_listener(ui_notifier.clone(), intent);
        let settings = crate::settings::ShellSettings::from_args(&args);

        match settings.engine {
            crate::settings::WebviewEngine::WebView2 => {
                crate::webview::WindowsWebView2Shell::new(settings, launch_intents, ui_notifier)?.run()
            }
            crate::settings::WebviewEngine::Servo => {
                run_servo_engine(settings, launch_intents, ui_notifier)
            }
        }
    }

    fn run_servo_engine(
        _settings: crate::settings::ShellSettings,
        _launch_intents: std::sync::mpsc::Receiver<crate::single_instance::LaunchIntent>,
        _ui_notifier: std::sync::Arc<std::sync::Mutex<Option<crate::window::UiThreadNotifier>>>,
    ) -> Result<(), String> {
        #[cfg(feature = "servo-engine")]
        {
            use crate::servo_runtime::ServoWebviewRuntime;
            use crate::webview::InjectionBundle;
            use crate::webview::WebviewShell;

            let injection = InjectionBundle::load_for_servo()?;
            let runtime = ServoWebviewRuntime::new(
                _settings.webui_url.clone(),
                _settings.devtools,
                injection,
                _launch_intents,
                _ui_notifier,
            );
            runtime.run()
        }

        #[cfg(not(feature = "servo-engine"))]
        {
            Err(
                "Servo engine is not available. Run with: \
                 cargo run -p stremio-lightning-windows --features servo-engine -- --engine=servo"
                    .to_string(),
            )
        }
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn run() -> Result<(), String> {
        Err("stremio-lightning-windows only runs on Windows".to_string())
    }
}
