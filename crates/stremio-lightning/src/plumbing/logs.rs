//! The app's log records: everything worth a line on the Logs page, from every part of the app
//! that has something to say.
//!
//! One [`Logs`] value lives for as long as the window does, and every part that reports something
//! holds an `Arc` of it: the plugin runtime records each line a plugin logs, the theme reader
//! records what a theme file says that the app cannot use, the Mods actions record what an enable
//! or a removal did, the streaming server records what became of it, and the app's own errors are
//! recorded where they surface. The Logs screen reads a [`Entry`] list out of it and decides what
//! to show; nothing here draws.
//!
//! The record is in memory for this run of the app, and nothing here writes a file. A record older
//! than the newest [`CAPACITY`] ones is dropped as it is added, so a plugin that logs in a loop
//! cannot grow the list without end. Cleared means emptied from memory: no file is touched.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

/// How many records the app holds at once. The oldest goes first.
pub const CAPACITY: usize = 2000;

/// Where a record came from, as the Logs page names it in each row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    Plugin,
    Theme,
    Mods,
    Server,
    App,
}

impl Source {
    /// Every source, in the order the filter's list gives them.
    pub const ALL: [Self; 5] = [
        Self::Plugin,
        Self::Theme,
        Self::Mods,
        Self::Server,
        Self::App,
    ];

    /// The id the window passes in the filter: a word with no spaces, so it can be a control's
    /// name on screen.
    pub fn id(self) -> &'static str {
        match self {
            Self::Plugin => "plugin",
            Self::Theme => "theme",
            Self::Mods => "mods",
            Self::Server => "server",
            Self::App => "app",
        }
    }

    /// The label in a row's first column.
    pub fn label(self) -> &'static str {
        match self {
            Self::Plugin => "Plugin",
            Self::Theme => "Theme",
            Self::Mods => "Mods",
            Self::Server => "Server",
            Self::App => "App",
        }
    }

    /// The source with this id, or `None` when there is none.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|source| source.id() == id)
    }

    /// A row is one line of a row's first column: the label, then what it is about.
    fn shown(self, origin: &str) -> String {
        if origin.is_empty() {
            self.label().to_owned()
        } else {
            format!("{} {}", self.label(), origin)
        }
    }
}

/// How bad a record is, from quietest to worst. The level filter keeps the records at its own
/// level and worse.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    pub const ALL: [Self; 4] = [Self::Debug, Self::Info, Self::Warn, Self::Error];

    pub fn id(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Debug => "Debug",
            Self::Info => "Info",
            Self::Warn => "Warning",
            Self::Error => "Error",
        }
    }

    /// The level a plugin named, or `None` for a name this app does not know. A plugin's
    /// `console.log` arrives as `log`, which is information.
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "debug" | "trace" | "verbose" => Some(Self::Debug),
            "info" | "log" => Some(Self::Info),
            "warn" | "warning" => Some(Self::Warn),
            "error" | "fatal" => Some(Self::Error),
            _ => None,
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::Debug => 0,
            Self::Info => 1,
            Self::Warn => 2,
            Self::Error => 3,
        }
    }
}

/// One record, as the Logs page reads it and as the diagnostics file writes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Counts every record the app ever recorded, cleared or not, so two records can be told apart.
    pub id: u64,
    /// When it was recorded: seconds since 1 January 1970, UTC.
    pub at: u64,
    pub source: Source,
    /// What the record is about within its source: the plugin's file name, the theme's file name,
    /// the server, or the part of the app.
    pub origin: String,
    pub level: Level,
    pub text: String,
}

impl Entry {
    /// The source column: the label, then the origin.
    pub fn source_label(&self) -> String {
        self.source.shown(&self.origin)
    }

    /// The time column, in the reader's own time zone: hours:minutes:seconds.
    pub fn at_shown(&self) -> String {
        local(self.at).map_or_else(String::new, |at| at.format("%H:%M:%S").to_string())
    }

    /// The date and time the diagnostics file writes: the reader's own clock, to the second, so a
    /// log sent in a report can be lined up with what the person saw.
    pub fn at_stamped(&self) -> String {
        local(self.at).map_or_else(String::new, |at| at.format("%Y-%m-%d %H:%M:%S").to_string())
    }

    /// The record as the diagnostics file writes it: the time, the level, the source, the text.
    pub fn written(&self) -> String {
        format!(
            "{} {} [{}] {}",
            self.at_stamped(),
            self.level.label(),
            self.source_label(),
            self.text
        )
    }

