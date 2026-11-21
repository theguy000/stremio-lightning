//! The Logs page: one list of everything the app has recorded, newest first.
//!
//! Every source records into the app's one [`Logs`] recorder (in `plumbing::logs`): a line a
//! plugin logs, what a theme file says that this app cannot use, what an enable, a removal or a
//! failed load did, what became of the streaming server, and the app's own errors. This page reads
//! that list and decides what to show. It records nothing itself except what its own buttons do.
//!
//! The list is held in memory for this run of the app and never written to a file. Clear empties
//! it, which deletes nothing: it is the page's own copy of what happened, and the page says so.
//! The list holds at most [`crate::plumbing::logs::CAPACITY`] records.

pub use crate::plumbing::logs::{Entry, Filter, Level, Source};

use crate::plumbing::logs::Logs;

/// What the page says when nothing is held at all.
pub const NONE: &str = "Nothing has been recorded yet.";
/// What the page says when records are held but the filter keeps none.
pub const NONE_MATCH: &str = "No record matches the filter.";
/// What the Clear button says before it is pressed: one press arms it and removes nothing.
pub const CLEAR: &str = "Clear";
/// What the Clear button says once it is armed: the second press is the one that empties the list.
pub const CLEAR_ARMED: &str = "Press again to empty the list";

/// One row, as the page draws it. The row names the record by its id, so a press on it is a press
/// on that record and not on whatever row has moved into its place since.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub id: u64,
    /// The source column: "Plugin", "Plugin builtin:aniskip", "Theme red.theme.css", "Server".
    pub source: String,
    pub level: String,
    /// The level as the filter names it, so a row can say whether it is bad news.
    pub level_id: String,
    pub time: String,
    /// The record's text, cut to one line: the full text is the detail.
    pub text: String,
    pub selected: bool,
}

impl Row {
    fn of(entry: &Entry, selected: bool) -> Self {
        Self {
            id: entry.id,
            source: entry.source_label(),
            level: entry.level.label().to_owned(),
            level_id: entry.level.id().to_owned(),
            time: entry.at_shown(),
            text: one_line(&entry.text),
            selected,
        }
    }
}

/// The source menu's choices, in the order it lists them: every source first.
pub fn source_options() -> Vec<&'static str> {
    let mut labels = vec!["Every source"];
    labels.extend(Source::ALL.into_iter().map(Source::label));
    labels
}

/// The level menu's choices: every level first, then the least level that is kept. A level keeps
/// its own records and every worse one, which is what the label says.
pub fn level_options() -> Vec<&'static str> {
    vec![
        "Every level",
        "Debug and worse",
        "Info and worse",
        "Warnings and errors",
        "Errors only",
    ]
}

/// The row a filter's source menu opens on: zero for every source.
pub fn source_index(filter: &Filter) -> usize {
    row_of(&Source::ALL, filter.picked_source())
}

/// The row a filter's level menu opens on: zero for every level.
pub fn level_index(filter: &Filter) -> usize {
    row_of(&Level::ALL, filter.least_level())
}

/// The source a menu row names, or `None` for "every source" and for a row past the list.
pub fn source_at(index: usize) -> Option<Source> {
    index
        .checked_sub(1)
        .and_then(|at| Source::ALL.get(at))
        .copied()
}

/// The level a menu row names as its least, or `None` for "every level" and for a row past the
/// list.
pub fn level_at(index: usize) -> Option<Level> {
    index
        .checked_sub(1)
        .and_then(|at| Level::ALL.get(at))
        .copied()
}

/// Applies a source menu row to the recorder's filter. Both menus speak rows, because that is what
/// the window's single-choice menu answers with; the recorder itself only ever hears a source.
pub fn pick_source(logs: &Logs, index: usize) {
    logs.set_source(source_at(index).map_or("", Source::id));
}

/// Applies a level menu row to the recorder's filter, as "the least level kept".
pub fn pick_level(logs: &Logs, index: usize) {
    logs.set_level(level_at(index).map_or("", Level::id));
}

/// A choice's row in its menu: zero for the "every" row, then one per choice. The one function the
/// two menus share, so a source row and a level row cannot be counted differently.
fn row_of<T: PartialEq + Copy>(all: &[T], picked: Option<T>) -> usize {
    picked
        .and_then(|picked| all.iter().position(|choice| *choice == picked))
        .map_or(0, |at| at + 1)
}

