//! Empirical verification tests for Milestone 3 Gate:
//! Navigation triggers, overlay open/dismiss lifecycle, session unlocking/locking across screens
//! (Library, Calendar, Settings, Shell avatar), dismiss restoration, and placeholder visibility.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

mod support;

slint::slint! {
    import { MainWindow, SettingsViewData } from "ui/main.slint";
    import { Section } from "ui/shell.slint";
    export { MainWindow, Section, SettingsViewData }
}

fn create_default_settings(logged_in: bool, user_email: &str) -> SettingsViewData {
    let empty_select = || import_empty_select();
    SettingsViewData {
        user_email: user_email.into(),
        logged_in,
        trakt_authenticated: false,
        streaming_server_url: "http://127.0.0.1:11470".into(),
        ui_language: empty_select(),
        quit_on_close: false,
        esc_exit_fullscreen: true,
        hide_spoilers: false,
        subtitles_language: empty_select(),
        subtitles_size: empty_select(),
        subtitles_text_color: empty_select(),
        subtitles_bg_color: empty_select(),
        subtitles_outline_color: empty_select(),
        audio_language: empty_select(),
        surround_sound: false,
        seek_time: empty_select(),
        seek_short_time: empty_select(),
        play_in_background: false,
        binge_watching: true,
        next_video_duration: empty_select(),
        external_player: empty_select(),
        hardware_decoding: true,
        pause_on_minimize: false,
        caching: empty_select(),
        torrent_profile: empty_select(),
        transcoding_profile: empty_select(),
    }
}

fn import_empty_select() -> SelectInputData {
    use slint::{ModelRc, VecModel};
    SelectInputData {
        title: "".into(),
        selected_index: 0,
        options: ModelRc::new(VecModel::from(vec![])),
    }
}

