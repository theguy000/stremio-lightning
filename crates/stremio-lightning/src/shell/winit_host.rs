//! Unified winit host adapter for `MainWindow`.
//!
//! Slint's winit backend exposes a single `on_winit_window_event` hook per window. Putting the
//! three platform-window concerns here lets them share that hook with zero polling timers:
//! - **Window icon**: applied once on the first winit event instead of polling every 16ms.
//! - **Window focus**: rising-edge `WindowEvent::Focused(true)` triggers account re-sync across
//!   Windows, Linux, and macOS instead of polling `GetForegroundWindow` every 700ms.
//! - **Instant mouse wheel**: bypasses Slint's 180ms animated `Flickable` wheel scroll on
//!   scrollable screens while honoring active popups and overlays.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use i_slint_backend_winit::{EventResult, WinitWindowAccessor};
use i_slint_core::window::WindowInner;
use slint::{ComponentHandle, LogicalPosition, Weak};
use winit::event::{MouseScrollDelta, WindowEvent};
use winit::window::{Icon, Window};

use crate::{MainWindow, Section};

/// Embedded 128x128 RGBA application icon shared with Linux packaging and top bar.
const LOGO: &[u8] = include_bytes!("../../../../assets/icons/128x128.png");

/// Logical pixels scrolled per mouse-wheel detent (matches a 3-line browser scroll).
const LINE_STEP: f32 = 120.0;

#[derive(Clone, Copy)]
struct Bounds {
    x: f32,
    y: f32,
    width: f32,
    viewport: f32,
    content: f32,
}

#[derive(Clone, Copy)]
enum Target {
    Board,
    Discover,
    Library,
    Addons,
    Settings,
}