/// The rows the page lists: the records the filter keeps, newest first.
pub fn rows(logs: &Logs, filter: &Filter) -> Vec<Row> {
    let selected = logs.selected_ids();
    logs.matching(filter)
        .iter()
        .map(|entry| Row::of(entry, selected.contains(&entry.id)))
        .collect()
}

/// How many visible records under `filter` are selected.
pub fn selected_count(logs: &Logs, filter: &Filter) -> usize {
    logs.selected_in(filter).len()
}

/// Formats the active selection (or fallback open detail record) for the clipboard, returning the
/// number of copied records and the newline-joined text.
pub fn copy_text(logs: &Logs, filter: &Filter) -> Option<(usize, String)> {
    let entries = logs.selected_in(filter);
    if !entries.is_empty() {
        let count = entries.len();
        let text = entries
            .iter()
            .map(Entry::written)
            .collect::<Vec<_>>()
            .join("\n");
        return Some((count, text));
    }
    logs.detail().map(|entry| (1, entry.written()))
}

/// The heading of the record on screen: what it came from, when, and how bad it is. Empty when
/// nothing is open.
pub fn detail_heading(logs: &Logs) -> String {
    logs.detail().map_or_else(String::new, |entry| {
        let base = format!(
            "{} at {} - {}",
            entry.source_label(),
            entry.at_shown(),
            entry.level.label()
        );
        let count = logs.selected_in(&logs.filter()).len();
        if count > 1 {
            format!("{count} records selected — {base}")
        } else {
            base
        }
    })
}

/// The text of the record on screen, one entry per line, empty when none is open. A plugin line
/// can carry a stack trace, so its own line breaks are kept.
pub fn detail(logs: &Logs) -> Vec<String> {
    logs.detail().map_or_else(Vec::new, |entry| {
        entry.text.split('\n').map(str::to_owned).collect()
    })
}

/// The id of the record on screen, zero when none is: what the list marks as the open row.
pub fn open_id(logs: &Logs) -> u64 {
    logs.detail().map_or(0, |entry| entry.id)
}

/// The row index of `open_id` in `rows`, if visible.
pub fn active_index(rows: &[Row], open_id: u64) -> Option<usize> {
    (open_id != 0)
        .then(|| rows.iter().position(|row| row.id == open_id))
        .flatten()
}

/// Compact count badge for the filter row: empty when nothing is logged, `"N records"` when
/// unfiltered, or `"shown of total"` while a filter narrows the list.
pub fn count_label(shown: usize, total: usize, filtered: bool) -> String {
    if total == 0 {
        String::new()
    } else if filtered {
        format!("{shown} of {total}")
    } else if total == 1 {
        "1 record".to_owned()
    } else {
        format!("{total} records")
    }
}

/// The Clear button's label: armed says what the second press does.
pub fn clear_label(armed: bool) -> &'static str {
    if armed {
        CLEAR_ARMED
    } else {
        CLEAR
    }
}

