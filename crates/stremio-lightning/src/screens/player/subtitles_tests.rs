use super::*;

fn facts(id: &str, language: &str, external: bool) -> SubtitleTrackFacts {
    SubtitleTrackFacts {
        id: id.into(),
        language: language.into(),
        external,
    }
}

fn loaded(id: &str, language: &str, external: bool, title: &str) -> TrackInfo {
    TrackInfo {
        id: id.into(),
        title: title.into(),
        language: language.into(),
        selected: false,
        external,
    }
}

fn offered(language: &str, title: &str, origin: &str) -> Subtitle {
    Subtitle {
        id: format!("{origin}:{title}"),
        title: title.into(),
        language: language.into(),
        origin: origin.into(),
        url: format!("https://example.com/{title}.srt"),
    }
}

#[test]
fn variants_and_languages_merge_file_tracks_and_addon_offers() {
    let candidates = [
        offered("spa", "", "OS"),
        offered("fre", "", "OS"),
        offered("eng", "SDH", "OS"),
        offered("ger", "Untertitel", "OS"),
    ];
    let codes: Vec<_> = subtitle_languages(&[], &candidates, &["ger", "en"])
        .into_iter()
        .map(|(c, _)| c)
        .collect();
    assert_eq!(codes, vec!["ger", "eng", "fre", "spa"]);

    let rows = subtitle_rows(&[loaded("1", "eng", false, "")], &candidates, "eng");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].title, "English");
    assert_eq!(
        (rows[1].title.as_str(), rows[1].origin.as_str()),
        ("SDH", "OS")
    );

    let fre = subtitle_rows(&[], &candidates, "fre");
    assert_eq!(fre[0].title, "Français");

    // Already-loaded external file is not duplicated.
    let dup = subtitle_rows(&[loaded("9", "eng", true, "SDH")], &candidates, "eng");
    assert_eq!(dup.len(), 1);
    assert_eq!(dup[0].id, "9");
}

#[test]
fn auto_subtitle_respects_choice_profile_and_embedded_priority() {
    let tracks = [
        facts("7", "eng", true),
        facts("3", "eng", false),
        facts("2", "ger", false),
    ];
    assert_eq!(
        auto_subtitle(&tracks, &SubtitleChoice::default(), "ger").as_deref(),
        Some("2")
    );
    assert_eq!(
        auto_subtitle(&tracks, &SubtitleChoice::default(), "en").as_deref(),
        Some("3")
    );
    assert_eq!(
        auto_subtitle(&tracks, &SubtitleChoice::picked("eng", true), "eng").as_deref(),
        Some("7")
    );
    assert_eq!(
        auto_subtitle(&tracks, &SubtitleChoice::picked("eng", false).off(), "eng"),
        None
    );
    assert_eq!(
        auto_subtitle(&[facts("1", "eng", false)], &SubtitleChoice::default(), ""),
        None
    );
    assert_eq!(
        auto_subtitle(
            &[facts("1", "eng", false)],
            &SubtitleChoice::picked("ger", false),
            ""
        )
        .as_deref(),
        Some("1")
    );
}

#[test]
fn browsed_language_follows_pressed_or_active_track() {
    assert_eq!(browsed_language("", "eng"), "eng");
    assert_eq!(browsed_language("spa", "eng"), "spa");
    assert_eq!(browsed_language("", ""), "");
}