    /// Whether the record is at the filter's level or worse.
    fn passes(&self, level: Level) -> bool {
        self.level.rank() >= level.rank()
    }
}

/// What the Logs page is showing: the source picked, the least level it keeps, and what the
/// search box holds. An empty source or a length filter are the "any" answers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    /// The id of the source picked, empty for every source.
    pub source: String,
    /// The least level kept, empty for every level.
    pub level: String,
    /// What the search box holds: matched against the record's text, its origin and its level.
    pub search: String,
}

impl Filter {
    /// The source this filter picks, if it picks one.
    pub fn picked_source(&self) -> Option<Source> {
        Source::from_id(&self.source)
    }

    /// The least level this filter keeps, if it sets one.
    pub fn least_level(&self) -> Option<Level> {
        Level::from_id(&self.level)
    }

    /// Whether `entry` passes every part of the filter.
    pub fn keeps(&self, entry: &Entry) -> bool {
        if self
            .picked_source()
            .is_some_and(|source| source != entry.source)
        {
            return false;
        }
        if self.least_level().is_some_and(|level| !entry.passes(level)) {
            return false;
        }
        let wanted = self.search.trim().to_lowercase();
        if wanted.is_empty() {
            return true;
        }
        let haystack = format!(
            "{} {} {} {}",
            entry.source_label(),
            entry.level.label(),
            entry.text,
            entry.origin
        )
        .to_lowercase();
        haystack.contains(&wanted)
    }
}

/// What a recorder tells when a record lands.
type Notify = std::sync::Arc<dyn Fn() + Send + Sync>;

/// The app's records and the counting of them. Shared by every part that records something.
#[derive(Default)]
pub struct Logs {
    entries: Mutex<VecDeque<Entry>>,
    /// Where every record is also written, line by line, so a log can be read when the window
    /// cannot be looked at. `None` in tests and in a build that does not want a file.
    file: Option<std::path::PathBuf>,
    next_id: AtomicU64,
    /// How many records have been recorded since the list was last cleared. Every view of the
    /// list comes from a snapshot, and a view only redraws when this changes.
    count: AtomicU64,
    /// The filter the Logs page is showing, as ids. Kept here rather than in the window so that a
    /// snapshot can carry it: a core event redraws the page, and it has to redraw the list the
    /// user filtered to.
    filter_source: Mutex<String>,
    filter_level: Mutex<String>,
    filter_search: Mutex<String>,
    /// The record whose full text the page is showing, by id, and zero when none is.
    detail: AtomicU64,
    /// What to tell when a record lands. The shell puts a repaint here while the Logs page is on
    /// screen; nothing in this module knows what a window is.
    notify: Mutex<Option<Notify>>,
}

impl std::fmt::Debug for Logs {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Logs")
            .field("entries", &lock(&self.entries).len())
            .field("watching", &lock(&self.notify).is_some())
            .finish()
    }
}

impl Logs {
    pub fn new() -> Logs {
        Logs::default()
    }

    /// A list that also appends every record to `path`, so the app's own words outlive the
    /// window. A file that cannot be written is no reason to lose the record: it stays in the
    /// list either way.
    pub fn recording_to(path: std::path::PathBuf) -> Logs {
        Logs {
            file: Some(path),
            ..Logs::default()
        }
    }

    /// Records one line, with the time it was recorded, and returns the record as stored.
    pub fn record(
        &self,
        source: Source,
        origin: impl Into<String>,
        level: Level,
        text: impl Into<String>,
    ) -> Entry {
        self.add(Entry {
            id: self.next_id.fetch_add(1, Ordering::SeqCst) + 1,
            at: now(),
            source,
            origin: origin.into(),
            level,
            text: text.into(),
        })
    }

    /// Stores a record built elsewhere, dropping the oldest when the list is full.
    pub fn add(&self, entry: Entry) -> Entry {
        if let Some(path) = &self.file {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            // Past the limit the current file becomes the one previous file, and a fresh one
            // starts: the newest run is always the whole of `app.log`.
            if std::fs::metadata(path).is_ok_and(|file| file.len() > FILE_LIMIT) {
                let _ = std::fs::rename(path, path.with_extension("log.1"));
            }
            if let Ok(mut file) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
            {
                use std::io::Write;
                // The same line the diagnostics export writes: one serializer, so the two files a
                // report can carry say the same thing about the same record.
                let _ = writeln!(file, "{}", entry.written());
            }
        }
        let mut entries = lock(&self.entries);
        entries.push_back(entry.clone());
        let over = entries.len().saturating_sub(CAPACITY);
        entries.drain(..over);
        drop(entries);
        self.count.fetch_add(1, Ordering::SeqCst);
        // Told after the lock is let go: the listener reads the list back.
        let notify = lock(&self.notify).clone();
        if let Some(notify) = notify {
            notify();
        }
        entry
    }

