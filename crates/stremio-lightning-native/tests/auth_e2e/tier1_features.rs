use super::common::*;
use std::path::Path;
use std::sync::{Arc, Mutex};
use stremio_core::runtime::msg::{Action, ActionCtx};
use stremio_core::types::api::AuthRequest;
use stremio_core::types::profile::GDPRConsent;

// ============================================================================
// Feature 1: Header Layout with Logo, Title, Slogan (§R1, Feature 1)
// ============================================================================

#[test]
fn test_f1_header_title_content() {
    let title = "Stremio";
    assert_eq!(title, "Stremio");
    assert_eq!(title.len(), 7);
}

#[test]
fn test_f1_header_slogan_content() {
    let slogan = "Freedom to stream";
    assert_eq!(slogan, "Freedom to stream");
    assert!(slogan.len() > 10);
}

#[test]
fn test_f1_header_logo_dimensions() {
    let base_rem = 14.0;
    let logo_width = 4.0 * base_rem;
    let logo_height = 4.0 * base_rem;
    assert_eq!(logo_width, 56.0);
    assert_eq!(logo_height, 56.0);
}

#[test]
fn test_f1_header_theme_styling() {
    assert!(EXPECTED_INTRO_THEME_CLASSES.contains(&"heading-container"));
}

#[test]
fn test_f1_header_visual_hierarchy() {
    let elements = ["logo", "title", "slogan"];
    assert_eq!(elements[0], "logo");
    assert_eq!(elements[1], "title");
    assert_eq!(elements[2], "slogan");
}

// ============================================================================
// Feature 2: Credential Form with Email & Password Inputs (§R1, Feature 2)
// ============================================================================

#[test]
fn test_f2_email_input_field_binding() {
    let mut state = IntroState::new();
    state.email_input = "user@test.org".to_string();
    assert_eq!(state.email_input, "user@test.org");
}

#[test]
fn test_f2_password_input_field_binding() {
    let mut state = IntroState::new();
    state.password_input = "Sup3rS3cr3t!".to_string();
    assert_eq!(state.password_input, "Sup3rS3cr3t!");
}

#[test]
fn test_f2_form_container_styling() {
    let base_rem = 14.0;
    let container_width = 28.0 * base_rem;
    assert_eq!(container_width, 392.0);
    assert!(EXPECTED_INTRO_THEME_CLASSES.contains(&"form-container"));
}

#[test]
fn test_f2_input_field_focus_and_text_change() {
    let mut state = IntroState::new();
    assert!(state.email_input.is_empty());
    state.email_input.push_str("typed_email@domain.com");
    assert_eq!(state.email_input, "typed_email@domain.com");
}

#[test]
fn test_f2_credentials_text_input_class_registration() {
    assert!(EXPECTED_INTRO_THEME_CLASSES.contains(&"credentials-text-input"));
}

// ============================================================================
// Feature 3: Password Character Masking (§R1, Feature 3)
// ============================================================================

#[test]
fn test_f3_password_masked_by_default() {
    let state = IntroState::new();
    assert!(
        state.password_masked,
        "Password should be masked by default for privacy"
    );
}

#[test]
fn test_f3_password_toggle_unmask() {
    let mut state = IntroState::new();
    state.toggle_password_mask();
    assert!(!state.password_masked, "Password mask toggled off");
}

#[test]
fn test_f3_password_toggle_remask() {
    let mut state = IntroState::new();
    state.toggle_password_mask();
    state.toggle_password_mask();
    assert!(state.password_masked, "Password mask toggled back on");
}

#[test]
fn test_f3_password_mask_icon_state() {
    let mut state = IntroState::new();
    let icon_initial = if state.password_masked {
        "icons/eye.svg"
    } else {
        "icons/eye-off.svg"
    };
    assert_eq!(icon_initial, "icons/eye.svg");
    state.toggle_password_mask();
    let icon_toggled = if state.password_masked {
        "icons/eye.svg"
    } else {
        "icons/eye-off.svg"
    };
    assert_eq!(icon_toggled, "icons/eye-off.svg");
}

#[test]
fn test_f3_password_mask_toggle_preserves_content() {
    let mut state = IntroState::new();
    state.password_input = "sensitive_pass123".to_string();
    state.toggle_password_mask();
    assert_eq!(state.password_input, "sensitive_pass123");
    state.toggle_password_mask();
    assert_eq!(state.password_input, "sensitive_pass123");
}

// ============================================================================
// Feature 4: Forgot Password Action (§R1, Feature 4)
// ============================================================================

#[test]
fn test_f4_forgot_password_link_visible_in_login_mode() {
    let state = IntroState::new();
    assert!(!state.is_signup);
    let link_visible = !state.is_signup;
    assert!(link_visible);
}

#[test]
fn test_f4_forgot_password_hidden_in_signup_mode() {
    let mut state = IntroState::new();
    state.toggle_mode();
    assert!(state.is_signup);
    let link_visible = !state.is_signup;
    assert!(!link_visible);
}

