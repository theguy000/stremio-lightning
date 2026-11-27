//! Cross-platform system tray adapter (`tray-icon` with pure-Rust D-Bus `ksni` on Linux, native
//! Win32 on Windows, native AppKit on macOS). Pure event-driven callbacks; zero Slint imports.

use std::sync::Arc;

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

const LOGO: &[u8] = include_bytes!("../../../../assets/icons/128x128.png");
const ID_SHOW: &str = "tray.show";
const ID_TOP: &str = "tray.always_on_top";
const ID_QUIT: &str = "tray.quit";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayCommand {
    Show,
    ToggleVisible,
    ToggleAlwaysOnTop,
    Quit,
}

pub struct TrayHandle {
    _tray: TrayIcon,
    show_item: CheckMenuItem,
    top_item: CheckMenuItem,
}

impl TrayHandle {
    pub fn spawn(on_command: impl Fn(TrayCommand) + Send + Sync + 'static) -> Result<Self, String> {
        let icon = load_icon()?;
        let show_item = CheckMenuItem::with_id(ID_SHOW, "Show window", true, true, None);
        let top_item = CheckMenuItem::with_id(ID_TOP, "Always on top", true, false, None);
        let quit_item = MenuItem::with_id(ID_QUIT, "Quit", true, None);

        let menu = Menu::new();
        menu.append(&show_item).map_err(|e| e.to_string())?;
        menu.append(&top_item).map_err(|e| e.to_string())?;
        menu.append(&PredefinedMenuItem::separator())
            .map_err(|e| e.to_string())?;
        menu.append(&quit_item).map_err(|e| e.to_string())?;

        let handler = Arc::new(on_command);
        let click_handler = Arc::clone(&handler);
        TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                click_handler(TrayCommand::Show);
            }
        }));

        let menu_handler = Arc::clone(&handler);
        MenuEvent::set_event_handler(Some(Arc::new(move |event: MenuEvent| {
            if let Some(cmd) = command_for_menu_id(&event.id) {
                menu_handler(cmd);
            }
        })));

        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("Stremio Lightning")
            .with_icon(icon)
            .build()
            .map_err(|e| format!("could not create system tray icon: {e}"))?;

        Ok(Self {
            _tray: tray,
            show_item,
            top_item,
        })
    }

    pub fn set_window_visible(&self, visible: bool) {
        self.show_item.set_checked(visible);
    }

    pub fn set_always_on_top(&self, on_top: bool) {
        self.top_item.set_checked(on_top);
    }
}

fn command_for_menu_id(id: &MenuId) -> Option<TrayCommand> {
    match id.0.as_str() {
        ID_SHOW => Some(TrayCommand::ToggleVisible),
        ID_TOP => Some(TrayCommand::ToggleAlwaysOnTop),
        ID_QUIT => Some(TrayCommand::Quit),
        _ => None,
    }
}

fn load_icon() -> Result<Icon, String> {
    let img = image::load_from_memory(LOGO)
        .map_err(|e| format!("tray icon decode failed: {e}"))?
        .to_rgba8();
    let (w, h) = (img.width(), img.height());
    Icon::from_rgba(img.into_raw(), w, h).map_err(|e| format!("tray icon invalid: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_menu_ids_and_decodes_tray_icon() {
        assert_eq!(
            command_for_menu_id(&MenuId::new(ID_SHOW)),
            Some(TrayCommand::ToggleVisible)
        );
        assert_eq!(
            command_for_menu_id(&MenuId::new(ID_TOP)),
            Some(TrayCommand::ToggleAlwaysOnTop)
        );
        assert_eq!(
            command_for_menu_id(&MenuId::new(ID_QUIT)),
            Some(TrayCommand::Quit)
        );
        assert_eq!(command_for_menu_id(&MenuId::new("other")), None);
        assert!(load_icon().is_ok());
    }
}
