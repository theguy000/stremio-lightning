#![cfg_attr(windows, allow(unsafe_code))]

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

#[cfg(windows)]
pub fn open_external_url(url: &str) -> Result<(), String> {
    use webview2_com::CoTaskMemPWSTR;
    use windows::core::w;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let url = CoTaskMemPWSTR::from(url.trim());
    // SAFETY: ShellExecuteW is called with the valid, null-terminated UTF-16 wide string
    // allocated by CoTaskMemPWSTR, a valid "open" operation verb, and SW_SHOWNORMAL flag.
    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            *url.as_ref().as_pcwstr(),
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
