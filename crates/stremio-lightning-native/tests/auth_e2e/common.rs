//! Common test helpers, interface contracts, state models, and validation routines
//! for Stremio Lightning Native Authentication E2E tests.

use std::sync::{Arc, Mutex};
use stremio_core::runtime::msg::{Action, Event};
use stremio_core::types::api::AuthRequest;
use stremio_core::types::profile::{Auth, GDPRConsent, User};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppSection {
    Board,
    Discover,
    Library,
    Calendar,
    Addons,
    Settings,
}

/// In-memory representation of Slint UI state for `IntroContent`.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct IntroState {
    pub is_signup: bool,
    pub error_message: String,
    pub is_loading: bool,
    pub rem_size: f32,
    pub email_input: String,
    pub password_input: String,
    pub password_masked: bool,
    pub gdpr_tos: bool,
    pub gdpr_privacy: bool,
    pub gdpr_marketing: bool,
}

impl Default for IntroState {
    fn default() -> Self {
        Self {
            is_signup: false,
            error_message: String::new(),
            is_loading: false,
            rem_size: 14.0,
            email_input: String::new(),
            password_input: String::new(),
            password_masked: true,
            gdpr_tos: false,
            gdpr_privacy: false,
            gdpr_marketing: false,
        }
    }
}

impl IntroState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn toggle_mode(&mut self) {
        self.is_signup = !self.is_signup;
        self.error_message.clear();
    }

    pub fn toggle_password_mask(&mut self) {
        self.password_masked = !self.password_masked;
    }

    pub fn validate_email(&self) -> Result<String, &'static str> {
        let trimmed = self.email_input.trim();
        if trimmed.is_empty() {
            return Err("Email cannot be empty");
        }
        if !trimmed.contains('@') {
            return Err("Email missing '@'");
        }
        let parts: Vec<&str> = trimmed.split('@').collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            return Err("Invalid email structure");
        }
        if !parts[1].contains('.') || parts[1].starts_with('.') || parts[1].ends_with('.') {
            return Err("Invalid email domain");
        }
        if trimmed.len() > 254 {
            return Err("Email exceeds maximum length");
        }
        Ok(trimmed.to_string())
    }

    pub fn validate_password(&self) -> Result<String, &'static str> {
        if self.password_input.is_empty() {
            return Err("Password cannot be empty");
        }
        if self.password_input.len() > 10000 {
            return Err("Password exceeds maximum length");
        }
        Ok(self.password_input.clone())
    }

    pub fn validate_gdpr(&self) -> Result<GDPRConsent, &'static str> {
        if !self.gdpr_tos {
            return Err("Terms of Service consent required");
        }
        if !self.gdpr_privacy {
            return Err("Privacy Policy consent required");
        }
        Ok(GDPRConsent {
            tos: true,
            privacy: true,
            marketing: self.gdpr_marketing,
            from: Some("desktop".to_string()),
        })
    }

    pub fn create_login_request(&self) -> Result<AuthRequest, &'static str> {
        let email = self.validate_email()?;
        let password = self.validate_password()?;
        Ok(AuthRequest::Login {
            email,
            password,
            facebook: false,
        })
    }

    pub fn create_register_request(&self) -> Result<AuthRequest, &'static str> {
        let email = self.validate_email()?;
        let password = self.validate_password()?;
        let gdpr_consent = self.validate_gdpr()?;
        Ok(AuthRequest::Register {
            email,
            password,
            gdpr_consent,
        })
    }

    pub fn generate_forgot_password_url(&self) -> String {
        let trimmed = self.email_input.trim();
        if trimmed.is_empty() {
            "https://www.strem.io/reset-password/".to_string()
        } else {
            let encoded =
                url::form_urlencoded::byte_serialize(trimmed.as_bytes()).collect::<String>();
            format!("https://www.strem.io/reset-password/?email={}", encoded)
        }
    }
}

/// In-memory representation of `MainWindow` overlay routing & shell navigation state.
#[derive(Debug, Clone)]
pub struct MainWindowOverlayState {
    pub auth_open: bool,
    pub auth_error: String,
    pub auth_loading: bool,
    pub auth_is_signup: bool,
    pub section: AppSection,
    pub previous_section: AppSection,
    pub logged_in: bool,
    pub user_email: String,
    pub user_avatar: Option<String>,
}

