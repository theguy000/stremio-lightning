//! Putting text on the system clipboard. One function, because the app only ever copies a link.

/// Copies `text`, reporting a message when the system refuses so the caller can show it.
#[cfg(target_os = "windows")]
pub fn copy(text: &str) -> Result<(), String> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::{EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

    /// The clipboard's plain-text format, from `winuser.h`. The `windows` crate types this as a
    /// constant of the Ole subsystem, which this app does not otherwise use, so the number is here.
    const CF_UNICODETEXT: u32 = 13;

    // The clipboard is one system-wide slot: while another process holds it the copy has to wait,
    // and saying so beats reporting a copy that did not happen.
    // SAFETY: the clipboard is opened and closed on this same thread, and no window owns it.
    unsafe { OpenClipboard(None) }.map_err(|error| format!("the clipboard is busy: {error}"))?;
    let _closing = Closing;
    // SAFETY: the clipboard is open on this thread, which is what EmptyClipboard requires.
    unsafe { EmptyClipboard() }.map_err(|error| format!("the clipboard is busy: {error}"))?;

    // A trailing nul makes a valid CF_UNICODETEXT payload.
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();

    // SAFETY: the block is allocated at the size of the buffer written into it just below, and then
    // handed to the clipboard, which owns it from there on.
    unsafe {
        let block = GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2)
            .map_err(|error| format!("could not allocate the clipboard memory: {error}"))?;
        let pointer = GlobalLock(block);
        if pointer.is_null() {
            return Err("could not reach the clipboard memory".to_owned());
        }
        std::ptr::copy_nonoverlapping(wide.as_ptr(), pointer.cast::<u16>(), wide.len());
        GlobalUnlock(block).ok();
        SetClipboardData(CF_UNICODETEXT, Some(HANDLE(block.0)))
            .map_err(|error| format!("could not set the clipboard: {error}"))?;
    }
    Ok(())
}

/// Closes the clipboard however the copy ends, so a failure never leaves it held.
#[cfg(target_os = "windows")]
struct Closing;

#[cfg(target_os = "windows")]
impl Drop for Closing {
    fn drop(&mut self) {
        // SAFETY: `copy` opens the clipboard on this thread before this value exists.
        unsafe { windows::Win32::System::DataExchange::CloseClipboard() }.ok();
    }
}

#[cfg(target_os = "macos")]
pub fn copy(text: &str) -> Result<(), String> {
    use std::io::Write;

    let mut child = std::process::Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not reach the clipboard: {error}"))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "could not reach the clipboard".to_owned())?;
    stdin
        .write_all(text.as_bytes())
        .map_err(|error| format!("could not write to the clipboard: {error}"))?;
    drop(stdin);
    child
        .wait()
        .map_err(|error| format!("could not close the clipboard: {error}"))?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn copy(_text: &str) -> Result<(), String> {
    Err("copying is not implemented on this platform yet".to_owned())
}
