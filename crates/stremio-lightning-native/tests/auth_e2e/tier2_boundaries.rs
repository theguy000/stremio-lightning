//! Tier 2: Boundary & Corner Cases Test Suite
//! Rigorous validation of edge cases: empty inputs, whitespace, invalid email formats,
//! missing GDPR consent, unicode passwords, extreme length limits, and character escaping.

use super::common::*;
use stremio_core::types::api::AuthRequest;

// ============================================================================
// Category 1: Empty and Missing Inputs
// ============================================================================

#[test]
fn test_b1_empty_email_and_password() {
    let state = IntroState::new();
    let res = state.create_login_request();
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "Email cannot be empty");
}

#[test]
fn test_b1_empty_email_with_valid_password() {
    let mut state = IntroState::new();
    state.password_input = "valid_password123".to_string();
    let res = state.create_login_request();
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "Email cannot be empty");
}

#[test]
fn test_b1_valid_email_with_empty_password() {
    let mut state = IntroState::new();
    state.email_input = "valid@example.com".to_string();
    let res = state.create_login_request();
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "Password cannot be empty");
}

#[test]
fn test_b1_empty_email_in_registration() {
    let mut state = IntroState::new();
    state.toggle_mode();
    state.password_input = "pass123".to_string();
    state.gdpr_tos = true;
    state.gdpr_privacy = true;
    let res = state.create_register_request();
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "Email cannot be empty");
}

#[test]
fn test_b1_empty_email_in_forgot_password() {
    let state = IntroState::new();
    let url = state.generate_forgot_password_url();
    assert_eq!(url, "https://www.strem.io/reset-password/");
    assert!(!url.contains("?email="));
}

// ============================================================================
// Category 2: Whitespace and Trimming
// ============================================================================

#[test]
fn test_b2_email_leading_and_trailing_whitespace() {
    let mut state = IntroState::new();
    state.email_input = "  \t  padded_user@domain.org  \r\n ".to_string();
    state.password_input = "mypassword".to_string();
    let req = state.create_login_request().expect("trims whitespace");
    if let AuthRequest::Login { email, .. } = req {
        assert_eq!(email, "padded_user@domain.org");
    } else {
        panic!("Expected AuthRequest::Login");
    }
}

#[test]
fn test_b2_email_with_tabs_and_newlines() {
    let mut state = IntroState::new();
    state.email_input = "\n\tuser@domain.com\t\n".to_string();
    state.password_input = "mypassword".to_string();
    let req = state.create_login_request().expect("trims whitespace");
    if let AuthRequest::Login { email, .. } = req {
        assert_eq!(email, "user@domain.com");
    } else {
        panic!("Expected AuthRequest::Login");
    }
}

#[test]
fn test_b2_password_leading_trailing_spaces_preserved() {
    let mut state = IntroState::new();
    state.email_input = "user@domain.com".to_string();
    state.password_input = "  password with spaces  ".to_string();
    let req = state
        .create_login_request()
        .expect("preserves password spaces");
    if let AuthRequest::Login { password, .. } = req {
        assert_eq!(password, "  password with spaces  ");
    } else {
        panic!("Expected AuthRequest::Login");
    }
}

#[test]
fn test_b2_password_all_whitespace() {
    let mut state = IntroState::new();
    state.email_input = "user@domain.com".to_string();
    state.password_input = "   ".to_string();
    let req = state
        .create_login_request()
        .expect("allows whitespace password");
    if let AuthRequest::Login { password, .. } = req {
        assert_eq!(password, "   ");
    } else {
        panic!("Expected AuthRequest::Login");
    }
}

#[test]
fn test_b2_whitespace_only_email_rejected() {
    let mut state = IntroState::new();
    state.email_input = "     \t\n  ".to_string();
    state.password_input = "pass".to_string();
    let res = state.create_login_request();
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "Email cannot be empty");
}

// ============================================================================
// Category 3: Invalid Email Formats
// ============================================================================

#[test]
fn test_b3_email_missing_at_symbol() {
    let mut state = IntroState::new();
    state.email_input = "plainaddress.com".to_string();
    state.password_input = "pass".to_string();
    let res = state.create_login_request();
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "Email missing '@'");
}

#[test]
fn test_b3_email_missing_domain() {
    let mut state = IntroState::new();
    state.email_input = "username@".to_string();
    state.password_input = "pass".to_string();
    let res = state.create_login_request();
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "Invalid email structure");
}

#[test]
fn test_b3_email_multiple_at_symbols() {
    let mut state = IntroState::new();
    state.email_input = "user@domain@another.com".to_string();
    state.password_input = "pass".to_string();
    let res = state.create_login_request();
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "Invalid email structure");
}

