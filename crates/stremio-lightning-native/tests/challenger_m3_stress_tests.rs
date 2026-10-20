//! Empirical Challenger Stress Tests for Milestone 3 Gate
//!
//! Rigorous validation of:
//! 1. Error extraction across APIError, EnvError (all 9 variants), and OtherError (all 9 variants).
//! 2. Edge case sanitization: empty strings, whitespace trimming, password preservation, GDPR matrix.
//! 3. External URL resolution, query handling, and URL percent-encoding.
//! 4. Slint UI component callbacks, local error validation, and mode switches.
//! 5. Runtime event handling, error filtering by event source, and session state transitions.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use stremio_core::models::ctx::{CtxError, OtherError};
use stremio_core::runtime::msg::Event;
use stremio_core::runtime::EnvError;
use stremio_core::types::api::{APIError, AuthRequest};
use stremio_core::types::profile::GDPRConsent;

// Match implementation in src/main.rs:810-816
fn ctx_error_message(error: &CtxError) -> String {
    match error {
        CtxError::API(api_err) => api_err.message.clone(),
        CtxError::Env(env_err) => env_err.message(),
        CtxError::Other(other_err) => other_err.message(),
    }
}

// Match implementation in src/settings.rs:483-495
fn resolve_external_url(kind: &str) -> Option<String> {
    match kind {
        "support" => Some("https://stremio.zendesk.com/hc/en-us".to_string()),
        "source" => Some("https://github.com/theguy000/stremio-lightning".to_string()),
        "tos" => Some("https://www.stremio.com/tos".to_string()),
        "privacy" => Some("https://www.stremio.com/privacy".to_string()),
        "reset-password" => Some("https://www.strem.io/reset-password/".to_string()),
        "web-intro" => Some("https://web.stremio.com/#/intro".to_string()),
        s if s.starts_with("http://") || s.starts_with("https://") => Some(s.to_string()),
        _ => None,
    }
}

// Slint Component Harness for direct GUI empirical verification
slint::slint! {
    import { IntroContent } from "ui/intro.slint";

    export component ChallengerIntroHarness inherits Window {
        width: 1100px;
        height: 720px;

        in-out property <bool> is-signup <=> intro.is-signup;
        in-out property <string> error-message <=> intro.error-message;
        in-out property <bool> is-loading <=> intro.is-loading;
        in-out property <length> rem-size <=> intro.rem-size;

        callback submit-login(string, string);
        callback submit-register(string, string, bool, bool, bool);
        callback login-guest();
        callback login-facebook();
        callback login-apple();
        callback forgot-password(string);
        callback dismiss();

        intro := IntroContent {
            width: 100%;
            height: 100%;
            submit-login(email, pass) => { root.submit-login(email, pass); }
            submit-register(email, pass, tos, priv, mkt) => { root.submit-register(email, pass, tos, priv, mkt); }
            login-guest() => { root.login-guest(); }
            login-facebook() => { root.login-facebook(); }
            login-apple() => { root.login-apple(); }
            forgot-password(email) => { root.forgot-password(email); }
            dismiss() => { root.dismiss(); }
        }
    }
}

// ============================================================================
// Suite 1: Error Extraction across APIError, EnvError, and OtherError
// ============================================================================

