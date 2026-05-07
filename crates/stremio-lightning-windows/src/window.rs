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
            width: 1280,
            height: 720,
            min_width: 640,
            min_height: 480,
        }
    }
}

pub const UI_THREAD_WAKE_MESSAGE: u32 = platform::UI_THREAD_WAKE_MESSAGE;

pub fn run_native_window(config: WindowConfig) -> Result<(), String> {
    platform::run_native_window(config)
}

#[cfg(windows)]
pub use platform::{
    focus_window, run_native_window_with_handler, NativeWindowHandler, UiThreadNotifier,
};

#[cfg(windows)]
mod platform {
    use super::WindowConfig;
    use std::ffi::c_void;
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::HBRUSH;
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::{
        SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
        GetMessageW, GetWindowLongPtrW, IsIconic, LoadCursorW, PostMessageW, PostQuitMessage,
        RegisterClassW, SetForegroundWindow, SetWindowLongPtrW, ShowWindow, TranslateMessage,
        CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, GWLP_USERDATA, IDC_ARROW, MINMAXINFO,
        MSG, SHOW_WINDOW_CMD, SW_RESTORE, SW_SHOWDEFAULT, WINDOW_EX_STYLE, WM_ACTIVATE, WM_APP,
        WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_GETMINMAXINFO, WM_NCCREATE, WM_NCDESTROY, WM_SIZE,
        WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
    };

    pub const UI_THREAD_WAKE_MESSAGE: u32 = WM_APP + 1;

    struct WindowState {
        config: WindowConfig,
        handler: Option<Box<dyn NativeWindowHandler>>,
    }

    pub trait NativeWindowHandler {
        fn on_created(&mut self, hwnd: HWND) -> Result<(), String>;
        fn on_resized(&mut self, _hwnd: HWND, _client_rect: RECT) -> Result<(), String> {
            Ok(())
        }
        fn on_ui_thread_wake(&mut self, _hwnd: HWND) -> Result<(), String> {
            Ok(())
        }
        fn on_destroying(&mut self, _hwnd: HWND) {}
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct UiThreadNotifier {
        pub(crate) hwnd: HWND,
    }

    unsafe impl Send for UiThreadNotifier {}
    unsafe impl Sync for UiThreadNotifier {}

    impl UiThreadNotifier {
        pub fn notify(self) -> Result<(), String> {
            unsafe {
                PostMessageW(
                    Some(self.hwnd),
                    UI_THREAD_WAKE_MESSAGE,
                    WPARAM(0),
                    LPARAM(0),
                )
            }
            .map_err(|error| format!("Failed to notify Windows UI thread: {error}"))
        }
    }

    pub fn run_native_window(config: WindowConfig) -> Result<(), String> {
        run_native_window_with_handler(config, NoopWindowHandler)
    }

