//! The desktop shell: the sole Slint/winit/mpv adapter attached to a Slint-free `Arc<App>`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak as RcWeak};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use slint::ComponentHandle;

use crate::app::{App, AppNotice};
use crate::plumbing::stream_server::Supervisor;
use crate::MainWindow;

pub(crate) mod auth;
pub(crate) mod gamepad;
pub(crate) mod shortcuts;
pub(crate) mod theme_apply;
pub(crate) mod tray;
pub(crate) mod video_host;
pub(crate) mod view;
pub(crate) mod winit_host;
pub(crate) mod wiring;

static NEXT_SHELL_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// UI-thread weak registry for attached shells so cross-thread `AppNotice::PlayerStreamChanged`
    /// notifications can dispatch directly to UI-thread-only `VideoPlayer` / `Wiring` handles with
    /// zero polling timers and automatic cleanup when a `MainWindow` drops.
    static ATTACHED: RefCell<HashMap<u64, RcWeak<wiring::Wiring>>> = RefCell::new(HashMap::new());
}

fn register_shell(wiring: &Rc<wiring::Wiring>) -> u64 {
    let id = NEXT_SHELL_ID.fetch_add(1, Ordering::Relaxed);
    ATTACHED.with(|map| {
        let mut map = map.borrow_mut();
        map.retain(|_, weak| weak.strong_count() > 0);
        map.insert(id, Rc::downgrade(wiring));
    });
    id
}

fn lookup_shell(id: u64) -> Option<Rc<wiring::Wiring>> {
    ATTACHED.with(|map| {
        let mut map = map.borrow_mut();
        let live = map.get(&id).and_then(RcWeak::upgrade);
        if live.is_none() {
            map.remove(&id);
        }
        live
    })
}

fn dispatch_player_sync(id: u64) {
    if let Some(wiring) = lookup_shell(id) {
        wiring.sync_player_stream();
    }
}

fn dispatch_settings_sync(id: u64) {
    if let Some(wiring) = lookup_shell(id) {
        wiring.sync_live_settings();
    }
}

/// Attaches the desktop window, winit host, themes, video host, stream server, and callbacks to
/// `app`, wiring `AppNotice` push notifications so zero polling timers are needed.
pub(crate) fn attach(window: &MainWindow, app: Arc<App>, tokio: tokio::runtime::Handle) {
    theme_apply::apply_and_record(window, app.data_dir(), app.logs());
    let video = video_host::VideoPlayer::install(window, app.logs().clone());
    let server = Arc::new(Supervisor::new(
        &crate::plumbing::stream_server::bundle_dir(),
        app.data_dir(),
        app.logs().clone(),
    ));

    let focused = Arc::new(AtomicBool::new(true));
    let gamepad = gamepad::spawn(window, app.clone(), focused.clone());
    let wiring = Rc::new(wiring::Wiring::new(
        window.as_weak(),
        app.clone(),
        video.clone(),
        server.clone(),
        tokio,
        gamepad,
    ));
    let shell_id = register_shell(&wiring);
    wiring::wire(window, (*wiring).clone());

    {
        let weak = window.as_weak();
        let app_obs = app.clone();
        let sync_pending = Arc::new(AtomicBool::new(false));
        app.set_observer(move |notice| match notice {
            AppNotice::Dirty => {
                let app_run = app_obs.clone();
                let _ = weak.upgrade_in_event_loop(move |window| {
                    let mask = view::apply_dirty(&window, &app_run);
                    if mask.contains(crate::app::ScreenMask::SETTINGS) {
                        dispatch_settings_sync(shell_id);
                    }
                });
            }
            AppNotice::PlayerStreamChanged => {
                if sync_pending.swap(true, Ordering::SeqCst) {
                    return;
                }
                let flag = sync_pending.clone();
                if weak
                    .upgrade_in_event_loop(move |_| {
                        flag.store(false, Ordering::SeqCst);
                        dispatch_player_sync(shell_id);
                    })
                    .is_err()
                {
                    sync_pending.store(false, Ordering::SeqCst);
                }
            }
            AppNotice::AuthSucceeded => {
                let _ = weak.upgrade_in_event_loop(|window| {
                    auth::clear_progress(&window);
                    let ui = window.global::<crate::AuthUi>();
                    ui.set_error("".into());
                    ui.set_open(false);
                });
            }
            AppNotice::AuthFailed(message) => {
                let _ = weak.upgrade_in_event_loop(move |window| {
                    auth::clear_progress(&window);
                    if let Some(message) = message {
                        window.global::<crate::AuthUi>().set_error(message.into());
                    }
                });
            }
        });
    }

    {
        let app_focus = app.clone();
        let app_min = app.clone();
        let video_min = video.clone();
        let win_min = window.as_weak();
        winit_host::install(
            window,
            app.clone(),
            focused,
            move || app_focus.sync_account(),
            move || {
                if win_min.upgrade().is_some_and(|w| w.get_playing())
                    && app_min.settings().view().pause_on_minimize
                {
                    let _ = video_min.set_paused(true);
                }
            },
        );
    }
    {
        // Keep `Rc<Wiring>` alive inside the window-owned `VideoPlayer` listener so the
        // `ATTACHED` weak entry stays valid for the exact lifetime of `window`.
        let wiring_vid = Rc::clone(&wiring);
        video.listen(move |status| wiring_vid.playback_changed(status));
    }

    let initial_settings = app.settings().view();
    video.apply_settings(&initial_settings.player_config);
    wiring.sync_live_settings();
    view::apply_settings(window, &initial_settings, app.images());
    let settings_ui = window.global::<crate::SettingsUi>();
    settings_ui.set_app_version(env!("CARGO_PKG_VERSION").into());
    settings_ui.set_build_version(option_env!("GITHUB_SHA").unwrap_or("dev").into());
    window
        .global::<crate::DetailsUi>()
        .set_wrap_streams(initial_settings.wrap_streams);

    tray::install(window, app.clone(), video.clone());
    wiring.report_server_state(server.start());
    wiring.init_mods();
}
