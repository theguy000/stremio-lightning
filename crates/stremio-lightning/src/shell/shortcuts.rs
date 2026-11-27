//! Global desktop keyboard shortcuts matching Stremio Web's `ShortcutsProvider`.
//!
//! Pure classification (`classify`) is separated from window execution (`execute`) so unit and
//! headless Slint tests can exercise every shortcut without a live winit event loop.

use slint::ComponentHandle;
use winit::keyboard::{Key, ModifiersState, NamedKey};

use crate::app::App;
use crate::{AddonsUi, AuthUi, CalendarUi, MainWindow, Section, SettingsUi, ShellModalsUi};

const ZOOM_STEPS: [i32; 5] = [75, 100, 125, 150, 175];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum GlobalShortcut {
    ZoomIn,
    ZoomOut,
    ZoomReset,
    SwitchSection(Section),
    FocusSearch,
    NavigateBack,
    ToggleFullscreen,
    ToggleShortcutsModal,
    ToggleGamepadModal,
    EscapeOverlay,
}

fn primary_mod(mods: ModifiersState) -> bool {
    if cfg!(target_os = "macos") {
        mods.super_key()
    } else {
        mods.control_key()
    }
}

/// Maps a winit key press to a global shortcut, honoring text-input and player-ownership guards.
pub(crate) fn classify(
    logical_key: &Key,
    pressed: bool,
    repeat: bool,
    mods: ModifiersState,
    text_input_focused: bool,
    playing: bool,
) -> Option<GlobalShortcut> {
    if !pressed || repeat || mods.alt_key() {
        return None;
    }

    let primary = primary_mod(mods);
    let ctrl = mods.control_key();
    let no_mods = !mods.control_key() && !mods.super_key() && !mods.shift_key();

    // Interface zoom works everywhere (even when an input is focused), matching Stremio Web.
    if primary && !mods.shift_key() {
        if let Key::Character(c) = &logical_key {
            match c.as_str() {
                "-" => return Some(GlobalShortcut::ZoomOut),
                "=" | "+" => return Some(GlobalShortcut::ZoomIn),
                "0" => return Some(GlobalShortcut::ZoomReset),
                _ => {}
            }
        }
    }

    // Modal toggles: Ctrl+/ (Shortcuts) and Ctrl+G (Gamepad Guide).
    if ctrl && !mods.super_key() && !mods.shift_key() && !text_input_focused {
        if let Key::Character(c) = &logical_key {
            match c.as_str() {
                "/" => return Some(GlobalShortcut::ToggleShortcutsModal),
                "g" | "G" => return Some(GlobalShortcut::ToggleGamepadModal),
                _ => {}
            }
        }
    }

    if let Key::Named(NamedKey::Escape) = logical_key {
        if !playing {
            return Some(GlobalShortcut::EscapeOverlay);
        }
        return None;
    }

    if playing || text_input_focused {
        return None;
    }

    if !no_mods {
        return None;
    }

    match &logical_key {
        Key::Named(NamedKey::F11) => Some(GlobalShortcut::ToggleFullscreen),
        Key::Named(NamedKey::Backspace) => Some(GlobalShortcut::NavigateBack),
        Key::Character(c) => match c.as_str() {
            "1" => Some(GlobalShortcut::SwitchSection(Section::Board)),
            "2" => Some(GlobalShortcut::SwitchSection(Section::Discover)),
            "3" => Some(GlobalShortcut::SwitchSection(Section::Library)),
            "4" => Some(GlobalShortcut::SwitchSection(Section::Calendar)),
            "5" => Some(GlobalShortcut::SwitchSection(Section::Addons)),
            "6" => Some(GlobalShortcut::SwitchSection(Section::Settings)),
            "0" => Some(GlobalShortcut::FocusSearch),
            "f" | "F" => Some(GlobalShortcut::ToggleFullscreen),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn any_modal_open(window: &MainWindow) -> bool {
    let modals = window.global::<ShellModalsUi>();
    modals.get_shortcuts_open()
        || modals.get_gamepad_open()
        || modals.get_spotlight_picker_open()
        || window.get_share_open()
        || window.global::<AuthUi>().get_open()
        || window.global::<AddonsUi>().get_add_open()
}

pub(crate) fn close_top_modal(window: &MainWindow) -> bool {
    let modals = window.global::<ShellModalsUi>();
    if modals.get_shortcuts_open() {
        modals.set_shortcuts_open(false);
        return true;
    }
    if modals.get_gamepad_open() {
        modals.set_gamepad_open(false);
        return true;
    }
    if modals.get_spotlight_picker_open() {
        modals.set_spotlight_picker_open(false);
        return true;
    }
    if window.get_share_open() {
        window.invoke_share_close();
        return true;
    }
    let auth = window.global::<AuthUi>();
    if auth.get_open() {
        auth.set_open(false);
        return true;
    }
    let addons = window.global::<AddonsUi>();
    if addons.get_add_open() {
        addons.set_add_open(false);
        return true;
    }
    false
}

pub(crate) fn switch_section(window: &MainWindow, target: Section) {
    let modals = window.global::<ShellModalsUi>();
    modals.set_shortcuts_open(false);
    modals.set_gamepad_open(false);
    modals.set_spotlight_picker_open(false);
    window.set_details_open(false);
    window.set_section(target);
    window.invoke_section_changed();
    if target == Section::Calendar {
        window.global::<CalendarUi>().invoke_shown();
    }
}

pub(crate) fn execute(window: &MainWindow, app: &App, action: GlobalShortcut) -> bool {
    match action {
        GlobalShortcut::ZoomIn | GlobalShortcut::ZoomOut | GlobalShortcut::ZoomReset => {
            let cur = app.settings().view().interface_scale as i32;
            let idx = ZOOM_STEPS.iter().position(|&s| s >= cur).unwrap_or(1);
            let next = match action {
                GlobalShortcut::ZoomIn => ZOOM_STEPS[(idx + 1).min(ZOOM_STEPS.len() - 1)],
                GlobalShortcut::ZoomOut => ZOOM_STEPS[idx.saturating_sub(1)],
                _ => 100,
            };
            if next != cur {
                window
                    .global::<SettingsUi>()
                    .invoke_set_interface_scale(next);
            }
            true
        }
        GlobalShortcut::SwitchSection(target) => {
            if any_modal_open(window) {
                return false;
            }
            switch_section(window, target);
            true
        }
        GlobalShortcut::FocusSearch => {
            if any_modal_open(window) {
                return false;
            }
            if window.get_details_open() || window.get_section() == Section::Addons {
                switch_section(window, Section::Board);
            }
            let modals = window.global::<ShellModalsUi>();
            modals.set_focus_search_seq(modals.get_focus_search_seq().wrapping_add(1));
            true
        }
        GlobalShortcut::NavigateBack => {
            if close_top_modal(window) {
                return true;
            }
            if window.get_details_open() {
                window.set_details_open(false);
                window.invoke_section_changed();
                return true;
            }
            false
        }
        GlobalShortcut::ToggleFullscreen => {
            let w = window.window();
            w.set_fullscreen(!w.is_fullscreen());
            true
        }
        GlobalShortcut::ToggleShortcutsModal => {
            let modals = window.global::<ShellModalsUi>();
            let next = !modals.get_shortcuts_open();
            modals.set_gamepad_open(false);
            modals.set_shortcuts_open(next);
            true
        }
        GlobalShortcut::ToggleGamepadModal => {
            let modals = window.global::<ShellModalsUi>();
            let next = !modals.get_gamepad_open();
            modals.set_shortcuts_open(false);
            modals.set_gamepad_open(next);
            true
        }
        GlobalShortcut::EscapeOverlay => {
            if close_top_modal(window) {
                return true;
            }
            if window.window().is_fullscreen() && app.settings().view().esc_exit_fullscreen {
                window.window().set_fullscreen(false);
                return true;
            }
            if window.get_details_open() {
                window.set_details_open(false);
                window.invoke_section_changed();
                return true;
            }
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_navigation_zoom_and_modal_shortcuts() {
        let none = ModifiersState::empty();
        let ctrl = ModifiersState::CONTROL;
        let prim = if cfg!(target_os = "macos") {
            ModifiersState::SUPER
        } else {
            ctrl
        };
        let ch =
            |s: &str, m, inp, play| classify(&Key::Character(s.into()), true, false, m, inp, play);
        assert_eq!(
            ch("1", none, false, false),
            Some(GlobalShortcut::SwitchSection(Section::Board))
        );
        assert_eq!(
            ch("6", none, false, false),
            Some(GlobalShortcut::SwitchSection(Section::Settings))
        );
        assert_eq!(
            ch("0", none, false, false),
            Some(GlobalShortcut::FocusSearch)
        );
        assert_eq!(
            ch("/", ctrl, false, false),
            Some(GlobalShortcut::ToggleShortcutsModal)
        );
        assert_eq!(
            ch("g", ctrl, false, false),
            Some(GlobalShortcut::ToggleGamepadModal)
        );
        assert_eq!(ch("+", prim, true, true), Some(GlobalShortcut::ZoomIn));
        assert_eq!(ch("1", none, true, false), None);
        assert_eq!(ch("1", none, false, true), None);
        let esc = Key::Named(NamedKey::Escape);
        assert_eq!(
            classify(&esc, true, false, none, true, false),
            Some(GlobalShortcut::EscapeOverlay)
        );
        assert_eq!(classify(&esc, true, false, none, false, true), None);
    }
}
