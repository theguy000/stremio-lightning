pub mod report;
pub mod sanitizer;
pub mod sink;
pub mod types;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, Once, OnceLock};

pub use report::build_report;
pub use sanitizer::{looks_like_url, sanitize_identifier, sanitize_message, sanitize_source};
pub use types::{
    format_timestamp, generate_session_id, truncate_utf8, unix_timestamp_ms, DiagnosticLimits,
    DiagnosticReportRuntime, ExternalLogEntry, LimitDecision, LogEntry, LogLevel, LoggingConfig,
    LoggingError, PersistentLine, SessionMetadata, APPLICATION_REPORT_LIMIT_BYTES,
    DIAGNOSTIC_SCHEMA_VERSION, MAX_ENTRIES, MAX_EXTERNAL_BATCH_BYTES, MAX_EXTERNAL_BATCH_ENTRIES,
    MAX_MESSAGE_LENGTH, MAX_SOURCE_LENGTH, REPORT_LIMIT_BYTES, RETAINED_SESSION_COUNT,
    SERVER_REPORT_LIMIT_BYTES, SESSION_FILE_PREFIX, SESSION_LIMIT_BYTES,
};

use sink::{ExternalLimitState, LogBuffer, PersistentSink};

#[derive(Debug, Default)]
pub struct DiagnosticsState {
    pub sink: Option<PersistentSink>,
    pub metadata: Option<SessionMetadata>,
    pub directory: Option<PathBuf>,
    pub initialized: bool,
    pub failure_reported: bool,
}

#[derive(Debug, Default)]
pub struct Logger {
    pub next_id: AtomicU64,
    pub next_receipt_id: AtomicU64,
    pub buffer: Mutex<LogBuffer>,
    pub diagnostics: Mutex<DiagnosticsState>,
    pub external_limits: Mutex<ExternalLimitState>,
    pub extended: AtomicBool,
    pub truncated_records: AtomicU64,
    pub dropped_records: AtomicU64,
    pub suppressed_records: AtomicU64,
}

impl Logger {
    pub fn push(&self, level: LogLevel, source: String, message: String) -> LogEntry {
        let mut buffer = lock_unpoisoned(&self.buffer);
        let entry = LogEntry {
            id: self.next_id.fetch_add(1, Ordering::Relaxed) + 1,
            timestamp: unix_timestamp_ms(),
            level,
            source,
            message,
        };
        buffer.push(entry.clone());
        entry
    }

    pub fn snapshot_after(&self, after_id: u64) -> Vec<LogEntry> {
        lock_unpoisoned(&self.buffer).snapshot_after(after_id)
    }

    /// # Errors
    /// Returns an error when the diagnostics directory or session file cannot be created.
    pub fn initialize(&self, config: LoggingConfig) -> Result<(), String> {
        let buffered = self.snapshot_after(0);
        let mut state = lock_unpoisoned(&self.diagnostics);
        if state.initialized {
            return Ok(());
        }
        state.initialized = true;
        state.directory = Some(config.directory.clone());
        state.metadata = Some(SessionMetadata::from_config(config.clone()));
        let mut sink = match PersistentSink::new(config, DiagnosticLimits::default()) {
            Ok(sink) => sink,
            Err(error) => {
                drop(state);
                self.report_sink_failure(&error);
                return Err(error);
            }
        };
        for entry in buffered {
            if entry.level.persists_at_baseline() || self.extended.load(Ordering::Relaxed) {
                let receipt_id = self.next_receipt_id.fetch_add(1, Ordering::Relaxed) + 1;
                if let Err(error) = sink.append_record(
                    receipt_id,
                    entry.timestamp,
                    entry.level,
                    &entry.source,
                    &entry.message,
                    "native",
                ) {
                    drop(state);
                    self.report_sink_failure(&error);
                    return Err(error);
                }
            }
        }
        state.metadata = Some(sink.metadata.clone());
        state.sink = Some(sink);
        Ok(())
    }