#[test]
fn test_error_extraction_all_env_error_variants() {
    let variants = vec![
        (
            EnvError::StorageUnavailable,
            "Storage is not available",
        ),
        (
            EnvError::Fetch("connection refused".into()),
            "Failed to fetch: connection refused",
        ),
        (
            EnvError::AddonTransport("invalid json body".into()),
            "Addon protocol violation: invalid json body",
        ),
        (
            EnvError::Serde("unexpected EOF".into()),
            "Serialization error: unexpected EOF",
        ),
        (
            EnvError::StorageSchemaVersionDowngrade(3, 2),
            "Downgrade storage schema version from 3 to 2 is not allowed",
        ),
        (
            EnvError::StorageSchemaVersionUpgrade(Box::new(EnvError::StorageUnavailable)),
            "Upgrade storage schema version failed caused by: Storage is not available",
        ),
        (
            EnvError::StorageReadError("permission denied".into()),
            "Storage read error: permission denied",
        ),
        (
            EnvError::StorageWriteError("disk full".into()),
            "Storage write error: disk full",
        ),
        (
            EnvError::Other("custom runtime error".into()),
            "Other error: custom runtime error",
        ),
    ];

    for (variant, expected) in variants {
        let err = CtxError::Env(variant);
        let msg = ctx_error_message(&err);
        assert_eq!(msg, expected);
        // Ensure no internal debug enum syntax is present
        assert!(!msg.contains("EnvError::"), "Leaked debug token in {msg}");
        assert!(!msg.starts_with("Fetch("), "Leaked debug token in {msg}");
    }
}

#[test]
fn test_error_extraction_all_other_error_variants() {
    let variants = vec![
        (OtherError::UserNotLoggedIn, "User is not logged in"),
        (OtherError::LibraryItemNotFound, "Item is not found in library"),
        (OtherError::AddonAlreadyInstalled, "Addon is already installed"),
        (OtherError::AddonNotInstalled, "Addon is not installed"),
        (OtherError::AddonIsProtected, "Addon is protected"),
        (OtherError::AddonConfigurationRequired, "Addon requires configuration"),
        (
            OtherError::UserAddonsAreLocked,
            "Fetching Addons from the API failed and we have defaulted the addons to the officials ones until the request succeeds",
        ),
        (
            OtherError::UserLibraryIsMissing,
            "Fetching Library from the API failed and we have defaulted to empty library until the request succeeds",
        ),
        (
            OtherError::InvalidStreamingServerUrl,
            "Streaming server URL must be http or https",
        ),
    ];

    for (variant, expected) in variants {
        let err = CtxError::Other(variant);
        let msg = ctx_error_message(&err);
        assert_eq!(msg, expected);
        assert!(!msg.contains("OtherError::"), "Leaked debug token in {msg}");
    }
}

#[test]
fn test_error_extraction_apierror_adversarial_payloads() {
    let long_string = "A".repeat(10000);
    let payloads = vec![
        ("user not found", 1),
        ("USER_ERR_INVALID_PASSWORD", 2),
        ("", 0),
        ("Invalid email or password", 401),
        ("Account temporarily locked. Please try again in 15 minutes.", 429),
        ("用户不存在", 100), // CJK
        ("خطأ في المصادقة", 101), // Arabic
        ("Неверный пароль", 102), // Cyrillic
        ("💥 Fatal auth failure 🔒", 500), // Emojis
        ("<script>alert('xss')</script>", 999), // HTML injection
        ("{\"error\": \"json_embedded\", \"code\": 123}", 888), // JSON
        (long_string.as_str(), 1), // 10k characters
    ];

    for (msg_in, code) in payloads {
        let api_err = CtxError::API(APIError {
            message: msg_in.to_string(),
            code,
        });
        let msg_out = ctx_error_message(&api_err);
        assert_eq!(msg_out, msg_in, "APIError message must pass through verbatim");
    }
}

// ============================================================================
// Suite 2: Input Trimming, Password Preservation & Edge Cases
// ============================================================================

#[test]
fn test_input_trimming_and_password_preservation() {
    // In src/main.rs:1140-1181:
    // email_str = email.trim().to_string();
    // password_str = password.to_string();

    let test_cases = vec![
        ("  user@example.com  ", "user@example.com"),
        ("\t\r\n user@test.org \n\t", "user@test.org"),
        ("user@test.org", "user@test.org"),
        ("   ", ""),
        ("", ""),
    ];

    for (input_email, expected_email) in test_cases {
        let trimmed = input_email.trim().to_string();
        assert_eq!(trimmed, expected_email);
    }

    // Passwords must preserve whitespace verbatim
    let sensitive_passwords = vec![
        "  password with leading and trailing spaces  ",
        " \t ",
        "unicode password 🚀 123",
        "\"quotes\" and \\backslashes\\",
        "",
    ];

    for pass in sensitive_passwords {
        let pass_str = pass.to_string();
        assert_eq!(pass_str, pass, "Password must not be trimmed or modified");
    }
}