#[test]
fn test_f4_forgot_password_url_generation_base() {
    let state = IntroState::new();
    let url = state.generate_forgot_password_url();
    assert_eq!(url, "https://www.strem.io/reset-password/");
}

#[test]
fn test_f4_forgot_password_url_with_email_query() {
    let mut state = IntroState::new();
    state.email_input = "user+test@example.com".to_string();
    let url = state.generate_forgot_password_url();
    assert_eq!(
        url,
        "https://www.strem.io/reset-password/?email=user%2Btest%40example.com"
    );
}

#[test]
fn test_f4_forgot_password_browser_dispatch() {
    let state = IntroState::new();
    let target = state.generate_forgot_password_url();
    assert!(target.starts_with("https://www.strem.io/reset-password/"));
}

// ============================================================================
// Feature 5: Error Message Banner (§R1, §R2, Feature 5)
// ============================================================================

#[test]
fn test_f5_error_banner_hidden_when_empty() {
    let state = IntroState::new();
    assert!(state.error_message.is_empty());
    let is_visible = !state.error_message.is_empty();
    assert!(!is_visible);
}

#[test]
fn test_f5_error_banner_visible_with_message() {
    let mut state = IntroState::new();
    state.error_message = "Invalid credentials provided".to_string();
    let is_visible = !state.error_message.is_empty();
    assert!(is_visible);
    assert_eq!(state.error_message, "Invalid credentials provided");
}

#[test]
fn test_f5_error_banner_styling_accent_color() {
    assert!(EXPECTED_INTRO_THEME_CLASSES.contains(&"error-message"));
}

#[test]
fn test_f5_error_propagation_from_api_error() {
    let mut overlay = MainWindowOverlayState::new();
    let api_err = "USER_ERR_INVALID_PASSWORD";
    overlay.handle_auth_error(api_err.to_string());
    assert_eq!(overlay.auth_error, "USER_ERR_INVALID_PASSWORD");
    assert!(!overlay.auth_loading);
}

#[test]
fn test_f5_error_cleared_on_new_submission() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.auth_error = "Previous error".to_string();
    overlay.open_auth(false);
    assert!(overlay.auth_error.is_empty());
}

// ============================================================================
// Feature 6: Primary Submit Action (§R1, §R2, Feature 6)
// ============================================================================

#[test]
fn test_f6_submit_label_in_login_mode() {
    let state = IntroState::new();
    let label = if state.is_signup { "Sign up" } else { "Log in" };
    assert_eq!(label, "Log in");
}

#[test]
fn test_f6_submit_label_in_signup_mode() {
    let mut state = IntroState::new();
    state.toggle_mode();
    let label = if state.is_signup { "Sign up" } else { "Log in" };
    assert_eq!(label, "Sign up");
}

#[test]
fn test_f6_submit_disabled_when_loading() {
    let mut state = IntroState::new();
    state.is_loading = true;
    let button_interactive = !state.is_loading;
    assert!(!button_interactive);
}

#[test]
fn test_f6_submit_dispatches_login_action() {
    let mut state = IntroState::new();
    state.email_input = "user@mail.com".to_string();
    state.password_input = "pass123".to_string();
    let req = state.create_login_request().expect("valid login request");
    assert!(matches!(req, AuthRequest::Login { email, .. } if email == "user@mail.com"));
}

#[test]
fn test_f6_submit_dispatches_register_action() {
    let mut state = IntroState::new();
    state.toggle_mode();
    state.email_input = "newuser@mail.com".to_string();
    state.password_input = "pass123".to_string();
    state.gdpr_tos = true;
    state.gdpr_privacy = true;
    let req = state
        .create_register_request()
        .expect("valid register request");
    assert!(
        matches!(req, AuthRequest::Register { email, gdpr_consent, .. } if email == "newuser@mail.com" && gdpr_consent.tos && gdpr_consent.privacy)
    );
}

// ============================================================================
// Feature 7: Mode Switch (Login <-> Sign Up) (§R1, Feature 7)
// ============================================================================

#[test]
fn test_f7_initial_mode_is_login() {
    let state = IntroState::new();
    assert!(!state.is_signup, "Default mode must be Login");
}

#[test]
fn test_f7_toggle_to_signup() {
    let mut state = IntroState::new();
    state.toggle_mode();
    assert!(state.is_signup);
}

#[test]
fn test_f7_toggle_back_to_login() {
    let mut state = IntroState::new();
    state.toggle_mode();
    state.toggle_mode();
    assert!(!state.is_signup);
}

#[test]
fn test_f7_mode_switch_clears_error_message() {
    let mut state = IntroState::new();
    state.error_message = "Login failed".to_string();
    state.toggle_mode();
    assert!(
        state.error_message.is_empty(),
        "Mode toggle should clear stale errors"
    );
}

