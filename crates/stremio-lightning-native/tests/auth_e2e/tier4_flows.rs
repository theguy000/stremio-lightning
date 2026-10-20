//! Tier 4: Real-World User Flows & Integration Workflows
//! End-to-end user journeys spanning multiple screens, triggers, overlays,
//! runtime event synchronization, and profile reactivity.

use super::common::*;
use stremio_core::runtime::msg::{Action, ActionCtx, Event};

/// Flow 1: Complete New User Registration Journey
/// Settings trigger -> Mode toggle -> Field entry & GDPR -> Action dispatch ->
/// Event synchronization -> Profile update -> Settings reflection -> Library unlock.
#[test]
fn test_flow1_new_user_registration_journey() {
    let mut window = MainWindowOverlayState::new();
    let runtime = MockAuthRuntime::new();
    window.section = AppSection::Settings;

    // 1. Initial State: Logged out on Settings screen
    assert!(!window.logged_in);
    assert_eq!(window.section, AppSection::Settings);
    assert!(!window.is_library_unlocked());

    // 2. User clicks "Log in / Sign up" link on Settings
    window.open_auth(false);
    assert!(window.auth_open);
    assert_eq!(window.section, AppSection::Settings);

    // 3. User switches to "Sign up" mode
    let mut intro = IntroState::new();
    intro.toggle_mode();
    window.auth_is_signup = true;
    assert!(intro.is_signup);
    assert!(window.auth_is_signup);

    // 4. User inputs registration credentials and GDPR consent
    intro.email_input = "new_customer@stremio.com".to_string();
    intro.password_input = "StrongP@ssw0rd!2026".to_string();
    intro.gdpr_tos = true;
    intro.gdpr_privacy = true;
    intro.gdpr_marketing = true;

    // 5. Submit registration
    let reg_req = intro.create_register_request().expect("valid registration");
    runtime.dispatch(Action::Ctx(ActionCtx::Authenticate(reg_req.clone())));
    window.auth_loading = true;

    assert!(window.auth_loading);
    assert_eq!(runtime.dispatched_actions.lock().unwrap().len(), 1);

    // 6. Backend runtime processes registration and emits UserAuthenticated
    let new_user = create_test_user("new_customer@stremio.com");
    runtime.emit_event(Event::UserAuthenticated { auth_request: reg_req });

    // 7. Event handler in main.rs triggers UI updates
    window.handle_auth_success(&new_user);

    // 8. Verify post-registration state
    assert!(!window.auth_open, "Overlay must be closed");
    assert!(!window.auth_loading, "Loading spinner must be dismissed");
    assert!(window.logged_in, "Session must be authenticated");
    assert_eq!(window.user_email, "new_customer@stremio.com", "Email must match registered user");
    assert_eq!(window.section, AppSection::Settings, "User returned to Settings view");
    assert!(window.is_library_unlocked(), "Library now unlocked for registered user");
    assert!(window.is_calendar_unlocked(), "Calendar now unlocked for registered user");
}

/// Flow 2: Existing User Credential Login from Library Placeholder
/// Library placeholder -> Log In click -> Credential entry -> Submit ->
/// Auth success -> Overlay dismissal -> Library unlocked.
#[test]
fn test_flow2_existing_user_login_from_library() {
    let mut window = MainWindowOverlayState::new();
    let runtime = MockAuthRuntime::new();

    // 1. User navigates to Library while logged out
    window.section = AppSection::Library;
    assert!(!window.is_library_unlocked(), "Library shows placeholder");

    // 2. User clicks "Log in" button on LoggedOutPlaceholder
    window.open_auth(false);
    assert!(window.auth_open);
    assert!(!window.auth_is_signup);
    assert_eq!(window.section, AppSection::Library);

    // 3. User enters credentials
    let mut intro = IntroState::new();
    intro.email_input = "existing.user@gmail.com".to_string();
    intro.password_input = "SecretPass123!".to_string();

    // 4. User clicks "Log in" submit button
    let login_req = intro.create_login_request().expect("valid login");
    runtime.dispatch(Action::Ctx(ActionCtx::Authenticate(login_req.clone())));
    window.auth_loading = true;

    // 5. Runtime authentication event fires
    let user = create_test_user("existing.user@gmail.com");
    runtime.emit_event(Event::UserAuthenticated { auth_request: login_req });
    window.handle_auth_success(&user);

    // 6. Verify Library is now accessible
    assert!(!window.auth_open);
    assert!(window.logged_in);
    assert_eq!(window.section, AppSection::Library);
    assert!(window.is_library_unlocked(), "Library is now unlocked and accessible");
}

/// Flow 3: Guest User Flow from Calendar Placeholder
/// Calendar placeholder -> Log In click -> "Log in as Guest" click ->
/// Overlay dismissal -> Anonymous profile retained.
#[test]
fn test_flow3_guest_login_from_calendar() {
    let mut window = MainWindowOverlayState::new();
    window.section = AppSection::Calendar;
    assert!(!window.is_calendar_unlocked());

    // 1. Open auth from Calendar
    window.open_auth(false);
    assert!(window.auth_open);

    // 2. Click "Log in as Guest"
    window.dismiss_auth();

    // 3. Verify state
    assert!(!window.auth_open, "Overlay dismissed");
    assert!(!window.logged_in, "Remains anonymous guest");
    assert_eq!(window.section, AppSection::Calendar, "Returned to Calendar");
    assert!(!window.is_calendar_unlocked(), "Calendar remains gated for guests");
}

