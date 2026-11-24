use super::*;
use crate::plumbing::skip::{Kind, Rate, Segment};

fn op_rate(id: &str, start: f64, end: f64) -> Rate {
    Rate {
        id: id.to_owned(),
        segment: Segment {
            kind: Kind::Opening,
            start,
            end,
        },
    }
}

#[test]
fn clock_and_parse_clock_round_trip() {
    assert_eq!(clock(0.0), "0:00");
    assert_eq!(clock(92.0), "1:32");
    assert_eq!(clock(3665.0), "1:01:05");
    assert_eq!(parse_clock("92"), Some(92.0));
    assert_eq!(parse_clock("1:32"), Some(92.0));
    assert_eq!(parse_clock("1:01:05"), Some(3665.0));
    assert_eq!(parse_clock("1:2:3:4"), None);
    assert_eq!(parse_clock("bad"), None);
}

#[test]
fn validate_submission_enforces_kind_format_and_bounds() {
    assert!(validate_submission("bogus", "0:10", "0:20", 100.0).is_err());
    assert!(validate_submission("op", "nope", "0:20", 100.0).is_err());
    assert!(validate_submission("op", "0:10", "0:20", 0.0).is_err());
    assert!(validate_submission("op", "0:30", "0:20", 100.0).is_err());
    assert!(validate_submission("op", "0:10", "2:00", 100.0).is_err());
    let ok = validate_submission("op", "1:30", "1:41", 100.0).unwrap();
    assert_eq!(ok.kind, Kind::Opening);
    assert_eq!(ok.length, 101.0);
}

#[test]
fn session_clear_fetch_reseed_and_overlay_transitions() {
    let mut s = SkipSession {
        generation: 3,
        video: "tt9054364:1:3".to_owned(),
        length: Some(1344),
        ..Default::default()
    };
    let rate = op_rate("id-1", 30.0, 115.0);
    assert!(s.accept_rates(3, "tt9054364:1:3", 1344, true, vec![rate.clone()]));
    assert_eq!(s.clear(), 4);
    assert_eq!(s.has_state(), (false, false));
    assert!(s.auto_times.is_none());

    // Fetch tolerance: <= 2s is current; > 2s bumps generation and asks to reset window.
    assert_eq!(
        s.prepare_fetch("v1", 1000),
        FetchPlan::Fetch {
            generation: 4,
            reset_window: false
        }
    );
    assert_eq!(s.prepare_fetch("v1", 1002), FetchPlan::AlreadyCurrent);
    assert_eq!(
        s.prepare_fetch("v1", 1010),
        FetchPlan::Fetch {
            generation: 5,
            reset_window: true
        }
    );

    // Reseed preserves user edits, overwrites auto-seeded times, and respects picked kind.
    let seeded = s.panel_for_rates(&[], "op", 0.0, true, true);
    assert_eq!(seeded.seed, Some(("0:00".into(), "0:00".into())));
    let fetched = vec![op_rate("id-2", 30.0, 115.0)];
    assert!(should_reseed(
        &fetched,
        "0:00",
        "0:00",
        s.auto_times.as_ref()
    ));
    assert!(!should_reseed(&[], "0:00", "0:00", s.auto_times.as_ref()));
    assert!(!should_reseed(
        &fetched,
        "0:45",
        "0:45",
        s.auto_times.as_ref()
    ));

    let ed_only = vec![Rate {
        id: "id-ed".into(),
        segment: Segment {
            kind: Kind::Ending,
            start: 1200.0,
            end: 1290.0,
        },
    }];
    s.kind.clear();
    let p1 = s.apply_fetched_panel(&ed_only, 0.0, "0:00", "0:00");
    assert_eq!(p1.kind, "ed");
    s.kind = "recap".into();
    let p2 = s.apply_fetched_panel(&ed_only, 0.0, "0:00", "0:00");
    assert_eq!(p2.kind, "recap");

    // Overlay shows marks/button only when skip_wanted is true.
    assert!(s.accept_rates(5, "v1", 1010, true, fetched));
    assert_eq!(
        s.sync_overlay_for_generation(5, 40.0, 1010.0, false),
        Some((false, None))
    );
    let (visible, marks) = s
        .sync_overlay_for_generation(5, 40.0, 1010.0, true)
        .unwrap();
    assert!(visible);
    assert_eq!(marks.unwrap().len(), 1);
    assert_eq!(s.skip_seek_target(40.0), Some(115.5));
}
