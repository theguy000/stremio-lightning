//! Shell lifecycle adapter for the system tray and `quitOnClose`.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use i_slint_backend_winit::WinitWindowAccessor;
use slint::{CloseRequestResponse, ComponentHandle};
use winit::window::{Window, WindowLevel};

use crate::app::App;
use crate::plumbing::tray::{TrayCommand, TrayHandle};
use crate::shell::video_host::VideoPlayer;
use crate::MainWindow;

struct TrayState {
    handle: Option<TrayHandle>,
    visible: bool,
    always_on_top: bool,
}

thread_local! {
    static ACTIVE_TRAY: RefCell<Option<Rc<RefCell<TrayState>>>> = const { RefCell::new(None) };
}

/// Decides whether closing the window should quit the process or hide to tray.
pub(crate) fn should_quit_on_close(quit_on_close: bool, tray_available: bool) -> bool {
    quit_on_close || !tray_available
}

pub(crate) fn install(window: &MainWindow, app: Arc<App>, video: VideoPlayer) {
    let tray_ready = Arc::new(AtomicBool::new(false));

    {
        let weak = window.as_weak();
        let app_c = app.clone();
        let video_c = video.clone();
        let ready_c = tray_ready.clone();
        window.window().on_close_requested(move || {
            let settings = app_c.settings().view();
            if should_quit_on_close(settings.quit_on_close, ready_c.load(Ordering::Relaxed)) {
                ACTIVE_TRAY.with(|slot| *slot.borrow_mut() = None);
                let _ = slint::quit_event_loop();
                return CloseRequestResponse::HideWindow;
            }
            if weak.upgrade().is_some_and(|w| w.get_playing()) && settings.pause_on_minimize {
                let _ = video_c.set_paused(true);
            }
            ACTIVE_TRAY.with(|slot| {
                if let Some(st) = slot.borrow().as_ref() {
                    let mut st = st.borrow_mut();
                    st.visible = false;
                    if let Some(h) = st.handle.as_ref() {
                        h.set_window_visible(false);
                    }
                }
            });
            CloseRequestResponse::HideWindow
        });
    }

    // Create the real OS tray icon once the event loop starts (required by macOS AppKit).
    // Skipped automatically under `i-slint-backend-testing` where `with_winit_window` is `None`.
    let weak = window.as_weak();
    let ready_init = tray_ready.clone();
    let _ = slint::invoke_from_event_loop(move || {
        let Some(window) = weak.upgrade() else {
            return;
        };
        if window.window().with_winit_window(|_: &Window| ()).is_none() {
            return;
        }
        let cmd_weak = window.as_weak();
        match TrayHandle::spawn(move |cmd| {
            let w = cmd_weak.clone();
            let _ = w.upgrade_in_event_loop(move |window| {
                ACTIVE_TRAY.with(|slot| {
                    if let Some(st) = slot.borrow().as_ref() {
                        handle_command(&window, st, cmd);
                    }
                });
            });
        }) {
            Ok(handle) => {
                let state = Rc::new(RefCell::new(TrayState {
                    handle: Some(handle),
                    visible: true,
                    always_on_top: false,
                }));
                ACTIVE_TRAY.with(|slot| *slot.borrow_mut() = Some(state));
                ready_init.store(true, Ordering::Relaxed);
            }
            Err(err) => {
                eprintln!("[tray] {err}");
            }
        }
    });
}

fn handle_command(window: &MainWindow, state: &RefCell<TrayState>, cmd: TrayCommand) {
    match cmd {
        TrayCommand::Show => show_window(window, state),
        TrayCommand::ToggleVisible => {
            if state.borrow().visible {
                let _ = window.hide();
                let mut st = state.borrow_mut();
                st.visible = false;
                if let Some(h) = st.handle.as_ref() {
                    h.set_window_visible(false);
                }
            } else {
                show_window(window, state);
            }
        }
        TrayCommand::ToggleAlwaysOnTop => {
            if window.window().is_fullscreen() {
                return;
            }
            let next = !state.borrow().always_on_top;
            let _ = window.window().with_winit_window(|w: &Window| {
                w.set_window_level(if next {
                    WindowLevel::AlwaysOnTop
                } else {
                    WindowLevel::Normal
                });
            });
            let mut st = state.borrow_mut();
            st.always_on_top = next;
            if let Some(h) = st.handle.as_ref() {
                h.set_always_on_top(next);
            }
        }
        TrayCommand::Quit => {
            ACTIVE_TRAY.with(|slot| *slot.borrow_mut() = None);
            let _ = slint::quit_event_loop();
        }
    }
}

fn show_window(window: &MainWindow, state: &RefCell<TrayState>) {
    let _ = window.show();
    let _ = window.window().with_winit_window(|w: &Window| {
        w.set_minimized(false);
        w.focus_window();
    });
    let mut st = state.borrow_mut();
    st.visible = true;
    if let Some(h) = st.handle.as_ref() {
        h.set_window_visible(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_policy_respects_quit_on_close_and_tray_availability() {
        assert!(should_quit_on_close(true, true));
        assert!(should_quit_on_close(true, false));
        assert!(should_quit_on_close(false, false));
        assert!(!should_quit_on_close(false, true));
    }
}
