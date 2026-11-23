//! The mouse wheel, taken before Slint sees it.
//!
//! Slint's `Flickable` animates every wheel notch over `WHEEL_SCROLL_DURATION` (180ms) and redraws
//! the whole window for each frame of that animation: measured on the Discover grid, one notch cost
//! about 200ms of a core and thirty-eight redraws, and holding the wheel kept a core pinned. Taking
//! the wheel here moves each screen's scroll offset directly in one frame at a browser-like step.
//!
//! The wheel is caught from winit rather than from Slint because Slint 1.18 has no wheel event in
//! its language (`'scroll' is not a member of PointerEventKind`). winit is what Slint's own winit
//! backend speaks, so this is the same on Windows, Linux and macOS -- only the shape of the delta
//! differs, and both shapes are handled below.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use i_slint_backend_winit::{EventResult, WinitWindowAccessor};
use i_slint_core::window::WindowInner;
use slint::{ComponentHandle, LogicalPosition, Weak};
use winit::event::{MouseScrollDelta, WindowEvent};

use crate::{MainWindow, Section};

/// How far one wheel notch (`LineDelta`) scrolls, in logical pixels. On Windows winit emits 1.0 per
/// detent (ignoring the OS 3-line setting); 120px matches a standard 3-line browser scroll.
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

/// Hooks the window's wheel events and each scrollable screen's reported viewport bounds.
pub(crate) fn wire(window: &MainWindow) {
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
    // Slint reports pointer positions in physical pixels; the window works in logical ones.
    let cursor = Rc::new(Cell::new(LogicalPosition::new(0.0, 0.0)));
    window
        .window()
        .on_winit_window_event(move |slint_window, event| match event {
            WindowEvent::CursorMoved { position, .. } => {
                let scale = f64::from(slint_window.scale_factor());
                cursor.set(LogicalPosition::new(
                    (position.x / scale) as f32,
                    (position.y / scale) as f32,
                ));
                EventResult::Propagate
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let Some(window) = weak.upgrade() else {
                    return EventResult::Propagate;
                };
                if !WindowInner::from_pub(slint_window)
                    .active_popups()
                    .is_empty()
                    || window.get_details_open()
                    || window.get_playing()
                    || window.get_auth_open()
                    || window.get_addon_add_open()
                    || window.get_share_open()
                {
                    return EventResult::Propagate;
                }
                let target = match window.get_section() {
                    Section::Board => Target::Board,
                    Section::Discover => Target::Discover,
                    Section::Library if window.get_settings().logged_in => Target::Library,
                    Section::Addons => Target::Addons,
                    Section::Settings => Target::Settings,
                    _ => return EventResult::Propagate,
                };
                let Some(area) = bounds.borrow()[target.index()] else {
                    return EventResult::Propagate;
                };
                let at = cursor.get();
                let inside = at.x >= area.x
                    && at.x <= area.x + area.width
                    && at.y >= area.y
                    && at.y <= area.y + area.viewport;
                if !inside {
                    return EventResult::Propagate;
                }
                // Down is positive here: winit gives a wheel turned towards the user as a negative
                // line count (and a negative pixel delta), the opposite of what the offset wants.
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
            _ => EventResult::Propagate,
        });
}
