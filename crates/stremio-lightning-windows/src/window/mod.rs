pub mod coalescer;
#[cfg(windows)]
pub mod controller;
#[cfg(windows)]
pub mod dark_mode;
#[cfg(test)]
mod tests;
pub mod types;
pub mod wndproc;

pub use coalescer::UiThreadNotifier;
pub use types::{WindowConfig, WindowError, UI_THREAD_WAKE_MESSAGE};
pub use wndproc::run_native_window;

#[cfg(windows)]
pub use controller::NativeWindowController;
#[cfg(windows)]
pub use types::{MediaKeyAction, NativeWindowHandler, WindowVisualState};
#[cfg(windows)]
pub use wndproc::{
    focus_window, run_native_window_with_handler, set_app_user_model_id,
};