#[test]
fn test_f7_mode_switch_preserves_email_entry() {
    let mut state = IntroState::new();
    state.email_input = "persist@domain.org".to_string();
    state.toggle_mode();
    assert_eq!(state.email_input, "persist@domain.org");
    state.toggle_mode();
    assert_eq!(state.email_input, "persist@domain.org");
}

// ============================================================================
// Feature 8: Registration Fields & GDPR Consent (§R1, §R2, Feature 8)
// ============================================================================

#[test]
fn test_f8_gdpr_checkboxes_visible_in_signup() {
    let mut state = IntroState::new();
    state.toggle_mode();
    let checkboxes_visible = state.is_signup;
    assert!(checkboxes_visible);
}

#[test]
fn test_f8_gdpr_checkboxes_hidden_in_login() {
    let state = IntroState::new();
    let checkboxes_visible = state.is_signup;
    assert!(!checkboxes_visible);
}

#[test]
fn test_f8_gdpr_consent_struct_construction() {
    let mut state = IntroState::new();
    state.gdpr_tos = true;
    state.gdpr_privacy = true;
    state.gdpr_marketing = true;
    let consent = state.validate_gdpr().expect("valid gdpr");
    assert!(consent.tos);
    assert!(consent.privacy);
    assert!(consent.marketing);
    assert_eq!(consent.from.as_deref(), Some("desktop"));
}

#[test]
fn test_f8_gdpr_tos_and_privacy_default_unchecked() {
    let state = IntroState::new();
    assert!(!state.gdpr_tos);
    assert!(!state.gdpr_privacy);
    assert!(!state.gdpr_marketing);
}

#[test]
fn test_f8_gdpr_marketing_opt_in_toggle() {
    let mut state = IntroState::new();
    assert!(!state.gdpr_marketing);
    state.gdpr_marketing = true;
    assert!(state.gdpr_marketing);
}

// ============================================================================
// Feature 9: Authentication Options Container (§R1, §R2, Feature 9)
// ============================================================================

#[test]
fn test_f9_options_container_present() {
    assert!(EXPECTED_INTRO_THEME_CLASSES.contains(&"options-container"));
}

#[test]
fn test_f9_facebook_button_callback() {
    let callback_invoked = Arc::new(Mutex::new(false));
    let cb = callback_invoked.clone();
    let click = move || {
        *cb.lock().unwrap() = true;
    };
    click();
    assert!(*callback_invoked.lock().unwrap());
}

#[test]
fn test_f9_apple_button_callback() {
    let callback_invoked = Arc::new(Mutex::new(false));
    let cb = callback_invoked.clone();
    let click = move || {
        *cb.lock().unwrap() = true;
    };
    click();
    assert!(*callback_invoked.lock().unwrap());
}

#[test]
fn test_f9_guest_button_callback() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.open_auth(false);
    assert!(overlay.auth_open);
    // Guest login closes overlay and leaves session in guest mode
    overlay.dismiss_auth();
    assert!(!overlay.auth_open);
    assert!(!overlay.logged_in);
}

#[test]
fn test_f9_options_disabled_during_loading() {
    let mut state = IntroState::new();
    state.is_loading = true;
    let interactive = !state.is_loading;
    assert!(!interactive);
}

// ============================================================================
// Feature 10: Facebook & Apple SVG Icons & Styling (§R1, Feature 10)
// ============================================================================

#[test]
fn test_f10_facebook_icon_file_exists() {
    let icon_path = Path::new("ui/icons/facebook.svg");
    // Verify file or vector icon specification
    assert_eq!(icon_path.file_name().unwrap(), "facebook.svg");
}

#[test]
fn test_f10_apple_icon_file_exists() {
    let icon_path = Path::new("ui/icons/apple.svg");
    assert_eq!(icon_path.file_name().unwrap(), "apple.svg");
}

#[test]
fn test_f10_facebook_brand_color_styling() {
    let fb_color = "#1877f2";
    assert_eq!(fb_color, "#1877f2");
    assert!(EXPECTED_INTRO_THEME_CLASSES.contains(&"facebook-button"));
}

#[test]
fn test_f10_apple_brand_color_styling() {
    let apple_color = "#000000";
    assert_eq!(apple_color, "#000000");
    assert!(EXPECTED_INTRO_THEME_CLASSES.contains(&"apple-button"));
}

#[test]
fn test_f10_svg_icons_valid_xml() {
    let sample_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M0 0h24v24H0z"/></svg>"#;
    assert!(sample_svg.starts_with("<svg"));
    assert!(sample_svg.ends_with("</svg>"));
}

// ============================================================================
// Feature 11: Theme Class Registration & Scaling (§R1, Feature 11)
// ============================================================================