    pub fn persist(
        &self,
        timestamp: u64,
        level: LogLevel,
        source: &str,
        message: &str,
        producer: &str,
    ) {
        let receipt_id = self.next_receipt_id.fetch_add(1, Ordering::Relaxed) + 1;
        let result = {
            let mut state = lock_unpoisoned(&self.diagnostics);
            let Some(sink) = state.sink.as_mut() else {
                return;
            };
            sink.append_record(receipt_id, timestamp, level, source, message, producer)
        };
        if let Err(error) = result {
            lock_unpoisoned(&self.diagnostics).sink = None;
            self.report_sink_failure(&error);
        }
    }

    pub fn report_sink_failure(&self, error: &str) {
        let mut state = lock_unpoisoned(&self.diagnostics);
        if state.failure_reported {
            return;
        }
        state.failure_reported = true;
        drop(state);
        let message = sanitize_message(&format!(
            "Persistent diagnostics disabled after a storage failure: {error}"
        ))
        .0;
        let entry = self.push(LogLevel::Warn, "native.diagnostics".to_string(), message);
        print_entry(&entry);
    }

    /// # Errors
    /// Returns an error when diagnostics files cannot be removed.
    pub fn clear(&self) -> Result<(), String> {
        lock_unpoisoned(&self.buffer).clear();
        {
            let mut limits = lock_unpoisoned(&self.external_limits);
            limits.fingerprints.clear();
            limits.sources.clear();
            limits.global = None;
        }
        self.truncated_records.store(0, Ordering::Relaxed);
        self.dropped_records.store(0, Ordering::Relaxed);
        self.suppressed_records.store(0, Ordering::Relaxed);
        let mut state = lock_unpoisoned(&self.diagnostics);
        if let Some(sink) = state.sink.as_mut() {
            let result = sink.clear();
            state.metadata = Some(sink.metadata.clone());
            result?;
        } else if let (Some(directory), Some(metadata)) =
            (state.directory.clone(), state.metadata.clone())
        {
            let mut first_error = None;
            for path in sink::session_files(&directory)? {
                if let Err(error) = sink::remove_if_exists(&path) {
                    first_error.get_or_insert_with(|| {
                        format!("failed to clear retained diagnostics: {error}")
                    });
                }
            }
            let sink =
                PersistentSink::from_metadata(directory, metadata, DiagnosticLimits::default())?;
            state.metadata = Some(sink.metadata.clone());
            state.sink = Some(sink);
            state.failure_reported = false;
            if let Some(error) = first_error {
                return Err(error);
            }
        }
        Ok(())
    }

    pub fn update_webview_metadata(&self, engine: &str, version: Option<&str>) {
        let engine = sanitize_identifier(engine, 64);
        let version = version.map(|value| sanitize_identifier(value, 128));
        let result = {
            let mut state = lock_unpoisoned(&self.diagnostics);
            if let Some(metadata) = state.metadata.as_mut() {
                metadata.webview_engine.clone_from(&engine);
                metadata.webview_version.clone_from(&version);
            }
            state
                .sink
                .as_mut()
                .map(|sink| sink.update_webview_metadata(engine, version))
        };
        if let Some(Err(error)) = result {
            lock_unpoisoned(&self.diagnostics).sink = None;
            self.report_sink_failure(&error);
        }
    }

