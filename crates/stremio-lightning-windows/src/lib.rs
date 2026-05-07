pub mod host;
pub mod player;
pub mod resources;
pub mod server;
pub mod settings;
pub mod single_instance;
pub mod webview;
pub mod window;

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
        let settings = crate::settings::WindowsShellSettings::from_args(&args);
        crate::webview::WindowsWebView2Shell::new(settings, launch_intents, ui_notifier)?.run()
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn run() -> Result<(), String> {
        Err("stremio-lightning-windows only runs on Windows".to_string())
    }
}
