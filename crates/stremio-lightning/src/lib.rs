//! The Stremio client, as a library.
//!
//! Four layers, kept strictly apart (ADR 0002):
//! - `screens`: pure functions from app state to plain descriptions of what a screen shows.
//! - `plumbing`: mpv, streaming server, image cache, OAuth, AniSkip, themes, and log recorder.
//! - `app`: deep, Slint-free `Arc<App>` owning core runtime, `Snapshot`, and `AppNotice` push.
//! - `shell`: sole Slint/winit/mpv adapter; decides nothing.

use std::sync::Arc;

pub use plumbing::env::AppEnv;

slint::include_modules!();

pub mod app;
pub mod plumbing;
pub mod screens;
pub(crate) mod shell;
pub(crate) use app::{AppModel, AppRuntime};

pub use plumbing::links;
pub use plumbing::theme as themes;
pub use screens::auth;

fn app_data_dir() -> Option<std::path::PathBuf> {
    let base = dirs::data_dir()?;
    let current = base.join("stremio-lightning");
    let previous = base.join("stremio-lightning-native");
    if !current.exists() && previous.exists() {
        let _ = std::fs::rename(&previous, &current);
    }
    Some(current)
}

/// The booted application before its event loop runs: the desktop window attached to a real
/// `Arc<App>`, core runtime, Tokio runtime, and log recorder.
pub struct Booted {
    pub window: MainWindow,
    pub runtime: Arc<AppRuntime>,
    pub tokio: tokio::runtime::Runtime,
    pub logs: Arc<plumbing::logs::Logs>,
    pub app: Arc<app::App>,
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = app_data_dir().ok_or("no data directory")?;
    let booted = boot(data_dir)?;
    booted.window.show()?;
    slint::run_event_loop_until_quit()?;
    Ok(())
}

/// Starts the full client against an isolated `data_dir` without entering the GUI event loop.
pub fn boot(data_dir: std::path::PathBuf) -> Result<Booted, Box<dyn std::error::Error>> {
    let tokio = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let app = app::App::start(data_dir, tokio.handle())?;
    let window = MainWindow::new()?;
    shell::attach(&window, app.clone(), tokio.handle().clone());
    app.load_initial();

    Ok(Booted {
        window,
        runtime: app.runtime().clone(),
        tokio,
        logs: app.logs().clone(),
        app,
    })
}
