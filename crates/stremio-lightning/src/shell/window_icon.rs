//! The window's own icon: the taskbar button, the titlebar and Alt-Tab.
//!
//! Slint's `Window` element carries an icon, but its language has no word to set one and its
//! public API exposes no setter for it. On Windows a window without one shows nothing at all --
//! an executable's icon is only the fallback -- so it is set here, on the winit window Slint's
//! backend is built on. The same winit the wheel hook in `shell::wheel` speaks to.
//!
//! The picture is the project's own logo, `assets/icons/128x128.png`, embedded so the shell needs
//! no image file beside it. It is the same file the Linux shell hands its window
//! (`stremio-lightning-linux/src/native_window/x11.rs`) and the one the top bar draws, so the
//! three cannot disagree.
//!
//! One thing to know before touching this: Slint's winit backend keeps a *single* window event
//! hook (`on_winit_window_event` replaces whatever was there). Registering one here would silently
//! take the wheel away from the Discover grid, which is why this polls on a timer instead.

use std::cell::Cell;
use std::rc::Rc;

use i_slint_backend_winit::{winit, WinitWindowAccessor};
use slint::{ComponentHandle, Timer, TimerMode};
use winit::window::{Icon, Window};

use crate::MainWindow;

/// The logo, as the project ships it.
const LOGO: &[u8] = include_bytes!("../../../../assets/icons/128x128.png");

/// How often the boot waits for the window to exist, and how many times.
const POLL: std::time::Duration = std::time::Duration::from_millis(16);
const TRIES: u32 = 300;

/// Hands the window its icon once the native window exists.
///
/// It does not exist while the app is being booted -- it is created when the event loop starts --
/// so this waits for it on a timer, which the event loop keeps alive from the moment the window
/// is shown. A window backed by anything else (the testing backend the UI tests use) never gets
/// one, and the timer stops rather than asking 300 times.
pub(crate) fn install(window: &MainWindow) {
    let tries = Rc::new(Cell::new(0));
    let timer = Rc::new(Timer::default());
    let weak = window.as_weak();
    let ticking = timer.clone();
    timer.start(TimerMode::Repeated, POLL, move || {
        tries.set(tries.get() + 1);
        let Some(window) = weak.upgrade() else {
            ticking.stop();
            return;
        };
        let set = window
            .window()
            .with_winit_window(|winit_window: &Window| match icon() {
                Ok(icon) => winit_window.set_window_icon(Some(icon)),
                Err(error) => eprintln!("[window] {error}"),
            });
        if set.is_some() || tries.get() >= TRIES {
            ticking.stop();
        }
    });
}

/// The logo as winit's icon, or why it cannot be made into one.
fn icon() -> Result<Icon, String> {
    decode().and_then(|(pixels, width, height)| {
        Icon::from_rgba(pixels, width, height)
            .map_err(|error| format!("the window icon is not a valid rgba bitmap: {error}"))
    })
}

/// The logo decoded to the straight (non-premultiplied) RGBA winit wants, with its size.
fn decode() -> Result<(Vec<u8>, u32, u32), String> {
    let image = image::load_from_memory(LOGO)
        .map_err(|error| format!("the window icon could not be decoded: {error}"))?
        .to_rgba8();
    Ok((image.as_raw().clone(), image.width(), image.height()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_logo_decodes_and_is_drawn() {
        // A blank or transparent bitmap is what a broken embed looks like, and it would pass
        // every other check: the window would simply come back with an invisible icon. A diamond
        // covers half its box, so a third is a floor that still catches a missing or off-centre
        // mark while staying clear of the shape itself.
        let (pixels, width, height) = decode().expect("the embedded logo is a valid PNG");
        assert_eq!((width, height), (128, 128));
        let drawn = pixels
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 0)
            .count();
        assert!(
            drawn > (128 * 128 / 3) as usize,
            "only {drawn} of 16384 pixels are drawn: assets/icons/128x128.png is not the logo"
        );
    }

    #[test]
    fn winit_accepts_the_logo() {
        // This is the conversion the running app does; an icon winit rejects would leave the
        // window blank again with no other sign.
        let icon = icon();
        assert!(icon.is_ok(), "winit rejected the logo: {:?}", icon.err());
    }
}
