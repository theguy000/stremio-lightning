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
        let settings = crate::settings::WindowsShellSettings::default();
        crate::webview::WindowsWebView2Shell::new(settings)?.run()
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn run() -> Result<(), String> {
        Err("stremio-lightning-windows only runs on Windows".to_string())
    }
}
