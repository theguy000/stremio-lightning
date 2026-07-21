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

fn window_activation_focused(wparam: usize) -> bool {
    // WM_ACTIVATE stores WA_INACTIVE in the low word and minimization state in the high word.
    wparam & 0xffff != 0
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
    use std::{ffi::c_void, ptr::NonNull};
    use stremio_lightning_core::pip::{PipRestoreSnapshot, PipWindowController};
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
        LoadCursorW, LoadIconW, PostMessageW, PostQuitMessage, RegisterClassW, SendMessageW,
        SetForegroundWindow, SetWindowLongPtrW, SetWindowPlacement, SetWindowPos, ShowWindow,
        TranslateMessage, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, GWLP_USERDATA,
        GWL_EXSTYLE, GWL_STYLE, HTCAPTION, HWND_NOTOPMOST, HWND_TOPMOST, IDC_ARROW, MINMAXINFO,
        MSG, SHOW_WINDOW_CMD, SIZE_MAXIMIZED, SIZE_MINIMIZED, SIZE_RESTORED, SWP_FRAMECHANGED,
        SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE,
        SW_SHOWDEFAULT, WINDOWPLACEMENT, WINDOW_EX_STYLE, WM_ACTIVATE, WM_APP, WM_APPCOMMAND,
        WM_CLOSE, WM_DESTROY, WM_DPICHANGED, WM_GETMINMAXINFO, WM_NCCREATE, WM_NCDESTROY,
        WM_NCLBUTTONDOWN, WM_SIZE, WNDCLASSW, WS_CLIPCHILDREN, WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW,
        WS_POPUP, WS_VISIBLE,
    };

    pub const UI_THREAD_WAKE_MESSAGE: u32 = WM_APP + 1;
    const APP_ICON_RESOURCE_ID: usize = 101;

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

    // SAFETY: Worker threads only use this HWND with `PostMessageW`.
    unsafe impl Send for UiThreadNotifier {}

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

    // SAFETY: The host mutex serializes access to the HWND-backed controller state.
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
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOOWNERZORDER | SWP_FRAMECHANGED,
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
        fn enter_pip(&mut self, width: i32, height: i32) -> Result<PipRestoreSnapshot, String> {
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
                    width,
                    height,
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
        let icon = unsafe {
            LoadIconW(
                Some(instance.into()),
                PCWSTR(APP_ICON_RESOURCE_ID as *const u16),
            )
        }
        .map_err(|error| format!("Failed to load application icon: {error}"))?;

        let window_class = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            hIcon: icon,
            hCursor: cursor,
            // Keep a black background behind transparent WebView2 while MPV loads.
            hbrBackground: unsafe { CreateSolidBrush(COLORREF(0x00000000)) },
            lpszClassName: class_name,
            ..Default::default()
        };

        let atom = unsafe { RegisterClassW(&window_class) };
        if atom == 0 {
            return Err("Failed to register Windows window class".to_string());
        }

        let title = to_wide_null(config.title);
        let state_handle = WindowStateHandle::from_box(Box::new(WindowState {
            config,
            handler: Some(handler),
        }));

        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                PCWSTR(title.as_ptr()),
                WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                state_handle.with_ref(|state| state.config.width),
                state_handle.with_ref(|state| state.config.height),
                None,
                None,
                Some(instance.into()),
                Some(state_handle.as_c_void()),
            )
        };

        match hwnd {
            Ok(hwnd) => {
                with_handler(hwnd, |handler| handler.on_created(hwnd)).transpose()?;
                Ok(hwnd)
            }
            Err(error) => {
                drop(state_handle.into_box());
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
        match message {
            WM_NCCREATE => {
                if let Some(state) = WindowStateHandle::from_create_params(lparam) {
                    state.store(hwnd);
                    LRESULT(1)
                } else {
                    LRESULT(0)
                }
            }
            WM_GETMINMAXINFO => {
                if let (Some(state), Some(mut minmax)) = (
                    WindowStateHandle::from_hwnd(hwnd),
                    NonNull::new(lparam.0 as *mut MINMAXINFO),
                ) {
                    state.with_ref(|state| {
                        let minmax = unsafe { minmax.as_mut() };
                        minmax.ptMinTrackSize.x = state.config.min_width;
                        minmax.ptMinTrackSize.y = state.config.min_height;
                    });
                }
                LRESULT(0)
            }
            WM_SIZE => {
                if WindowStateHandle::from_hwnd(hwnd).is_some() {
                    let visual_state = window_visual_state(wparam);
                    let mut rect = RECT::default();
                    if let Err(error) = unsafe { GetClientRect(hwnd, &mut rect) } {
                        stremio_lightning_core::logging::error(
                            "native.window",
                            format!(
                                "[StremioLightning] Failed to read Windows client rect: {error}"
                            ),
                        );
                    } else {
                        notify_handler(hwnd, "resize", |handler| handler.on_resized(hwnd, rect));
                        if let Some(visual_state) = visual_state {
                            notify_handler(hwnd, "state", |handler| {
                                handler.on_window_state_changed(hwnd, visual_state)
                            });
                        }
                    }
                }
                LRESULT(0)
            }
            WM_ACTIVATE => {
                notify_handler(hwnd, "focus", |handler| {
                    handler.on_focus_changed(hwnd, super::window_activation_focused(wparam.0))
                });
                default_window_proc(hwnd, message, wparam, lparam)
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
                    notify_handler(hwnd, "media-key", |handler| {
                        handler.on_media_key(hwnd, action)
                    });
                    return LRESULT(1);
                }
                default_window_proc(hwnd, message, wparam, lparam)
            }
            UI_THREAD_WAKE_MESSAGE => {
                notify_handler(hwnd, "ui-thread-wake", |handler| {
                    handler.on_ui_thread_wake(hwnd)
                });
                LRESULT(0)
            }
            WM_CLOSE => {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                unsafe {
                    PostQuitMessage(0);
                }
                LRESULT(0)
            }
            WM_NCDESTROY => {
                if let Some(mut state) = WindowStateHandle::take(hwnd) {
                    if let Some(handler) = state.handler.as_mut() {
                        handler.on_destroying(hwnd);
                    }
                }
                default_window_proc(hwnd, message, wparam, lparam)
            }
            WM_DPICHANGED => default_window_proc(hwnd, message, wparam, lparam),
            _ => default_window_proc(hwnd, message, wparam, lparam),
        }
    }

    #[derive(Clone, Copy)]
    struct WindowStateHandle {
        ptr: NonNull<WindowState>,
    }

    impl WindowStateHandle {
        fn from_box(state: Box<WindowState>) -> Self {
            Self {
                ptr: NonNull::from(Box::leak(state)),
            }
        }

        fn from_raw(ptr: *mut WindowState) -> Option<Self> {
            NonNull::new(ptr).map(|ptr| Self { ptr })
        }

        fn from_create_params(lparam: LPARAM) -> Option<Self> {
            let create = NonNull::new(lparam.0 as *mut CREATESTRUCTW)?;
            // SAFETY: Windows sends a valid CREATESTRUCTW pointer with WM_NCCREATE.
            // `lpCreateParams` is the boxed WindowState pointer supplied to CreateWindowExW.
            let state = unsafe { create.as_ref().lpCreateParams.cast::<WindowState>() };
            Self::from_raw(state)
        }

        fn from_hwnd(hwnd: HWND) -> Option<Self> {
            let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
            Self::from_raw(ptr)
        }

        fn as_c_void(self) -> *const c_void {
            self.ptr.as_ptr().cast::<c_void>()
        }

        fn store(self, hwnd: HWND) {
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, self.ptr.as_ptr() as isize);
            }
        }

        fn take(hwnd: HWND) -> Option<Box<WindowState>> {
            let state = Self::from_hwnd(hwnd)?;
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                Some(Box::from_raw(state.ptr.as_ptr()))
            }
        }

        fn into_box(self) -> Box<WindowState> {
            unsafe { Box::from_raw(self.ptr.as_ptr()) }
        }

        fn with_ref<R>(self, f: impl FnOnce(&WindowState) -> R) -> R {
            unsafe { f(&*self.ptr.as_ptr()) }
        }

        fn with_mut<R>(self, f: impl FnOnce(&mut WindowState) -> R) -> R {
            unsafe { f(&mut *self.ptr.as_ptr()) }
        }
    }

    fn window_visual_state(wparam: WPARAM) -> Option<WindowVisualState> {
        match wparam.0 as u32 {
            SIZE_MINIMIZED => Some(WindowVisualState::Minimized),
            SIZE_MAXIMIZED => Some(WindowVisualState::Maximized),
            SIZE_RESTORED => Some(WindowVisualState::Restored),
            _ => None,
        }
    }

    fn with_handler<R>(hwnd: HWND, f: impl FnOnce(&mut dyn NativeWindowHandler) -> R) -> Option<R> {
        WindowStateHandle::from_hwnd(hwnd)?.with_mut(|state| {
            let handler = state.handler.as_mut()?;
            Some(f(handler.as_mut()))
        })
    }

    fn notify_handler(
        hwnd: HWND,
        event: &'static str,
        f: impl FnOnce(&mut dyn NativeWindowHandler) -> Result<(), String>,
    ) {
        let Some(result) = with_handler(hwnd, f) else {
            return;
        };

        if let Err(error) = result {
            stremio_lightning_core::logging::error(
                "native.window",
                format!("[StremioLightning] Windows window {event} handler failed: {error}"),
            );
        }
    }

    fn default_window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
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
        assert_eq!((config.width, config.height), (1500, 850));
        assert_eq!((config.min_width, config.min_height), (800, 600));
    }

    #[test]
    fn ui_thread_wake_message_uses_app_message_range() {
        const { assert!(UI_THREAD_WAKE_MESSAGE >= 0x8000) };
    }

    #[test]
    fn window_activation_uses_only_the_low_word() {
        assert!(!window_activation_focused(0));
        assert!(window_activation_focused(1));
        assert!(window_activation_focused(2));
        assert!(!window_activation_focused(1 << 16));
    }
}
