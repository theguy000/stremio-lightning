//! Routes `GamepadAction` events onto `MainWindow` when the window has OS focus.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use slint::platform::{Key, WindowEvent};
use slint::ComponentHandle;

use crate::app::App;
use crate::plumbing::gamepad::{GamepadAction, GamepadService};
use crate::shell::shortcuts::{self, GlobalShortcut};
use crate::{MainWindow, PlayerUi, Section, ShellModalsUi};

const TAB_ORDER: [Section; 7] = [
    Section::Board,
    Section::Discover,
    Section::Library,
    Section::Calendar,
    Section::Addons,
    Section::Mods,
    Section::Settings,
];

const SCROLL_STEP: f32 = 160.0;

pub(crate) fn step_section(current: Section, delta: i32) -> Section {
    let idx = TAB_ORDER.iter().position(|&s| s == current).unwrap_or(0) as i32;
    let next = (idx + delta).clamp(0, (TAB_ORDER.len() - 1) as i32) as usize;
    TAB_ORDER[next]
}

pub(crate) fn spawn(
    window: &MainWindow,
    app: Arc<App>,
    focused: Arc<AtomicBool>,
) -> Arc<GamepadService> {
    let weak = window.as_weak();
    Arc::new(GamepadService::new(move |action| {
        if !matches!(
            action,
            GamepadAction::Connected(_) | GamepadAction::Disconnected
        ) && !focused.load(Ordering::Relaxed)
        {
            return;
        }
        let w = weak.clone();
        let app_run = app.clone();
        let _ = w.upgrade_in_event_loop(move |window| {
            dispatch(&window, &app_run, action);
        });
    }))
}

pub(crate) fn dispatch(window: &MainWindow, app: &App, action: GamepadAction) {
    let modals = window.global::<ShellModalsUi>();
    let cid = action.control_id();
    if !cid.is_empty() && modals.get_gamepad_open() {
        modals.set_gamepad_active(cid.into());
    }

    match action {
        GamepadAction::Connected(family) => {
            modals.set_gamepad_family(family.as_str().into());
            window.set_toast("Gamepad detected".into());
            return;
        }
        GamepadAction::Disconnected => {
            window.set_toast("Gamepad disconnected".into());
            return;
        }
        _ => {}
    }

    if modals.get_gamepad_open() {
        if matches!(action, GamepadAction::Back | GamepadAction::ActionX) {
            modals.set_gamepad_open(false);
            modals.set_gamepad_active("".into());
        }
        return;
    }

    if window.get_playing() {
        dispatch_player(window, app, action);
        return;
    }

    match action {
        GamepadAction::PrevTab => {
            if !shortcuts::any_modal_open(window) {
                shortcuts::switch_section(window, step_section(window.get_section(), -1));
            }
        }
        GamepadAction::NextTab => {
            if !shortcuts::any_modal_open(window) {
                shortcuts::switch_section(window, step_section(window.get_section(), 1));
            }
        }
        GamepadAction::Back => {
            shortcuts::execute(window, app, GlobalShortcut::NavigateBack);
        }
        GamepadAction::ActionX => {
            shortcuts::execute(window, app, GlobalShortcut::ToggleGamepadModal);
        }
        GamepadAction::Fullscreen => {
            shortcuts::execute(window, app, GlobalShortcut::ToggleFullscreen);
        }
        GamepadAction::MoveUp => scroll_or_key(window, SCROLL_STEP, Key::UpArrow),
        GamepadAction::MoveDown => scroll_or_key(window, -SCROLL_STEP, Key::DownArrow),
        GamepadAction::MoveLeft => tap_key(window, Key::Backtab),
        GamepadAction::MoveRight => tap_key(window, Key::Tab),
        GamepadAction::Confirm => tap_key(window, Key::Return),
        _ => {}
    }
}

fn dispatch_player(window: &MainWindow, app: &App, action: GamepadAction) {
    let ui = window.global::<PlayerUi>();
    match action {
        GamepadAction::ActionX | GamepadAction::Confirm => ui.invoke_play_pause(),
        GamepadAction::Back => {
            if window.window().is_fullscreen() && app.settings().view().esc_exit_fullscreen {
                window.window().set_fullscreen(false);
                ui.set_fullscreen(false);
            } else {
                ui.invoke_back();
            }
        }
        GamepadAction::Fullscreen => ui.invoke_toggle_fullscreen(),
        GamepadAction::SeekBack | GamepadAction::MoveLeft => {
            ui.invoke_seek_by(-ui.get_seek_step());
        }
        GamepadAction::SeekForward | GamepadAction::MoveRight => {
            ui.invoke_seek_by(ui.get_seek_step());
        }
        GamepadAction::VolumeUp | GamepadAction::MoveUp => {
            ui.invoke_set_volume((ui.get_volume() + 5.0).clamp(0.0, 100.0));
        }
        GamepadAction::VolumeDown | GamepadAction::MoveDown => {
            ui.invoke_set_volume((ui.get_volume() - 5.0).clamp(0.0, 100.0));
        }
        _ => {}
    }
}

fn scroll_or_key(window: &MainWindow, delta: f32, fallback_key: Key) {
    if window.get_details_open() || shortcuts::any_modal_open(window) {
        tap_key(window, fallback_key);
        return;
    }
    match window.get_section() {
        Section::Board => window.set_board_scroll((window.get_board_scroll() + delta).min(0.0)),
        Section::Discover => {
            window.set_discover_scroll((window.get_discover_scroll() + delta).min(0.0))
        }
        Section::Library => {
            window.set_library_scroll((window.get_library_scroll() + delta).min(0.0))
        }
        Section::Addons => window.set_addons_scroll((window.get_addons_scroll() + delta).min(0.0)),
        Section::Settings => {
            window.set_settings_scroll((window.get_settings_scroll() + delta).min(0.0))
        }
        _ => tap_key(window, fallback_key),
    }
}

fn tap_key(window: &MainWindow, key: Key) {
    let text: slint::SharedString = key.into();
    window
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window
        .window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_stepping_clamps_without_wrapping() {
        assert_eq!(step_section(Section::Board, -1), Section::Board);
        assert_eq!(step_section(Section::Board, 1), Section::Discover);
        assert_eq!(step_section(Section::Settings, 1), Section::Settings);
        assert_eq!(step_section(Section::Settings, -1), Section::Mods);
    }
}