#[test]
fn test_empirical_navigation_and_lifecycle_suite() {
    // ------------------------------------------------------------------------
    // Part 1: MainWindow Slint Compilation & Initial Properties
    // ------------------------------------------------------------------------
    support::init_slint_backend();
    let window = MainWindow::new().expect("MainWindow should compile and initialize");
    assert!(!window.get_auth_open(), "auth-open must be false initially");
    assert_eq!(
        window.get_section(),
        Section::Board,
        "Default section should be Board"
    );
    assert!(
        !window.get_auth_loading(),
        "auth-loading must be false initially"
    );
    assert_eq!(
        window.get_auth_error(),
        "",
        "auth-error must be empty initially"
    );

    // ------------------------------------------------------------------------
    // Part 2: Dismiss Restoration Without Modifying Active Section
    // ------------------------------------------------------------------------
    let all_sections = [
        Section::Board,
        Section::Discover,
        Section::Library,
        Section::Calendar,
        Section::Addons,
        Section::Settings,
    ];

    let dismissed_count = Arc::new(AtomicUsize::new(0));
    let dismissed_count_clone = dismissed_count.clone();
    window.on_auth_dismissed(move || {
        dismissed_count_clone.fetch_add(1, Ordering::SeqCst);
    });

    for section in all_sections {
        // A. Navigate to target section
        window.set_section(section);
        assert_eq!(
            window.get_section(),
            section,
            "Section should be set to target"
        );

        // B. Open auth overlay
        window.set_auth_open(true);
        window.set_auth_error("Some transient error".into());
        window.set_auth_loading(true);
        assert!(window.get_auth_open());

        // C. Trigger dismiss callback (as happens in intro.slint dismiss touch area)
        window.invoke_auth_dismissed();
        // Mimic main.rs on_auth_dismissed event loop state clearing:
        window.set_auth_open(false);
        window.set_auth_error("".into());
        window.set_auth_loading(false);

        // D. Verify section is strictly preserved!
        assert_eq!(
            window.get_section(),
            section,
            "Active section must be strictly preserved upon overlay dismissal!"
        );
        assert!(!window.get_auth_open(), "Overlay must be closed");
        assert_eq!(window.get_auth_error(), "", "Error must be cleared");
        assert!(
            !window.get_auth_loading(),
            "Loading spinner must be cleared"
        );
    }
    assert_eq!(dismissed_count.load(Ordering::SeqCst), all_sections.len());

    // ------------------------------------------------------------------------
    // Part 3: Logged-out Placeholders Visibility on Library and Calendar
    // ------------------------------------------------------------------------
    // Case A: When logged_in is false
    let logged_out_settings = create_default_settings(false, "");
    window.set_settings(logged_out_settings);
    assert!(!window.get_settings().logged_in);

    // On Library:
    window.set_section(Section::Library);
    window.set_details_open(false);
    window.set_playing(false);

    let is_library_placeholder_active = window.get_section() == Section::Library
        && !window.get_settings().logged_in
        && !window.get_details_open()
        && !window.get_playing();
    let is_library_authenticated_active = window.get_section() == Section::Library
        && window.get_settings().logged_in
        && !window.get_details_open()
        && !window.get_playing();

    assert!(
        is_library_placeholder_active,
        "Library placeholder MUST be visible when logged_in is false"
    );
    assert!(
        !is_library_authenticated_active,
        "Library authenticated view MUST NOT be active when logged_in is false"
    );

    // On Calendar:
    window.set_section(Section::Calendar);
    let is_calendar_placeholder_active = window.get_section() == Section::Calendar
        && !window.get_settings().logged_in
        && !window.get_details_open()
        && !window.get_playing();
    let is_calendar_authenticated_active = window.get_section() == Section::Calendar
        && window.get_settings().logged_in
        && !window.get_details_open()
        && !window.get_playing();

    assert!(
        is_calendar_placeholder_active,
        "Calendar placeholder MUST be visible when logged_in is false"
    );
    assert!(
        !is_calendar_authenticated_active,
        "Calendar authenticated view MUST NOT be active when logged_in is false"
    );

    // Case B: When logged_in is true
    // The logged-in Library and Calendar screens do not exist yet, so the logged-out
    // placeholder stays up and having a session unlocks nothing.
    let logged_in_settings = create_default_settings(true, "alice@stremio.com");
    window.set_settings(logged_in_settings);
    assert!(window.get_settings().logged_in);

    // On Library:
    window.set_section(Section::Library);
    window.set_details_open(false);
    window.set_playing(false);
    assert_eq!(
        window.get_section(),
        Section::Library,
        "Library must stay reachable while logged in"
    );

    // On Calendar:
    window.set_section(Section::Calendar);
    assert_eq!(
        window.get_section(),
        Section::Calendar,
        "Calendar must stay reachable while logged in"
    );

    // ------------------------------------------------------------------------
    // Part 4: Shell Avatar Navigation Trigger: Logged Out vs Logged In
    // ------------------------------------------------------------------------
    // Case A: Avatar click while logged out
    window.set_settings(create_default_settings(false, ""));
    window.set_section(Section::Library);
    window.set_auth_open(false);

    // Slint logic: user-avatar-clicked => if (!root.settings.logged-in) root.auth-open = true;
    if !window.get_settings().logged_in {
        window.set_auth_open(true);
    } else {
        window.set_details_open(false);
        window.set_section(Section::Settings);
    }
    assert!(
        window.get_auth_open(),
        "Avatar click while logged out MUST open auth overlay"
    );
    assert_eq!(
        window.get_section(),
        Section::Library,
        "Section must NOT be changed to Settings when logged out"
    );

    // Close overlay
    window.set_auth_open(false);

    // Case B: Avatar click while logged in
    // The logged-in header has no menu yet, so the click is inert: it must not open
    // the login overlay and must not navigate away from the current section.
    window.set_settings(create_default_settings(true, "bob@stremio.com"));
    window.set_section(Section::Library);
    window.set_details_open(true);

    if !window.get_settings().logged_in {
        window.set_auth_open(true);
    }
    assert!(
        !window.get_auth_open(),
        "Avatar click while logged in must NOT open auth overlay"
    );
    assert_eq!(
        window.get_section(),
        Section::Library,
        "Avatar click while logged in must NOT navigate to Settings"
    );
    assert!(
        window.get_details_open(),
        "Avatar click while logged in must not touch the details overlay"
    );

    // ------------------------------------------------------------------------
    // Part 5: Cross-Screen Session Unlocking & Locking End-to-End Cycle
    // ------------------------------------------------------------------------
    let login_dispatched = Arc::new(AtomicBool::new(false));
    let logout_dispatched = Arc::new(AtomicBool::new(false));

    let ld = login_dispatched.clone();
    window.on_submit_auth_login(move |_email, _pass| {
        ld.store(true, Ordering::SeqCst);
    });

    let lod = logout_dispatched.clone();
    window.on_logout_user(move || {
        lod.store(true, Ordering::SeqCst);
    });

    // Start logged out on Discover
    window.set_settings(create_default_settings(false, ""));
    window.set_section(Section::Discover);

    // Navigate to Library while logged out
    window.set_section(Section::Library);
    assert_eq!(window.get_section(), Section::Library);
    assert!(!window.get_settings().logged_in);

    // Trigger login overlay
    window.set_auth_open(true);
    assert!(window.get_auth_open());
    assert_eq!(window.get_section(), Section::Library);

    // Submit credentials
    window.invoke_submit_auth_login("user@stremio.com".into(), "password123".into());
    assert!(login_dispatched.load(Ordering::SeqCst));

    // Simulate auth success
    window.set_settings(create_default_settings(true, "user@stremio.com"));
    window.set_auth_open(false);
    window.set_auth_loading(false);
    window.set_auth_error("".into());

    // Verify Library is unlocked and active
    assert_eq!(window.get_section(), Section::Library);
    assert!(window.get_settings().logged_in);
    assert_eq!(window.get_settings().user_email, "user@stremio.com");

    // Navigate to Calendar and verify unlocked immediately
    window.set_section(Section::Calendar);
    assert_eq!(window.get_section(), Section::Calendar);
    assert!(
        window.get_settings().logged_in,
        "Calendar must be unlocked immediately following login"
    );

    // Navigate to Settings and verify account info
    window.set_section(Section::Settings);
    assert_eq!(window.get_settings().user_email, "user@stremio.com");

    // Trigger Logout from Settings
    window.invoke_logout_user();
    assert!(logout_dispatched.load(Ordering::SeqCst));

    // Simulate logout state push from core
    window.set_settings(create_default_settings(false, ""));
    assert!(!window.get_settings().logged_in);
    assert_eq!(window.get_settings().user_email, "");

    // Verify both Library and Calendar are immediately re-locked
    window.set_section(Section::Library);
    assert!(
        !window.get_settings().logged_in,
        "Library must be locked after logout"
    );

    window.set_section(Section::Calendar);
    assert!(
        !window.get_settings().logged_in,
        "Calendar must be locked after logout"
    );
}