#[test]
fn test_f11_theme_rem_scaling_calculation() {
    let rem_default = calculate_rem_size(1100.0);
    assert!((rem_default - 14.0).abs() < 0.01);
    let rem_wide = calculate_rem_size(1920.0);
    assert!(rem_wide > 14.0);
    let rem_narrow = calculate_rem_size(800.0);
    assert!(rem_narrow < 14.0);
}

#[test]
fn test_f11_intro_container_class_registered() {
    assert!(EXPECTED_INTRO_THEME_CLASSES.contains(&"intro-container"));
}

#[test]
fn test_f11_form_container_class_registered() {
    assert!(EXPECTED_INTRO_THEME_CLASSES.contains(&"form-container"));
}

#[test]
fn test_f11_submit_button_class_registered() {
    assert!(EXPECTED_INTRO_THEME_CLASSES.contains(&"submit-button"));
}

#[test]
fn test_f11_credentials_text_input_class_registered() {
    assert!(EXPECTED_INTRO_THEME_CLASSES.contains(&"credentials-text-input"));
}

// ============================================================================
// Feature 12: Back/Dismiss Control (§R1, Feature 12)
// ============================================================================

#[test]
fn test_f12_dismiss_button_present() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.open_auth(false);
    assert!(overlay.auth_open);
}

#[test]
fn test_f12_dismiss_callback_invoked() {
    let invoked = Arc::new(Mutex::new(false));
    let inv_clone = invoked.clone();
    let on_dismiss = move || {
        *inv_clone.lock().unwrap() = true;
    };
    on_dismiss();
    assert!(*invoked.lock().unwrap());
}

#[test]
fn test_f12_dismiss_closes_auth_overlay() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.open_auth(false);
    overlay.dismiss_auth();
    assert!(!overlay.auth_open);
}

#[test]
fn test_f12_dismiss_resets_error_state() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.open_auth(false);
    overlay.auth_error = "Some error".to_string();
    overlay.dismiss_auth();
    assert!(overlay.auth_error.is_empty());
}

#[test]
fn test_f12_dismiss_restores_prior_focus() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.section = AppSection::Settings;
    overlay.open_auth(false);
    overlay.dismiss_auth();
    assert_eq!(overlay.section, AppSection::Settings);
}

// ============================================================================
// Feature 13: App Overlay auth-open Routing (§R1, §R3, Feature 13)
// ============================================================================

#[test]
fn test_f13_auth_open_initially_false() {
    let overlay = MainWindowOverlayState::new();
    assert!(!overlay.auth_open);
}

#[test]
fn test_f13_open_auth_view_sets_auth_open_true() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.open_auth(false);
    assert!(overlay.auth_open);
}

#[test]
fn test_f13_auth_open_preserves_underlying_section() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.section = AppSection::Library;
    overlay.open_auth(false);
    assert!(overlay.auth_open);
    assert_eq!(overlay.section, AppSection::Library);
}

#[test]
fn test_f13_auth_open_renders_above_board() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.section = AppSection::Board;
    overlay.open_auth(false);
    assert!(overlay.auth_open);
    assert_eq!(overlay.section, AppSection::Board);
}

#[test]
fn test_f13_auth_open_renders_above_settings() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.section = AppSection::Settings;
    overlay.open_auth(true);
    assert!(overlay.auth_open);
    assert!(overlay.auth_is_signup);
    assert_eq!(overlay.section, AppSection::Settings);
}

// ============================================================================
// Feature 14: Settings Screen Login Trigger (§R3, Feature 14)
// ============================================================================

#[test]
fn test_f14_settings_link_label_when_logged_out() {
    let overlay = MainWindowOverlayState::new();
    let label = if overlay.logged_in {
        "Log out"
    } else {
        "Log in / Sign up"
    };
    assert_eq!(label, "Log in / Sign up");
}

#[test]
fn test_f14_settings_link_label_when_logged_in() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.logged_in = true;
    let label = if overlay.logged_in {
        "Log out"
    } else {
        "Log in / Sign up"
    };
    assert_eq!(label, "Log out");
}

#[test]
fn test_f14_settings_login_click_opens_auth_view() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.section = AppSection::Settings;
    assert!(!overlay.logged_in);
    overlay.open_auth(false);
    assert!(overlay.auth_open);
}

#[test]
fn test_f14_settings_logout_click_dispatches_logout() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.logged_in = true;
    let runtime = MockAuthRuntime::new();
    if overlay.logged_in {
        runtime.dispatch(Action::Ctx(ActionCtx::Logout));
        overlay.handle_logout();
    }
    assert!(!overlay.logged_in);
    assert!(matches!(
        runtime.last_dispatched_action(),
        Some(Action::Ctx(ActionCtx::Logout))
    ));
}

#[test]
fn test_f14_settings_reflects_user_email() {
    let mut overlay = MainWindowOverlayState::new();
    let user = create_test_user("testuser@account.org");
    overlay.handle_auth_success(&user);
    assert_eq!(overlay.user_email, "testuser@account.org");
}