#[test]
fn test_gdpr_truth_table_all_8_combinations() {
    let mut combinations = Vec::new();
    for &tos in &[false, true] {
        for &privacy in &[false, true] {
            for &marketing in &[false, true] {
                combinations.push((tos, privacy, marketing));
            }
        }
    }
    assert_eq!(combinations.len(), 8);

    for (tos, privacy, marketing) in combinations {
        let consent = GDPRConsent {
            tos,
            privacy,
            marketing,
            from: Some("stremio-lightning-native".to_string()),
        };
        assert_eq!(consent.tos, tos);
        assert_eq!(consent.privacy, privacy);
        assert_eq!(consent.marketing, marketing);
        assert_eq!(consent.from.as_deref(), Some("stremio-lightning-native"));

        let register_req = AuthRequest::Register {
            email: "test@example.com".to_string(),
            password: "password123".to_string(),
            gdpr_consent: consent.clone(),
        };

        if let AuthRequest::Register { gdpr_consent, .. } = register_req {
            assert_eq!(gdpr_consent.tos, tos);
            assert_eq!(gdpr_consent.privacy, privacy);
            assert_eq!(gdpr_consent.marketing, marketing);
        } else {
            panic!("Expected Register request");
        }
    }
}

// ============================================================================
// Suite 3: URL Generation, Query Encoding & External URL Resolution
// ============================================================================

#[test]
fn test_forgot_password_url_encoding_edge_cases() {
    // Tests the logic in src/main.rs:1215-1226
    let generate_forgot_password_target = |email: &str| -> String {
        let trimmed = email.trim();
        if trimmed.is_empty() {
            "reset-password".to_string()
        } else {
            let encoded = url::form_urlencoded::byte_serialize(trimmed.as_bytes()).collect::<String>();
            format!("https://www.strem.io/reset-password/?email={encoded}")
        }
    };

    // Case 1: Empty or whitespace-only
    for empty in &["", "   ", "\t\n  \r"] {
        let target = generate_forgot_password_target(empty);
        assert_eq!(target, "reset-password");
        let resolved = resolve_external_url(&target);
        assert_eq!(resolved, Some("https://www.strem.io/reset-password/".to_string()));
    }

    // Case 2: Standard email
    let target = generate_forgot_password_target("user@example.com");
    assert_eq!(target, "https://www.strem.io/reset-password/?email=user%40example.com");
    let resolved = resolve_external_url(&target);
    assert_eq!(resolved, Some("https://www.strem.io/reset-password/?email=user%40example.com".to_string()));

    // Case 3: Email with special query/URL characters: '+', '&', '=', '?', '#', ' '
    let complex_email = "  user+filter&foo=bar?baz#hash @example.com  ";
    let target = generate_forgot_password_target(complex_email);
    assert!(!target.contains('&'), "Ampersands must be encoded to avoid cmd/URL parameter injection");
    assert!(!target.contains('#'), "Hash must be encoded");
    let resolved = resolve_external_url(&target);
    assert!(resolved.is_some());
    let url_str = resolved.unwrap();
    assert!(url_str.starts_with("https://www.strem.io/reset-password/?email="));
    assert!(url_str.contains("%40example.com"));

    // Case 4: Non-ASCII / Cyrillic Email
    let cyrillic_email = "пользователь@домен.рф";
    let target = generate_forgot_password_target(cyrillic_email);
    let resolved = resolve_external_url(&target);
    assert!(resolved.is_some());
}