impl Default for MainWindowOverlayState {
    fn default() -> Self {
        Self {
            auth_open: false,
            auth_error: String::new(),
            auth_loading: false,
            auth_is_signup: false,
            section: AppSection::Board,
            previous_section: AppSection::Board,
            logged_in: false,
            user_email: String::new(),
            user_avatar: None,
        }
    }
}

impl MainWindowOverlayState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open_auth(&mut self, is_signup: bool) {
        self.previous_section = self.section;
        self.auth_open = true;
        self.auth_is_signup = is_signup;
        self.auth_error.clear();
        self.auth_loading = false;
    }

    pub fn dismiss_auth(&mut self) {
        self.auth_open = false;
        self.auth_error.clear();
        self.auth_loading = false;
        self.section = self.previous_section;
    }

    pub fn handle_auth_success(&mut self, user: &User) {
        self.logged_in = true;
        self.user_email = user.email.clone();
        self.user_avatar = user.avatar.clone();
        self.auth_open = false;
        self.auth_loading = false;
        self.auth_error.clear();
    }

    pub fn handle_auth_error(&mut self, error_msg: String) {
        self.auth_loading = false;
        self.auth_error = error_msg;
    }

    pub fn handle_logout(&mut self) {
        self.logged_in = false;
        self.user_email.clear();
        self.user_avatar = None;
    }

    pub fn is_library_unlocked(&self) -> bool {
        self.logged_in
    }

    pub fn is_calendar_unlocked(&self) -> bool {
        self.logged_in
    }
}

/// Simulated runtime dispatcher for opaque-box testing of action flow and event loop.
#[derive(Debug, Default)]
pub struct MockAuthRuntime {
    pub dispatched_actions: Arc<Mutex<Vec<Action>>>,
    pub emitted_events: Arc<Mutex<Vec<Event>>>,
}

impl MockAuthRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn dispatch(&self, action: Action) {
        self.dispatched_actions.lock().unwrap().push(action);
    }

    pub fn emit_event(&self, event: Event) {
        self.emitted_events.lock().unwrap().push(event);
    }

    pub fn last_dispatched_action(&self) -> Option<Action> {
        self.dispatched_actions.lock().unwrap().last().cloned()
    }

    #[allow(dead_code)]
    pub fn clear(&self) {
        self.dispatched_actions.lock().unwrap().clear();
        self.emitted_events.lock().unwrap().clear();
    }
}

/// Helper to create a dummy authenticated User struct for testing.
pub fn create_test_user(email: &str) -> User {
    serde_json::from_value::<User>(serde_json::json!({
        "_id": "usr_test_123456789",
        "email": email,
        "fbId": null,
        "appleId": null,
        "avatar": "https://example.com/avatar.png",
        "lastModified": "2026-10-18T21:00:00Z",
        "dateRegistered": "2026-10-18T21:00:00Z",
        "trakt": null,
        "premiumExpire": null,
        "gdpr_consent": {
            "tos": true,
            "privacy": true,
            "marketing": false,
            "from": "desktop"
        }
    }))
    .expect("deserialize test user")
}

/// Helper to create a dummy Auth struct for testing.
#[allow(dead_code)]
pub fn create_test_auth(email: &str) -> Auth {
    serde_json::from_value::<Auth>(serde_json::json!({
        "key": "authkey_abc123xyz",
        "user": {
            "_id": "usr_test_123456789",
            "email": email,
            "fbId": null,
            "appleId": null,
            "avatar": "https://example.com/avatar.png",
            "lastModified": "2026-10-18T21:00:00Z",
            "dateRegistered": "2026-10-18T21:00:00Z",
            "trakt": null,
            "premiumExpire": null,
            "gdpr_consent": {
                "tos": true,
                "privacy": true,
                "marketing": false,
                "from": "desktop"
            }
        }
    }))
    .expect("deserialize test auth")
}

/// Registered styling class names, as documented in `THEMES.md`.
pub const EXPECTED_INTRO_THEME_CLASSES: &[&str] = &[
    "intro-container",
    "form-container",
    "heading-container",
    "options-container",
    "credentials-text-input",
    "error-message",
    "facebook-button",
    "apple-button",
    "guest-login-button",
    "submit-button",
    "submit-button-container",
];

/// Helper to verify Theme rem scaling logic: `Theme.rem-size(width)`
pub fn calculate_rem_size(window_width_px: f32) -> f32 {
    let base_size = 14.0;
    let base_width = 1100.0;
    let scale = (window_width_px / base_width).clamp(0.75, 1.5);
    base_size * scale
}