/// A record's text on one row: its line breaks become spaces and it is cut to what a row shows.
fn one_line(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    const SHOWN: usize = 240;
    if flat.chars().count() <= SHOWN {
        return flat;
    }
    let cut: String = flat.chars().take(SHOWN).collect();
    format!("{cut}...")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_filter_menus_list_every_choice_and_open_on_the_picked_one() {
        let filter = Filter::default();
        assert_eq!(
            source_options(),
            ["Every source", "Plugin", "Theme", "Mods", "Server", "App"]
        );
        assert_eq!(
            level_options(),
            [
                "Every level",
                "Debug and worse",
                "Info and worse",
                "Warnings and errors",
                "Errors only"
            ]
        );
        assert_eq!((source_index(&filter), level_index(&filter)), (0, 0));
        let picked = Filter {
            source: "theme".to_owned(),
            level: "warn".to_owned(),
            ..Filter::default()
        };
        assert_eq!((source_index(&picked), level_index(&picked)), (2, 3));

        // Every row names the choice its label names, both ways round, and every label a menu
        // shows is the label the record's own column would carry.
        for (at, source) in Source::ALL.iter().enumerate() {
            assert_eq!(source_at(at + 1), Some(*source));
            assert_eq!(source_options()[at + 1], source.label());
        }
        assert_eq!(source_at(0), None);
        assert_eq!(source_at(Source::ALL.len() + 1), None);
        for (at, level) in Level::ALL.iter().enumerate() {
            assert_eq!(
                level_at(at + 1),
                Some(*level),
                "the level menu's row {at} names the level the page keeps from"
            );
            assert_eq!(
                level_index(&Filter {
                    level: level.id().to_owned(),
                    ..Filter::default()
                }),
                at + 1
            );
        }
        assert_eq!(level_at(0), None);
    }

    #[test]
    fn a_menu_row_narrows_the_recorder_and_every_row_widens_it_back() {
        let logs = Logs::new();
        logs.record(Source::Plugin, "builtin:aniskip", Level::Info, "one");
        logs.record(Source::Theme, "red.theme.css", Level::Warn, "two");
        pick_source(&logs, 2);
        assert_eq!(logs.filter().picked_source(), Some(Source::Theme));
        assert_eq!(rows(&logs, &logs.filter()).len(), 1);
        pick_level(&logs, 3);
        assert_eq!(logs.filter().least_level(), Some(Level::Warn));
        pick_source(&logs, 0);
        pick_level(&logs, 0);
        assert_eq!(logs.filter(), Filter::default());
        assert_eq!(rows(&logs, &logs.filter()).len(), 2);
    }

    #[test]
    fn a_row_is_one_line_and_the_detail_keeps_every_line() {
        let logs = Logs::new();
        let entry = logs.record(
            Source::Plugin,
            "builtin:aniskip",
            Level::Error,
            "first\nsecond",
        );
        let rows = rows(&logs, &Filter::default());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "first second");
        assert_eq!(rows[0].source, "Plugin builtin:aniskip");
        assert_eq!(rows[0].level_id, "error");
        assert!(logs.set_detail(Some(entry.id)));
        assert_eq!(
            detail_heading(&logs),
            format!("Plugin builtin:aniskip at {} - Error", entry.at_shown()),
            "the open record is named by its source, the time and its level"
        );
        assert_eq!(detail(&logs), ["first", "second"], "every line is kept");
        assert_eq!(open_id(&logs), entry.id, "the list marks this row");
    }

    #[test]
    fn the_clear_button_says_what_the_second_press_does() {
        assert_eq!(clear_label(false), "Clear");
        assert!(clear_label(true).starts_with("Press again"));
    }

    #[test]
    fn multi_selected_rows_format_heading_and_copy_text_for_visible_records() {
        let logs = Logs::new();
        let one = logs.record(Source::App, "Core", Level::Info, "first");
        let two = logs.record(Source::Server, "", Level::Error, "second");
        let three = logs.record(Source::App, "Core", Level::Warn, "third");

        logs.press_record(three.id, false, false);
        logs.drag_record(one.id);
        logs.release_record(one.id);

        let all_rows = rows(&logs, &Filter::default());
        assert!(all_rows.iter().all(|row| row.selected));
        assert_eq!(selected_count(&logs, &Filter::default()), 3);
        assert!(detail_heading(&logs).starts_with("3 records selected — "));

        let (count, copied) = copy_text(&logs, &Filter::default()).expect("selected rows copy");
        assert_eq!(count, 3);
        assert_eq!(
            copied,
            format!("{}\n{}\n{}", three.written(), two.written(), one.written())
        );

        let only_app = Filter {
            source: "app".to_owned(),
            ..Filter::default()
        };
        assert_eq!(selected_count(&logs, &only_app), 2);
        let (filtered_count, filtered_copied) =
            copy_text(&logs, &only_app).expect("filtered selected rows copy");
        assert_eq!(filtered_count, 2);
        assert_eq!(
            filtered_copied,
            format!("{}\n{}", three.written(), one.written())
        );
    }

    #[test]
    fn count_label_and_active_index_reflect_filter_and_open_record() {
        assert_eq!(count_label(0, 0, false), "");
        assert_eq!(count_label(1, 1, false), "1 record");
        assert_eq!(count_label(5, 5, false), "5 records");
        assert_eq!(count_label(2, 5, true), "2 of 5");
        assert_eq!(count_label(0, 5, true), "0 of 5");

        let logs = Logs::new();
        let one = logs.record(Source::App, "", Level::Info, "first");
        let two = logs.record(Source::App, "", Level::Info, "second");
        let listed = rows(&logs, &Filter::default());
        assert_eq!(active_index(&listed, 0), None);
        assert_eq!(active_index(&listed, two.id), Some(0));
        assert_eq!(active_index(&listed, one.id), Some(1));
    }
}