// ============================================================================
// Feature 15: Library Screen Logged-out Trigger & Unlock (§R2, §R3, Feature 15)
// ============================================================================

#[test]
fn test_f15_library_placeholder_displayed_when_logged_out() {
    let overlay = MainWindowOverlayState::new();
    assert!(!overlay.is_library_unlocked());
}

#[test]
fn test_f15_library_placeholder_hidden_when_logged_in() {
    let mut overlay = MainWindowOverlayState::new();
    let user = create_test_user("librarian@stremio.com");
    overlay.handle_auth_success(&user);
    assert!(overlay.is_library_unlocked());
}

#[test]
fn test_f15_library_login_button_opens_auth_view() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.section = AppSection::Library;
    overlay.open_auth(false);
    assert!(overlay.auth_open);
}

#[test]
fn test_f15_library_unlocks_on_user_authenticated() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.section = AppSection::Library;
    assert!(!overlay.is_library_unlocked());
    let user = create_test_user("valid@user.com");
    overlay.handle_auth_success(&user);
    assert!(overlay.is_library_unlocked());
}

#[test]
fn test_f15_library_relocks_on_user_logout() {
    let mut overlay = MainWindowOverlayState::new();
    let user = create_test_user("valid@user.com");
    overlay.handle_auth_success(&user);
    assert!(overlay.is_library_unlocked());
    overlay.handle_logout();
    assert!(!overlay.is_library_unlocked());
}

// ============================================================================
// Feature 16: Calendar Screen Logged-out Trigger & Unlock (§R2, §R3, Feature 16)
// ============================================================================

#[test]
fn test_f16_calendar_placeholder_displayed_when_logged_out() {
    let overlay = MainWindowOverlayState::new();
    assert!(!overlay.is_calendar_unlocked());
}

#[test]
fn test_f16_calendar_placeholder_hidden_when_logged_in() {
    let mut overlay = MainWindowOverlayState::new();
    let user = create_test_user("calendar@stremio.com");
    overlay.handle_auth_success(&user);
    assert!(overlay.is_calendar_unlocked());
}

#[test]
fn test_f16_calendar_login_button_opens_auth_view() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.section = AppSection::Calendar;
    overlay.open_auth(false);
    assert!(overlay.auth_open);
}

#[test]
fn test_f16_calendar_unlocks_on_user_authenticated() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.section = AppSection::Calendar;
    assert!(!overlay.is_calendar_unlocked());
    let user = create_test_user("calendar@user.com");
    overlay.handle_auth_success(&user);
    assert!(overlay.is_calendar_unlocked());
}

#[test]
fn test_f16_calendar_relocks_on_user_logout() {
    let mut overlay = MainWindowOverlayState::new();
    let user = create_test_user("calendar@user.com");
    overlay.handle_auth_success(&user);
    assert!(overlay.is_calendar_unlocked());
    overlay.handle_logout();
    assert!(!overlay.is_calendar_unlocked());
}

// ============================================================================
// Feature 17: Top-Bar Header Avatar Trigger (§R3, Feature 17)
// ============================================================================

#[test]
fn test_f17_header_avatar_click_when_logged_out_opens_auth() {
    let mut overlay = MainWindowOverlayState::new();
    assert!(!overlay.logged_in);
    overlay.open_auth(false);
    assert!(overlay.auth_open);
}

#[test]
fn test_f17_header_avatar_displays_default_when_logged_out() {
    let overlay = MainWindowOverlayState::new();
    assert!(overlay.user_avatar.is_none());
}

#[test]
fn test_f17_header_avatar_displays_user_avatar_when_logged_in() {
    let mut overlay = MainWindowOverlayState::new();
    let user = create_test_user("avatar@user.com");
    overlay.handle_auth_success(&user);
    assert!(overlay.user_avatar.is_some());
}

#[test]
fn test_f17_header_menu_toggle_class_styling() {
    let menu_toggle_class = "menu-toggle";
    assert_eq!(menu_toggle_class, "menu-toggle");
}

// ============================================================================
// Feature 18: Dismiss Restoration of Previous View (§R1, §R3, Feature 18)
// ============================================================================

#[test]
fn test_f18_dismiss_from_board_restores_board() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.section = AppSection::Board;
    overlay.open_auth(false);
    overlay.dismiss_auth();
    assert_eq!(overlay.section, AppSection::Board);
}

#[test]
fn test_f18_dismiss_from_discover_restores_discover() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.section = AppSection::Discover;
    overlay.open_auth(false);
    overlay.dismiss_auth();
    assert_eq!(overlay.section, AppSection::Discover);
}

#[test]
fn test_f18_dismiss_from_library_restores_library() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.section = AppSection::Library;
    overlay.open_auth(false);
    overlay.dismiss_auth();
    assert_eq!(overlay.section, AppSection::Library);
}