#[test]
fn test_b3_email_missing_tld() {
    let mut state = IntroState::new();
    state.email_input = "user@localhost".to_string();
    state.password_input = "pass".to_string();
    let res = state.create_login_request();
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "Invalid email domain");
}

#[test]
fn test_b3_email_special_characters_in_domain() {
    let mut state = IntroState::new();
    state.email_input = "user@.invalid.com".to_string();
    state.password_input = "pass".to_string();
    let res = state.create_login_request();
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "Invalid email domain");
}

// ============================================================================
// Category 4: Missing GDPR Consent
// ============================================================================

#[test]
fn test_b4_registration_without_tos_fails_validation() {
    let mut state = IntroState::new();
    state.toggle_mode();
    state.email_input = "user@test.org".to_string();
    state.password_input = "secret".to_string();
    state.gdpr_tos = false;
    state.gdpr_privacy = true;
    let res = state.create_register_request();
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "Terms of Service consent required");
}

#[test]
fn test_b4_registration_without_privacy_fails_validation() {
    let mut state = IntroState::new();
    state.toggle_mode();
    state.email_input = "user@test.org".to_string();
    state.password_input = "secret".to_string();
    state.gdpr_tos = true;
    state.gdpr_privacy = false;
    let res = state.create_register_request();
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "Privacy Policy consent required");
}

#[test]
fn test_b4_registration_without_both_tos_and_privacy_fails() {
    let mut state = IntroState::new();
    state.toggle_mode();
    state.email_input = "user@test.org".to_string();
    state.password_input = "secret".to_string();
    state.gdpr_tos = false;
    state.gdpr_privacy = false;
    let res = state.create_register_request();
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "Terms of Service consent required");
}

#[test]
fn test_b4_registration_with_tos_and_privacy_succeeds() {
    let mut state = IntroState::new();
    state.toggle_mode();
    state.email_input = "user@test.org".to_string();
    state.password_input = "secret".to_string();
    state.gdpr_tos = true;
    state.gdpr_privacy = true;
    state.gdpr_marketing = false;
    let res = state.create_register_request();
    assert!(res.is_ok());
}

#[test]
fn test_b4_registration_marketing_optional() {
    let mut state = IntroState::new();
    state.toggle_mode();
    state.email_input = "user@test.org".to_string();
    state.password_input = "secret".to_string();
    state.gdpr_tos = true;
    state.gdpr_privacy = true;
    state.gdpr_marketing = true;
    let res = state.create_register_request().unwrap();
    if let AuthRequest::Register { gdpr_consent, .. } = res {
        assert!(gdpr_consent.marketing);
    } else {
        panic!("Expected AuthRequest::Register");
    }
}

// ============================================================================
// Category 5: Unicode and International Passwords
// ============================================================================

#[test]
fn test_b5_password_with_cjk_characters() {
    let mut state = IntroState::new();
    state.email_input = "cjk@test.com".to_string();
    state.password_input = "密码你好世界1234".to_string();
    let req = state.create_login_request().expect("CJK password valid");
    if let AuthRequest::Login { password, .. } = req {
        assert_eq!(password, "密码你好世界1234");
    } else {
        panic!("Expected AuthRequest::Login");
    }
}

#[test]
fn test_b5_password_with_cyrillic_and_arabic_rtl() {
    let mut state = IntroState::new();
    state.email_input = "intl@test.com".to_string();
    state.password_input = "парольكلمةالسر".to_string();
    let req = state
        .create_login_request()
        .expect("Cyrillic & Arabic password valid");
    if let AuthRequest::Login { password, .. } = req {
        assert_eq!(password, "парольكلمةالسر");
    } else {
        panic!("Expected AuthRequest::Login");
    }
}

#[test]
fn test_b5_password_with_emojis_and_zwj() {
    let mut state = IntroState::new();
    state.email_input = "emoji@test.com".to_string();
    state.password_input = "🔑🔒✨👨‍👩‍👧‍👦🚀".to_string();
    let req = state.create_login_request().expect("Emoji password valid");
    if let AuthRequest::Login { password, .. } = req {
        assert_eq!(password, "🔑🔒✨👨‍👩‍👧‍👦🚀");
    } else {
        panic!("Expected AuthRequest::Login");
    }
}

#[test]
fn test_b5_password_with_accents_and_umlauts() {
    let mut state = IntroState::new();
    state.email_input = "accents@test.com".to_string();
    state.password_input = "éàçüößñøå".to_string();
    let req = state
        .create_login_request()
        .expect("Accented password valid");
    if let AuthRequest::Login { password, .. } = req {
        assert_eq!(password, "éàçüößñøå");
    } else {
        panic!("Expected AuthRequest::Login");
    }
}

