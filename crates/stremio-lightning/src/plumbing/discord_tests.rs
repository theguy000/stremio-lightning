use super::*;

#[test]
fn builds_watching_paused_and_debounces_five_second_drift() {
    let playing = PresenceInput {
        title: "Severance - Good News About Hell (1x1)".into(),
        image: Some("https://img.example/poster.jpg".into()),
        paused: false,
        time_secs: Some(60.0),
        duration_secs: Some(3600.0),
    };
    let a = build_activity(&playing, 10_000, None).unwrap();
    assert_eq!(a.state, "Watching");
    assert_eq!(a.large_image, "https://img.example/poster.jpg");
    assert_eq!(a.start_timestamp, Some(9_940));
    assert_eq!(a.end_timestamp, Some(13_540));

    // 2 seconds later at t=62s -> same start/end within 5s drift
    let next = PresenceInput {
        time_secs: Some(62.0),
        ..playing.clone()
    };
    let b = build_activity(&next, 10_002, Some(&a)).unwrap();
    assert_eq!(b, a);

    // Seek jump > 5s updates timestamps
    let seeked = PresenceInput {
        time_secs: Some(600.0),
        ..playing.clone()
    };
    let c = build_activity(&seeked, 10_003, Some(&a)).unwrap();
    assert_eq!(c.start_timestamp, Some(9_403));

    // Paused formats time and drops timestamps
    let paused = PresenceInput {
        paused: true,
        image: None,
        ..playing
    };
    let p = build_activity(&paused, 10_010, Some(&c)).unwrap();
    assert_eq!(p.state, "Paused at 01:00 / 01:00:00");
    assert_eq!(p.large_image, "stremio_logo");
    assert_eq!(p.start_timestamp, None);
    assert_eq!(p.end_timestamp, None);
}
