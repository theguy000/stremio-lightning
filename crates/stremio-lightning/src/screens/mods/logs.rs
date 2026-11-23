//! The Logs page: one list of everything the app has recorded, newest first.
//! Pure projection from `&[Entry]` + `&LogSelection`; zero I/O or plumbing locks.

pub use super::log_selection::{Filter, LogSelection};
pub use crate::plumbing::logs::{Entry, Level, Source};

pub const NONE: &str = "Nothing has been recorded yet.";
pub const NONE_MATCH: &str = "No record matches the filter.";
pub const CLEAR: &str = "Clear";
pub const CLEAR_ARMED: &str = "Press again to empty the list";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub id: u64,
    pub source: String,
    pub level: String,
    pub level_id: String,
    pub time: String,
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

pub fn source_options() -> Vec<&'static str> {
    let mut labels = vec!["Every source"];
    labels.extend(Source::ALL.into_iter().map(Source::label));
    labels
}

pub fn level_options() -> Vec<&'static str> {
    vec![
        "Every level",
        "Debug and worse",
        "Info and worse",
        "Warnings and errors",
        "Errors only",
    ]
}

pub fn source_index(filter: &Filter) -> usize {
    row_of(&Source::ALL, filter.picked_source())
}

pub fn level_index(filter: &Filter) -> usize {
    row_of(&Level::ALL, filter.least_level())
}

pub fn source_at(index: usize) -> Option<Source> {
    index
        .checked_sub(1)
        .and_then(|at| Source::ALL.get(at))
        .copied()
}

pub fn level_at(index: usize) -> Option<Level> {
    index
        .checked_sub(1)
        .and_then(|at| Level::ALL.get(at))
        .copied()
}

pub fn pick_source(sel: &mut LogSelection, index: usize) {
    sel.set_source(source_at(index).map_or("", Source::id));
}

pub fn pick_level(sel: &mut LogSelection, index: usize) {
    sel.set_level(level_at(index).map_or("", Level::id));
}

fn row_of<T: PartialEq + Copy>(all: &[T], picked: Option<T>) -> usize {
    picked
        .and_then(|picked| all.iter().position(|choice| *choice == picked))
        .map_or(0, |at| at + 1)
}

pub fn rows(entries: &[Entry], sel: &LogSelection, filter: &Filter) -> Vec<Row> {
    let selected = sel.selected_ids();
    LogSelection::matching(entries, filter)
        .iter()
        .map(|entry| Row::of(entry, selected.contains(&entry.id)))
        .collect()
}

pub fn selected_count(entries: &[Entry], sel: &LogSelection, filter: &Filter) -> usize {
    sel.selected_in(entries, filter).len()
}

pub fn copy_text(
    entries: &[Entry],
    sel: &LogSelection,
    filter: &Filter,
) -> Option<(usize, String)> {
    let selected = sel.selected_in(entries, filter);
    if !selected.is_empty() {
        let count = selected.len();
        let text = selected
            .iter()
            .map(Entry::written)
            .collect::<Vec<_>>()
            .join("\n");
        return Some((count, text));
    }
    sel.detail(entries).map(|entry| (1, entry.written()))
}

pub fn detail_heading(entries: &[Entry], sel: &LogSelection) -> String {
    sel.detail(entries).map_or_else(String::new, |entry| {
        let base = format!(
            "{} at {} - {}",
            entry.source_label(),
            entry.at_shown(),
            entry.level.label()
        );
        let count = sel.selected_in(entries, sel.filter()).len();
        if count > 1 {
            format!("{count} records selected — {base}")
        } else {
            base
        }
    })
}

pub fn detail(entries: &[Entry], sel: &LogSelection) -> Vec<String> {
    sel.detail(entries).map_or_else(Vec::new, |entry| {
        entry.text.split('\n').map(str::to_owned).collect()
    })
}

pub fn open_id(entries: &[Entry], sel: &LogSelection) -> u64 {
    sel.detail(entries).map_or(0, |entry| entry.id)
}

pub fn active_index(rows: &[Row], open_id: u64) -> Option<usize> {
    (open_id != 0)
        .then(|| rows.iter().position(|row| row.id == open_id))
        .flatten()
}

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

pub fn clear_label(armed: bool) -> &'static str {
    if armed {
        CLEAR_ARMED
    } else {
        CLEAR
    }
}

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

    fn mk(id: u64, source: Source, origin: &str, level: Level, text: &str) -> Entry {
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
    fn filter_menus_roundtrip_and_pick_helpers() {
        assert_eq!(
            source_options(),
            ["Every source", "Plugin", "Theme", "Mods", "Server", "App"]
        );
        assert_eq!(level_options().len(), 5);
        for (at, source) in Source::ALL.iter().enumerate() {
            assert_eq!(source_at(at + 1), Some(*source));
            assert_eq!(source_options()[at + 1], source.label());
        }
        for (at, level) in Level::ALL.iter().enumerate() {
            assert_eq!(level_at(at + 1), Some(*level));
            assert_eq!(
                level_index(&Filter {
                    level: level.id().to_owned(),
                    ..Filter::default()
                }),
                at + 1
            );
        }
        let entries = vec![
            mk(1, Source::Plugin, "builtin:aniskip", Level::Info, "one"),
            mk(2, Source::Theme, "red.theme.css", Level::Warn, "two"),
        ];
        let mut sel = LogSelection::default();
        pick_source(&mut sel, 2);
        assert_eq!(rows(&entries, &sel, sel.filter()).len(), 1);
        pick_source(&mut sel, 0);
        assert_eq!(rows(&entries, &sel, sel.filter()).len(), 2);
    }

    #[test]
    fn multi_select_detail_copy_and_labels() {
        let entries = vec![
            mk(1, Source::App, "Core", Level::Info, "first\nline2"),
            mk(2, Source::Server, "", Level::Error, "second"),
            mk(3, Source::App, "Core", Level::Warn, "third"),
        ];
        let mut sel = LogSelection::default();
        sel.press_record(&entries, 3, false, false);
        sel.drag_record(&entries, 1);
        sel.release_record(&entries, 1);

        let all_rows = rows(&entries, &sel, &Filter::default());
        assert!(all_rows.iter().all(|r| r.selected));
        assert_eq!(all_rows[2].text, "first line2");
        assert_eq!(detail(&entries, &sel), ["first", "line2"]);
        assert!(detail_heading(&entries, &sel).starts_with("3 records selected — "));
        let (count, copied) = copy_text(&entries, &sel, &Filter::default()).unwrap();
        assert_eq!(count, 3);
        assert_eq!(
            copied,
            format!(
                "{}\n{}\n{}",
                entries[2].written(),
                entries[1].written(),
                entries[0].written()
            )
        );

        let only_app = Filter {
            source: "app".to_owned(),
            ..Filter::default()
        };
        assert_eq!(selected_count(&entries, &sel, &only_app), 2);
        assert_eq!(count_label(2, 5, true), "2 of 5");
        assert_eq!(active_index(&all_rows, 3), Some(0));
        assert_eq!(clear_label(false), CLEAR);
        assert_eq!(clear_label(true), CLEAR_ARMED);
    }
}