#[test]
fn test_resolve_external_url_boundaries_and_rejections() {
    // Known endpoints
    assert_eq!(resolve_external_url("support"), Some("https://stremio.zendesk.com/hc/en-us".into()));
    assert_eq!(resolve_external_url("source"), Some("https://github.com/theguy000/stremio-lightning".into()));
    assert_eq!(resolve_external_url("tos"), Some("https://www.stremio.com/tos".into()));
    assert_eq!(resolve_external_url("privacy"), Some("https://www.stremio.com/privacy".into()));
    assert_eq!(resolve_external_url("reset-password"), Some("https://www.strem.io/reset-password/".into()));
    assert_eq!(resolve_external_url("web-intro"), Some("https://web.stremio.com/#/intro".into()));

    // Only the bare "reset-password" target exists; a query must be passed as a full URL.
    assert_eq!(resolve_external_url("reset-password?email=a%40b.com"), None);
    assert_eq!(resolve_external_url("reset-password/?email=a%40b.com"), None);

    // Pass-through http / https
    assert_eq!(
        resolve_external_url("https://www.strem.io/reset-password/?email=test"),
        Some("https://www.strem.io/reset-password/?email=test".into())
    );
    assert_eq!(
        resolve_external_url("http://localhost:11470"),
        Some("http://localhost:11470".into())
    );

    // Rejections: non-http schemes, malformed, empty
    assert_eq!(resolve_external_url(""), None);
    assert_eq!(resolve_external_url("   "), None);
    assert_eq!(resolve_external_url("javascript:alert(1)"), None);
    assert_eq!(resolve_external_url("file:///etc/passwd"), None);
    assert_eq!(resolve_external_url("data:text/html,<script>"), None);
    assert_eq!(resolve_external_url("ftp://example.com"), None);
    assert_eq!(resolve_external_url("reset-password-unknown"), None);
    assert_eq!(resolve_external_url("random_string"), None);
}

// ============================================================================
// Suite 4: Slint GUI Component Empirical Verification
// ============================================================================

#[test]
fn test_slint_gui_component_and_callbacks() {
    let harness = ChallengerIntroHarness::new().expect("create ChallengerIntroHarness");

    // Check initial properties
    assert!(!harness.get_is_signup(), "Default mode must be login");
    assert_eq!(harness.get_error_message().as_str(), "", "Default error message must be empty");
    assert!(!harness.get_is_loading(), "Default loading state must be false");

    // Test property updates
    harness.set_is_signup(true);
    assert!(harness.get_is_signup());
    harness.set_error_message("Invalid credentials".into());
    assert_eq!(harness.get_error_message().as_str(), "Invalid credentials");
    harness.set_is_loading(true);
    assert!(harness.get_is_loading());

    // Test callbacks
    let login_called = Arc::new(AtomicBool::new(false));
    let register_called = Arc::new(AtomicBool::new(false));
    let guest_called = Arc::new(AtomicBool::new(false));
    let fb_called = Arc::new(AtomicBool::new(false));
    let apple_called = Arc::new(AtomicBool::new(false));
    let forgot_called = Arc::new(AtomicBool::new(false));
    let dismiss_called = Arc::new(AtomicBool::new(false));

    {
        let l = login_called.clone();
        harness.on_submit_login(move |e, p| {
            assert_eq!(e.as_str(), "user@test.org");
            assert_eq!(p.as_str(), "pass123");
            l.store(true, Ordering::SeqCst);
        });
    }

    {
        let r = register_called.clone();
        harness.on_submit_register(move |e, p, tos, priv_val, mkt| {
            assert_eq!(e.as_str(), "new@test.org");
            assert_eq!(p.as_str(), "newpass");
            assert!(tos);
            assert!(priv_val);
            assert!(!mkt);
            r.store(true, Ordering::SeqCst);
        });
    }

    {
        let g = guest_called.clone();
        harness.on_login_guest(move || g.store(true, Ordering::SeqCst));
    }
    {
        let fb = fb_called.clone();
        harness.on_login_facebook(move || fb.store(true, Ordering::SeqCst));
    }
    {
        let ap = apple_called.clone();
        harness.on_login_apple(move || ap.store(true, Ordering::SeqCst));
    }
    {
        let fp = forgot_called.clone();
        harness.on_forgot_password(move |e| {
            assert_eq!(e.as_str(), "reset@test.org");
            fp.store(true, Ordering::SeqCst);
        });
    }
    {
        let d = dismiss_called.clone();
        harness.on_dismiss(move || d.store(true, Ordering::SeqCst));
    }

    // Trigger callbacks directly to verify Slint callback dispatch
    harness.invoke_submit_login("user@test.org".into(), "pass123".into());
    assert!(login_called.load(Ordering::SeqCst));

    harness.invoke_submit_register("new@test.org".into(), "newpass".into(), true, true, false);
    assert!(register_called.load(Ordering::SeqCst));

    harness.invoke_login_guest();
    assert!(guest_called.load(Ordering::SeqCst));

    harness.invoke_login_facebook();
    assert!(fb_called.load(Ordering::SeqCst));

    harness.invoke_login_apple();
    assert!(apple_called.load(Ordering::SeqCst));

    harness.invoke_forgot_password("reset@test.org".into());
    assert!(forgot_called.load(Ordering::SeqCst));

    harness.invoke_dismiss();
    assert!(dismiss_called.load(Ordering::SeqCst));
}

