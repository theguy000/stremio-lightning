use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

pub const MAX_ENTRIES: usize = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: u64,
    pub timestamp: u64,
    pub level: LogLevel,
    pub source: String,
    pub message: String,
}

#[derive(Debug, Default)]
struct LogBuffer {
    entries: VecDeque<LogEntry>,
}

impl LogBuffer {
    fn push(&mut self, entry: LogEntry) {
        if self.entries.len() == MAX_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    fn snapshot_after(&self, after_id: u64) -> Vec<LogEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.id > after_id)
            .cloned()
            .collect()
    }
}

#[derive(Debug, Default)]
struct Logger {
    next_id: AtomicU64,
    buffer: Mutex<LogBuffer>,
}

impl Logger {
    fn push(&self, level: LogLevel, source: String, message: String) -> LogEntry {
        let mut guard = match self.buffer.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let entry = LogEntry {
            id: self.next_id.fetch_add(1, Ordering::Relaxed) + 1,
            timestamp: unix_timestamp_ms(),
            level,
            source,
            message,
        };
        guard.push(entry.clone());
        entry
    }

    fn snapshot_after(&self, after_id: u64) -> Vec<LogEntry> {
        let guard = match self.buffer.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.snapshot_after(after_id)
    }
}

static LOGGER: OnceLock<Logger> = OnceLock::new();

fn logger() -> &'static Logger {
    LOGGER.get_or_init(Logger::default)
}

fn unix_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

pub fn snapshot_after(after_id: u64) -> Vec<LogEntry> {
    logger().snapshot_after(after_id)
}

pub fn log(level: LogLevel, source: impl Into<String>, message: impl Into<String>) {
    let entry = logger().push(level, source.into(), message.into());
    let line = serde_json::to_string(&entry).unwrap_or_else(|_| {
        format!(
            "[{}] {}: {}",
            entry.level.as_str(),
            entry.source,
            entry.message
        )
    });
    eprintln!("{line}");
}

pub fn debug(source: impl Into<String>, message: impl Into<String>) {
    log(LogLevel::Debug, source, message);
}

pub fn info(source: impl Into<String>, message: impl Into<String>) {
    log(LogLevel::Info, source, message);
}

pub fn warn(source: impl Into<String>, message: impl Into<String>) {
    log(LogLevel::Warn, source, message);
}

pub fn error(source: impl Into<String>, message: impl Into<String>) {
    log(LogLevel::Error, source, message);
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn entry(id: u64) -> LogEntry {
        LogEntry {
            id,
            timestamp: 1,
            level: LogLevel::Info,
            source: "native.test".to_string(),
            message: id.to_string(),
        }
    }

    #[test]
    fn levels_serialize_as_lowercase() {
        assert_eq!(
            serde_json::to_string(&LogLevel::Debug).unwrap(),
            "\"debug\""
        );
        assert_eq!(serde_json::to_string(&LogLevel::Info).unwrap(), "\"info\"");
        assert_eq!(serde_json::to_string(&LogLevel::Warn).unwrap(), "\"warn\"");
        assert_eq!(
            serde_json::to_string(&LogLevel::Error).unwrap(),
            "\"error\""
        );
    }

    #[test]
    fn entries_serialize_with_camel_case_fields() {
        let value = serde_json::to_value(entry(1)).unwrap();
        assert_eq!(value["id"], 1);
        assert_eq!(value["timestamp"], 1);
        assert_eq!(value["level"], "info");
        assert_eq!(value["source"], "native.test");
        assert_eq!(value["message"], "1");
    }

    #[test]
    fn buffer_evicts_oldest_entries_and_filters_ascending() {
        let mut buffer = LogBuffer::default();
        for id in 1..=(MAX_ENTRIES as u64 + 1) {
            buffer.push(entry(id));
        }

        let entries = buffer.snapshot_after(1);
        assert_eq!(entries.len(), MAX_ENTRIES);
        assert_eq!(entries.first().unwrap().id, 2);
        assert_eq!(entries.last().unwrap().id, MAX_ENTRIES as u64 + 1);
        assert_eq!(buffer.snapshot_after(MAX_ENTRIES as u64).len(), 1);
    }

    #[test]
    fn isolated_logger_assigns_ordered_ids_across_threads() {
        let logger = Arc::new(Logger::default());
        let threads = (0..4)
            .map(|_| {
                let logger = Arc::clone(&logger);
                std::thread::spawn(move || {
                    for _ in 0..100 {
                        logger.push(LogLevel::Debug, "native.test".into(), "message".into());
                    }
                })
            })
            .collect::<Vec<_>>();
        for thread in threads {
            thread.join().unwrap();
        }

        let entries = logger.snapshot_after(0);
        assert_eq!(entries.len(), 400);
        assert!(entries.iter().all(|entry| entry.timestamp > 0));
        assert_eq!(
            entries.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            (1..=400).collect::<Vec<_>>()
        );
    }

    #[test]
    fn isolated_logger_recovers_from_a_poisoned_buffer() {
        let logger = Arc::new(Logger::default());
        let poisoner = Arc::clone(&logger);
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.buffer.lock().unwrap();
            panic!("poison logger buffer");
        })
        .join();

        logger.push(LogLevel::Warn, "native.test".into(), "recovered".into());
        assert_eq!(logger.snapshot_after(0)[0].message, "recovered");
    }
}