    pub fn focus_window(hwnd: HWND) {
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SHOW_WINDOW_CMD(SW_RESTORE.0));
            }
            let _ = SetForegroundWindow(hwnd);
        }
    }

    pub fn run_native_window_with_handler(
        config: WindowConfig,
        handler: impl NativeWindowHandler + 'static,
    ) -> Result<(), String> {
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }

        let hwnd = create_main_window(config, Box::new(handler))?;
        unsafe {
            let notifier = UiThreadNotifier { hwnd };
            notifier.notify()?;
            let _ = ShowWindow(hwnd, SHOW_WINDOW_CMD(SW_SHOWDEFAULT.0));
            run_message_loop()
        }
    }

    struct NoopWindowHandler;

    impl NativeWindowHandler for NoopWindowHandler {
        fn on_created(&mut self, _hwnd: HWND) -> Result<(), String> {
            Ok(())
        }
    }

    fn create_main_window(
        config: WindowConfig,
        handler: Box<dyn NativeWindowHandler>,
    ) -> Result<HWND, String> {
        let instance = unsafe { GetModuleHandleW(None) }
            .map_err(|error| format!("Failed to get module handle: {error}"))?;
        let class_name = w!("StremioLightningWindow");

        let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }
            .map_err(|error| format!("Failed to load default cursor: {error}"))?;

        let window_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            hCursor: cursor,
            hbrBackground: HBRUSH::default(),
            lpszClassName: class_name,
            ..Default::default()
        };

        let atom = unsafe { RegisterClassW(&window_class) };
        if atom == 0 {
            return Err("Failed to register Windows window class".to_string());
        }

        let title = to_wide_null(config.title);
        let state = Box::new(WindowState {
            config,
            handler: Some(handler),
        });
        let state_ptr = Box::into_raw(state);

        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                PCWSTR(title.as_ptr()),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                (*state_ptr).config.width,
                (*state_ptr).config.height,
                None,
                None,
                Some(instance.into()),
                Some(state_ptr.cast::<c_void>()),
            )
        };

        match hwnd {
            Ok(hwnd) => {
                let state = unsafe { window_state(hwnd) };
                if !state.is_null() {
                    if let Some(handler) = unsafe { (*state).handler.as_mut() } {
                        handler.on_created(hwnd)?;
                    }
                }
                Ok(hwnd)
            }
            Err(error) => {
                unsafe {
                    drop(Box::from_raw(state_ptr));
                }
                Err(format!("Failed to create Windows window: {error}"))
            }
        }
    }

    unsafe fn run_message_loop() -> Result<(), String> {
        let mut message = MSG::default();
        loop {
            let result = GetMessageW(&mut message, None, 0, 0).0;
            if result == -1 {
                return Err("Windows message loop failed".to_string());
            }
            if result == 0 {
                return Ok(());
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        unsafe {
            match message {
                WM_NCCREATE => {
                    let create = lparam.0 as *const CREATESTRUCTW;
                    let state = (*create).lpCreateParams.cast::<WindowState>();
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
                    LRESULT(1)
                }
                WM_GETMINMAXINFO => {
                    let state = window_state(hwnd);
                    if !state.is_null() {
                        let minmax = lparam.0 as *mut MINMAXINFO;
                        (*minmax).ptMinTrackSize.x = (*state).config.min_width;
                        (*minmax).ptMinTrackSize.y = (*state).config.min_height;
                    }
                    LRESULT(0)
                }
                WM_SIZE => {
                    let state = window_state(hwnd);
                    if !state.is_null() {
                        let mut rect = RECT::default();
                        let _ = GetClientRect(hwnd, &mut rect);
                        if let Some(handler) = (*state).handler.as_mut() {
                            let _ = handler.on_resized(hwnd, rect);
                        }
                    }
                    LRESULT(0)
                }
                UI_THREAD_WAKE_MESSAGE => {
                    let state = window_state(hwnd);
                    if !state.is_null() {
                        if let Some(handler) = (*state).handler.as_mut() {
                            let _ = handler.on_ui_thread_wake(hwnd);
                        }
                    }
                    LRESULT(0)
                }
                WM_CLOSE => {
                    let _ = DestroyWindow(hwnd);
                    LRESULT(0)
                }
                WM_DESTROY => {
                    PostQuitMessage(0);
                    LRESULT(0)
                }
                WM_NCDESTROY => {
                    let state = window_state(hwnd);
                    if !state.is_null() {
                        if let Some(handler) = (*state).handler.as_mut() {
                            handler.on_destroying(hwnd);
                        }
                        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                        drop(Box::from_raw(state));
                    }
                    DefWindowProcW(hwnd, message, wparam, lparam)
                }
                WM_ACTIVATE | WM_DPICHANGED => LRESULT(0),
                _ => DefWindowProcW(hwnd, message, wparam, lparam),
            }
        }
    }

    unsafe fn window_state(hwnd: HWND) -> *mut WindowState {
        GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState
    }

    fn to_wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(not(windows))]
mod platform {
    use super::WindowConfig;

    pub const UI_THREAD_WAKE_MESSAGE: u32 = 0x8001;

    pub fn run_native_window(_config: WindowConfig) -> Result<(), String> {
        Err("Native Windows window can only run on Windows".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_window_config_matches_milestone_baseline() {
        let config = WindowConfig::default();

        assert_eq!(config.title, crate::APP_NAME);
        assert_eq!((config.width, config.height), (1280, 720));
        assert_eq!((config.min_width, config.min_height), (640, 480));
    }

    #[test]
    fn ui_thread_wake_message_uses_app_message_range() {
        assert!(UI_THREAD_WAKE_MESSAGE >= 0x8000);
    }
}