    /// Every record held, newest last.
    pub fn entries(&self) -> Vec<Entry> {
        lock(&self.entries).iter().cloned().collect()
    }

    /// The records the filter keeps, newest first: the order the Logs page lists them in.
    pub fn matching(&self, filter: &Filter) -> Vec<Entry> {
        lock(&self.entries)
            .iter()
            .rev()
            .filter(|entry| filter.keeps(entry))
            .cloned()
            .collect()
    }

    /// How many records are held now, whatever the filter.
    pub fn len(&self) -> usize {
        lock(&self.entries).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Empties the list. It deletes no file: the record is this run of the app's, and nothing here
    /// writes one. Returns how many records went.
    pub fn clear(&self) -> usize {
        let went = {
            let mut entries = lock(&self.entries);
            let went = entries.len();
            entries.clear();
            went
        };
        self.detail.store(0, Ordering::SeqCst);
        self.count.fetch_add(1, Ordering::SeqCst);
        went
    }

    /// A number that changes whenever a record is added or the list is cleared.
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::SeqCst)
    }

    /// Says where a new record is announced. Called after the record is stored and no lock is
    /// held, from whichever thread recorded it: the shell decides whether that is worth a repaint.
    pub fn watch_records(&self, notify: impl Fn() + Send + Sync + 'static) {
        *lock(&self.notify) = Some(std::sync::Arc::new(notify));
    }

    /// The filter the Logs page is showing.
    pub fn filter(&self) -> Filter {
        Filter {
            source: lock(&self.filter_source).clone(),
            level: lock(&self.filter_level).clone(),
            search: lock(&self.filter_search).clone(),
        }
    }

    /// Changes one part of the filter: what the page's own controls do.
    pub fn set_source(&self, source: &str) {
        *lock(&self.filter_source) = match Source::from_id(source) {
            Some(source) => source.id().to_owned(),
            None => String::new(),
        };
    }

    pub fn set_level(&self, level: &str) {
        *lock(&self.filter_level) = match Level::from_id(level) {
            Some(level) => level.id().to_owned(),
            None => String::new(),
        };
    }

    pub fn set_search(&self, search: &str) {
        *lock(&self.filter_search) = search.to_owned();
    }

    /// The record whose full text is on screen, if one is.
    pub fn detail(&self) -> Option<Entry> {
        let id = self.detail.load(Ordering::SeqCst);
        if id == 0 {
            return None;
        }
        lock(&self.entries)
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
    }

    /// Shows the full text of the record with this id, or takes the detail back with `None`.
    /// Returns whether the record is one the list holds.
    pub fn set_detail(&self, id: Option<u64>) -> bool {
        let Some(id) = id else {
            self.detail.store(0, Ordering::SeqCst);
            return true;
        };
        let held = lock(&self.entries).iter().any(|entry| entry.id == id);
        self.detail
            .store(if held { id } else { 0 }, Ordering::SeqCst);
        held
    }

    /// Records what a theme file says that this app does not understand, one record per warning.
    pub fn theme(&self, filename: &str, warnings: &[String]) {
        for warning in warnings {
            self.record(Source::Theme, filename, Level::Warn, warning.clone());
        }
    }

    /// Records a theme file that could not be read at all.
    pub fn theme_unreadable(&self, filename: &str, error: impl Into<String>) {
        self.record(Source::Theme, filename, Level::Error, error);
    }
}

/// How large the app's log file may grow before it is rotated. One previous file is kept, so an
/// install left running for months cannot fill the disk.
const FILE_LIMIT: u64 = 4 * 1024 * 1024;

/// A record's time on the reader's own clock. The app records UTC seconds; a person reads their
/// wall clock, and so does whoever is handed the file.
fn local(at: u64) -> Option<chrono::DateTime<chrono::Local>> {
    let at = i64::try_from(at).ok()?;
    chrono::DateTime::from_timestamp(at, 0).map(|utc| utc.with_timezone(&chrono::Local))
}

