//! Tests for the three kinds of code the native client is made of.
//!
//! These replace three suites that asserted on copies of production logic: one kept its own copy
//! of the sign-in error wording, one its own copy of the link table, one reimplemented the CSS
//! parser. Those could not fail when the app broke. Everything here calls the app itself, with no
//! window, no display and no network.

use stremio_lightning_native::{auth, links, themes};

// ============================================================================
// What a sign-in failure says
// ============================================================================

#[test]
fn sign_in_failures_read_as_instructions_a_person_can_follow() {
    use stremio_core::models::ctx::{CtxError, OtherError};
    use stremio_core::runtime::EnvError;
    use stremio_core::types::api::APIError;

    let api = |message: &str| {
        auth::error_message(&CtxError::API(APIError {
            message: message.to_string(),
            code: 1,
        }))
    };

    // The three answers that do not read like a login form get rewritten...
    assert_eq!(api("Wrong passphrase"), "Wrong password. Please try again.");
    assert_eq!(
        api("Wrong passphrase (hint: account created with Facebook login, you can try logging in with Facebook)"),
        "Wrong password. This account was created with Facebook, so log in with the Facebook button instead."
    );
    assert_eq!(
        api("User not found"),
        "No account uses that email address. Check the spelling, or sign up."
    );
    // ...and one that already reads like one is left alone.
    assert_eq!(
        api("User with this email already exists"),
        "User with this email already exists"
    );

    assert_eq!(
        auth::error_message(&CtxError::Env(EnvError::StorageUnavailable)),
        "Storage is not available"
    );
    assert_eq!(
        auth::error_message(&CtxError::Other(OtherError::UserNotLoggedIn)),
        "User is not logged in"
    );
}

#[test]
fn a_sign_in_attempt_can_no_longer_speak_once_it_is_superseded_or_dismissed() {
    let generation = auth::Generation::default();
    let attempt = generation.begin();
    assert!(generation.is_current(attempt));

    // A newer attempt takes over, which silences the first.
    let newer = generation.begin();
    assert!(!generation.is_current(attempt));
    assert!(generation.is_current(newer));

    // Dismissing the screen silences whatever is in flight.
    generation.cancel();
    assert!(!generation.is_current(newer));
}

// ============================================================================
// Which addresses the app will open
// ============================================================================

#[test]
fn every_link_the_settings_screen_offers_resolves_to_an_address() {
    for kind in ["support", "source", "tos", "privacy", "reset-password"] {
        let url = links::resolve(kind).unwrap_or_else(|| panic!("{kind} resolves"));
        assert!(url.starts_with("https://"), "{kind} is a secure address");
    }
    // The web intro never signed anyone in, so nothing points at it.
    assert_eq!(links::resolve("web-intro"), None);
    assert_eq!(links::resolve("nonsense"), None);
    assert_eq!(links::resolve("file:///etc/passwd"), None);
}

#[test]
fn an_address_the_ui_passes_through_is_opened_as_given() {
    let given = "https://www.strem.io/reset-password/?email=user%40test.com";
    assert_eq!(links::resolve(given), Some(given.to_string()));
}

// ============================================================================
// What a theme file means
// ============================================================================

#[test]
fn a_theme_restyles_the_component_it_names_and_leaves_the_others_alone() {
    let theme = themes::inspect(
        "/** @name Demo */\n\
         .title-label { color: #fff; opacity: 0.5 !important }\n\
         [class*=\"see-all\"] { visibility: hidden }\n\
         .nothing-like-this { color: #fff }",
    );

    let title = theme
        .style_of("title-label")
        .expect("the title label exists");
    assert_eq!(title.color, Some(themes::Rgba(255, 255, 255, 255)));
    assert_eq!(title.opacity, Some(0.5));

    let see_all = theme.style_of("see-all-container").expect("see-all exists");
    assert_eq!(see_all.hidden, Some(true));

    // An untouched component keeps the app's own styling.
    assert_eq!(theme.style_of("board-row"), Some(themes::Style::default()));

    // A selector that reaches nothing is reported, not silently ignored.
    assert_eq!(theme.warnings.len(), 1, "{:?}", theme.warnings);

    // And an id this app does not have is not invented.
    assert_eq!(theme.style_of("no-such-component"), None);
}
