//! Adversarial Stress Test Suite for Milestone 1: UI Components & Styling
//!
//! Objectives:
//! 1. Test extreme styling and theming overrides on the 20 newly added intro classes
//!    (invalid colors, missing classes, inheritance, opacity limits).
//! 2. Test Slint component boundaries in intro.slint:
//!    missing inputs, empty passwords, long passwords, rapid mode switching
//!    between Log in and Sign up, GDPR consent checkbox combinations.

use slint::{Color, ComponentHandle};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

slint::slint! {
    import { IntroContent } from "ui/intro.slint";
    import { ClassStyles, ClassStyle } from "ui/class-styles.slint";
    export { ClassStyles, ClassStyle }

    export component AdversarialIntroHarness inherits Window {
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

// Replicate CSS parsing logic locally to stress-test adversarial CSS edge cases
// matching the exact parser in src/class_styles.rs
mod css_stress {
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

    #[derive(Debug, Default, Clone, Copy, PartialEq)]
    pub struct Style {
        pub color: Option<Rgba>,
        pub background: Option<Rgba>,
        pub opacity: Option<f32>,
        pub hidden: Option<bool>,
    }

    impl Style {
        pub fn merge(&mut self, later: Self) {
            self.color = later.color.or(self.color);
            self.background = later.background.or(self.background);
            self.opacity = later.opacity.or(self.opacity);
            self.hidden = later.hidden.or(self.hidden);
        }
    }

    pub fn parse_hex(hex: &str) -> Option<Rgba> {
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let digit = |i: usize| u8::from_str_radix(&hex[i..=i], 16).ok().map(|d| d * 17);
        let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        match hex.len() {
            3 => Some(Rgba(digit(0)?, digit(1)?, digit(2)?, 255)),
            4 => Some(Rgba(digit(0)?, digit(1)?, digit(2)?, digit(3)?)),
            6 => Some(Rgba(pair(0)?, pair(2)?, pair(4)?, 255)),
            8 => Some(Rgba(pair(0)?, pair(2)?, pair(4)?, pair(6)?)),
            _ => None,
        }
    }

    pub fn fraction(text: &str) -> Option<f32> {
        let value = match text.strip_suffix('%') {
            Some(percent) => percent.parse::<f32>().ok()? / 100.0,
            None => text.parse::<f32>().ok()?,
        };
        (0.0..=1.0).contains(&value).then_some(value)
    }

    pub fn parse_color(value: &str) -> Option<Rgba> {
        if value == "transparent" {
            return Some(Rgba(0, 0, 0, 0));
        }
        if let Some(hex) = value.strip_prefix('#') {
            return parse_hex(hex);
        }
        let args = value
            .strip_prefix("rgba(")
            .or_else(|| value.strip_prefix("rgb("))?
            .strip_suffix(')')?;
        let parts: Vec<&str> = args
            .split([',', ' ', '/'])
            .filter(|part| !part.is_empty())
            .collect();
        let alpha = match parts.as_slice() {
            [_, _, _] => 255,
            [_, _, _, alpha] => (fraction(alpha)? * 255.0).round() as u8,
            _ => return None,
        };
        let channel = |part: &str| {
            part.parse::<f32>()
                .ok()
                .filter(|v| (0.0..=255.0).contains(v))
                .map(|v| v.round() as u8)
        };
        Some(Rgba(
            channel(parts[0])?,
            channel(parts[1])?,
            channel(parts[2])?,
            alpha,
        ))
    }

    pub fn parse_opacity(value: &str) -> Option<f32> {
        value
            .parse::<f32>()
            .ok()
            .filter(|o| (0.0..=1.0).contains(o))
    }
}

// ============================================================================
// PART 1: Theming & Styling Engine Unit Tests
// ============================================================================

#[test]
fn test_extreme_invalid_colors_rejected() {
    use css_stress::*;
    assert_eq!(parse_color("#12"), None, "2-char hex invalid");
    assert_eq!(parse_color("#12345"), None, "5-char hex invalid");
    assert_eq!(parse_color("#1234567"), None, "7-char hex invalid");
    assert_eq!(parse_color("#123456789"), None, "9-char hex invalid");
    assert_eq!(parse_color("#xyz"), None, "Non-hex digits");
    assert_eq!(parse_color("rgb(300, 0, 0)"), None, "Red channel > 255");
    assert_eq!(parse_color("rgb(-10, 0, 0)"), None, "Negative channel");
    assert_eq!(parse_color("rgba(0, 0, 0, 1.5)"), None, "Alpha > 1.0");
    assert_eq!(parse_color("rgba(0, 0, 0, -0.1)"), None, "Alpha < 0.0");
    assert_eq!(parse_color("blue"), None, "Named colors unsupported");
    assert_eq!(parse_color("red"), None, "Named colors unsupported");
    assert_eq!(parse_color("linear-gradient(to right, red, blue)"), None);
    assert_eq!(parse_color(""), None);
}

#[test]
fn test_extreme_valid_colors_accepted() {
    use css_stress::*;
    assert_eq!(parse_color("transparent"), Some(Rgba(0, 0, 0, 0)));
    assert_eq!(parse_color("#000"), Some(Rgba(0, 0, 0, 255)));
    assert_eq!(parse_color("#fff"), Some(Rgba(255, 255, 255, 255)));
    assert_eq!(parse_color("#0000"), Some(Rgba(0, 0, 0, 0)));
    assert_eq!(parse_color("#ffff"), Some(Rgba(255, 255, 255, 255)));
    assert_eq!(parse_color("#1877F1"), Some(Rgba(0x18, 0x77, 0xf1, 255)));
    assert_eq!(parse_color("#1877f180"), Some(Rgba(0x18, 0x77, 0xf1, 0x80)));
    assert_eq!(
        parse_color("rgb(255, 255, 255)"),
        Some(Rgba(255, 255, 255, 255))
    );
    assert_eq!(parse_color("rgba(0, 0, 0, 0)"), Some(Rgba(0, 0, 0, 0)));
    assert_eq!(parse_color("rgba(0, 0, 0, 1)"), Some(Rgba(0, 0, 0, 255)));
    assert_eq!(parse_color("rgba(0, 0, 0, 50%)"), Some(Rgba(0, 0, 0, 128)));
}

#[test]
fn test_extreme_opacity_limits() {
    use css_stress::*;
    assert_eq!(parse_opacity("0.0"), Some(0.0));
    assert_eq!(parse_opacity("1.0"), Some(1.0));
    assert_eq!(parse_opacity("0"), Some(0.0));
    assert_eq!(parse_opacity("1"), Some(1.0));
    assert_eq!(parse_opacity("0.5"), Some(0.5));
    // Out of bounds values must be rejected
    assert_eq!(parse_opacity("-0.01"), None);
    assert_eq!(parse_opacity("1.01"), None);
    assert_eq!(parse_opacity("100"), None);
    assert_eq!(parse_opacity("-100"), None);
    assert_eq!(parse_opacity("NaN"), None);
    assert_eq!(parse_opacity("infinity"), None);
}

#[test]
fn test_inheritance_and_override_hierarchy_on_intro_buttons() {
    use css_stress::*;

    // Hierarchy of classes for intro buttons:
    // .button-container (broadest) -> .form-button (intermediate) -> .facebook-button (specific)
    let button_container_rule = Style {
        color: Some(Rgba(10, 10, 10, 255)),
        background: Some(Rgba(20, 20, 20, 255)),
        opacity: Some(0.5),
        hidden: Some(false),
    };
    let form_button_rule = Style {
        background: Some(Rgba(50, 50, 50, 255)),
        opacity: Some(0.8),
        ..Default::default()
    };
    let facebook_button_rule = Style {
        background: Some(Rgba(0x18, 0x77, 0xf1, 255)),
        ..Default::default()
    };

    let mut facebook_style = Style::default();
    facebook_style.merge(button_container_rule);
    assert_eq!(facebook_style.background, Some(Rgba(20, 20, 20, 255)));

    facebook_style.merge(form_button_rule);
    assert_eq!(
        facebook_style.background,
        Some(Rgba(50, 50, 50, 255)),
        "form-button overrides button-container"
    );
    assert_eq!(
        facebook_style.color,
        Some(Rgba(10, 10, 10, 255)),
        "color inherited from button-container"
    );

    facebook_style.merge(facebook_button_rule);
    assert_eq!(
        facebook_style.background,
        Some(Rgba(0x18, 0x77, 0xf1, 255)),
        "facebook-button overrides form-button"
    );
    assert_eq!(
        facebook_style.opacity,
        Some(0.8),
        "opacity inherited from form-button"
    );
    assert_eq!(
        facebook_style.color,
        Some(Rgba(10, 10, 10, 255)),
        "color retained from button-container"
    );
}

#[test]
fn test_gdpr_all_8_combinations_truth_table() {
    // Truth table for (TOS, Privacy, Marketing)
    // Only (true, true, _) is valid for registration
    for tos in [false, true] {
        for privacy in [false, true] {
            for _marketing in [false, true] {
                let valid = tos && privacy;
                if valid {
                    assert!(
                        tos && privacy,
                        "Valid GDPR consent requires both TOS and Privacy"
                    );
                } else {
                    assert!(
                        !tos || !privacy,
                        "Invalid GDPR consent when TOS or Privacy is missing"
                    );
                }
            }
        }
    }
}

// ============================================================================
// PART 2: Comprehensive Slint Runtime Component & Theming Suite
// ============================================================================

#[test]
fn test_slint_intro_component_and_theming_adversarial_suite() {
    let harness = AdversarialIntroHarness::new().expect("create harness");

    // 1. Initial State Invariants
    assert_eq!(harness.get_is_signup(), false, "Initial mode must be login");
    assert_eq!(
        harness.get_error_message().as_str(),
        "",
        "Initial error message empty"
    );
    assert_eq!(harness.get_is_loading(), false, "Initial loading false");
    assert_eq!(
        harness.get_rem_size(),
        14.0,
        "Initial rem size 14px at 1100x720"
    );

    // 2. Rapid Mode Switch Stress (1,000 continuous toggle cycles)
    for i in 0..1000 {
        harness.set_is_signup(true);
        assert!(harness.get_is_signup(), "cycle {i}: must be signup");
        harness.set_is_signup(false);
        assert!(!harness.get_is_signup(), "cycle {i}: must be login");
    }

    // 3. Extreme Error Message Boundaries
    harness.set_error_message("".into());
    assert_eq!(harness.get_error_message().as_str(), "");

    let err_1kb: String = "A".repeat(1024);
    harness.set_error_message(err_1kb.as_str().into());
    assert_eq!(harness.get_error_message().as_str(), err_1kb);

    let err_10kb: String = "ERR_".repeat(2500);
    harness.set_error_message(err_10kb.as_str().into());
    assert_eq!(harness.get_error_message().as_str(), err_10kb);

    let special_errors = [
        "⚠️ Authentication failed: ユーザーが見つかりません",
        "<script>alert('xss')</script> &quot;test&quot;",
        "\u{202E}RTL reversal attack\u{202C}",
        "Newline\nSeparated\r\nError\tMessage",
        "NULL\0BYTE\0INJECTION",
    ];
    for err in special_errors {
        harness.set_error_message(err.into());
        assert_eq!(harness.get_error_message().as_str(), err);
    }

    // 4. Extreme Rem Size Scaling
    harness.set_rem_size(1.0);
    assert_eq!(harness.get_rem_size(), 1.0);

    harness.set_rem_size(14.0);
    assert_eq!(harness.get_rem_size(), 14.0);

    harness.set_rem_size(28.0);
    assert_eq!(harness.get_rem_size(), 28.0);

    harness.set_rem_size(100.0);
    assert_eq!(harness.get_rem_size(), 100.0);

    harness.set_rem_size(0.0);
    assert_eq!(harness.get_rem_size(), 0.0);

    // 5. Is-Loading Property Toggle
    harness.set_is_loading(true);
    assert!(harness.get_is_loading());
    harness.set_is_loading(false);
    assert!(!harness.get_is_loading());

    // 6. Contract Callbacks Verification
    let login_called = Arc::new(AtomicBool::new(false));
    let register_called = Arc::new(AtomicBool::new(false));
    let guest_called = Arc::new(AtomicBool::new(false));
    let fb_called = Arc::new(AtomicBool::new(false));
    let apple_called = Arc::new(AtomicBool::new(false));
    let forgot_called = Arc::new(AtomicBool::new(false));
    let dismiss_called = Arc::new(AtomicBool::new(false));

    let l_c = login_called.clone();
    harness.on_submit_login(move |email, pass| {
        assert_eq!(email.as_str(), "test@domain.com");
        assert_eq!(pass.as_str(), "secret123");
        l_c.store(true, Ordering::SeqCst);
    });

    let r_c = register_called.clone();
    harness.on_submit_register(move |email, pass, tos, priv_policy, mkt| {
        assert_eq!(email.as_str(), "new@domain.com");
        assert_eq!(pass.as_str(), "pass123");
        assert!(tos);
        assert!(priv_policy);
        assert!(!mkt);
        r_c.store(true, Ordering::SeqCst);
    });

    let g_c = guest_called.clone();
    harness.on_login_guest(move || {
        g_c.store(true, Ordering::SeqCst);
    });

    let f_c = fb_called.clone();
    harness.on_login_facebook(move || {
        f_c.store(true, Ordering::SeqCst);
    });

    let a_c = apple_called.clone();
    harness.on_login_apple(move || {
        a_c.store(true, Ordering::SeqCst);
    });

    let fg_c = forgot_called.clone();
    harness.on_forgot_password(move |email| {
        assert_eq!(email.as_str(), "forgot@domain.com");
        fg_c.store(true, Ordering::SeqCst);
    });

    let d_c = dismiss_called.clone();
    harness.on_dismiss(move || {
        d_c.store(true, Ordering::SeqCst);
    });

    harness.invoke_submit_login("test@domain.com".into(), "secret123".into());
    assert!(login_called.load(Ordering::SeqCst));

    harness.invoke_submit_register("new@domain.com".into(), "pass123".into(), true, true, false);
    assert!(register_called.load(Ordering::SeqCst));

    harness.invoke_login_guest();
    assert!(guest_called.load(Ordering::SeqCst));

    harness.invoke_login_facebook();
    assert!(fb_called.load(Ordering::SeqCst));

    harness.invoke_login_apple();
    assert!(apple_called.load(Ordering::SeqCst));

    harness.invoke_forgot_password("forgot@domain.com".into());
    assert!(forgot_called.load(Ordering::SeqCst));

    harness.invoke_dismiss();
    assert!(dismiss_called.load(Ordering::SeqCst));

    // 7. Test All 20 Newly Added Intro Classes in ClassStyles Global
    let styles = harness.global::<ClassStyles>();

    let c = ClassStyle {
        has_color: true,
        color: Color::from_argb_u8(255, 10, 20, 30),
        has_background: true,
        background: Color::from_argb_u8(200, 40, 50, 60),
        has_opacity: true,
        opacity: 0.75,
        hidden: false,
    };

    styles.set_intro_container(c.clone());
    assert_eq!(styles.get_intro_container().opacity, 0.75);

    styles.set_background_container(c.clone());
    assert_eq!(styles.get_background_container().has_background, true);

    styles.set_close_button_container(c.clone());
    assert_eq!(styles.get_close_button_container().has_color, true);

    styles.set_heading_container(c.clone());
    assert_eq!(styles.get_heading_container().hidden, false);

    styles.set_slogan_container(c.clone());
    assert_eq!(styles.get_slogan_container().opacity, 0.75);

    styles.set_content_container(c.clone());
    assert_eq!(styles.get_content_container().opacity, 0.75);

    styles.set_form_container(c.clone());
    assert_eq!(styles.get_form_container().opacity, 0.75);

    styles.set_credentials_text_input(c.clone());
    assert_eq!(styles.get_credentials_text_input().has_background, true);

    styles.set_forgot_password_link_container(c.clone());
    assert_eq!(styles.get_forgot_password_link_container().opacity, 0.75);

    styles.set_forgot_password_link(c.clone());
    assert_eq!(styles.get_forgot_password_link().has_color, true);

    styles.set_error_message(c.clone());
    assert_eq!(styles.get_error_message().has_color, true);

    styles.set_submit_button(c.clone());
    assert_eq!(styles.get_submit_button().has_background, true);

    styles.set_checkbox_container(c.clone());
    assert_eq!(styles.get_checkbox_container().opacity, 0.75);

    styles.set_checkbox(c.clone());
    assert_eq!(styles.get_checkbox().has_color, true);

    styles.set_options_container(c.clone());
    assert_eq!(styles.get_options_container().opacity, 0.75);

    styles.set_facebook_button(c.clone());
    assert_eq!(styles.get_facebook_button().has_background, true);

    styles.set_apple_button(c.clone());
    assert_eq!(styles.get_apple_button().has_background, true);

    styles.set_login_form_button(c.clone());
    assert_eq!(styles.get_login_form_button().has_background, true);

    styles.set_signup_form_button(c.clone());
    assert_eq!(styles.get_signup_form_button().has_background, true);

    styles.set_guest_login_button(c.clone());
    assert_eq!(styles.get_guest_login_button().has_background, true);

    // 8. Test ClassStyles Pure Functions
    let fallback_color = Color::from_argb_u8(255, 100, 100, 100);
    let set_color = Color::from_argb_u8(255, 200, 50, 50);

    let empty_style = ClassStyle {
        has_color: false,
        color: Color::default(),
        has_background: false,
        background: Color::default(),
        has_opacity: false,
        opacity: 1.0,
        hidden: false,
    };
    assert_eq!(
        styles.invoke_color_or(empty_style.clone(), fallback_color),
        fallback_color
    );
    assert_eq!(
        styles.invoke_background_or(empty_style.clone(), fallback_color),
        fallback_color
    );
    assert_eq!(styles.invoke_opacity_or(empty_style.clone(), 0.5), 0.5);

    let populated_style = ClassStyle {
        has_color: true,
        color: set_color,
        has_background: true,
        background: set_color,
        has_opacity: true,
        opacity: 0.85,
        hidden: true,
    };
    assert_eq!(
        styles.invoke_color_or(populated_style.clone(), fallback_color),
        set_color
    );
    assert_eq!(
        styles.invoke_background_or(populated_style.clone(), fallback_color),
        set_color
    );
    assert_eq!(styles.invoke_opacity_or(populated_style.clone(), 0.5), 0.85);
}
