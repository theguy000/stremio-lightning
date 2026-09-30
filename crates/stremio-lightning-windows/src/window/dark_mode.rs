#![cfg_attr(windows, allow(unsafe_code))]

#[cfg(windows)]
mod windows_impl {
    use std::ffi::c_void;
    use windows::core::BOOL;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWINDOWATTRIBUTE,
    };

    // Before Windows 10 20H1 (build 18985) the immersive dark mode attribute had id 19;
    // 20H1 and Windows 11 use the documented DWMWA_USE_IMMERSIVE_DARK_MODE (20).
    const DWMWA_USE_IMMERSIVE_DARK_MODE_LEGACY: DWMWINDOWATTRIBUTE = DWMWINDOWATTRIBUTE(19);

    pub(crate) fn set_dark_title_bar(hwnd: HWND, dark: bool) {
        let value = BOOL::from(dark);
        let size = u32::try_from(std::mem::size_of::<BOOL>()).unwrap_or(4);
        let pointer = (&raw const value).cast::<c_void>();

        // SAFETY: DwmSetWindowAttribute safely sets the title bar dark mode attribute on hwnd.
        let result =
            unsafe { DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, pointer, size) };
        if result.is_ok() {
            return;
        }

        // SAFETY: Legacy fallback for pre-20H1 Windows 10 builds using attribute id 19.
        let legacy = unsafe {
            DwmSetWindowAttribute(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE_LEGACY, pointer, size)
        };
        if let Err(error) = legacy {
            stremio_lightning_core::logging::debug(
                "native.window",
                format!("[StremioLightning] Failed to apply dark title bar: {error}"),
            );
        }
    }
}

#[cfg(windows)]
pub(crate) use windows_impl::set_dark_title_bar;