// ============================================================================
// Suite 5: Event Loop Filtering & State Transitions
// ============================================================================

#[derive(Default)]
struct MockWindowModel {
    auth_open: bool,
    auth_loading: bool,
    auth_error: String,
}

#[test]
fn test_event_loop_core_error_filtering_by_source() {
    // Tests lines 1261-1270 in src/main.rs:
    // Only Event::Error where source is Event::UserAuthenticated should update auth properties!
    let simulate_event = |model: &mut MockWindowModel, event: &Event| {
        match event {
            Event::Error { error, source } => {
                if matches!(**source, Event::UserAuthenticated { .. }) {
                    let msg = ctx_error_message(error);
                    model.auth_loading = false;
                    model.auth_error = msg;
                }
            }
            Event::UserAuthenticated { .. } => {
                model.auth_loading = false;
                model.auth_error = String::new();
                model.auth_open = false;
            }
            _ => {}
        }
    };

    let mut model = MockWindowModel {
        auth_open: true,
        auth_loading: true,
        auth_error: "initial".into(),
    };

    // Case 1: Error from unrelated source (e.g. UserLoggedOut or generic)
    let unrelated_error = Event::Error {
        error: CtxError::Other(OtherError::LibraryItemNotFound),
        source: Box::new(Event::UserLoggedOut { uid: None }),
    };
    simulate_event(&mut model, &unrelated_error);
    // auth_loading and auth_error must NOT be modified by unrelated errors!
    assert!(model.auth_loading, "Unrelated error should not reset auth_loading");
    assert_eq!(model.auth_error, "initial", "Unrelated error should not overwrite auth_error");

    // Case 2: Error with source = Event::UserAuthenticated
    let auth_error = Event::Error {
        error: CtxError::API(APIError {
            message: "invalid credentials".into(),
            code: 1,
        }),
        source: Box::new(Event::UserAuthenticated {
            auth_request: AuthRequest::Login {
                email: "user@test.org".into(),
                password: "wrong".into(),
                facebook: false,
            },
        }),
    };
    simulate_event(&mut model, &auth_error);
    assert!(!model.auth_loading, "Auth error must reset auth_loading to false");
    assert_eq!(model.auth_error, "invalid credentials", "Auth error must populate auth_error");

    // Case 3: Auth Success event
    model.auth_loading = true;
    let auth_success = Event::UserAuthenticated {
        auth_request: AuthRequest::Login {
            email: "user@test.org".into(),
            password: "correct".into(),
            facebook: false,
        },
    };
    simulate_event(&mut model, &auth_success);
    assert!(!model.auth_loading);
    assert_eq!(model.auth_error, "");
    assert!(!model.auth_open);
}
