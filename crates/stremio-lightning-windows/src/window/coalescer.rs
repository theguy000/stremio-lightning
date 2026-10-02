#![cfg_attr(windows, allow(unsafe_code))]

use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use std::sync::Arc;
#[cfg(windows)]
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

#[cfg(windows)]
use super::types::{WindowError, UI_THREAD_WAKE_MESSAGE};

/// Coalesces redundant UI-thread wake-ups: while one wake-up is waiting to be
/// handled, later requests are dropped and covered by the queued wake-up.
#[derive(Debug)]
pub(crate) struct WakeCoalescer {
    pending: AtomicBool,
}

impl WakeCoalescer {
    pub(crate) const fn new() -> Self {
        Self {
            pending: AtomicBool::new(false),
        }
    }

    /// Returns `true` when the caller must post a wake-up, or `false` when one is
    /// already pending and this request can be dropped.
    pub(crate) fn try_begin(&self) -> bool {
        !self.pending.swap(true, Ordering::AcqRel)
    }

    /// Clears the pending marker. Callers must clear it *before* draining their
    /// updates so a request arriving during processing posts a fresh wake-up.
    pub(crate) fn clear(&self) {
        self.pending.store(false, Ordering::Release);
    }
}

#[cfg(windows)]
#[derive(Debug, Clone)]
pub struct UiThreadNotifier {
    pub(crate) hwnd: HWND,
    wake: Arc<WakeCoalescer>,
}

// SAFETY: Worker threads only use this HWND with `PostMessageW`, and the
// pending-wake marker is an atomic shared across the notifier's clones.
#[cfg(windows)]
unsafe impl Send for UiThreadNotifier {}

#[cfg(windows)]
impl UiThreadNotifier {
    #[must_use]
    pub fn new(hwnd: HWND) -> Self {
        Self {
            hwnd,
            wake: Arc::new(WakeCoalescer::new()),
        }
    }

    /// Queues a wake-up unless one is already pending. Every update is drained
    /// when the window wakes, so dropping redundant wake-ups loses nothing.
    /// # Errors
    /// Returns an error when the resize notification cannot be scheduled.
    pub fn notify(&self) -> Result<(), WindowError> {
        if !self.wake.try_begin() {
            return Ok(());
        }

        // SAFETY: PostMessageW is thread-safe and safely posts UI_THREAD_WAKE_MESSAGE to hwnd.
        let result = unsafe {
            PostMessageW(
                Some(self.hwnd),
                UI_THREAD_WAKE_MESSAGE,
                WPARAM(0),
                LPARAM(0),
            )
        };

        if let Err(error) = result {
            self.wake.clear();
            return Err(WindowError::UiNotify(error.to_string()));
        }
        Ok(())
    }

    /// Clears the pending marker so the next update posts a fresh wake-up.
    pub(crate) fn clear_pending(&self) {
        self.wake.clear();
    }
}
