#![cfg_attr(windows, allow(unsafe_code))]

pub(crate) fn window_activation_focused(wparam: usize) -> bool {
    // WM_ACTIVATE stores WA_INACTIVE in the low word and minimization state in the high word.
    wparam & 0xffff != 0
}

#[cfg(windows)]
mod windows_impl {
    use super::super::coalescer::UiThreadNotifier;
    use super::super::dark_mode::set_dark_title_bar;
    use super::super::types::{
        MediaKeyAction, NativeWindowHandler, WindowConfig, WindowError, WindowState,
        WindowVisualState, UI_THREAD_WAKE_MESSAGE,
    };
    use super::window_activation_focused;
    use std::{ffi::c_void, ptr::NonNull};
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{GetStockObject, BLACK_BRUSH, HBRUSH};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::HiDpi::{
        SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    };
    use windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect,
        GetMessageW, GetWindowLongPtrW, IsIconic, LoadCursorW, LoadIconW, PostQuitMessage,
        RegisterClassW, SetForegroundWindow, SetWindowLongPtrW, SetWindowTextW, ShowWindow,
        TranslateMessage,
        CREATESTRUCTW, CW_USEDEFAULT, GWLP_USERDATA, IDC_ARROW, MINMAXINFO, MSG, SHOW_WINDOW_CMD,
        SIZE_MAXIMIZED, SIZE_MINIMIZED, SIZE_RESTORED, SW_MAXIMIZE, SW_RESTORE, WINDOW_EX_STYLE,
        WM_ACTIVATE, WM_APPCOMMAND, WM_CLOSE, WM_DESTROY, WM_GETMINMAXINFO, WM_NCCREATE,
        WM_NCDESTROY, WM_SIZE, WNDCLASSW, WS_CLIPCHILDREN, WS_MAXIMIZE, WS_OVERLAPPEDWINDOW,
        WS_VISIBLE,
    };

    const APP_ICON_RESOURCE_ID: usize = 101;

    struct NoopWindowHandler;

    impl NativeWindowHandler for NoopWindowHandler {
        fn on_created(&mut self, _hwnd: HWND) -> Result<(), String> {
            Ok(())
        }
    }

    pub fn run_native_window(config: WindowConfig) -> Result<(), String> {
        run_native_window_with_handler(config, NoopWindowHandler)
    }

    pub fn run_native_window_with_handler(
        config: WindowConfig,
        handler: impl NativeWindowHandler + 'static,
    ) -> Result<(), String> {
        // SAFETY: Sets DPI awareness context before any native window creation.
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        }

        let hwnd = create_main_window(config, Box::new(handler))?;
        let notifier = UiThreadNotifier::new(hwnd);
        notifier.notify().map_err(|error| error.to_string())?;
        // SAFETY: hwnd is valid window; ShowWindow shows window, set_dark_title_bar styles it.
        unsafe {
            let _ = ShowWindow(hwnd, SHOW_WINDOW_CMD(SW_MAXIMIZE.0));
            set_dark_title_bar(hwnd, true);
        }
        run_message_loop()
    }

    pub fn focus_window(hwnd: HWND) {
        // SAFETY: Checks iconic state, restores if needed, and brings window to foreground.
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SHOW_WINDOW_CMD(SW_RESTORE.0));
            }
            let _ = SetForegroundWindow(hwnd);
        }
    }

    pub fn set_app_user_model_id(app_id: &str) -> Result<(), String> {
        let app_id = HSTRING::from(app_id);
        // SAFETY: app_id is a null-terminated UTF-16 wide string.
        unsafe { SetCurrentProcessExplicitAppUserModelID(PCWSTR(app_id.as_ptr())) }
            .map_err(|error| WindowError::AppUserModelId(error.to_string()).to_string())
    }

    /// Re-applies the shell's own caption after `WebView2` overwrites it with
    /// the page's `document.title`.
    pub fn set_window_title(hwnd: HWND) {
        let title = HSTRING::from(crate::APP_NAME);
        // SAFETY: hwnd is a valid window handle and title is a null-terminated UTF-16 string.
        unsafe {
            let _ = SetWindowTextW(hwnd, PCWSTR(title.as_ptr()));
        }
    }

    fn create_main_window(
        config: WindowConfig,
        handler: Box<dyn NativeWindowHandler>,
    ) -> Result<HWND, String> {
        // SAFETY: Retrieves module instance handle for current executable.
        let instance = unsafe { GetModuleHandleW(None) }
            .map_err(|error| format!("Failed to get module handle: {error}"))?;
        let class_name = w!("StremioLightningWindow");

        // SAFETY: Loads standard system arrow cursor.
        let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }
            .map_err(|error| format!("Failed to load default cursor: {error}"))?;
        // SAFETY: Loads icon from application resources.
        let icon = unsafe {
            LoadIconW(
                Some(instance.into()),
                PCWSTR(APP_ICON_RESOURCE_ID as *const u16),
            )
        }
        .map_err(|error| format!("Failed to load application icon: {error}"))?;

        let window_class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            hIcon: icon,
            hCursor: cursor,
            // SAFETY: System stock black brush handle.
            hbrBackground: unsafe { HBRUSH(GetStockObject(BLACK_BRUSH).0) },
            lpszClassName: class_name,
            ..Default::default()
        };

        // SAFETY: Registers window class for StremioLightningWindow.
        let atom = unsafe { RegisterClassW(&window_class) };
        if atom == 0 {
            return Err("Failed to register Windows window class".to_string());
        }

        let title = HSTRING::from(config.title);
        let state_handle = WindowStateHandle::from_box(Box::new(WindowState {
            config,
            handler: Some(handler),
        }));

        // SAFETY: Creates top-level overlapped window; lpCreateParams passes state handle.
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                PCWSTR(title.as_ptr()),
                WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN | WS_MAXIMIZE | WS_VISIBLE,
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

    fn run_message_loop() -> Result<(), String> {
        let mut message = MSG::default();
        loop {
            // SAFETY: GetMessageW retrieves next message from thread queue into valid pointer.
            let result = unsafe { GetMessageW(&raw mut message, None, 0, 0).0 };
            if result == -1 {
                return Err("Windows message loop failed".to_string());
            }
            if result == 0 {
                return Ok(());
            }
            // SAFETY: Standard Win32 message translation and dispatching on valid message.
            unsafe {
                let _ = TranslateMessage(&raw const message);
                DispatchMessageW(&raw const message);
            }
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
                        // SAFETY: lparam points to valid MINMAXINFO structure.
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
                    // SAFETY: hwnd is valid window handle; rect is mutable buffer.
                    if let Err(error) = unsafe { GetClientRect(hwnd, &raw mut rect) } {
                        stremio_lightning_core::logging::error(
                            "native.window",
                            format!(
                                "[StremioLightning] Failed to read Windows client rect: {error}"
                            ),
                        );
                    } else {
                        notify_handler(hwnd, "resize", |handler| handler.on_resized(hwnd, rect));
                        if let Some(visual_state) = visual_state {
                            set_dark_title_bar(hwnd, true);
                            notify_handler(hwnd, "state", |handler| {
                                handler.on_window_state_changed(hwnd, visual_state)
                            });
                        }
                    }
                }
                LRESULT(0)
            }
            WM_ACTIVATE => {
                let result = default_window_proc(hwnd, message, wparam, lparam);
                notify_handler(hwnd, "focus", |handler| {
                    handler.on_focus_changed(hwnd, window_activation_focused(wparam.0))
                });
                result
            }
            WM_APPCOMMAND => {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
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
                // SAFETY: DestroyWindow destroys window on close request.
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                // SAFETY: PostQuitMessage posts quit code to thread queue.
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
            // SAFETY: Windows passes CREATESTRUCTW in WM_NCCREATE with lpCreateParams.
            let state = unsafe { create.as_ref().lpCreateParams.cast::<WindowState>() };
            Self::from_raw(state)
        }

        fn from_hwnd(hwnd: HWND) -> Option<Self> {
            // SAFETY: Retrieves stored user data pointer from HWND.
            let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState };
            Self::from_raw(ptr)
        }

        fn as_c_void(self) -> *const c_void {
            self.ptr.as_ptr().cast::<c_void>()
        }

        fn store(self, hwnd: HWND) {
            // SAFETY: Stores WindowState pointer in GWLP_USERDATA.
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, self.ptr.as_ptr() as isize);
            }
        }

        fn take(hwnd: HWND) -> Option<Box<WindowState>> {
            let state = Self::from_hwnd(hwnd)?;
            // SAFETY: Resets GWLP_USERDATA and reclaims Box ownership.
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                Some(Box::from_raw(state.ptr.as_ptr()))
            }
        }

        fn into_box(self) -> Box<WindowState> {
            // SAFETY: Reclaims Box allocated via Box::leak.
            unsafe { Box::from_raw(self.ptr.as_ptr()) }
        }

        fn with_ref<R>(self, f: impl FnOnce(&WindowState) -> R) -> R {
            // SAFETY: self.ptr points to valid live WindowState.
            unsafe { f(&*self.ptr.as_ptr()) }
        }

        fn with_mut<R>(self, f: impl FnOnce(&mut WindowState) -> R) -> R {
            // SAFETY: self.ptr points to valid live WindowState.
            unsafe { f(&mut *self.ptr.as_ptr()) }
        }
    }

    fn window_visual_state(wparam: WPARAM) -> Option<WindowVisualState> {
        let state = u32::try_from(wparam.0).ok()?;
        match state {
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
        // SAFETY: DefWindowProcW safely processes default message handling for hwnd.
        unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
    }
}

#[cfg(windows)]
pub use windows_impl::*;

#[cfg(not(windows))]
mod fallback_impl {
    use super::super::types::WindowConfig;

    pub fn run_native_window(_config: WindowConfig) -> Result<(), String> {
        Err("Native Windows window can only run on Windows".to_string())
    }
}

#[cfg(not(windows))]
pub use fallback_impl::*;
