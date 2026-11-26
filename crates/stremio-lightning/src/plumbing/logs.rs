//! Bounded in-memory log recorder (`CAPACITY` entries, oldest dropped first) with optional
//! synchronized, sanitized rotating file persistence (`app.log`) and push-based record notification.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use super::rotating_log::RotatingLogWriter;

/// How many records the app holds at once. The oldest goes first.
pub const CAPACITY: usize = 2000;
const FILE_LIMIT: u64 = 4 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Source {
    Plugin,
    Theme,
    Mods,
    Server,
    App,
}

impl Source {
    pub const ALL: [Self; 5] = [
        Self::Plugin,
        Self::Theme,
        Self::Mods,
        Self::Server,
        Self::App,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Plugin => "plugin",
            Self::Theme => "theme",
            Self::Mods => "mods",
            Self::Server => "server",
            Self::App => "app",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Plugin => "Plugin",
            Self::Theme => "Theme",
            Self::Mods => "Mods",
            Self::Server => "Server",
            Self::App => "App",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|source| source.id() == id)
    }

    fn shown(self, origin: &str) -> String {
        if origin.is_empty() {
            self.label().to_owned()
        } else {
            format!("{} {}", self.label(), origin)
        }
    }
}

/// Severity ordered from quietest to worst (`Debug < Info < Warn < Error`).
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

    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "debug" | "trace" | "verbose" => Some(Self::Debug),
            "info" | "log" => Some(Self::Info),
            "warn" | "warning" => Some(Self::Warn),
            "error" | "fatal" => Some(Self::Error),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub id: u64,
    pub at: u64,
    pub source: Source,
    pub origin: String,
    pub level: Level,
    pub text: String,
}

impl Entry {
    pub fn source_label(&self) -> String {
        self.source.shown(&self.origin)
    }

    pub fn at_shown(&self) -> String {
        local(self.at).map_or_else(String::new, |at| at.format("%H:%M:%S").to_string())
    }

    pub fn at_stamped(&self) -> String {
        local(self.at).map_or_else(String::new, |at| at.format("%Y-%m-%d %H:%M:%S").to_string())
    }

    pub fn written(&self) -> String {
        format!(
            "{} {} [{}] {}",
            self.at_stamped(),
            self.level.label(),
            self.source_label(),
            self.text
        )
    }
}

type Notify = Arc<dyn Fn() + Send + Sync>;

#[derive(Default)]
pub struct Logs {
    entries: Mutex<VecDeque<Entry>>,
    file: Option<RotatingLogWriter>,
    next_id: AtomicU64,
    count: AtomicU64,
    notify: Mutex<Option<Notify>>,
}

impl std::fmt::Debug for Logs {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Logs(entries={})", lock(&self.entries).len())
    }
}

impl Logs {
    pub fn new() -> Logs {
        Logs::default()
    }

    pub fn recording_to(path: std::path::PathBuf) -> Logs {
        Logs {
            file: Some(RotatingLogWriter::new(path, FILE_LIMIT)),
            ..Logs::default()
        }
    }

    pub fn record(
        &self,
        source: Source,
        origin: impl Into<String>,
        level: Level,
        text: impl Into<String>,
    ) -> Entry {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        self.add(Entry {
            id,
            at: now(),
            source,
            origin: origin.into(),
            level,
            text: text.into(),
        })
    }

    pub fn add(&self, entry: Entry) -> Entry {
        let _ = self.file.as_ref().map(|w| w.write_line(&entry.written()));
        let mut entries = lock(&self.entries);
        if entries.len() >= CAPACITY {
            entries.pop_front();
        }
        entries.push_back(entry.clone());
        drop(entries);
        self.count.fetch_add(1, Ordering::SeqCst);
        let notify = lock(&self.notify).clone();
        if let Some(notify) = notify {
            notify();
        }
        entry
    }

    pub fn watch_records(&self, on_record: impl Fn() + Send + Sync + 'static) {
        *lock(&self.notify) = Some(Arc::new(on_record));
    }

    pub fn entries(&self) -> Vec<Entry> {
        lock(&self.entries).iter().cloned().collect()
    }

    pub fn len(&self) -> usize {
        lock(&self.entries).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn count(&self) -> u64 {
        self.count.load(Ordering::SeqCst)
    }

    pub fn clear(&self) -> usize {
        let removed = std::mem::take(&mut *lock(&self.entries)).len();
        if removed > 0 {
            self.count.fetch_add(1, Ordering::SeqCst);
        }
        removed
    }
}

fn local(at: u64) -> Option<chrono::DateTime<chrono::Local>> {
    chrono::DateTime::from_timestamp(i64::try_from(at).ok()?, 0)
        .map(|utc| utc.with_timezone(&chrono::Local))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_cap_clear_and_watch_notify() {
        let logs = Arc::new(Logs::new());
        let seen = Arc::new(Mutex::new(Vec::new()));
        {
            let (l, s) = (logs.clone(), seen.clone());
            logs.watch_records(move || s.lock().unwrap().push(l.len()));
        }
        let first = logs.record(Source::Plugin, "builtin:aniskip", Level::Warn, "slow");
        assert_eq!(first.source_label(), "Plugin builtin:aniskip");
        assert_eq!(*seen.lock().unwrap(), vec![1]);
        assert!(Level::Error >= Level::Warn && Level::Warn >= Level::Info);

        for i in 0..CAPACITY + 5 {
            logs.record(Source::App, "", Level::Info, format!("line {i}"));
        }
        assert_eq!(logs.len(), CAPACITY);
        assert_eq!(
            logs.entries().last().unwrap().text,
            format!("line {}", CAPACITY + 4)
        );

        let before = logs.count();
        assert_eq!(logs.clear(), CAPACITY);
        assert!(logs.is_empty() && logs.count() > before);
        assert_eq!(logs.clear(), 0);
    }

    #[test]
    fn recording_to_appends_sanitized_lines_and_rotates() {
        let dir = std::env::temp_dir().join(format!("stremio-log-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("app.log");
        let logs = Logs::recording_to(path.clone());
        let first = logs.record(Source::App, "Core", Level::Info, "the first line");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap().trim_end(),
            first.written()
        );
        std::fs::write(&path, vec![b'x'; FILE_LIMIT as usize + 16]).unwrap();
        logs.record(Source::App, "Core", Level::Info, "after the limit");
        let cur = std::fs::read_to_string(&path).unwrap();
        let prev = std::fs::read_to_string(path.with_extension("log.1")).unwrap();
        assert!(cur.contains("after the limit") && prev.starts_with("xxxx"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