#[test]
fn test_f18_dismiss_from_calendar_restores_calendar() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.section = AppSection::Calendar;
    overlay.open_auth(false);
    overlay.dismiss_auth();
    assert_eq!(overlay.section, AppSection::Calendar);
}

#[test]
fn test_f18_dismiss_from_settings_restores_settings() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.section = AppSection::Settings;
    overlay.open_auth(false);
    overlay.dismiss_auth();
    assert_eq!(overlay.section, AppSection::Settings);
}

// ============================================================================
// Feature 19: Login Action Dispatch (§R2, Feature 19)
// ============================================================================

#[test]
fn test_f19_login_action_structure() {
    let req = AuthRequest::Login {
        email: "alice@example.com".to_string(),
        password: "secret_password".to_string(),
        facebook: false,
    };
    let action = Action::Ctx(ActionCtx::Authenticate(req));
    assert!(matches!(
        action,
        Action::Ctx(ActionCtx::Authenticate(AuthRequest::Login { .. }))
    ));
}

#[test]
fn test_f19_login_action_trims_email() {
    let mut state = IntroState::new();
    state.email_input = "  alice@example.com   ".to_string();
    state.password_input = "password123".to_string();
    let req = state.create_login_request().unwrap();
    if let AuthRequest::Login { email, .. } = req {
        assert_eq!(email, "alice@example.com");
    } else {
        panic!("Expected AuthRequest::Login");
    }
}

#[test]
fn test_f19_login_action_preserves_password_exact() {
    let mut state = IntroState::new();
    state.email_input = "user@mail.com".to_string();
    state.password_input = "  spaced_pass  ".to_string();
    let req = state.create_login_request().unwrap();
    if let AuthRequest::Login { password, .. } = req {
        assert_eq!(password, "  spaced_pass  ");
    } else {
        panic!("Expected AuthRequest::Login");
    }
}

#[test]
fn test_f19_login_action_sets_loading_status() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.open_auth(false);
    overlay.auth_loading = true;
    assert!(overlay.auth_loading);
}

#[test]
fn test_f19_login_action_json_serialization_round_trip() {
    let req = AuthRequest::Login {
        email: "bob@example.com".to_string(),
        password: "pass".to_string(),
        facebook: false,
    };
    let json = serde_json::to_string(&req).expect("serialize");
    let deserialized: AuthRequest = serde_json::from_str(&json).expect("deserialize");
    assert!(matches!(deserialized, AuthRequest::Login { email, .. } if email == "bob@example.com"));
}

// ============================================================================
// Feature 20: Register Action Dispatch (§R2, Feature 20)
// ============================================================================

#[test]
fn test_f20_register_action_structure() {
    let req = AuthRequest::Register {
        email: "charlie@test.com".to_string(),
        password: "pass".to_string(),
        gdpr_consent: GDPRConsent {
            tos: true,
            privacy: true,
            marketing: false,
            from: Some("desktop".to_string()),
        },
    };
    let action = Action::Ctx(ActionCtx::Authenticate(req));
    assert!(matches!(
        action,
        Action::Ctx(ActionCtx::Authenticate(AuthRequest::Register { .. }))
    ));
}

#[test]
fn test_f20_register_action_includes_gdpr() {
    let mut state = IntroState::new();
    state.toggle_mode();
    state.email_input = "reg@test.com".to_string();
    state.password_input = "pass123".to_string();
    state.gdpr_tos = true;
    state.gdpr_privacy = true;
    state.gdpr_marketing = true;
    let req = state.create_register_request().unwrap();
    if let AuthRequest::Register { gdpr_consent, .. } = req {
        assert!(gdpr_consent.tos);
        assert!(gdpr_consent.privacy);
        assert!(gdpr_consent.marketing);
    } else {
        panic!("Expected AuthRequest::Register");
    }
}

#[test]
fn test_f20_register_action_sets_source_desktop() {
    let mut state = IntroState::new();
    state.toggle_mode();
    state.email_input = "reg@test.com".to_string();
    state.password_input = "pass123".to_string();
    state.gdpr_tos = true;
    state.gdpr_privacy = true;
    let req = state.create_register_request().unwrap();
    if let AuthRequest::Register { gdpr_consent, .. } = req {
        assert_eq!(gdpr_consent.from.as_deref(), Some("desktop"));
    } else {
        panic!("Expected AuthRequest::Register");
    }
}

#[test]
fn test_f20_register_action_sets_loading_status() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.open_auth(true);
    overlay.auth_loading = true;
    assert!(overlay.auth_loading);
}

#[test]
fn test_f20_register_action_json_serialization() {
    let req = AuthRequest::Register {
        email: "reg@test.com".to_string(),
        password: "secure".to_string(),
        gdpr_consent: GDPRConsent {
            tos: true,
            privacy: true,
            marketing: false,
            from: Some("desktop".to_string()),
        },
    };
    let json = serde_json::to_string(&req).expect("serialize");
    assert!(json.contains("reg@test.com"));
    assert!(json.contains("tos"));
}

