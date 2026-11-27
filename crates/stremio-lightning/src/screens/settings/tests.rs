use super::*;

#[test]
fn defaults_and_mutations_project_to_menus_and_player_config() {
    let profile = Profile::default();
    let mut extras = LocalExtras::default();
    let d = data_from_profile(&profile, &extras);
    assert_eq!(
        (d.user_email.as_str(), d.logged_in),
        ("Anonymous user", false)
    );
    assert_eq!(
        (
            d.ui_language.title.as_str(),
            d.subtitles_size.title.as_str()
        ),
        ("English", "100%")
    );
    assert_eq!(
        (d.seek_time.title.as_str(), d.caching.title.as_str()),
        ("10 seconds", "2 GB")
    );
    assert_eq!((d.interface_scale, d.subtitles_offset_val), (100, 5));
    assert!(d.binge_watching && d.hardware_decoding);

    let updated = apply_select_option(&profile.settings, &mut extras, "subtitlesSize", 3);
    let p2 = Profile {
        settings: updated.unwrap_or_else(|| profile.settings.clone()),
        ..profile
    };
    let d2 = data_from_profile(&p2, &extras);
    assert_eq!(
        (
            d2.subtitles_size.title.as_str(),
            d2.player_config.subtitles_size
        ),
        ("150%", 150)
    );

    let toggled = apply_toggle_option(&p2.settings, &mut extras, "hardwareDecoding", false);
    let p3 = Profile {
        settings: toggled.unwrap_or_else(|| p2.settings.clone()),
        ..p2
    };
    let d3 = data_from_profile(&p3, &extras);
    assert!(!d3.hardware_decoding && !d3.player_config.hardware_decoding);

    let _ = apply_select_option(&p3.settings, &mut extras, "caching", 3);
    let _ = apply_toggle_option(&p3.settings, &mut extras, "quitOnClose", true);
    let _ = apply_toggle_option(&p3.settings, &mut extras, "subtitlesBold", true);
    let _ = apply_toggle_option(&p3.settings, &mut extras, "audioPassthrough", true);
    let _ = apply_select_option(&p3.settings, &mut extras, "subtitlesOffset", 4);
    let d4 = data_from_profile(&p3, &extras);
    assert_eq!(
        (d4.caching.title.as_str(), d4.quit_on_close),
        ("10 GB", true)
    );
    assert!(d4.player_config.subtitles_bold && d4.player_config.audio_passthrough);
    assert_eq!(d4.player_config.subtitles_offset, 20);
}

#[test]
fn settings_search_matches_prefix_tokens_and_gates_offline_or_guest_rows() {
    let guest_offline = search_settings("pass", false, false);
    assert!(guest_offline.iter().any(|h| h.row == "passthrough"));
    assert!(!guest_offline.iter().any(|h| h.row == "password"));

    let authed_online = search_settings("pass", true, true);
    assert!(authed_online.iter().any(|h| h.row == "password"));

    let cache_offline = search_settings("cache", true, false);
    assert!(!cache_offline.iter().any(|h| h.row == "cache"));
    let cache_online = search_settings("cache", true, true);
    assert!(cache_online.iter().any(|h| h.row == "cache"));
}
