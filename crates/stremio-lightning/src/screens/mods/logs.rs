//! The Logs page: pure projection from `&[Entry]` + `&LogSelection` into `logs::Data`.

pub use super::log_selection::{Filter, LogSelection};
pub use crate::plumbing::logs::{Entry, Level, Source};

pub const NONE: &str = "Nothing has been recorded yet.";
pub const NONE_MATCH: &str = "No record matches the filter.";

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

/// Complete view snapshot of the Mods -> Logs page.
#[derive(Clone, Debug)]
pub struct Data {
    pub filter: Filter,
    pub source_options: Vec<String>,
    pub level_options: Vec<String>,
    pub source_index: i32,
    pub level_index: i32,
    pub rows: Vec<Row>,
    pub empty: String,
    pub detail_heading: String,
    pub detail: Vec<String>,
    pub open_id: u64,
    pub active_index: i32,
    pub selected_count: usize,
    pub count: String,
    pub filtered: bool,
    pub clear_armed: bool,
}

impl Default for Data {
    fn default() -> Self {
        Self {
            filter: Filter::default(),
            source_options: source_options().into_iter().map(str::to_owned).collect(),
            level_options: level_options().into_iter().map(str::to_owned).collect(),
            source_index: 0,
            level_index: 0,
            rows: Vec::new(),
            empty: NONE.to_owned(),
            detail_heading: String::new(),
            detail: Vec::new(),
            open_id: 0,
            active_index: -1,
            selected_count: 0,
            count: String::new(),
            filtered: false,
            clear_armed: false,
        }
    }
}

/// Projects the current log records and selection state into `logs::Data` in a single pass.
pub fn data(entries: &[Entry], sel: &LogSelection, armed: bool) -> Data {
    let filter = sel.filter().clone();
    let rows = rows(entries, sel, &filter);
    let total = entries.len();
    let filtered = filter.is_active();
    let selected_count = rows.iter().filter(|r| r.selected).count();
    let detail_entry = sel.detail(entries);
    let open_id = detail_entry.as_ref().map_or(0, |e| e.id);
    let detail_heading = detail_entry.as_ref().map_or_else(String::new, |e| {
        let base = format!(
            "{} at {} - {}",
            e.source_label(),
            e.at_shown(),
            e.level.label()
        );
        if selected_count > 1 {
            format!("{selected_count} records selected — {base}")
        } else {
            base
        }
    });
    let detail = detail_entry.map_or_else(Vec::new, |e| {
        e.text.split('\n').map(str::to_owned).collect()
    });
    let active_index = active_index(&rows, open_id)
        .and_then(|at| i32::try_from(at).ok())
        .unwrap_or(-1);
    Data {
        source_index: i32::try_from(source_index(&filter)).unwrap_or(0),
        level_index: i32::try_from(level_index(&filter)).unwrap_or(0),
        count: count_label(rows.len(), total, filtered),
        empty: if total == 0 { NONE } else { NONE_MATCH }.to_owned(),
        filter,
        source_options: source_options().into_iter().map(str::to_owned).collect(),
        level_options: level_options().into_iter().map(str::to_owned).collect(),
        rows,
        detail_heading,
        detail,
        open_id,
        active_index,
        selected_count,
        filtered,
        clear_armed: armed,
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

fn source_at(index: usize) -> Option<Source> {
    index
        .checked_sub(1)
        .and_then(|at| Source::ALL.get(at))
        .copied()
}

fn level_at(index: usize) -> Option<Level> {
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
        .and_then(|p| all.iter().position(|c| *c == p))
        .map_or(0, |at| at + 1)
}

pub fn rows(entries: &[Entry], sel: &LogSelection, filter: &Filter) -> Vec<Row> {
    let selected = sel.selected_ids();
    LogSelection::matching(entries, filter)
        .iter()
        .map(|entry| Row::of(entry, selected.contains(&entry.id)))
        .collect()
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

fn active_index(rows: &[Row], open_id: u64) -> Option<usize> {
    (open_id != 0)
        .then(|| rows.iter().position(|row| row.id == open_id))
        .flatten()
}

fn count_label(shown: usize, total: usize, filtered: bool) -> String {
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
            origin: origin.into(),
            level,
            text: text.into(),
        }
    }

    #[test]
    fn logs_data_filter_selection_detail_and_copy() {
        let entries = vec![
            mk(1, Source::App, "Core", Level::Info, "first\nline2"),
            mk(2, Source::Server, "", Level::Error, "second"),
            mk(3, Source::App, "Core", Level::Warn, "third"),
        ];
        let mut sel = LogSelection::default();
        sel.press_record(&entries, 3, false, false);
        sel.drag_record(&entries, 1);
        sel.release_record(&entries, 1);

        let view = data(&entries, &sel, true);
        assert_eq!(
            (
                view.rows.len(),
                view.selected_count,
                view.open_id,
                view.active_index
            ),
            (3, 3, 1, 2)
        );
        assert_eq!(view.rows[2].text, "first line2");
        assert_eq!(view.detail, ["first", "line2"]);
        assert!(view.detail_heading.starts_with("3 records selected — "));
        assert!(view.clear_armed);

        let (count, copied) = copy_text(&entries, &sel, &Filter::default()).unwrap();
        assert_eq!(count, 3);
        assert!(copied.contains("third") && copied.contains("first"));

        pick_source(&mut sel, 5);
        let filtered = data(&entries, &sel, false);
        assert_eq!(
            (
                filtered.rows.len(),
                filtered.selected_count,
                filtered.count.as_str()
            ),
            (2, 2, "2 of 3")
        );
    }
}