    /// # Errors
    /// Returns an error when external entries cannot be appended to the log.
    pub fn submit_external(&self, entries: Vec<ExternalLogEntry>) -> Result<(), String> {
        if entries.len() > MAX_EXTERNAL_BATCH_ENTRIES {
            return Err(format!(
                "Diagnostic batch exceeds {MAX_EXTERNAL_BATCH_ENTRIES} records"
            ));
        }
        let bytes = entries.iter().fold(0usize, |total, entry| {
            total
                .saturating_add(entry.source.len())
                .saturating_add(entry.message.len())
                .saturating_add(32)
        });
        if bytes > MAX_EXTERNAL_BATCH_BYTES {
            return Err(format!(
                "Diagnostic batch exceeds {MAX_EXTERNAL_BATCH_BYTES} bytes"
            ));
        }

        for entry in entries {
            if matches!(entry.level, LogLevel::Debug) && !self.extended.load(Ordering::Relaxed) {
                continue;
            }
            if entry.source == "bridge.diagnostics" {
                if let Some(count) = entry
                    .message
                    .strip_prefix("Dropped ")
                    .and_then(|message| message.split_whitespace().next())
                    .and_then(|count| count.parse::<u64>().ok())
                {
                    self.dropped_records.fetch_add(count, Ordering::Relaxed);
                }
            }
            let sanitized_external_source = if looks_like_url(&entry.source) {
                "external".to_string()
            } else {
                sanitize_message(&entry.source).0
            };
            let (source, source_truncated) = sanitize_source(&sanitized_external_source);
            let (source, prefix_truncated) = sanitize_source(&format!("browser.{source}"));
            let (message, message_truncated) = sanitize_message(&entry.message);
            if source_truncated || prefix_truncated || message_truncated {
                self.truncated_records.fetch_add(1, Ordering::Relaxed);
            }
            let now = unix_timestamp_ms();
            let decision =
                lock_unpoisoned(&self.external_limits).allow(&source, entry.level, &message, now);
            match decision {
                LimitDecision::Suppress => {
                    self.suppressed_records.fetch_add(1, Ordering::Relaxed);
                }
                LimitDecision::Allow { summary } => {
                    if summary > 0 {
                        self.persist(
                            now,
                            LogLevel::Warn,
                            "browser.diagnostics",
                            &format!(
                                "Suppressed {summary} duplicate or \
                                 rate-limited browser records from {source}"
                            ),
                            "browser",
                        );
                    }
                    self.persist(now, entry.level, &source, &message, "browser");
                }
            }
        }
        Ok(())
    }

    pub fn report(&self, runtime: DiagnosticReportRuntime) -> String {
        let (metadata, directory) = {
            let state = lock_unpoisoned(&self.diagnostics);
            (state.metadata.clone(), state.directory.clone())
        };
        let in_memory_records = self.snapshot_after(0);
        build_report(
            metadata.as_ref(),
            directory.as_deref(),
            runtime,
            &self.extended,
            &self.truncated_records,
            &self.dropped_records,
            &self.suppressed_records,
            &in_memory_records,
        )
    }
}

static LOGGER: OnceLock<Logger> = OnceLock::new();
static PANIC_HOOK: Once = Once::new();

pub fn logger() -> &'static Logger {
    LOGGER.get_or_init(Logger::default)
}

/// # Errors
/// Returns an error when the diagnostics directory or session file cannot be created.
pub fn initialize(config: LoggingConfig) -> Result<(), String> {
    let result = logger().initialize(config);
    install_panic_hook();
    result
}

#[must_use]
pub fn diagnostics_dir_for_platform(platform: &str) -> PathBuf {
    let base = match platform {
        "windows" => std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
        "macos" => std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|path| path.join("Library/Application Support")),
        _ => std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|path| path.join(".local/share"))
            }),
    }
    .or_else(|| std::env::current_dir().ok())
    .unwrap_or_else(|| PathBuf::from("."));
    base.join("stremio-lightning/logs")
}

pub fn update_webview_metadata(engine: &str, version: Option<&str>) {
    logger().update_webview_metadata(engine, version);
}

pub fn set_extended(enabled: bool) {
    logger().extended.store(enabled, Ordering::Relaxed);
}

#[must_use]
pub fn is_extended() -> bool {
    logger().extended.load(Ordering::Relaxed)
}

#[must_use]
pub fn is_persistent_available() -> bool {
    lock_unpoisoned(&logger().diagnostics).sink.is_some()
}

#[must_use]
pub fn webview_metadata() -> (String, Option<String>) {
    lock_unpoisoned(&logger().diagnostics)
        .metadata
        .as_ref()
        .map_or_else(
            || ("unavailable".to_string(), None),
            |metadata| {
                (
                    metadata.webview_engine.clone(),
                    metadata.webview_version.clone(),
                )
            },
        )
}

/// # Errors
/// Returns an error when external entries cannot be appended to the log.
pub fn submit_external(entries: Vec<ExternalLogEntry>) -> Result<(), String> {
    logger().submit_external(entries)
}

