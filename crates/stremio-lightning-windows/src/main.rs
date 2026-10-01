#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
use windows::core::{HSTRING, PCWSTR};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    if let Err(error) = stremio_lightning_windows::run() {
        show_fatal_error(&error.to_string());
        stremio_lightning_core::logging::error("native.application", error.to_string());
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn show_fatal_error(message: &str) {
    let title = HSTRING::from("Stremio Lightning");
    let body = HSTRING::from(format!("Failed to start Stremio Lightning:\n\n{message}"));
    // SAFETY: title and body are null-terminated wide strings that remain allocated for the call.
    #[allow(unsafe_code)]
    unsafe {
        MessageBoxW(
            None,
            PCWSTR(body.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_ICONERROR | MB_OK,
        );
    }
}

#[cfg(not(windows))]
fn show_fatal_error(_message: &str) {}
