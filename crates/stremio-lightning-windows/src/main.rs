#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(windows)]
use windows::core::PCWSTR;
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};

fn main() {
    if let Err(error) = stremio_lightning_windows::run() {
        show_fatal_error(&error);
        stremio_lightning_core::logging::error("native.application", error);
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn show_fatal_error(message: &str) {
    let title = wide_string("Stremio Lightning");
    let body = wide_string(&format!("Failed to start Stremio Lightning:\n\n{message}"));
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

#[cfg(windows)]
fn wide_string(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
