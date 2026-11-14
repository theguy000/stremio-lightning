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
}

impl Row {
    fn of(entry: &Entry) -> Self {
        Self {
            id: entry.id,
            source: entry.source_label(),
            level: entry.level.label().to_owned(),
            level_id: entry.level.id().to_owned(),
            time: entry.at_shown(),
            text: one_line(&entry.text),
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

/// The row the source menu opens on, and the one the level menu opens on.
pub fn menu_indexes(filter: &Filter) -> (usize, usize) {
    (filter.source_index(), filter.level_index())
}

/// The rows the page lists: the records the filter keeps, newest first.
pub fn rows(logs: &Logs, filter: &Filter) -> Vec<Row> {
    logs.matching(filter).iter().map(Row::of).collect()
}

/// The heading of the record on screen: what it came from, when, and how bad it is. Empty when
/// nothing is open.
pub fn detail_heading(logs: &Logs) -> String {
    logs.detail().map_or_else(String::new, |entry| {
        format!(
            "{} at {} - {}",
            entry.source_label(),
            entry.at_shown(),
            entry.level.label()
        )
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
        assert_eq!(menu_indexes(&filter), (0, 0), "both open on every-thing");
        let picked = Filter {
            source: "theme".to_owned(),
            level: "warn".to_owned(),
            ..Filter::default()
        };
        assert_eq!(
            menu_indexes(&picked),
            (2, 3),
            "the menus open on the choices the filter holds"
        );
        // Every label a menu shows is the label the record's own column would carry.
        for source in Source::ALL {
            assert_eq!(
                source_options()[source.index()],
                source.label(),
                "the menu and the source column agree"
            );
        }
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
}