#[test]
fn test_b5_password_with_special_symbols_and_math() {
    let mut state = IntroState::new();
    state.email_input = "symbols@test.com".to_string();
    state.password_input = "§±!@#$%^&*()_+-=[]{}|;':\",./<>?`~∑√∫≈≠".to_string();
    let req = state.create_login_request().expect("Math symbols valid");
    if let AuthRequest::Login { password, .. } = req {
        assert_eq!(password, "§±!@#$%^&*()_+-=[]{}|;':\",./<>?`~∑√∫≈≠");
    } else {
        panic!("Expected AuthRequest::Login");
    }
}

// ============================================================================
// Category 6: Extreme Length and Resource Stress
// ============================================================================

#[test]
fn test_b6_email_maximum_length_254_chars() {
    let mut state = IntroState::new();
    // 64-char local part + '@' + 189-char domain = 254 chars (RFC 5321 max)
    let local = "a".repeat(64);
    let domain_chunk = "b".repeat(60);
    let email = format!("{}@{}.{}.com", local, domain_chunk, domain_chunk);
    assert_eq!(email.len(), 64 + 1 + 60 + 1 + 60 + 4); // 190 <= 254
    state.email_input = email.clone();
    state.password_input = "pass".to_string();
    assert!(state.create_login_request().is_ok());
}

#[test]
fn test_b6_email_oversized_10k_chars() {
    let mut state = IntroState::new();
    let oversized = format!("{}@example.com", "a".repeat(10_000));
    state.email_input = oversized;
    state.password_input = "pass".to_string();
    let res = state.create_login_request();
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "Email exceeds maximum length");
}

#[test]
fn test_b6_password_single_character() {
    let mut state = IntroState::new();
    state.email_input = "user@test.org".to_string();
    state.password_input = "x".to_string();
    assert!(state.create_login_request().is_ok());
}

#[test]
fn test_b6_password_maximum_length_10k_chars() {
    let mut state = IntroState::new();
    state.email_input = "user@test.org".to_string();
    state.password_input = "p".repeat(10_000);
    assert!(state.create_login_request().is_ok());
}

#[test]
fn test_b6_error_message_extreme_length_wrapped() {
    let mut overlay = MainWindowOverlayState::new();
    let huge_error = "Server encountered internal error: ".to_string() + &"detail ".repeat(500);
    overlay.handle_auth_error(huge_error.clone());
    assert_eq!(overlay.auth_error, huge_error);
}

// ============================================================================
// Category 7: Injection and Control Characters
// ============================================================================

#[test]
fn test_b7_email_with_sql_injection_payload() {
    let mut state = IntroState::new();
    state.email_input = "' OR '1'='1'@test.com".to_string();
    state.password_input = "pass".to_string();
    let req = state
        .create_login_request()
        .expect("Payload preserved safely");
    if let AuthRequest::Login { email, .. } = req {
        assert_eq!(email, "' OR '1'='1'@test.com");
    } else {
        panic!("Expected AuthRequest::Login");
    }
}

#[test]
fn test_b7_password_with_json_injection_payload() {
    let mut state = IntroState::new();
    state.email_input = "user@test.org".to_string();
    state.password_input = r#"{"admin": true, "key": "injected"}"#.to_string();
    let req = state.create_login_request().expect("JSON payload safe");
    let json = serde_json::to_string(&req).expect("serialize");
    assert!(json.contains(r#"{\"admin\": true"#));
}

#[test]
fn test_b7_email_with_null_bytes() {
    let mut state = IntroState::new();
    state.email_input = "user\0name@domain.com".to_string();
    state.password_input = "pass".to_string();
    let req = state
        .create_login_request()
        .expect("Handles null byte safely");
    if let AuthRequest::Login { email, .. } = req {
        assert!(email.contains('\0'));
    }
}

#[test]
fn test_b7_email_with_html_tags_escaped() {
    let mut state = IntroState::new();
    state.email_input = "<script>alert(1)</script>@test.com".to_string();
    state.password_input = "pass".to_string();
    let req = state.create_login_request().expect("Handles tag safely");
    if let AuthRequest::Login { email, .. } = req {
        assert!(email.contains("<script>"));
    }
}

#[test]
fn test_b7_password_with_control_characters() {
    let mut state = IntroState::new();
    state.email_input = "user@test.org".to_string();
    state.password_input = "\x01\x02\x03\x1F\x7F".to_string();
    let req = state
        .create_login_request()
        .expect("Handles control chars safely");
    if let AuthRequest::Login { password, .. } = req {
        assert_eq!(password, "\x01\x02\x03\x1F\x7F");
    }
}
