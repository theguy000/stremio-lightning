use thiserror::Error;

#[cfg(windows)]
use windows::Win32::Foundation::{HWND, RECT};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::WINDOWPLACEMENT;

#[derive(Debug, Error)]
pub enum WindowError {
    #[error("Failed to notify Windows UI thread: {0}")]
    UiNotify(String),

    #[error("Failed to set application user model ID: {0}")]
    AppUserModelId(String),

    #[error("Failed to read window placement: {0}")]
    WindowPlacement(String),

    #[error("Failed to read fullscreen monitor bounds")]
    MonitorBounds,

    #[error("Failed to enter fullscreen: {0}")]
    EnterFullscreen(String),

    #[error("Failed to exit fullscreen: {0}")]
    ExitFullscreen(String),

    #[error("Failed to restore window placement: {0}")]
    RestorePlacement(String),

    #[error("Failed to read PiP window placement: {0}")]
    PipPlacement(String),

    #[error("Failed to read PiP window bounds: {0}")]
    PipBounds(String),

    #[error("Failed to enter PiP: {0}")]
    EnterPip(String),

    #[error("Failed to exit PiP: {0}")]
    ExitPip(String),

    #[error("Failed to get module handle: {0}")]
    ModuleHandle(String),

    #[error("Failed to load default cursor: {0}")]
    LoadCursor(String),

    #[error("Failed to load application icon: {0}")]
    LoadIcon(String),

    #[error("Failed to register Windows window class")]
    RegisterClass,

    #[error("Failed to create Windows window: {0}")]
    CreateWindow(String),

    #[error("Windows message loop failed")]
    MessageLoop,

    #[error("Native Windows window can only run on Windows")]
    WindowsOnly,

    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowConfig {
    pub title: &'static str,
    pub width: i32,
    pub height: i32,
    pub min_width: i32,
    pub min_height: i32,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: crate::APP_NAME,
            width: 1500,
            height: 850,
            min_width: 800,
            min_height: 600,
        }
    }
}

#[cfg(windows)]
pub const UI_THREAD_WAKE_MESSAGE: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 1;

#[cfg(not(windows))]
pub const UI_THREAD_WAKE_MESSAGE: u32 = 0x8001;

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowVisualState {
    Minimized,
    Maximized,
    Restored,
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKeyAction {
    PlayPause,
    NextTrack,
    PreviousTrack,
}

#[cfg(windows)]
pub trait NativeWindowHandler {
    fn on_created(&mut self, hwnd: HWND) -> Result<(), String>;
    fn on_resized(&mut self, _hwnd: HWND, _client_rect: RECT) -> Result<(), String> {
        Ok(())
    }
    fn on_window_state_changed(
        &mut self,
        _hwnd: HWND,
        _state: WindowVisualState,
    ) -> Result<(), String> {
        Ok(())
    }
    fn on_focus_changed(&mut self, _hwnd: HWND, _focused: bool) -> Result<(), String> {
        Ok(())
    }
    fn on_media_key(&mut self, _hwnd: HWND, _action: MediaKeyAction) -> Result<(), String> {
        Ok(())
    }
    /// Called when a coalesced UI-thread wake-up is delivered. Implementations
    /// that own a [`UiThreadNotifier`] must clear its pending marker before
    /// draining updates, so wake-ups posted during processing are not lost.
    fn on_ui_thread_wake(&mut self, _hwnd: HWND) -> Result<(), String> {
        Ok(())
    }
    fn on_destroying(&mut self, _hwnd: HWND) {}
}

#[cfg(windows)]
pub(crate) struct WindowState {
    pub config: WindowConfig,
    pub handler: Option<Box<dyn NativeWindowHandler>>,
}

#[cfg(windows)]
#[derive(Debug)]
pub(crate) struct FullscreenSnapshot {
    pub style: isize,
    pub ex_style: isize,
    pub placement: WINDOWPLACEMENT,
}

#[cfg(windows)]
#[derive(Debug)]
pub(crate) struct PipWindowSnapshot {
    pub style: isize,
    pub ex_style: isize,
    pub placement: WINDOWPLACEMENT,
}
