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
    focus_window, run_native_window_with_handler, MediaKeyAction, NativeWindowController,
    NativeWindowHandler, UiThreadNotifier, WindowVisualState,
};

#[cfg(windows)]
mod platform {
    use super::WindowConfig;
    use std::ffi::c_void;
    use stremio_lightning_core::pip::{
        PipRestoreSnapshot, PipWindowController, PIP_WINDOW_HEIGHT, PIP_WINDOW_WIDTH,
    };
    use windows::core::{w, PCWSTR};
    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        CreateSolidBrush, GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::{
        SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
        GetMessageW, GetWindowLongPtrW, GetWindowPlacement, GetWindowRect, IsIconic, IsZoomed,
        LoadCursorW, PostMessageW, PostQuitMessage, RegisterClassW, SendMessageW,
        SetForegroundWindow, SetWindowLongPtrW, SetWindowPlacement, SetWindowPos, ShowWindow,
        TranslateMessage, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, GWLP_USERDATA,
        GWL_EXSTYLE, GWL_STYLE, HTCAPTION, HWND_NOTOPMOST, HWND_TOPMOST, IDC_ARROW, MINMAXINFO,
        MSG, SHOW_WINDOW_CMD, SIZE_MAXIMIZED, SIZE_MINIMIZED, SIZE_RESTORED, SWP_FRAMECHANGED,
        SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE,
        SW_SHOWDEFAULT, WINDOWPLACEMENT, WINDOW_EX_STYLE, WM_ACTIVATE, WM_APP, WM_APPCOMMAND,
        WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_GETMINMAXINFO, WM_KILLFOCUS, WM_NCCREATE,
        WM_NCDESTROY, WM_NCLBUTTONDOWN, WM_SETFOCUS, WM_SIZE, WNDCLASSW, WS_EX_TOPMOST,
        WS_OVERLAPPEDWINDOW, WS_POPUP, WS_VISIBLE,
    };

    pub const UI_THREAD_WAKE_MESSAGE: u32 = WM_APP + 1;

    struct WindowState {
        config: WindowConfig,
        handler: Option<Box<dyn NativeWindowHandler>>,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum WindowVisualState {
        Minimized,
        Maximized,
        Restored,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MediaKeyAction {
        PlayPause,
        NextTrack,
        PreviousTrack,
    }

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

    #[derive(Debug)]
    pub struct NativeWindowController {
        hwnd: HWND,
        fullscreen: Option<FullscreenSnapshot>,
        pip: Option<PipWindowSnapshot>,
    }

    unsafe impl Send for NativeWindowController {}

    impl NativeWindowController {
        pub fn new(hwnd: HWND) -> Self {
            Self {
                hwnd,
                fullscreen: None,
                pip: None,
            }
        }

        pub fn minimize(&self) {
            unsafe {
                let _ = ShowWindow(self.hwnd, SHOW_WINDOW_CMD(SW_MINIMIZE.0));
            }
        }

        pub fn focus(&self) {
            focus_window(self.hwnd);
        }

        pub fn toggle_maximize(&self) -> bool {
            unsafe {
                if IsZoomed(self.hwnd).as_bool() {
                    let _ = ShowWindow(self.hwnd, SHOW_WINDOW_CMD(SW_RESTORE.0));
                    false
                } else {
                    let _ = ShowWindow(self.hwnd, SHOW_WINDOW_CMD(SW_MAXIMIZE.0));
                    true
                }
            }
        }

        pub fn close(&self) {
            unsafe {
                let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }

        pub fn start_dragging(&self) {
            unsafe {
                let _ = ReleaseCapture();
                let _ = SendMessageW(
                    self.hwnd,
                    WM_NCLBUTTONDOWN,
                    Some(WPARAM(HTCAPTION as usize)),
                    Some(LPARAM(0)),
                );
            }
        }

        pub fn is_maximized(&self) -> bool {
            unsafe { IsZoomed(self.hwnd).as_bool() }
        }

        pub fn is_fullscreen(&self) -> bool {
            self.fullscreen.is_some()
        }

        pub fn set_fullscreen(&mut self, fullscreen: bool) -> Result<bool, String> {
            if fullscreen == self.is_fullscreen() {
                return Ok(false);
            }

            if fullscreen {
                self.enter_fullscreen()?;
            } else {
                self.exit_fullscreen()?;
            }
            Ok(true)
        }

        fn enter_fullscreen(&mut self) -> Result<(), String> {
            let mut placement = WINDOWPLACEMENT {
                length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                ..Default::default()
            };
            unsafe {
                GetWindowPlacement(self.hwnd, &mut placement)
                    .map_err(|error| format!("Failed to read window placement: {error}"))?;
            }

            let style = unsafe { GetWindowLongPtrW(self.hwnd, GWL_STYLE) };
            let ex_style = unsafe { GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE) };
            let monitor = unsafe { MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST) };
            let mut monitor_info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            unsafe {
                if !GetMonitorInfoW(monitor, &mut monitor_info).as_bool() {
                    return Err("Failed to read fullscreen monitor bounds".to_string());
                }
            }

            self.fullscreen = Some(FullscreenSnapshot {
                style,
                ex_style,
                placement,
            });

            let fullscreen_style = (style as u32 & !WS_OVERLAPPEDWINDOW.0) | WS_POPUP.0;
            let rect = monitor_info.rcMonitor;
            unsafe {
                SetWindowLongPtrW(self.hwnd, GWL_STYLE, fullscreen_style as isize);
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex_style);
                SetWindowPos(
                    self.hwnd,
                    None,
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOOWNERZORDER | SWP_FRAMECHANGED,
                )
                .map_err(|error| format!("Failed to enter fullscreen: {error}"))?;
            }
            Ok(())
        }

