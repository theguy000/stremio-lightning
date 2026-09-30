use super::coalescer::WakeCoalescer;
use super::types::{WindowConfig, UI_THREAD_WAKE_MESSAGE};
use super::wndproc::window_activation_focused;

#[test]
fn default_window_config_matches_milestone_baseline() {
    let config = WindowConfig::default();

    assert_eq!(config.title, crate::APP_NAME);
    assert_eq!((config.width, config.height), (1500, 850));
    assert_eq!((config.min_width, config.min_height), (800, 600));
}

#[test]
fn ui_thread_wake_message_uses_app_message_range() {
    const { assert!(UI_THREAD_WAKE_MESSAGE >= 0x8000) };
}

#[test]
fn wake_coalescer_drops_requests_while_one_is_pending() {
    let wake = WakeCoalescer::new();

    assert!(wake.try_begin(), "first request posts a wake-up");
    assert!(!wake.try_begin(), "second request is coalesced");
    assert!(!wake.try_begin(), "further requests stay coalesced");

    wake.clear();
    assert!(wake.try_begin(), "request after clearing posts again");
}

#[test]
fn wake_coalescer_clear_before_draining_keeps_late_requests() {
    let wake = WakeCoalescer::new();

    assert!(wake.try_begin());
    // The consumer clears the marker before processing its updates.
    wake.clear();
    assert!(
        wake.try_begin(),
        "an update arriving during processing queues another wake-up"
    );
    assert!(!wake.try_begin());
}

#[test]
fn window_activation_uses_only_the_low_word() {
    assert!(!window_activation_focused(0));
    assert!(window_activation_focused(1));
    assert!(window_activation_focused(2));
    assert!(!window_activation_focused(1 << 16));
}