/// # Errors
/// Returns an error when diagnostics files cannot be removed.
pub fn clear_diagnostics() -> Result<(), String> {
    logger().clear()
}

#[must_use]
pub fn diagnostic_report(runtime: DiagnosticReportRuntime) -> String {
    logger().report(runtime)
}

#[must_use]
pub fn snapshot_after(after_id: u64) -> Vec<LogEntry> {
    logger().snapshot_after(after_id)
}

pub fn log(level: LogLevel, source: impl Into<String>, message: impl Into<String>) {
    let sanitized_source = sanitize_message(&source.into()).0;
    let (source, source_truncated) = sanitize_source(&sanitized_source);
    let (message, message_truncated) = sanitize_message(&message.into());
    if source_truncated || message_truncated {
        logger().truncated_records.fetch_add(1, Ordering::Relaxed);
    }
    if source.starts_with("native.webview.") || source == "streaming-server.stderr" {
        let now = unix_timestamp_ms();
        match lock_unpoisoned(&logger().external_limits).allow(&source, level, &message, now) {
            LimitDecision::Suppress => {
                logger().suppressed_records.fetch_add(1, Ordering::Relaxed);
                return;
            }
            LimitDecision::Allow { summary } if summary > 0 => {
                let summary_entry = logger().push(
                    LogLevel::Warn,
                    "native.diagnostics".to_string(),
                    format!("Suppressed {summary} duplicate or rate-limited records from {source}"),
                );
                print_entry(&summary_entry);
                logger().persist(
                    summary_entry.timestamp,
                    summary_entry.level,
                    &summary_entry.source,
                    &summary_entry.message,
                    "native",
                );
            }
            LimitDecision::Allow { .. } => {}
        }
    }
    let entry = logger().push(level, source, message);
    print_entry(&entry);
    if level.persists_at_baseline() || logger().extended.load(Ordering::Relaxed) {
        logger().persist(
            entry.timestamp,
            entry.level,
            &entry.source,
            &entry.message,
            "native",
        );
    }
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

#[must_use]
pub fn sanitize_text(value: &str) -> String {
    sanitize_message(value).0
}

fn install_panic_hook() {
    PANIC_HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            let payload = panic_info
                .payload()
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| {
                    panic_info
                        .payload()
                        .downcast_ref::<String>()
                        .map(String::as_str)
                })
                .unwrap_or("non-string panic payload");
            let location = panic_info.location().map_or_else(
                || "unknown location".to_string(),
                |location| format!("{}:{}", location.file(), location.line()),
            );
            record_panic(&format!("Unhandled panic at {location}: {payload}"));
            previous(panic_info);
        }));
    });
}

fn record_panic(message: &str) {
    let entry = LogEntry {
        id: logger().next_id.fetch_add(1, Ordering::Relaxed) + 1,
        timestamp: unix_timestamp_ms(),
        level: LogLevel::Error,
        source: "native.panic".to_string(),
        message: sanitize_message(message).0,
    };
    match logger().buffer.try_lock() {
        Ok(mut buffer) => buffer.push(entry.clone()),
        Err(std::sync::TryLockError::Poisoned(poisoned)) => {
            poisoned.into_inner().push(entry.clone());
        }
        Err(std::sync::TryLockError::WouldBlock) => {}
    }
    print_entry(&entry);

    if let Ok(mut state) = logger().diagnostics.try_lock() {
        if let Some(sink) = state.sink.as_mut() {
            let receipt_id = logger().next_receipt_id.fetch_add(1, Ordering::Relaxed) + 1;
            let _ = sink.append_record(
                receipt_id,
                entry.timestamp,
                entry.level,
                &entry.source,
                &entry.message,
                "native",
            );
        }
    }
}

fn print_entry(entry: &LogEntry) {
    let line = serde_json::to_string(entry).unwrap_or_else(|_| {
        format!(
            "[{}] {}: {}",
            entry.level.as_str(),
            entry.source,
            entry.message
        )
    });
    eprintln!("{line}");
}

pub fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