        fn exit_fullscreen(&mut self) -> Result<(), String> {
            let Some(snapshot) = self.fullscreen.take() else {
                return Ok(());
            };
            unsafe {
                SetWindowLongPtrW(self.hwnd, GWL_STYLE, snapshot.style);
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, snapshot.ex_style);
                SetWindowPlacement(self.hwnd, &snapshot.placement)
                    .map_err(|error| format!("Failed to restore window placement: {error}"))?;
                SetWindowPos(
                    self.hwnd,
                    None,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOOWNERZORDER | SWP_FRAMECHANGED,
                )
                .map_err(|error| format!("Failed to exit fullscreen: {error}"))?;
            }
            Ok(())
        }
    }

    #[derive(Debug)]
    struct FullscreenSnapshot {
        style: isize,
        ex_style: isize,
        placement: WINDOWPLACEMENT,
    }

    #[derive(Debug)]
    struct PipWindowSnapshot {
        style: isize,
        ex_style: isize,
        placement: WINDOWPLACEMENT,
    }

    impl PipWindowController for NativeWindowController {
        fn enter_pip(&mut self) -> Result<PipRestoreSnapshot, String> {
            let was_fullscreen = self.is_fullscreen();
            if was_fullscreen {
                self.set_fullscreen(false)?;
            }

            let mut placement = WINDOWPLACEMENT {
                length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
                ..Default::default()
            };
            let mut rect = RECT::default();
            unsafe {
                GetWindowPlacement(self.hwnd, &mut placement)
                    .map_err(|error| format!("Failed to read PiP window placement: {error}"))?;
                GetWindowRect(self.hwnd, &mut rect)
                    .map_err(|error| format!("Failed to read PiP window bounds: {error}"))?;
            }
            let captured_width = rect.right - rect.left;
            let captured_height = rect.bottom - rect.top;

            let style = unsafe { GetWindowLongPtrW(self.hwnd, GWL_STYLE) };
            let ex_style = unsafe { GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE) };
            self.pip = Some(PipWindowSnapshot {
                style,
                ex_style,
                placement,
            });

            let pip_style = (style as u32 & !WS_OVERLAPPEDWINDOW.0) | WS_POPUP.0 | WS_VISIBLE.0;
            unsafe {
                SetWindowLongPtrW(self.hwnd, GWL_STYLE, pip_style as isize);
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, ex_style);
                SetWindowPos(
                    self.hwnd,
                    Some(HWND_TOPMOST),
                    rect.left,
                    rect.top,
                    PIP_WINDOW_WIDTH,
                    PIP_WINDOW_HEIGHT,
                    SWP_NOOWNERZORDER | SWP_FRAMECHANGED,
                )
                .map_err(|error| format!("Failed to enter PiP: {error}"))?;
            }

            Ok(PipRestoreSnapshot {
                was_fullscreen,
                saved_size: (!was_fullscreen).then_some((captured_width, captured_height)),
            })
        }

        fn exit_pip(&mut self, snapshot: PipRestoreSnapshot) -> Result<(), String> {
            let Some(pip) = self.pip.take() else {
                if snapshot.was_fullscreen {
                    self.set_fullscreen(true)?;
                }
                return Ok(());
            };

            let topmost = if pip.ex_style as u32 & WS_EX_TOPMOST.0 != 0 {
                HWND_TOPMOST
            } else {
                HWND_NOTOPMOST
            };
            let (width, height, size_flags) = if let Some((width, height)) = snapshot.saved_size {
                (width, height, SWP_NOMOVE)
            } else {
                (0, 0, SWP_NOMOVE | SWP_NOSIZE)
            };
            unsafe {
                SetWindowLongPtrW(self.hwnd, GWL_STYLE, pip.style);
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, pip.ex_style);
                SetWindowPlacement(self.hwnd, &pip.placement)
                    .map_err(|error| format!("Failed to restore PiP placement: {error}"))?;
                SetWindowPos(
                    self.hwnd,
                    Some(topmost),
                    0,
                    0,
                    width,
                    height,
                    size_flags | SWP_NOOWNERZORDER | SWP_FRAMECHANGED,
                )
                .map_err(|error| format!("Failed to exit PiP: {error}"))?;
            }

            if snapshot.was_fullscreen {
                self.set_fullscreen(true)?;
            }
            Ok(())
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
            // shell-ng keeps a dark splash/background behind transparent WebView2 while MPV loads.
            hbrBackground: unsafe { CreateSolidBrush(COLORREF(0x0026111b)) },
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
                        let visual_state = match wparam.0 as u32 {
                            SIZE_MINIMIZED => Some(WindowVisualState::Minimized),
                            SIZE_MAXIMIZED => Some(WindowVisualState::Maximized),
                            SIZE_RESTORED => Some(WindowVisualState::Restored),
                            _ => None,
                        };
                        let mut rect = RECT::default();
                        let _ = GetClientRect(hwnd, &mut rect);
                        if let Some(handler) = (*state).handler.as_mut() {
                            let _ = handler.on_resized(hwnd, rect);
                            if let Some(visual_state) = visual_state {
                                let _ = handler.on_window_state_changed(hwnd, visual_state);
                            }
                        }
                    }
                    LRESULT(0)
                }
                WM_SETFOCUS | WM_KILLFOCUS => {
                    with_handler(hwnd, |handler| {
                        let _ = handler.on_focus_changed(hwnd, message == WM_SETFOCUS);
                    });
                    DefWindowProcW(hwnd, message, wparam, lparam)
                }
                WM_APPCOMMAND => {
                    let command = ((lparam.0 >> 16) & 0x0fff) as u32;
                    let action = match command {
                        11 => Some(MediaKeyAction::NextTrack),
                        12 => Some(MediaKeyAction::PreviousTrack),
                        14 | 46 | 47 => Some(MediaKeyAction::PlayPause),
                        _ => None,
                    };
                    if let Some(action) = action {
                        with_handler(hwnd, |handler| {
                            let _ = handler.on_media_key(hwnd, action);
                        });
                        return LRESULT(1);
                    }
                    DefWindowProcW(hwnd, message, wparam, lparam)
                }
                UI_THREAD_WAKE_MESSAGE => {
                    with_handler(hwnd, |handler| {
                        let _ = handler.on_ui_thread_wake(hwnd);
                    });
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

    unsafe fn with_handler(hwnd: HWND, f: impl FnOnce(&mut dyn NativeWindowHandler)) -> bool {
        let state = window_state(hwnd);
        if !state.is_null() {
            if let Some(handler) = (*state).handler.as_mut() {
                f(handler.as_mut());
                return true;
            }
        }
        false
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