/// The current time, in seconds since 1 January 1970. A clock before 1970 records zero rather than
/// failing: a log line is not worth an error of its own.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default()
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A record is replaced whole and never half-written, so a panic elsewhere leaves the list
    // usable, and a poisoned lock is the right one to take.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plugin_line_is_a_record_at_its_own_level() {
        let logs = Logs::new();
        logs.record(Source::Plugin, "builtin:aniskip", Level::Warn, "slow");
        let entry = &logs.entries()[0];
        assert_eq!(entry.source, Source::Plugin);
        assert_eq!(entry.origin, "builtin:aniskip");
        assert_eq!(entry.level, Level::Warn);
        assert_eq!(entry.text, "slow");
        assert_eq!(entry.source_label(), "Plugin builtin:aniskip");
    }

    #[test]
    fn level_ids_map_known_names() {
        assert_eq!(Level::from_id("log"), Some(Level::Info));
        assert_eq!(Level::from_id("trace"), Some(Level::Debug));
        assert_eq!(Level::from_id("fatal"), Some(Level::Error));
        assert_eq!(Level::from_id("shouting"), None);
    }

    #[test]
    fn the_list_is_newest_first_and_the_capacity_holds() {
        let logs = Logs::new();
        for index in 0..CAPACITY + 5 {
            logs.record(Source::App, "", Level::Info, format!("line {index}"));
        }
        assert_eq!(logs.len(), CAPACITY, "the oldest go first");
        let listed = logs.matching(&Filter::default());
        assert_eq!(listed.len(), CAPACITY);
        assert_eq!(
            listed[0].text,
            format!("line {}", CAPACITY + 4),
            "newest first"
        );
        assert_eq!(listed[CAPACITY - 1].text, "line 5");
    }

    #[test]
    fn the_filter_keeps_a_source_a_level_and_a_search() {
        let logs = Logs::new();
        logs.record(Source::Plugin, "builtin:aniskip", Level::Info, "started");
        logs.record(Source::Plugin, "builtin:aniskip", Level::Error, "threw");
        logs.record(
            Source::Theme,
            "red.theme.css",
            Level::Warn,
            "unknown property",
        );
        logs.record(Source::Server, "", Level::Info, "ready");

        let mut filter = Filter {
            source: "plugin".to_owned(),
            ..Filter::default()
        };
        assert_eq!(logs.matching(&filter).len(), 2);
        filter.level = "error".to_owned();
        let listed = logs.matching(&filter);
        assert_eq!(listed.len(), 1, "a level keeps its own and worse");
        assert_eq!(listed[0].text, "threw");

        let search = Filter {
            search: "PROPERTY".to_owned(),
            ..Filter::default()
        };
        assert_eq!(logs.matching(&search).len(), 1, "the search ignores case");

        let by_origin = Filter {
            search: "red.theme".to_owned(),
            ..Filter::default()
        };
        assert_eq!(logs.matching(&by_origin).len(), 1);

        let warn = Filter {
            level: "warn".to_owned(),
            ..Filter::default()
        };
        assert_eq!(logs.matching(&warn).len(), 2, "the warning and the error");
    }

    #[test]
    fn clearing_empties_the_list_and_keeps_the_count_moving() {
        let logs = Logs::new();
        logs.record(Source::Mods, "a.theme.css", Level::Info, "removed");
        let before = logs.count();
        assert_eq!(logs.clear(), 1);
        assert!(logs.is_empty());
        assert!(logs.matching(&Filter::default()).is_empty());
        assert!(
            logs.count() > before,
            "a clear is a change the view must see"
        );
        logs.record(Source::Mods, "b.theme.css", Level::Info, "removed");
        assert_eq!(logs.len(), 1, "recording goes on after a clear");
    }

    #[test]
    fn a_detail_is_the_record_the_list_still_holds() {
        let logs = Logs::new();
        let one = logs.record(Source::App, "", Level::Error, "one");
        let two = logs.record(Source::App, "", Level::Error, "two");
        assert!(logs.set_detail(Some(one.id)));
        assert_eq!(logs.detail(), Some(one.clone()));
        assert!(
            !logs.set_detail(Some(9_999)),
            "a record not held is no detail"
        );
        assert_eq!(logs.detail(), None);
        assert!(logs.set_detail(Some(two.id)));
        logs.clear();
        assert_eq!(logs.detail(), None, "a clear takes the detail with it");
    }

    #[test]
    fn a_theme_warning_is_one_record_each() {
        let logs = Logs::new();
        logs.theme(
            "odd.theme.css",
            &["unknown property".to_owned(), "reaches nothing".to_owned()],
        );
        logs.theme_unreadable("gone.theme.css", "the file could not be read");
        let listed = logs.matching(&Filter::default());
        assert_eq!(listed.len(), 3);
        assert_eq!(listed[2].source, Source::Theme);
        assert_eq!(listed[0].level, Level::Error);
    }

    #[test]
    fn a_new_record_is_announced_once_and_the_announcement_reads_the_list() {
        let logs = std::sync::Arc::new(Logs::new());
        let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
        {
            let (page_logs, seen) = (logs.clone(), seen.clone());
            logs.watch_records(move || {
                // The listener reads the recorder back while it runs: no lock is held on the way.
                seen.lock().unwrap().push(page_logs.len());
            });
        }
        logs.record(Source::App, "", Level::Info, "one");
        assert_eq!(
            *seen.lock().unwrap(),
            [1],
            "the page is told, and the line is there"
        );
        logs.record(Source::App, "", Level::Info, "two");
        assert_eq!(*seen.lock().unwrap(), [1, 2]);
    }

    #[test]
    fn the_filter_the_page_shows_is_held_with_the_records() {
        let logs = Logs::new();
        logs.set_source("theme");
        logs.set_level("warn");
        logs.set_search("colour");
        assert_eq!(
            logs.filter(),
            Filter {
                source: "theme".to_owned(),
                level: "warn".to_owned(),
                search: "colour".to_owned(),
            }
        );
        // A source or level the app does not know is the "any" answer, not a filter that hides
        // every record.
        logs.set_source("nowhere");
        logs.set_level("loud");
        assert_eq!(logs.filter().source, "");
        assert_eq!(logs.filter().level, "");
    }

    #[test]
    fn a_record_is_written_with_the_readers_own_time() {
        let entry = Entry {
            id: 1,
            at: 3_600 + 120 + 3,
            source: Source::Theme,
            origin: "red.theme.css".to_owned(),
            level: Level::Warn,
            text: "unknown property".to_owned(),
        };
        // The clock the page shows and the clock the file keeps are the reader's, not UTC.
        let local = chrono::DateTime::from_timestamp(3_723, 0)
            .unwrap()
            .with_timezone(&chrono::Local);
        assert_eq!(entry.at_shown(), local.format("%H:%M:%S").to_string());
        assert_eq!(
            entry.at_stamped(),
            local.format("%Y-%m-%d %H:%M:%S").to_string()
        );
        assert_eq!(
            entry.written(),
            format!(
                "{} Warning [Theme red.theme.css] unknown property",
                local.format("%Y-%m-%d %H:%M:%S")
            )
        );
    }

    #[test]
    fn the_file_keeps_one_previous_run_and_starts_a_fresh_one() {
        let folder =
            std::env::temp_dir().join(format!("stremio-lightning-logs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let path = folder.join("app.log");
        let logs = Logs::recording_to(path.clone());
        logs.record(Source::App, "Core", Level::Info, "the first line");

        let written = std::fs::read_to_string(&path).expect("the file the recorder opened");
        let first = logs.entries()[0].clone();
        assert!(
            written.starts_with(&first.at_stamped()),
            "a line begins with the reader's own date and time: {written:?}"
        );
        assert!(
            written.contains(&first.written()),
            "the file holds the record exactly as the diagnostics export writes it: {written:?}"
        );
        assert!(
            written.contains("Info [App Core] the first line"),
            "a line says its level, what it came from, and what it said: {written:?}"
        );

        // The file past its limit becomes the one previous file, and the next line starts a
        // fresh one.
        std::fs::write(&path, vec![b'x'; (FILE_LIMIT + 1) as usize]).expect("a full file");
        logs.record(Source::App, "Core", Level::Info, "after the limit");
        let rotated =
            std::fs::read_to_string(path.with_extension("log.1")).expect("the previous run's file");
        assert_eq!(rotated.len(), (FILE_LIMIT + 1) as usize);
        let fresh = std::fs::read_to_string(&path).expect("the fresh file");
        assert!(fresh.contains("after the limit"), "{fresh:?}");
        assert!(!fresh.contains("the first line"), "{fresh:?}");
        let _ = std::fs::remove_dir_all(&folder);
    }
}