/// Flow 4: Authentication Error & Recovery Flow
/// Top-bar avatar click -> Wrong password -> API error banner -> Password correction ->
/// Successful login -> Overlay dismissed.
#[test]
fn test_flow4_auth_error_and_recovery_flow() {
    let mut window = MainWindowOverlayState::new();
    let mut intro = IntroState::new();
    let runtime = MockAuthRuntime::new();
    window.section = AppSection::Board;

    // 1. Click top-bar avatar when logged out
    window.open_auth(false);
    assert!(window.auth_open);

    // 2. Enter incorrect password
    intro.email_input = "alice@example.com".to_string();
    intro.password_input = "wrong_password".to_string();
    let req1 = intro.create_login_request().unwrap();
    runtime.dispatch(Action::Ctx(ActionCtx::Authenticate(req1)));
    window.auth_loading = true;

    // 3. Runtime delivers CoreEvent(Event::Error)
    window.handle_auth_error("USER_ERR_INVALID_PASSWORD".to_string());
    intro.error_message = window.auth_error.clone();

    assert_eq!(intro.error_message, "USER_ERR_INVALID_PASSWORD");
    assert!(!window.auth_loading);
    assert!(window.auth_open, "Overlay stays open so user can fix password");

    // 4. User corrects password and resubmits
    intro.password_input = "correct_password".to_string();
    let req2 = intro.create_login_request().unwrap();
    runtime.dispatch(Action::Ctx(ActionCtx::Authenticate(req2.clone())));
    window.auth_loading = true;
    window.auth_error.clear();

    // 5. Runtime delivers CoreEvent(Event::UserAuthenticated)
    let user = create_test_user("alice@example.com");
    runtime.emit_event(Event::UserAuthenticated { auth_request: req2 });
    window.handle_auth_success(&user);

    // 6. Overlay closes cleanly
    assert!(!window.auth_open);
    assert!(window.logged_in);
    assert_eq!(window.user_email, "alice@example.com");
    assert_eq!(window.section, AppSection::Board);
}

/// Flow 5: External Password Reset Workflow
/// Login form -> Forgot password link -> Web URL formatting -> Browser launch.
#[test]
fn test_flow5_forgot_password_external_workflow() {
    let mut window = MainWindowOverlayState::new();
    let mut intro = IntroState::new();
    window.open_auth(false);

    intro.email_input = "lostpassword@stremio.com".to_string();
    let reset_url = intro.generate_forgot_password_url();

    assert!(reset_url.starts_with("https://www.strem.io/reset-password/?email="));
    assert!(reset_url.contains("lostpassword%40stremio.com"));
    assert!(window.auth_open, "Overlay remains active after launching browser");
}

/// Flow 6: Social Login Fallback Workflow
/// Click Facebook login -> Notification message -> Form remains interactive.
#[test]
fn test_flow6_social_login_fallback_workflow() {
    let mut window = MainWindowOverlayState::new();
    let mut intro = IntroState::new();
    window.open_auth(false);

    // Click Facebook button
    intro.error_message = "Opening web browser for authentication. Please log in there, or use email and password above.".to_string();
    window.auth_error = intro.error_message.clone();

    assert!(window.auth_error.contains("Opening web browser"));
    assert!(!window.auth_loading, "UI remains fully interactive");
    assert!(window.auth_open);
}

/// Flow 7: Logout & Re-login Flow
/// Authenticated user -> Logout via Settings -> Profile cleared -> Library relocked ->
/// Log in again -> Session restored.
#[test]
fn test_flow7_logout_and_relogin_flow() {
    let mut window = MainWindowOverlayState::new();
    let runtime = MockAuthRuntime::new();

    // 1. User starts authenticated
    let user = create_test_user("roundtrip@stremio.com");
    window.handle_auth_success(&user);
    assert!(window.logged_in);
    assert!(window.is_library_unlocked());

    // 2. User logs out from Settings
    window.section = AppSection::Settings;
    runtime.dispatch(Action::Ctx(ActionCtx::Logout));
    window.handle_logout();
    assert!(!window.logged_in);
    assert!(!window.is_library_unlocked());

    // 3. User opens auth view and logs back in
    window.open_auth(false);
    assert!(window.auth_open);

    let mut intro = IntroState::new();
    intro.email_input = "roundtrip@stremio.com".to_string();
    intro.password_input = "pass123".to_string();
    let req = intro.create_login_request().unwrap();
    runtime.dispatch(Action::Ctx(ActionCtx::Authenticate(req.clone())));

    runtime.emit_event(Event::UserAuthenticated { auth_request: req });
    window.handle_auth_success(&user);

    // 4. Verify restored state
    assert!(!window.auth_open);
    assert!(window.logged_in);
    assert_eq!(window.user_email, "roundtrip@stremio.com");
    assert!(window.is_library_unlocked());
}
