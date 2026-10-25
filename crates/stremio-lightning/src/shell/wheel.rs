//! The mouse wheel, taken before Slint sees it.
//!
//! Slint's `Flickable` animates every wheel notch over `WHEEL_SCROLL_DURATION` (180ms) and redraws
//! the whole window for each frame of that animation: measured on the Discover grid, one notch cost
//! about 200ms of a core and thirty-eight redraws, and holding the wheel kept a core pinned. The
//! Discover grid is therefore not a `Flickable`: it draws `discover-scroll`, and this moves that,
//! so a notch is one frame.
//!
//! The wheel is caught from winit rather than from Slint because Slint 1.18 has no wheel event in
//! its language (`'scroll' is not a member of PointerEventKind`). winit is what Slint's own winit
//! backend speaks, so this is the same on Windows, Linux and macOS -- only the shape of the delta
//! differs, and both shapes are handled below.

use std::cell::Cell;
use std::rc::Rc;

use i_slint_backend_winit::{EventResult, WinitWindowAccessor};
use slint::{ComponentHandle, LogicalPosition, Weak};
use winit::event::{MouseScrollDelta, WindowEvent};

use crate::{MainWindow, Section};

/// How far one line of a `LineDelta` scrolls, in logical pixels. winit reports a mouse wheel notch
/// as one line on Windows, and a browser scrolls about 100px for one.
const LINE_STEP: f32 = 100.0;

/// The Discover grid as it last reported itself: where it is on the window, how tall its viewport
/// is, and how tall everything inside it is.
#[derive(Clone, Copy)]
struct Grid {
    x: f32,
    y: f32,
    width: f32,
    viewport: f32,
    content: f32,
}

/// Hooks the window's own events and the grid's report of where it is.
pub(crate) fn wire(window: &MainWindow) {
    let grid: Rc<Cell<Option<Grid>>> = Rc::new(Cell::new(None));
    {
        let grid = grid.clone();
        window.on_grid_bounds(move |x, y, width, viewport, content| {
            grid.set(Some(Grid {
                x,
                y,
                width,
                viewport,
                content,
            }));
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
                // The bounds are the *last* ones the grid reported, and the grid is gone the moment
                // another screen opens: taking the wheel there would swallow it into an offset
                // nothing is drawing. On the board that is most of the page, which is why its wheel
                // stopped working after a visit to Discover.
                if window.get_section() != Section::Discover
                    || window.get_details_open()
                    || window.get_playing()
                {
                    return EventResult::Propagate;
                }
                let Some(grid) = grid.get() else {
                    return EventResult::Propagate;
                };
                let at = cursor.get();
                let inside = at.x >= grid.x
                    && at.x <= grid.x + grid.width
                    && at.y >= grid.y
                    && at.y <= grid.y + grid.viewport;
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
                // Clamped here, because the shell owns the offset: the grid only draws it.
                let furthest = (grid.content - grid.viewport).max(0.0);
                let next = (window.get_discover_scroll() - down).clamp(-furthest, 0.0);
                window.set_discover_scroll(next);
                EventResult::PreventDefault
            }
            _ => EventResult::Propagate,
        });
}
