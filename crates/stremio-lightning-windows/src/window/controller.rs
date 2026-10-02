#![cfg_attr(windows, allow(unsafe_code))]

#[cfg(windows)]
mod windows_impl {
    // Win32 ABI conversions: struct size fields and style words are small bitmasks and
    // constants on every supported target, so these casts cannot lose meaningful data.
    #![allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_possible_wrap
    )]
    use super::super::types::{FullscreenSnapshot, PipWindowSnapshot};
    use super::super::wndproc::focus_window;
    use stremio_lightning_core::pip::{PipRestoreSnapshot, PipWindowController};
    use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, GetWindowPlacement, GetWindowRect, IsZoomed, PostMessageW, SendMessageW,
        SetWindowLongPtrW, SetWindowPlacement, SetWindowPos, ShowWindow, GWL_EXSTYLE, GWL_STYLE,
        HTCAPTION, HWND_NOTOPMOST, HWND_TOPMOST, SHOW_WINDOW_CMD, SWP_FRAMECHANGED, SWP_NOMOVE,
        SWP_NOOWNERZORDER, SWP_NOSIZE, SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, WINDOWPLACEMENT,
        WM_CLOSE, WM_NCLBUTTONDOWN, WS_EX_TOPMOST, WS_OVERLAPPEDWINDOW, WS_POPUP, WS_VISIBLE,
    };

    #[derive(Debug)]
    pub struct NativeWindowController {
        hwnd: HWND,
        fullscreen: Option<FullscreenSnapshot>,
        pip: Option<PipWindowSnapshot>,
    }

    // SAFETY: The host mutex serializes access to the HWND-backed controller state.
    unsafe impl Send for NativeWindowController {}

    impl NativeWindowController {
        #[must_use]
        pub fn new(hwnd: HWND) -> Self {
            Self {
                hwnd,
                fullscreen: None,
                pip: None,
            }
        }

        pub fn minimize(&self) {
            // SAFETY: ShowWindow minimizes the window referenced by the valid HWND.
            unsafe {
                let _ = ShowWindow(self.hwnd, SHOW_WINDOW_CMD(SW_MINIMIZE.0));
            }
        }

        pub fn focus(&self) {
            focus_window(self.hwnd);
        }

        #[must_use]
        pub fn toggle_maximize(&self) -> bool {
            // SAFETY: IsZoomed checks maximized state; ShowWindow updates window show state.
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
            // SAFETY: PostMessageW safely posts WM_CLOSE to the valid HWND.
            unsafe {
                let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }

        pub fn start_dragging(&self) {
            // SAFETY: ReleaseCapture releases mouse capture; SendMessageW sends caption drag.
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

        #[must_use]
        pub fn is_maximized(&self) -> bool {
            // SAFETY: IsZoomed safely queries window zoom/maximized state from HWND.
            unsafe { IsZoomed(self.hwnd).as_bool() }
        }

        #[must_use]
        pub fn is_fullscreen(&self) -> bool {
            self.fullscreen.is_some()
        }

        /// # Errors
        /// Returns an error when the window cannot enter or leave fullscreen.
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
            // SAFETY: GetWindowPlacement reads current window placement into mutable buffer.
            unsafe {
                GetWindowPlacement(self.hwnd, &raw mut placement)
                    .map_err(|error| format!("Failed to read window placement: {error}"))?;
            }

            // SAFETY: GetWindowLongPtrW queries styles; MonitorFromWindow reads monitor;
            // GetMonitorInfoW reads monitor bounds into mutable MONITORINFO buffer.
            let (style, ex_style, rect) = unsafe {
                let style = GetWindowLongPtrW(self.hwnd, GWL_STYLE);
                let ex_style = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
                let monitor = MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST);
                let mut monitor_info = MONITORINFO {
                    cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                    ..Default::default()
                };
                if !GetMonitorInfoW(monitor, &raw mut monitor_info).as_bool() {
                    return Err("Failed to read fullscreen monitor bounds".to_string());
                }
                (style, ex_style, monitor_info.rcMonitor)
            };

            self.fullscreen = Some(FullscreenSnapshot {
                style,
                ex_style,
                placement,
            });

            let fullscreen_style = (style as u32 & !WS_OVERLAPPEDWINDOW.0) | WS_POPUP.0;
            // SAFETY: SetWindowLongPtrW and SetWindowPos configure window style and dimensions.
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
            // SAFETY: SetWindowLongPtrW restores styles; SetWindowPlacement restores position;
            // SetWindowPos recalculates the non-client frame.
            unsafe {
                SetWindowLongPtrW(self.hwnd, GWL_STYLE, snapshot.style);
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, snapshot.ex_style);
                SetWindowPlacement(self.hwnd, &raw const snapshot.placement)
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
            // SAFETY: Placement and rect buffers are passed for Win32 query functions.
            unsafe {
                GetWindowPlacement(self.hwnd, &raw mut placement)
                    .map_err(|error| format!("Failed to read PiP window placement: {error}"))?;
                GetWindowRect(self.hwnd, &raw mut rect)
                    .map_err(|error| format!("Failed to read PiP window bounds: {error}"))?;
            }
            let captured_width = rect.right - rect.left;
            let captured_height = rect.bottom - rect.top;

            // SAFETY: GetWindowLongPtrW queries styles from valid HWND.
            let (style, ex_style) = unsafe {
                (
                    GetWindowLongPtrW(self.hwnd, GWL_STYLE),
                    GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE),
                )
            };
            self.pip = Some(PipWindowSnapshot {
                style,
                ex_style,
                placement,
            });

            let pip_style = (style as u32 & !WS_OVERLAPPEDWINDOW.0) | WS_POPUP.0 | WS_VISIBLE.0;
            // SAFETY: SetWindowLongPtrW and SetWindowPos make window topmost PiP overlay.
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
            // SAFETY: SetWindowLongPtrW restores styles; SetWindowPlacement restores position;
            // SetWindowPos removes topmost state and updates frame.
            unsafe {
                SetWindowLongPtrW(self.hwnd, GWL_STYLE, pip.style);
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, pip.ex_style);
                SetWindowPlacement(self.hwnd, &raw const pip.placement)
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
}

#[cfg(windows)]
pub use windows_impl::NativeWindowController;
