use std::collections::HashSet;

use super::*;

fn entry(id: u64, source: Source, origin: &str, level: Level, text: &str) -> Entry {
    Entry {
        id,
        at: 1_700_000_000 + id,
        source,
        origin: origin.to_owned(),
        level,
        text: text.to_owned(),
    }
}

#[test]
fn filters_by_source_level_and_search_and_resets_cleanly() {
    let entries = vec![
        entry(1, Source::Plugin, "builtin:aniskip", Level::Info, "started"),
        entry(2, Source::Plugin, "builtin:aniskip", Level::Error, "threw"),
        entry(
            3,
            Source::Theme,
            "red.theme.css",
            Level::Warn,
            "unsupported property",
        ),
        entry(4, Source::Server, "", Level::Info, "ready"),
    ];
    let mut sel = LogSelection::default();
    sel.set_source("plugin");
    assert_eq!(LogSelection::matching(&entries, sel.filter()).len(), 2);
    sel.set_level("error");
    let narrowed = LogSelection::matching(&entries, sel.filter());
    assert_eq!(narrowed.len(), 1);
    assert_eq!(narrowed[0].text, "threw");

    sel.reset_filter();
    assert!(!sel.filter().is_active());
    sel.set_search("UNSUPPORTED");
    assert_eq!(LogSelection::matching(&entries, sel.filter()).len(), 1);
    sel.set_source("nowhere");
    sel.set_level("loud");
    assert_eq!(sel.filter().source, "");
    assert_eq!(sel.filter().level, "");
}

#[test]
fn click_shift_ctrl_and_drag_select_in_visible_order() {
    let entries = vec![
        entry(1, Source::App, "Core", Level::Info, "row a"),
        entry(2, Source::App, "Core", Level::Info, "row b"),
        entry(3, Source::App, "Core", Level::Info, "row c"),
        entry(4, Source::App, "Core", Level::Info, "row d"),
    ];
    let (a, b, c, d) = (1, 2, 3, 4);
    let mut sel = LogSelection::default();

    sel.press_record(&entries, c, false, false);
    assert_eq!(*sel.selected_ids(), HashSet::from([c]));
    assert!(sel.detail(&entries).is_none());
    sel.release_record(&entries, c);
    assert_eq!(sel.detail(&entries).map(|e| e.id), Some(c));

    // Plain click again on the sole selected row toggles it off.
    sel.press_record(&entries, c, false, false);
    sel.release_record(&entries, c);
    assert!(sel.selected_ids().is_empty());
    assert!(sel.detail(&entries).is_none());

    // Shift-click range from anchor b -> d, then b -> a.
    sel.press_record(&entries, b, false, false);
    sel.release_record(&entries, b);
    sel.press_record(&entries, d, true, false);
    sel.release_record(&entries, d);
    assert_eq!(*sel.selected_ids(), HashSet::from([d, c, b]));
    assert_eq!(sel.detail(&entries).map(|e| e.id), Some(d));
    sel.press_record(&entries, a, true, false);
    sel.release_record(&entries, a);
    assert_eq!(*sel.selected_ids(), HashSet::from([b, a]));

    // Ctrl-click toggles individual rows.
    sel.press_record(&entries, d, false, true);
    sel.release_record(&entries, d);
    assert_eq!(*sel.selected_ids(), HashSet::from([d, b, a]));
    sel.press_record(&entries, b, false, true);
    sel.release_record(&entries, b);
    assert_eq!(*sel.selected_ids(), HashSet::from([d, a]));

    // Drag selects contiguous range and keeps detail deferred until release.
    sel.press_record(&entries, d, false, false);
    sel.drag_record(&entries, b);
    assert_eq!(*sel.selected_ids(), HashSet::from([d, c, b]));
    sel.release_record(&entries, b);
    assert_eq!(sel.detail(&entries).map(|e| e.id), Some(b));

    // Ctrl-drag adds / subtracts ranges.
    sel.press_record(&entries, a, false, true);
    sel.drag_record(&entries, b);
    sel.release_record(&entries, b);
    assert_eq!(*sel.selected_ids(), HashSet::from([d, c, b, a]));
    sel.press_record(&entries, c, false, true);
    sel.drag_record(&entries, b);
    sel.release_record(&entries, b);
    assert_eq!(*sel.selected_ids(), HashSet::from([d, a]));
}

#[test]
fn keyboard_step_and_select_all_respect_active_filter() {
    let mut sel = LogSelection::default();
    assert_eq!(sel.step_record(&[], 1, false), None);

    let entries = vec![
        entry(1, Source::App, "Core", Level::Info, "row a"),
        entry(2, Source::Theme, "t.css", Level::Warn, "row b"),
        entry(3, Source::App, "Core", Level::Error, "row c"),
    ];
    let (a, b, c) = (1, 2, 3);
    assert_eq!(sel.step_record(&entries, 1, false), Some(c));
    assert_eq!(sel.step_record(&entries, 1, true), Some(b));
    assert_eq!(sel.step_record(&entries, 1, true), Some(a));
    assert_eq!(sel.step_record(&entries, 1, true), Some(a));
    assert_eq!(*sel.selected_ids(), HashSet::from([c, b, a]));
    assert_eq!(sel.step_record(&entries, -1, false), Some(b));
    assert_eq!(*sel.selected_ids(), HashSet::from([b]));

    sel.set_source("app");
    sel.select_all(&entries);
    assert_eq!(*sel.selected_ids(), HashSet::from([c, a]));
    assert_eq!(sel.detail(&entries).map(|e| e.id), Some(c));
}
