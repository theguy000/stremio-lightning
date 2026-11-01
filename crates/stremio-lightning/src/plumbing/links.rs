//! Opening a web address in whatever the system has registered for it. This is the only place
//! that decides which addresses the app will open.

/// The address behind a name the UI uses, or the address itself, or nothing.
pub fn resolve(kind: &str) -> Option<String> {
    match kind {
        "support" => Some("https://stremio.zendesk.com/hc/en-us".to_string()),
        "source" => Some("https://github.com/theguy000/stremio-lightning".to_string()),
        "tos" => Some("https://www.stremio.com/tos".to_string()),
        "privacy" => Some("https://www.stremio.com/privacy".to_string()),
        "reset-password" => Some("https://www.strem.io/reset-password/".to_string()),
        s if s.starts_with("http://") || s.starts_with("https://") => Some(s.to_string()),
        _ => None,
    }
}

/// What an address is handed to instead of the system, when a test is watching.
type Opener = Box<dyn Fn(&str) + Send + Sync>;

/// Where an address goes, when a test is watching. The app never sets this: a test does, so that
/// pressing Configure can be checked without a browser opening on the machine running it.
static OPENER: std::sync::Mutex<Option<Opener>> = std::sync::Mutex::new(None);

/// Hands every address the app opens to `opener` instead of the system, until it is cleared.
pub fn watch(opener: Option<Opener>) {
    *OPENER.lock().unwrap_or_else(|error| error.into_inner()) = opener;
}

pub(crate) fn open(kind: &str) {
    let Some(url) = resolve(kind) else {
        return;
    };
    if let Some(opener) = OPENER
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .as_ref()
    {
        opener(&url);
        return;
    }
    #[cfg(target_os = "windows")]
    open_with_default_browser(&url);
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(&url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
}

/// Hands the URL to whatever Windows has registered for it, with no `cmd.exe` in between: the
/// caller becomes the shell itself, so the browser is started by the process that actually asked
/// for it instead of by a console host, and the result is a status code we can report. It runs off
/// the calling thread because `ShellExecuteW` blocks until the registered handler is launched,
/// which would otherwise stall the UI while the browser starts.
#[cfg(target_os = "windows")]
fn open_with_default_browser(url: &str) {
    let url = url.to_string();
    std::thread::spawn(move || {
        use windows::core::{HSTRING, PCWSTR};
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let operation = HSTRING::from("open");
        let target = HSTRING::from(url.as_str());
        // SAFETY: both strings outlive the call and are valid, null-terminated wide strings.
        let result = unsafe {
            ShellExecuteW(
                None,
                PCWSTR(operation.as_ptr()),
                PCWSTR(target.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                SW_SHOWNORMAL,
            )
        };
        // ShellExecuteW reports failure as a value <= 32, not through GetLastError.
        if result.0 as usize <= 32 {
            eprintln!(
                "could not open {url} in the default browser: ShellExecuteW returned {}",
                result.0 as usize
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_targets_resolve_and_unknown_ones_do_not() {
        assert_eq!(
            resolve("reset-password"),
            Some("https://www.strem.io/reset-password/".to_string())
        );
        assert_eq!(
            resolve("https://www.strem.io/reset-password/?email=user%40test.com"),
            Some("https://www.strem.io/reset-password/?email=user%40test.com".to_string())
        );
        assert_eq!(
            resolve("tos"),
            Some("https://www.stremio.com/tos".to_string())
        );
        assert_eq!(resolve("invalid-target"), None);
    }
}