impl Target {
    fn from_id(id: &str) -> Option<Self> {
        match id {
            "board" => Some(Self::Board),
            "discover" => Some(Self::Discover),
            "library" => Some(Self::Library),
            "addons" => Some(Self::Addons),
            "settings" => Some(Self::Settings),
            _ => None,
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    fn current_offset(self, window: &MainWindow) -> f32 {
        match self {
            Self::Board => window.get_board_scroll(),
            Self::Discover => window.get_discover_scroll(),
            Self::Library => window.get_library_scroll(),
            Self::Addons => window.get_addons_scroll(),
            Self::Settings => window.get_settings_scroll(),
        }
    }

    fn set_offset(self, window: &MainWindow, value: f32) {
        match self {
            Self::Board => window.set_board_scroll(value),
            Self::Discover => window.set_discover_scroll(value),
            Self::Library => window.set_library_scroll(value),
            Self::Addons => window.set_addons_scroll(value),
            Self::Settings => window.set_settings_scroll(value),
        }
    }
}

/// Installs the unified winit window-event hook and scroll-bounds callback on `window`.
///
/// `on_focus_gain` fires on every rising `WindowEvent::Focused(true)` edge after an unfocused
/// state (startup already performs the initial account pull, so initial focus does not double-pull).
pub(crate) fn install(window: &MainWindow, on_focus_gain: impl Fn() + 'static) {
    let bounds: Rc<RefCell<[Option<Bounds>; 5]>> = Rc::new(RefCell::new([None; 5]));
    {
        let bounds = bounds.clone();
        window.on_scroll_bounds(move |id, x, y, width, viewport, content| {
            if let Some(target) = Target::from_id(id.as_str()) {
                bounds.borrow_mut()[target.index()] = Some(Bounds {
                    x,
                    y,
                    width,
                    viewport,
                    content,
                });
            }
        });
    }

    let weak: Weak<MainWindow> = window.as_weak();
    let cursor = Rc::new(Cell::new(LogicalPosition::new(0.0, 0.0)));
    let icon_installed = Rc::new(Cell::new(false));
    let was_focused = Rc::new(Cell::new(true));

    window
        .window()
        .on_winit_window_event(move |slint_window, event| {
            if !icon_installed.get()
                && slint_window
                    .with_winit_window(|winit_window: &Window| match icon() {
                        Ok(icon) => winit_window.set_window_icon(Some(icon)),
                        Err(error) => eprintln!("[window] {error}"),
                    })
                    .is_some()
            {
                icon_installed.set(true);
            }

            match event {
                WindowEvent::Focused(focused) => {
                    if *focused && !was_focused.get() {
                        on_focus_gain();
                    }
                    was_focused.set(*focused);
                    EventResult::Propagate
                }
                WindowEvent::CursorMoved { position, .. } => {
                    let scale = f64::from(slint_window.scale_factor());
                    cursor.set(LogicalPosition::new(
                        (position.x / scale) as f32,
                        (position.y / scale) as f32,
                    ));
                    EventResult::Propagate
                }
                WindowEvent::MouseWheel { delta, .. } => {
                    handle_wheel(&weak, slint_window, &bounds, cursor.get(), *delta)
                }
                _ => EventResult::Propagate,
            }
        });
}

fn handle_wheel(
    weak: &Weak<MainWindow>,
    slint_window: &slint::Window,
    bounds: &RefCell<[Option<Bounds>; 5]>,
    at: LogicalPosition,
    delta: MouseScrollDelta,
) -> EventResult {
    let Some(window) = weak.upgrade() else {
        return EventResult::Propagate;
    };
    if !WindowInner::from_pub(slint_window)
        .active_popups()
        .is_empty()
        || window.get_details_open()
        || window.get_playing()
        || window.global::<crate::AuthUi>().get_open()
        || window.global::<crate::AddonsUi>().get_add_open()
        || window.get_share_open()
    {
        return EventResult::Propagate;
    }
    let target = match window.get_section() {
        Section::Board => Target::Board,
        Section::Discover => Target::Discover,
        Section::Library if window.global::<crate::SettingsUi>().get_data().logged_in => {
            Target::Library
        }
        Section::Addons => Target::Addons,
        Section::Settings => Target::Settings,
        _ => return EventResult::Propagate,
    };
    let Some(area) = bounds.borrow()[target.index()] else {
        return EventResult::Propagate;
    };
    let inside = at.x >= area.x
        && at.x <= area.x + area.width
        && at.y >= area.y
        && at.y <= area.y + area.viewport;
    if !inside {
        return EventResult::Propagate;
    }
    let down = match delta {
        MouseScrollDelta::LineDelta(_, lines) => -lines * LINE_STEP,
        MouseScrollDelta::PixelDelta(position) => {
            -(position.y / f64::from(slint_window.scale_factor())) as f32
        }
    };
    if down == 0.0 {
        return EventResult::Propagate;
    }
    let furthest = (area.content - area.viewport).max(0.0);
    let next = (target.current_offset(&window) - down).clamp(-furthest, 0.0);
    target.set_offset(&window, next);
    EventResult::PreventDefault
}

fn icon() -> Result<Icon, String> {
    decode().and_then(|(pixels, width, height)| {
        Icon::from_rgba(pixels, width, height)
            .map_err(|error| format!("the window icon is not a valid rgba bitmap: {error}"))
    })
}

fn decode() -> Result<(Vec<u8>, u32, u32), String> {
    let image = image::load_from_memory(LOGO)
        .map_err(|error| format!("the window icon could not be decoded: {error}"))?
        .to_rgba8();
    Ok((image.as_raw().clone(), image.width(), image.height()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_logo_decodes_and_is_drawn() {
        let (pixels, width, height) = decode().expect("the embedded logo is a valid PNG");
        assert_eq!((width, height), (128, 128));
        let drawn = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 0)
            .count();
        assert!(
            drawn > (128 * 128 / 3),
            "only {drawn} of 16384 pixels are drawn: assets/icons/128x128.png is not the logo"
        );
    }

    #[test]
    fn winit_accepts_the_logo() {
        let icon = icon();
        assert!(icon.is_ok(), "winit rejected the logo: {:?}", icon.err());
    }
}
