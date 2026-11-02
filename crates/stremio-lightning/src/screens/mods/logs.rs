//! The Logs page: one list of everything the app has recorded, newest first.
//!
//! Every source records into the app's one [`Logs`] recorder (in `plumbing::logs`): a line a
//! plugin logs, what a theme file says that this app cannot use, what an enable, a removal or a
//! failed load did, what became of the streaming server, and the app's own errors. This page reads
//! that list and decides what to show. It records nothing itself except what its own buttons do.
//!
//! The list is held in memory for this run of the app and never written to a file. Clear empties
//! it, which deletes nothing: it is the page's own copy of what happened, and the page says so.
//! The list holds at most [`CAPACITY`] records.

pub use crate::plumbing::logs::{Entry, Filter, Level, Source};

use crate::plumbing::logs::{Logs, CAPACITY};

/// What the page says above its list about where the records come from and how long they last.
pub const NOTE: &str = "One line per thing that happened, newest first. The list is kept in memory until the app closes, so nothing here is written to a file. Clear empties it and deletes nothing.";
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
    /// The source column: "Plugin", "Plugin a.plugin.js", "Theme red.theme.css", "Server".
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

/// One choice of a filter: the id the window passes back and the label it shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub id: String,
    pub label: String,
    pub selected: bool,
}

/// The filter's source choices, with the one picked marked. The first choice is every source.
pub fn sources(filter: &Filter) -> Vec<Choice> {
    let mut choices = vec![Choice {
        id: String::new(),
        label: "Every source".to_owned(),
        selected: filter.picked_source().is_none(),
    }];
    choices.extend(Source::ALL.into_iter().map(|source| Choice {
        id: source.id().to_owned(),
        label: source.label().to_owned(),
        selected: filter.picked_source() == Some(source),
    }));
    choices
}

/// The filter's level choices, with the one picked marked: the least level that is kept. A level
/// keeps its own records and every worse one, which is what the label says.
pub fn levels(filter: &Filter) -> Vec<Choice> {
    let mut choices = vec![Choice {
        id: String::new(),
        label: "Every level".to_owned(),
        selected: filter.least_level().is_none(),
    }];
    choices.extend(Level::ALL.into_iter().map(|level| Choice {
        id: level.id().to_owned(),
        label: match level {
            Level::Debug => "Debug and worse".to_owned(),
            Level::Info => "Info and worse".to_owned(),
            Level::Warn => "Warnings and errors".to_owned(),
            Level::Error => "Errors only".to_owned(),
        },
        selected: filter.least_level() == Some(level),
    }));
    choices
}

/// The header line: how many records are held, and how many the filter keeps of them.
pub fn held(logs: &Logs, filter: &Filter) -> String {
    let (held, shown) = (logs.len(), logs.matching(filter).len());
    let kept = match shown {
        0 => "The filter keeps none of them.".to_owned(),
        1 => "The filter keeps 1 of them.".to_owned(),
        shown => format!("The filter keeps {shown} of them."),
    };
    match held {
        0 => format!(
            "The app holds no records. It holds up to {} at once.",
            CAPACITY
        ),
        1 => format!("The app holds 1 record. {kept}"),
        held => format!("The app holds {held} of up to {} records. {kept}", CAPACITY),
    }
}

/// The rows the page lists: the records the filter keeps, newest first.
pub fn rows(logs: &Logs, filter: &Filter) -> Vec<Row> {
    logs.matching(filter).iter().map(Row::of).collect()
}

/// The full text of the record on screen, empty when none is open.
pub fn detail(logs: &Logs) -> Vec<String> {
    let Some(entry) = logs.detail() else {
        return Vec::new();
    };
    let mut lines = vec![format!("{} at {}", entry.source_label(), entry.at_shown())];
    lines.push(format!("Level: {}", entry.level.label()));
    // A plugin line can carry a stack trace, so the text keeps its own line breaks.
    lines.extend(entry.text.split('\n').map(str::to_owned));
    lines
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
    fn the_header_says_how_many_are_held_and_how_many_show() {
        let logs = Logs::new();
        assert!(held(&logs, &Filter::default()).contains("holds no records"));
        logs.record(Source::App, "", Level::Info, "one");
        logs.record(Source::App, "", Level::Error, "two");
        let all = held(&logs, &Filter::default());
        assert!(all.contains("holds 2 of up to 2000 records"), "{all}");
        assert!(all.contains("keeps 2 of them"), "{all}");
        let errors = Filter {
            level: "error".to_owned(),
            ..Filter::default()
        };
        assert!(held(&logs, &errors).contains("keeps 1 of them"));
    }

    #[test]
    fn the_filter_lists_every_source_and_level_first() {
        let filter = Filter::default();
        let sources = sources(&filter);
        assert_eq!(sources[0].label, "Every source");
        assert!(sources[0].selected);
        let listed = levels(&filter);
        assert_eq!(listed[0].label, "Every level");
        assert!(listed[0].selected);
        let picked = Filter {
            level: "warn".to_owned(),
            ..Filter::default()
        };
        let listed = levels(&picked);
        let chosen: Vec<&str> = listed
            .iter()
            .filter(|choice| choice.selected)
            .map(|choice| choice.label.as_str())
            .collect();
        assert_eq!(chosen, ["Warnings and errors"]);
    }

    #[test]
    fn a_row_is_one_line_and_the_detail_keeps_every_line() {
        let logs = Logs::new();
        let entry = logs.record(Source::Plugin, "a.plugin.js", Level::Error, "first\nsecond");
        let rows = rows(&logs, &Filter::default());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "first second");
        assert_eq!(rows[0].source, "Plugin a.plugin.js");
        assert_eq!(rows[0].level_id, "error");
        assert!(logs.set_detail(Some(entry.id)));
        let detail = detail(&logs);
        assert_eq!(detail[0], "Plugin a.plugin.js at 00:00:00");
        assert!(detail.contains(&"first".to_owned()));
        assert!(detail.contains(&"second".to_owned()));
    }

    #[test]
    fn the_clear_button_says_what_the_second_press_does() {
        assert_eq!(clear_label(false), "Clear");
        assert!(clear_label(true).starts_with("Press again"));
    }
}
