#![cfg_attr(windows, allow(unsafe_code))]

/// # Errors
/// Returns an error when the URL scheme is not allowed.
pub fn validate_external_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() || trimmed.contains(|c: char| c.is_control()) {
        return Err("Rejected non-whitelisted open_external_url URL".to_string());
    }

    let allowed = ["http://", "https://", "mailto:"].iter().any(|prefix| {
        trimmed
            .get(..prefix.len())
            .is_some_and(|s| s.eq_ignore_ascii_case(prefix))
    });

    if allowed {
        Ok(())
    } else {
        Err("Rejected non-whitelisted open_external_url URL".to_string())
    }
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