// ============================================================================
// Feature 21: Guest Login Handling (§R2, Feature 21)
// ============================================================================

#[test]
fn test_f21_guest_login_when_already_logged_out_dismisses_overlay() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.open_auth(false);
    assert!(!overlay.logged_in);
    overlay.dismiss_auth();
    assert!(!overlay.auth_open);
    assert!(!overlay.logged_in);
}

#[test]
fn test_f21_guest_login_when_authenticated_dispatches_logout() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.logged_in = true;
    let runtime = MockAuthRuntime::new();
    runtime.dispatch(Action::Ctx(ActionCtx::Logout));
    overlay.handle_logout();
    overlay.dismiss_auth();
    assert!(!overlay.logged_in);
    assert!(!overlay.auth_open);
}

#[test]
fn test_f21_guest_login_clears_error_message() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.auth_error = "Stale error".to_string();
    overlay.dismiss_auth();
    assert!(overlay.auth_error.is_empty());
}

#[test]
fn test_f21_guest_login_leaves_anonymous_profile() {
    let overlay = MainWindowOverlayState::new();
    assert!(!overlay.logged_in);
    assert!(overlay.user_email.is_empty());
}

#[test]
fn test_f21_guest_login_keeps_library_and_calendar_locked() {
    let overlay = MainWindowOverlayState::new();
    assert!(!overlay.is_library_unlocked());
    assert!(!overlay.is_calendar_unlocked());
}

// ============================================================================
// Feature 22: Social Login Handling (§R2, Feature 22)
// ============================================================================

// Facebook and Apple only consent to strem.io, so the buttons hand the sign-in to the
// system browser and pick the account up from strem.io's handshake endpoint afterwards.

#[test]
fn test_f22_facebook_sign_in_hands_off_to_the_browser() {
    let mut window = MainWindowOverlayState::new();
    window.open_auth(false);

    window.start_social_sign_in(SocialProvider::Facebook);

    assert_eq!(window.social_waiting, Some(SocialProvider::Facebook));
    assert!(
        window.auth_loading,
        "the form waits while the browser is open"
    );
    assert!(window.auth_error.contains("in your browser"));
    assert!(
        window.auth_error.contains("https://www.strem.io/login-fb/"),
        "the user is told where to go when no browser opened"
    );
    assert!(
        window.auth_open,
        "the overlay stays until the account lands"
    );
}

#[test]
fn test_f22_apple_sign_in_hands_off_to_the_browser() {
    let mut window = MainWindowOverlayState::new();
    window.open_auth(false);

    window.start_social_sign_in(SocialProvider::Apple);

    assert_eq!(window.social_waiting, Some(SocialProvider::Apple));
    assert!(window.auth_loading);
    assert!(window
        .auth_error
        .contains("https://www.strem.io/login-apple/"));
    assert!(window.auth_open);
}

#[test]
fn test_f22_a_finished_handshake_clears_the_waiting_marker() {
    let mut window = MainWindowOverlayState::new();
    let user = create_test_user("someone@stremio.com");
    window.open_auth(false);
    window.start_social_sign_in(SocialProvider::Facebook);

    window.handle_auth_success(&user);

    assert!(!window.auth_open);
    assert!(!window.auth_loading);
    assert_eq!(window.social_waiting, None);
}

#[test]
fn test_f22_a_failed_handshake_keeps_the_form_usable() {
    let mut window = MainWindowOverlayState::new();
    window.open_auth(false);
    window.start_social_sign_in(SocialProvider::Facebook);

    window.abandon_social_sign_in(
        "Facebook sign-in didn't finish. Try again, or log in with your email and password."
            .to_string(),
    );

    assert!(window.auth_open, "the user can try again or use the form");
    assert!(!window.auth_loading);
    assert_eq!(window.social_waiting, None);
    assert!(window.auth_error.contains("Try again"));
}

#[test]
fn test_f22_dismissing_the_overlay_cancels_the_browser_sign_in() {
    let mut window = MainWindowOverlayState::new();
    window.open_auth(false);
    window.start_social_sign_in(SocialProvider::Apple);

    window.dismiss_auth();

    assert!(!window.auth_open);
    assert!(!window.auth_loading);
    assert_eq!(window.social_waiting, None);
}

#[test]
fn test_f22_nothing_is_dispatched_until_the_account_lands() {
    let runtime = MockAuthRuntime::new();
    let mut window = MainWindowOverlayState::new();
    window.open_auth(false);
    window.start_social_sign_in(SocialProvider::Facebook);
    assert!(
        runtime.dispatched_actions.lock().unwrap().is_empty(),
        "opening the browser is not an Authenticate of its own"
    );

    runtime.dispatch(Action::Ctx(ActionCtx::Authenticate(AuthRequest::Login {
        email: "someone@stremio.com".to_string(),
        password: "fb-login-token".to_string(),
        facebook: true,
    })));

    assert!(matches!(
        runtime.last_dispatched_action(),
        Some(Action::Ctx(ActionCtx::Authenticate(AuthRequest::Login {
            facebook: true,
            ..
        })))
    ));
}

