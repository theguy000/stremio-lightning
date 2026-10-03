#![cfg_attr(windows, allow(unsafe_code))]

/// # Errors
/// Returns an error when the URL scheme is not allowed.
pub fn validate_external_url(url: &str) -> Result<(), String> {
    stremio_lightning_core::navigation::validate_external_url(url)
}

/// # Errors
/// Returns an error when the URL scheme is not allowed or the system cannot open it.
#[cfg(windows)]
pub fn open_external_url(url: &str) -> Result<(), String> {
    use windows::core::{w, HSTRING, PCWSTR};
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let url = HSTRING::from(url.trim());
    // SAFETY: ShellExecuteW is called with the valid, null-terminated UTF-16 wide string
    // held by the HSTRING, a valid "open" operation verb, and SW_SHOWNORMAL flag.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(url.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };

    if result.0 as isize > 32 {
        Ok(())
    } else {
        Err("Failed to open external URL".to_string())
    }
}

#[cfg(not(windows))]
pub fn open_external_url(_url: &str) -> Result<(), String> {
    Ok(())
}
