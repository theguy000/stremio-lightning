//! The Stremio client, as a library.
//!
//! Three kinds of code, kept apart:
//!
//! - **Screen data** (`screens`): plain functions from the app state to a plain description of
//!   what a screen shows. No window, no Slint types. This is what the tests call.
//! - **Wiring** (`shell`): one line per window callback, naming an action. It decides nothing.
//! - **Plumbing** (`plumbing`): mpv, the streaming server, image downloads, browser links and
//!   theme files, each behind a couple of functions and reporting plain values.
//!
//! `app` sits between them: it holds the state, turns core's events into a snapshot of what the
//! window shows, and hands that snapshot to the shell.
//!
//! The binary is `main.rs`, which does nothing but start this.

use std::sync::atomic::{AtomicU64, AtomicUsize};
use std::sync::{Arc, Mutex};

use stremio_core::runtime::Env;
use stremio_core::types::profile::Profile;
use stremio_lightning_env::AppEnv;

slint::include_modules!();

pub mod app;
pub mod plumbing;
pub mod screens;
pub(crate) mod shell;
pub(crate) use app::{AppModel, AppRuntime};

/// The addresses the app is willing to open, by the name the UI uses for them.
pub use plumbing::links;
/// The theme reader, so a theme file's meaning can be checked without a window.
pub use plumbing::theme as themes;
/// What the app says when a sign-in fails, the guard that owns an attempt, and the one place
/// that stops the spinners when a sign-in screen is left.
pub use screens::auth;

/// Starts the app and runs it until the window closes.
/// The app's own folder under the platform data directory: the core's storage, the stream
/// server's logs and this shell's settings all live here.
///
/// It used to be called `stremio-lightning`. A folder still carrying that name is moved
/// across once, so a saved session, its addons and its settings survive the rename.
fn app_data_dir() -> Option<std::path::PathBuf> {
    let base = dirs::data_dir()?;
    let current = base.join("stremio-lightning");
    let previous = base.join("stremio-lightning-native");
    if !current.exists() && previous.exists() {
        let _ = std::fs::rename(&previous, &current);
    }
    Some(current)
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let tokio = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let data_dir = app_data_dir().ok_or("no data directory")?;
    AppEnv::init(data_dir.clone(), tokio.handle().clone()).map_err(|e| e.message())?;

    let window = MainWindow::new()?;
    plumbing::theme::apply(&window);
    let video = plumbing::player::VideoPlayer::install(&window);

    let server = Arc::new(plumbing::stream_server::Supervisor::new(
        stremio_lightning_core::streaming_server::RealProcessSpawner,
        plumbing::stream_server::spec(&plumbing::stream_server::bundle_dir(), &data_dir),
    ));

    let profile: Profile = tokio
        .block_on(AppEnv::get_storage(
            stremio_core::constants::PROFILE_STORAGE_KEY,
        ))
        .map_err(|e| e.message())?
        .unwrap_or_default();
    let (runtime, events) = app::build::runtime(profile);
    let runtime = Arc::new(runtime);

    // Inside the runtime: `Images` keeps its handle, because a repaint asks for pictures from the
    // window's thread, where `tokio::spawn` has no runtime to reach for.
    let images = tokio.block_on(async { plumbing::images::Images::default() });
    let local_extras = Arc::new(Mutex::new(screens::settings::LocalExtras::load(&data_dir)));
    let latest: Arc<Mutex<app::Snapshot>> = Default::default();
    let selected = Arc::new(AtomicUsize::new(0));
    // The Discover cards on screen, until the grid says otherwise. A screenful and a bit, so the
    // first frame of a catalog is never empty while the grid works its own range out.
    let visible = Arc::new(Mutex::new((0, 24)));
    let season = Arc::new(AtomicUsize::new(screens::details::NO_SEASON));
    let picked: Arc<Mutex<Option<String>>> = Default::default();
    let video_query: Arc<Mutex<String>> = Default::default();
    let moved: Arc<Mutex<Option<(u32, u32)>>> = Default::default();
    let said: Arc<AtomicU64> = Default::default();
    let player_season = Arc::new(AtomicUsize::new(screens::player::NO_SEASON));
    let playback_error: Arc<Mutex<String>> = Default::default();

    let wiring = shell::wiring::Wiring {
        window: window.as_weak(),
        runtime: runtime.clone(),
        video: video.clone(),
        images: images.clone(),
        latest: latest.clone(),
        selected: selected.clone(),
        visible: visible.clone(),
        season: season.clone(),
        picked: picked.clone(),
        video_query: video_query.clone(),
        moved: moved.clone(),
        said: said.clone(),
        local_extras: local_extras.clone(),
        data_dir: data_dir.clone(),
        server: server.clone(),
        auth: Default::default(),
        handle: tokio.handle().clone(),
        player_season: player_season.clone(),
        player_loaded: Default::default(),
        playback_error: playback_error.clone(),
        playback: Default::default(),
        player_sync: Default::default(),
        // Fires once the Discover grid has stopped moving, to fetch the row of cards either side of
        // the screen. Started and restarted by the grid itself.
        settle: Default::default(),
    };
    shell::wire(&window, wiring.clone());
    // The wheel is taken from the window itself: left to a Slint Flickable it costs a whole
    // animation, and a whole animation is a core.
    shell::wheel::wire(&window);
    // Playback news goes to the wiring, which is the only thing that knows both mpv and the
    // core: it moves the window, tells the core where the play head is, and moves on to the
    // next Video when a stream ends.
    {
        let wiring = wiring.clone();
        video.listen(move |status| wiring.playback_changed(status));
    }

    // The settings screen and the player start from whatever the profile already holds.
    if let Ok(model) = runtime.model() {
        let data = screens::settings::data(&model, &local_extras.lock().unwrap());
        video.apply_settings(&data.player_config);
        window.set_settings(shell::view::settings_view(&data));
        latest.lock().unwrap().settings = data;
    }

    window.set_app_version(env!("CARGO_PKG_VERSION").into());
    window.set_build_version(option_env!("GITHUB_SHA").unwrap_or("dev").into());

    let repaint = {
        let (window, latest, images) = (window.as_weak(), latest.clone(), images.clone());
        Arc::new(move || {
            let snapshot = latest.lock().unwrap().clone();
            shell::show(&window, snapshot, &images);
        })
    };
    tokio.spawn(app::events::pump(
        events,
        app::events::Context {
            weak: window.as_weak(),
            runtime: runtime.clone(),
            images,
            latest,
            selected,
            visible,
            season,
            player_season,
            picked,
            video_query,
            moved,
            local_extras,
            playback_error,
            auth: wiring.auth.clone(),
            repaint,
        },
    ));

    wiring.report_server_state(server.start());
    app::build::load_everything(&runtime);
    // The same pulls the window repeats whenever it comes back to the front: the web does this
    // once on mount too, so a mark made elsewhere is there before the first frame settles.
    app::build::pull_from_api(&runtime);
    window.run()?;
    Ok(())
}