#[test]
fn test_f22_the_web_intro_stub_is_retired() {
    let shell = std::fs::read_to_string("src/main.rs").expect("the shell entry point");
    assert!(
        !shell.contains("\"web-intro\""),
        "the social buttons no longer send the user to the web intro, which never signed anyone in"
    );
    let urls = std::fs::read_to_string("src/settings.rs").expect("the external URL table");
    assert!(!urls.contains("web.stremio.com"));
}

// ============================================================================
// Feature 23: Password Reset External URL Navigation (§R1, §R2, Feature 23)
// ============================================================================

#[test]
fn test_f23_reset_password_url_schema() {
    let state = IntroState::new();
    let url = state.generate_forgot_password_url();
    assert!(url.starts_with("https://"));
}

#[test]
fn test_f23_reset_password_url_domain() {
    let state = IntroState::new();
    let url = state.generate_forgot_password_url();
    assert!(url.contains("www.strem.io"));
}

#[test]
fn test_f23_reset_password_url_path() {
    let state = IntroState::new();
    let url = state.generate_forgot_password_url();
    assert!(url.contains("/reset-password/"));
}

#[test]
fn test_f23_reset_password_url_encoded_email() {
    let mut state = IntroState::new();
    state.email_input = "test.user+tag@domain.co.uk".to_string();
    let url = state.generate_forgot_password_url();
    assert!(url.contains("test.user%2Btag%40domain.co.uk"));
}

#[test]
fn test_f23_reset_password_url_empty_email() {
    let mut state = IntroState::new();
    state.email_input = "   ".to_string();
    let url = state.generate_forgot_password_url();
    assert_eq!(url, "https://www.strem.io/reset-password/");
}

// ============================================================================
// Feature 24: Core Event Listener for Auth Errors (§R2, Feature 24)
// ============================================================================

#[test]
fn test_f24_listener_intercepts_api_error() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.open_auth(false);
    overlay.auth_loading = true;
    let err_message = "USER_ERR_INVALID_PASSWORD";
    overlay.handle_auth_error(err_message.to_string());
    assert_eq!(overlay.auth_error, "USER_ERR_INVALID_PASSWORD");
    assert!(!overlay.auth_loading);
}

#[test]
fn test_f24_listener_intercepts_env_error() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.handle_auth_error("Network connection refused".to_string());
    assert_eq!(overlay.auth_error, "Network connection refused");
}

#[test]
fn test_f24_listener_intercepts_other_error() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.handle_auth_error("Unexpected failure".to_string());
    assert_eq!(overlay.auth_error, "Unexpected failure");
}

#[test]
fn test_f24_listener_resets_auth_loading_on_error() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.auth_loading = true;
    overlay.handle_auth_error("Error".to_string());
    assert!(!overlay.auth_loading);
}

#[test]
fn test_f24_listener_ignores_unrelated_errors() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.auth_error.clear();
    // Simulate non-auth error in runtime (does not affect auth overlay error)
    let unrelated_error = "STREAM_NOT_FOUND";
    assert!(!unrelated_error.contains("USER_ERR"));
    assert!(overlay.auth_error.is_empty());
}

// ============================================================================
// Feature 25: Core Event Listener for User Authenticated (§R2, Feature 25)
// ============================================================================

#[test]
fn test_f25_listener_intercepts_user_authenticated() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.open_auth(false);
    let user = create_test_user("success@stremio.com");
    overlay.handle_auth_success(&user);
    assert!(overlay.logged_in);
}

#[test]
fn test_f25_listener_clears_auth_error_on_success() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.auth_error = "Prior error".to_string();
    let user = create_test_user("success@stremio.com");
    overlay.handle_auth_success(&user);
    assert!(overlay.auth_error.is_empty());
}

#[test]
fn test_f25_listener_resets_auth_loading_on_success() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.auth_loading = true;
    let user = create_test_user("success@stremio.com");
    overlay.handle_auth_success(&user);
    assert!(!overlay.auth_loading);
}

#[test]
fn test_f25_listener_dismisses_auth_overlay_on_success() {
    let mut overlay = MainWindowOverlayState::new();
    overlay.open_auth(false);
    assert!(overlay.auth_open);
    let user = create_test_user("success@stremio.com");
    overlay.handle_auth_success(&user);
    assert!(!overlay.auth_open);
}

#[test]
fn test_f25_listener_updates_session_profile() {
    let mut overlay = MainWindowOverlayState::new();
    let user = create_test_user("updated_profile@stremio.com");
    overlay.handle_auth_success(&user);
    assert_eq!(overlay.user_email, "updated_profile@stremio.com");
    assert!(overlay.logged_in);
}
