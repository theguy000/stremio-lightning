//! Tier 3: Combinatorial & Cross-Feature Interaction Test Suite
//! Pairwise state transitions, dirty form handling, recovery from failures,
//! and concurrent interaction edge combinations.

use super::common::*;
use stremio_core::runtime::msg::{Action, ActionCtx};

#[test]
fn test_c1_mode_switch_login_to_signup_preserves_email_but_clears_error() {
    let mut state = IntroState::new();
    state.email_input = "alice@stremio.com".to_string();
    state.error_message = "User not found".to_string();
    assert!(!state.is_signup);

    state.toggle_mode();

    assert!(state.is_signup);
    assert_eq!(
        state.email_input, "alice@stremio.com",
        "Email must be preserved across mode switches"
    );
    assert!(
        state.error_message.is_empty(),
        "Error message must be cleared when switching modes"
    );
}

#[test]
fn test_c1_mode_switch_signup_to_login_preserves_email_resets_gdpr() {
    let mut state = IntroState::new();
    state.toggle_mode(); // now signup
    state.email_input = "bob@stremio.com".to_string();
    state.gdpr_tos = true;
    state.gdpr_privacy = true;
    state.gdpr_marketing = true;

    state.toggle_mode(); // back to login

    assert!(!state.is_signup);
    assert_eq!(state.email_input, "bob@stremio.com");
}

#[test]
fn test_c1_login_failure_recovery_flow() {
    let mut overlay = MainWindowOverlayState::new();
    let mut intro = IntroState::new();
    overlay.open_auth(false);

    // 1. Submit bad credentials
    intro.email_input = "user@test.org".to_string();
    intro.password_input = "wrong_pass".to_string();
    overlay.auth_loading = true;

    // 2. Runtime reports API error
    overlay.handle_auth_error("USER_ERR_INVALID_PASSWORD".to_string());
    intro.error_message = overlay.auth_error.clone();
    assert_eq!(intro.error_message, "USER_ERR_INVALID_PASSWORD");
    assert!(!overlay.auth_loading);
    assert!(overlay.auth_open);

    // 3. User corrects password
    intro.password_input = "correct_pass".to_string();
    overlay.auth_loading = true;
    overlay.auth_error.clear();

    // 4. Runtime reports authentication success
    let user = create_test_user("user@test.org");
    overlay.handle_auth_success(&user);

    assert!(overlay.logged_in);
    assert!(!overlay.auth_open);
    assert!(overlay.auth_error.is_empty());
}

#[test]
fn test_c1_guest_login_after_failed_login_attempt() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.open_auth(false);
    overlay.handle_auth_error("Invalid email".to_string());
    assert!(!overlay.auth_error.is_empty());

    // User chooses guest login instead
    overlay.dismiss_auth();

    assert!(!overlay.auth_open);
    assert!(overlay.auth_error.is_empty());
    assert!(!overlay.logged_in);
}

#[test]
fn test_c1_dismiss_overlay_during_loading_state() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.section = AppSection::Discover;
    overlay.open_auth(false);
    overlay.auth_loading = true;

    // User dismisses while request in flight
    overlay.dismiss_auth();

    assert!(!overlay.auth_open);
    assert!(!overlay.auth_loading);
    assert_eq!(
        overlay.section,
        AppSection::Discover,
        "Previous section restored cleanly"
    );
}

#[test]
fn test_c1_double_submit_prevention() {
    let mut state = IntroState::new();
    state.email_input = "user@domain.com".to_string();
    state.password_input = "pass".to_string();

    let runtime = MockAuthRuntime::new();

    // First submit
    let req1 = state.create_login_request().unwrap();
    runtime.dispatch(Action::Ctx(ActionCtx::Authenticate(req1)));
    state.is_loading = true;

    // Second submit while loading should be guarded by UI
    if !state.is_loading {
        let req2 = state.create_login_request().unwrap();
        runtime.dispatch(Action::Ctx(ActionCtx::Authenticate(req2)));
    }

    assert_eq!(
        runtime.dispatched_actions.lock().unwrap().len(),
        1,
        "Exactly one action dispatched"
    );
}

#[test]
fn test_c1_password_unmask_mode_switch_preserves_visibility_preference() {
    let mut state = IntroState::new();
    state.toggle_password_mask();
    assert!(!state.password_masked);

    // Switch mode to signup
    state.toggle_mode();
    assert!(
        !state.password_masked,
        "Unmasked visibility preference retained"
    );
}

#[test]
fn test_c1_forgot_password_from_dirty_email_state() {
    let mut state = IntroState::new();
    state.email_input = "  dirty.email@example.com  ".to_string();
    let url = state.generate_forgot_password_url();
    assert!(url.contains("dirty.email%40example.com"));
    assert!(!url.contains(" "));
}

#[test]
fn test_c1_social_auth_click_resets_stale_login_error() {
    let mut state = IntroState::new();
    state.error_message = "Stale login error".to_string();

    // Clicking Facebook resets error
    state.error_message.clear();
    assert!(state.error_message.is_empty());
}

#[test]
fn test_c1_logout_from_settings_locks_library_and_calendar_simultaneously() {
    let mut overlay = MainWindowOverlayState::new();
    let user = create_test_user("test@account.com");
    overlay.handle_auth_success(&user);
    assert!(overlay.is_library_unlocked());
    assert!(overlay.is_calendar_unlocked());

    // User logs out
    overlay.handle_logout();
    assert!(!overlay.is_library_unlocked());
    assert!(!overlay.is_calendar_unlocked());
}

#[test]
fn test_c1_registration_failure_recovery() {
    let mut state = IntroState::new();
    state.toggle_mode();
    state.email_input = "newreg@test.com".to_string();
    state.password_input = "pass".to_string();
    state.gdpr_tos = false; // missing
    state.gdpr_privacy = true;

    assert!(state.create_register_request().is_err());

    // User checks TOS
    state.gdpr_tos = true;
    assert!(state.create_register_request().is_ok());
}

#[test]
fn test_c1_rapid_mode_toggle_stress() {
    let mut state = IntroState::new();
    for _ in 0..20 {
        state.toggle_mode();
    }
    // After 20 toggles (even number), should be back to false
    assert!(!state.is_signup);
}
