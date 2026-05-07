pub mod host;
pub mod player;
pub mod webview;

pub const APP_NAME: &str = "Stremio Lightning";

pub fn run() -> Result<(), String> {
    platform::run()
}

#[cfg(windows)]
mod platform {
    pub fn run() -> Result<(), String> {
        crate::webview::WindowsWebView2Shell::new("https://web.stremio.com/")?.run()
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn run() -> Result<(), String> {
        Err("stremio-lightning-windows only runs on Windows".to_string())
    }
}
