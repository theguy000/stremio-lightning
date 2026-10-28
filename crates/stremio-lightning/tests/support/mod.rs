//! Shared helpers for the integration tests.
//!
//! Included with `mod support;` from test binaries that need it. Cargo only treats
//! `tests/*.rs` as separate binaries, so this directory is not built on its own.

/// Slint needs a platform backend before the first `Window` is created, and the default
/// winit backend needs `DISPLAY` or `WAYLAND_DISPLAY`, which CI containers do not have.
/// The testing backend renders nothing, and `init_no_event_loop` gives each test thread
/// its own instance, so the tests in one binary stay independent of each other.
///
/// Not every binary that includes this module makes a window -- the live addon run builds its
/// own with a real event loop -- so the one that does not leaves this unused.
#[allow(dead_code)]
pub fn init_slint_backend() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(i_slint_backend_testing::init_no_event_loop);
}

pub mod ui_driver;
