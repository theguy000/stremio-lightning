use std::collections::{hash_map::DefaultHasher, HashMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, Once, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde::{Deserialize, Serialize};

pub const MAX_ENTRIES: usize = 2_000;
pub const MAX_EXTERNAL_BATCH_ENTRIES: usize = 50;
pub const MAX_EXTERNAL_BATCH_BYTES: usize = 256 * 1024;
pub const MAX_SOURCE_LENGTH: usize = 256;
pub const MAX_MESSAGE_LENGTH: usize = 16 * 1024;
pub const SESSION_LIMIT_BYTES: u64 = 2 * 1024 * 1024;
pub const RETAINED_SESSION_COUNT: usize = 3;
pub const REPORT_LIMIT_BYTES: usize = 10 * 1024 * 1024;
const APPLICATION_REPORT_LIMIT_BYTES: usize = 6 * 1024 * 1024;
const SERVER_REPORT_LIMIT_BYTES: usize = 2 * 1024 * 1024;
const DIAGNOSTIC_SCHEMA_VERSION: u32 = 1;
const SESSION_FILE_PREFIX: &str = "diagnostics-session-";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    fn persists_at_baseline(self) -> bool {
        !matches!(self, Self::Debug)
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExternalLogEntry {
    pub level: LogLevel,
    pub source: String,
    pub message: String,
    #[serde(default)]
    pub timestamp: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct LoggingConfig {
    pub directory: PathBuf,
    pub app_version: String,
    pub platform: String,
    pub architecture: String,
    pub shell: String,
    pub webview_engine: String,
    pub webview_version: Option<String>,
    pub session_id: String,
}

impl LoggingConfig {
    pub fn new(
        directory: impl Into<PathBuf>,
        app_version: impl Into<String>,
        platform: impl Into<String>,
        shell: impl Into<String>,
        webview_engine: impl Into<String>,
    ) -> Self {
        Self {
            directory: directory.into(),
            app_version: app_version.into(),
            platform: platform.into(),
            architecture: std::env::consts::ARCH.to_string(),
            shell: shell.into(),
            webview_engine: webview_engine.into(),
            webview_version: None,
            session_id: generate_session_id(),
        }
    }
}

#[derive(Debug)]
pub struct DiagnosticReportRuntime {
    pub native_player_status: String,
    pub streaming_server_running: bool,
    pub server_stdout: Result<String, String>,
    pub server_stderr: Result<String, String>,
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

    fn clear(&mut self) {
        self.entries.clear();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionMetadata {
    schema_version: u32,
    session_id: String,
    started_at: u64,
    app_version: String,
    platform: String,
    architecture: String,
    shell: String,
    webview_engine: String,
    webview_version: Option<String>,
}

impl SessionMetadata {
    fn from_config(config: LoggingConfig) -> Self {
        Self {
            schema_version: DIAGNOSTIC_SCHEMA_VERSION,
            session_id: sanitize_identifier(&config.session_id, 64),
            started_at: unix_timestamp_ms(),
            app_version: sanitize_identifier(&config.app_version, 64),
            platform: sanitize_identifier(&config.platform, 32),
            architecture: sanitize_identifier(&config.architecture, 32),
            shell: sanitize_identifier(&config.shell, 64),
            webview_engine: sanitize_identifier(&config.webview_engine, 64),
            webview_version: config
                .webview_version
                .map(|value| sanitize_identifier(&value, 128)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum PersistentLine {
    Session {
        #[serde(flatten)]
        metadata: SessionMetadata,
    },
    Metadata {
        webview_engine: String,
        webview_version: Option<String>,
    },
    Record {
        schema_version: u32,
        session_id: String,
        receipt_id: u64,
        timestamp: u64,
        level: LogLevel,
        source: String,
        message: String,
        producer: String,
    },
}

#[derive(Debug, Clone, Copy)]
struct DiagnosticLimits {
    session_bytes: u64,
    sessions: usize,
}

impl Default for DiagnosticLimits {
    fn default() -> Self {
        Self {
            session_bytes: SESSION_LIMIT_BYTES,
            sessions: RETAINED_SESSION_COUNT,
        }
    }
}

#[derive(Debug)]
struct PersistentSink {
    directory: PathBuf,
    active_path: PathBuf,
    metadata: SessionMetadata,
    limits: DiagnosticLimits,
}

impl PersistentSink {
    fn new(config: LoggingConfig, limits: DiagnosticLimits) -> Result<Self, String> {
        let directory = config.directory.clone();
        fs::create_dir_all(&directory)
            .map_err(|error| format!("failed to create diagnostics directory: {error}"))?;
        bound_existing_sessions(&directory, limits)?;
        let metadata = SessionMetadata::from_config(config);
        Self::from_metadata(directory, metadata, limits)
    }

    fn from_metadata(
        directory: PathBuf,
        metadata: SessionMetadata,
        limits: DiagnosticLimits,
    ) -> Result<Self, String> {
        let active_path = session_path(&directory, metadata.started_at, &metadata.session_id);
        let mut sink = Self {
            directory,
            active_path,
            metadata,
            limits,
        };
        sink.write_fresh_header()?;
        sink.enforce_retention()?;
        Ok(sink)
    }

    fn append_record(
        &mut self,
        receipt_id: u64,
        timestamp: u64,
        level: LogLevel,
        source: &str,
        message: &str,
        producer: &str,
    ) -> Result<(), String> {
        self.append(&PersistentLine::Record {
            schema_version: DIAGNOSTIC_SCHEMA_VERSION,
            session_id: self.metadata.session_id.clone(),
            receipt_id,
            timestamp,
            level,
            source: source.to_string(),
            message: message.to_string(),
            producer: producer.to_string(),
        })?;
        if fs::metadata(&self.active_path)
            .map(|metadata| metadata.len() > self.limits.session_bytes)
            .unwrap_or(false)
        {
            self.compact()?;
        }
        self.enforce_retention()
    }

    fn update_webview_metadata(
        &mut self,
        engine: String,
        version: Option<String>,
    ) -> Result<(), String> {
        self.metadata.webview_engine = engine.clone();
        self.metadata.webview_version = version.clone();
        self.append(&PersistentLine::Metadata {
            webview_engine: engine,
            webview_version: version,
        })
    }

    fn append(&self, line: &PersistentLine) -> Result<(), String> {
        let mut serialized = serde_json::to_vec(line)
            .map_err(|error| format!("failed to serialize diagnostic record: {error}"))?;
        serialized.push(b'\n');
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.active_path)
            .map_err(|error| format!("failed to open diagnostics file: {error}"))?;
        file.write_all(&serialized)
            .map_err(|error| format!("failed to write diagnostics file: {error}"))
    }

    fn write_fresh_header(&mut self) -> Result<(), String> {
        self.metadata.started_at =
            unix_timestamp_ms().max(self.metadata.started_at.saturating_add(1));
        self.active_path = session_path(
            &self.directory,
            self.metadata.started_at,
            &self.metadata.session_id,
        );
        let line = PersistentLine::Session {
            metadata: self.metadata.clone(),
        };
        let mut serialized = serde_json::to_vec(&line)
            .map_err(|error| format!("failed to serialize diagnostics header: {error}"))?;
        serialized.push(b'\n');
        fs::write(&self.active_path, serialized)
            .map_err(|error| format!("failed to create diagnostics session: {error}"))
    }

    fn compact(&self) -> Result<(), String> {
        let file = File::open(&self.active_path)
            .map_err(|error| format!("failed to open diagnostics file for compaction: {error}"))?;
        let header = serde_json::to_string(&PersistentLine::Session {
            metadata: self.metadata.clone(),
        })
        .map_err(|error| format!("failed to serialize diagnostics header: {error}"))?;
        let available = self
            .limits
            .session_bytes
            .saturating_sub((header.len() + 1) as u64) as usize;
        let mut records = VecDeque::new();
        let mut bytes = 0usize;
        for line in BufReader::new(file).lines().map_while(Result::ok) {
            if !matches!(
                serde_json::from_str::<PersistentLine>(&line),
                Ok(PersistentLine::Record { .. }) | Ok(PersistentLine::Metadata { .. })
            ) {
                continue;
            }
            let line_bytes = line.len() + 1;
            records.push_back(line);
            bytes = bytes.saturating_add(line_bytes);
            while bytes > available {
                if let Some(removed) = records.pop_front() {
                    bytes = bytes.saturating_sub(removed.len() + 1);
                } else {
                    break;
                }
            }
        }

        let temporary = self.active_path.with_extension("ndjson.tmp");
        let mut output = File::create(&temporary)
            .map_err(|error| format!("failed to create compacted diagnostics file: {error}"))?;
        writeln!(output, "{header}")
            .map_err(|error| format!("failed to write compacted diagnostics header: {error}"))?;
        for line in records {
            writeln!(output, "{line}").map_err(|error| {
                format!("failed to write compacted diagnostics record: {error}")
            })?;
        }
        output
            .flush()
            .map_err(|error| format!("failed to flush compacted diagnostics file: {error}"))?;
        replace_file_safely(&temporary, &self.active_path)
            .map_err(|error| format!("failed to replace compacted diagnostics file: {error}"))
    }

    fn clear(&mut self) -> Result<(), String> {
        let mut first_error = None;
        for path in session_files(&self.directory)? {
            if let Err(error) = remove_if_exists(&path) {
                first_error.get_or_insert_with(|| {
                    format!("failed to clear retained diagnostics: {error}")
                });
            }
        }
        if let Err(error) = self.write_fresh_header() {
            first_error.get_or_insert(error);
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn enforce_retention(&self) -> Result<(), String> {
        let mut files = session_files(&self.directory)?;
        files.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
        for path in files.into_iter().skip(self.limits.sessions.max(1)) {
            remove_if_exists(&path)
                .map_err(|error| format!("failed to remove old diagnostic session: {error}"))?;
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
struct DiagnosticsState {
    sink: Option<PersistentSink>,
    metadata: Option<SessionMetadata>,
    directory: Option<PathBuf>,
    initialized: bool,
    failure_reported: bool,
}

#[derive(Debug, Default)]
struct ExternalLimitState {
    fingerprints: HashMap<u64, (u64, u64)>,
    sources: HashMap<String, SourceWindow>,
    global: Option<SourceWindow>,
}

#[derive(Debug, Clone, Copy)]
struct SourceWindow {
    started_at: u64,
    emitted: u32,
    suppressed: u64,
}

impl ExternalLimitState {
    fn allow(&mut self, source: &str, level: LogLevel, message: &str, now: u64) -> LimitDecision {
        const DUPLICATE_WINDOW_MS: u64 = 10_000;
        const SOURCE_WINDOW_MS: u64 = 60_000;
        const SOURCE_LIMIT: u32 = 100;
        const GLOBAL_LIMIT: u32 = 500;

        if self.fingerprints.len() >= 1_024 {
            self.fingerprints
                .retain(|_, (last_at, _)| now.saturating_sub(*last_at) < DUPLICATE_WINDOW_MS);
        }

        let mut hasher = DefaultHasher::new();
        source.hash(&mut hasher);
        level.as_str().hash(&mut hasher);
        message.hash(&mut hasher);
        let fingerprint = hasher.finish();
        if let Some((last_at, count)) = self.fingerprints.get_mut(&fingerprint) {
            if now.saturating_sub(*last_at) < DUPLICATE_WINDOW_MS {
                *last_at = now;
                *count = count.saturating_add(1);
                if self.sources.len() >= 128 && !self.sources.contains_key(source) {
                    return LimitDecision::Suppress;
                }
                let window = self
                    .sources
                    .entry(source.to_string())
                    .or_insert(SourceWindow {
                        started_at: now,
                        emitted: 0,
                        suppressed: 0,
                    });
                window.suppressed = window.suppressed.saturating_add(1);
                return LimitDecision::Suppress;
            }
        }
        if self.fingerprints.len() >= 1_024 && !self.fingerprints.contains_key(&fingerprint) {
            return LimitDecision::Suppress;
        }
        self.fingerprints.insert(fingerprint, (now, 0));

        let global = self.global.get_or_insert(SourceWindow {
            started_at: now,
            emitted: 0,
            suppressed: 0,
        });
        if now.saturating_sub(global.started_at) >= SOURCE_WINDOW_MS {
            global.started_at = now;
            global.emitted = 0;
            global.suppressed = 0;
        }
        if global.emitted >= GLOBAL_LIMIT {
            global.suppressed = global.suppressed.saturating_add(1);
            return LimitDecision::Suppress;
        }
        global.emitted += 1;

        if self.sources.len() >= 128 && !self.sources.contains_key(source) {
            return LimitDecision::Suppress;
        }

        let window = self
            .sources
            .entry(source.to_string())
            .or_insert(SourceWindow {
                started_at: now,
                emitted: 0,
                suppressed: 0,
            });
        if now.saturating_sub(window.started_at) >= SOURCE_WINDOW_MS {
            let summary = std::mem::take(&mut window.suppressed);
            window.started_at = now;
            window.emitted = 0;
            window.emitted += 1;
            return LimitDecision::Allow { summary };
        }
        if window.emitted >= SOURCE_LIMIT {
            window.suppressed = window.suppressed.saturating_add(1);
            return LimitDecision::Suppress;
        }
        window.emitted += 1;
        let summary = std::mem::take(&mut window.suppressed);
        LimitDecision::Allow { summary }
    }
}

enum LimitDecision {
    Allow { summary: u64 },
    Suppress,
}

#[derive(Debug, Default)]
struct Logger {
    next_id: AtomicU64,
    next_receipt_id: AtomicU64,
    buffer: Mutex<LogBuffer>,
    diagnostics: Mutex<DiagnosticsState>,
    external_limits: Mutex<ExternalLimitState>,
    extended: AtomicBool,
    truncated_records: AtomicU64,
    dropped_records: AtomicU64,
    suppressed_records: AtomicU64,
}

impl Logger {
    fn push(&self, level: LogLevel, source: String, message: String) -> LogEntry {
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

    fn snapshot_after(&self, after_id: u64) -> Vec<LogEntry> {
        lock_unpoisoned(&self.buffer).snapshot_after(after_id)
    }

    fn initialize(&self, config: LoggingConfig) -> Result<(), String> {
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

    fn persist(
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

    fn report_sink_failure(&self, error: &str) {
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

    fn clear(&self) -> Result<(), String> {
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
            for path in session_files(&directory)? {
                if let Err(error) = remove_if_exists(&path) {
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

    fn update_webview_metadata(&self, engine: &str, version: Option<&str>) {
        let engine = sanitize_identifier(engine, 64);
        let version = version.map(|value| sanitize_identifier(value, 128));
        let result = {
            let mut state = lock_unpoisoned(&self.diagnostics);
            if let Some(metadata) = state.metadata.as_mut() {
                metadata.webview_engine = engine.clone();
                metadata.webview_version = version.clone();
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

    fn submit_external(&self, entries: Vec<ExternalLogEntry>) -> Result<(), String> {
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
                            &format!("Suppressed {summary} duplicate or rate-limited browser records from {source}"),
                            "browser",
                        );
                    }
                    self.persist(now, entry.level, &source, &message, "browser");
                }
            }
        }
        Ok(())
    }

    fn report(&self, runtime: DiagnosticReportRuntime) -> String {
        let (metadata, directory) = {
            let state = lock_unpoisoned(&self.diagnostics);
            (state.metadata.clone(), state.directory.clone())
        };
        let mut report = String::with_capacity(64 * 1024);
        push_line(
            &mut report,
            "Stremio Lightning diagnostic report",
            REPORT_LIMIT_BYTES,
        );
        push_line(
            &mut report,
            &format!("schema-version: {DIAGNOSTIC_SCHEMA_VERSION}"),
            REPORT_LIMIT_BYTES,
        );
        push_line(
            &mut report,
            &format!("generated-at: {}", format_timestamp(unix_timestamp_ms())),
            REPORT_LIMIT_BYTES,
        );
        if let Some(metadata) = &metadata {
            push_line(
                &mut report,
                &format!("app-version: {}", metadata.app_version),
                REPORT_LIMIT_BYTES,
            );
            push_line(
                &mut report,
                &format!("platform: {}", metadata.platform),
                REPORT_LIMIT_BYTES,
            );
            push_line(
                &mut report,
                &format!("architecture: {}", metadata.architecture),
                REPORT_LIMIT_BYTES,
            );
            push_line(
                &mut report,
                &format!("shell: {}", metadata.shell),
                REPORT_LIMIT_BYTES,
            );
            let webview = metadata.webview_version.as_deref().unwrap_or("unavailable");
            push_line(
                &mut report,
                &format!("webview: {} {webview}", metadata.webview_engine),
                REPORT_LIMIT_BYTES,
            );
        } else {
            push_line(
                &mut report,
                "application-metadata: unavailable",
                REPORT_LIMIT_BYTES,
            );
        }
        push_line(
            &mut report,
            &format!(
                "native-player-status: {}",
                sanitize_identifier(&runtime.native_player_status, 64)
            ),
            REPORT_LIMIT_BYTES,
        );
        push_line(
            &mut report,
            &format!(
                "streaming-server-running: {}",
                runtime.streaming_server_running
            ),
            REPORT_LIMIT_BYTES,
        );
        push_line(
            &mut report,
            &format!(
                "extended-diagnostics: {}",
                self.extended.load(Ordering::Relaxed)
            ),
            REPORT_LIMIT_BYTES,
        );
        push_line(
            &mut report,
            &format!(
                "truncated-records: {}",
                self.truncated_records.load(Ordering::Relaxed)
            ),
            REPORT_LIMIT_BYTES,
        );
        push_line(
            &mut report,
            &format!(
                "dropped-records: {}",
                self.dropped_records.load(Ordering::Relaxed)
            ),
            REPORT_LIMIT_BYTES,
        );
        push_line(
            &mut report,
            &format!(
                "suppressed-records: {}",
                self.suppressed_records.load(Ordering::Relaxed)
            ),
            REPORT_LIMIT_BYTES,
        );
        push_line(
            &mut report,
            "redaction-notice: credentials, secret values, request bodies, identifiers, and local paths are excluded or redacted; URLs are retained.",
            REPORT_LIMIT_BYTES,
        );

        push_line(
            &mut report,
            "\n=== Application sessions ===",
            REPORT_LIMIT_BYTES,
        );
        let mut wrote_session = false;
        if let Some(directory) = directory {
            match session_files(&directory) {
                Ok(mut files) => {
                    files.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
                    for path in files.into_iter().take(RETAINED_SESSION_COUNT) {
                        if report.len() >= APPLICATION_REPORT_LIMIT_BYTES {
                            break;
                        }
                        match append_session_report(
                            &mut report,
                            &path,
                            APPLICATION_REPORT_LIMIT_BYTES,
                        ) {
                            Ok(wrote) => wrote_session |= wrote,
                            Err(_) => push_line(
                                &mut report,
                                "[A retained application session could not be read.]",
                                APPLICATION_REPORT_LIMIT_BYTES,
                            ),
                        }
                    }
                }
                Err(_) => push_line(
                    &mut report,
                    "[Retained application sessions are unavailable.]",
                    APPLICATION_REPORT_LIMIT_BYTES,
                ),
            }
        }
        if !wrote_session {
            push_line(
                &mut report,
                "[Persistent application records are unavailable; current in-memory records follow.]",
                APPLICATION_REPORT_LIMIT_BYTES,
            );
            for entry in self.snapshot_after(0) {
                push_record_line(&mut report, &entry, APPLICATION_REPORT_LIMIT_BYTES);
            }
        }

        append_server_report(
            &mut report,
            "Streaming server stdout",
            runtime.server_stdout,
        );
        append_server_report(
            &mut report,
            "Streaming server stderr",
            runtime.server_stderr,
        );
        truncate_utf8(&report, REPORT_LIMIT_BYTES)
    }
}

static LOGGER: OnceLock<Logger> = OnceLock::new();
static PANIC_HOOK: Once = Once::new();

fn logger() -> &'static Logger {
    LOGGER.get_or_init(Logger::default)
}

pub fn initialize(config: LoggingConfig) -> Result<(), String> {
    let result = logger().initialize(config);
    install_panic_hook();
    result
}

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

pub fn is_extended() -> bool {
    logger().extended.load(Ordering::Relaxed)
}

pub fn is_persistent_available() -> bool {
    lock_unpoisoned(&logger().diagnostics).sink.is_some()
}

pub fn webview_metadata() -> (String, Option<String>) {
    lock_unpoisoned(&logger().diagnostics)
        .metadata
        .as_ref()
        .map(|metadata| {
            (
                metadata.webview_engine.clone(),
                metadata.webview_version.clone(),
            )
        })
        .unwrap_or_else(|| ("unavailable".to_string(), None))
}

pub fn submit_external(entries: Vec<ExternalLogEntry>) -> Result<(), String> {
    logger().submit_external(entries)
}

pub fn clear_diagnostics() -> Result<(), String> {
    logger().clear()
}

pub fn diagnostic_report(runtime: DiagnosticReportRuntime) -> String {
    logger().report(runtime)
}

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
            let location = panic_info
                .location()
                .map(|location| format!("{}:{}", location.file(), location.line()))
                .unwrap_or_else(|| "unknown location".to_string());
            record_panic(format!("Unhandled panic at {location}: {payload}"));
            previous(panic_info);
        }));
    });
}

fn record_panic(message: String) {
    let entry = LogEntry {
        id: logger().next_id.fetch_add(1, Ordering::Relaxed) + 1,
        timestamp: unix_timestamp_ms(),
        level: LogLevel::Error,
        source: "native.panic".to_string(),
        message: sanitize_message(&message).0,
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

fn sanitize_source(source: &str) -> (String, bool) {
    let mut output = String::with_capacity(source.len().min(MAX_SOURCE_LENGTH));
    let mut truncated = false;
    for character in source.chars() {
        let character = if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-')
        {
            character
        } else {
            '_'
        };
        if output.len() + character.len_utf8() > MAX_SOURCE_LENGTH {
            truncated = true;
            break;
        }
        output.push(character);
    }
    if output.is_empty() {
        output.push_str("unknown");
    }
    let changed_length = output.len() < source.len();
    (output, truncated || changed_length)
}

fn looks_like_url(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    value.contains("://")
        || value.starts_with("magnet:")
        || value.starts_with("data:")
        || value.starts_with("file:")
}

fn sanitize_message(message: &str) -> (String, bool) {
    static URL_CREDENTIALS: OnceLock<Regex> = OnceLock::new();
    static SECRETS: OnceLock<Regex> = OnceLock::new();
    static WINDOWS_PATHS: OnceLock<Regex> = OnceLock::new();
    static HOME_PATHS: OnceLock<Regex> = OnceLock::new();

    let mut clean = String::with_capacity(message.len().min(MAX_MESSAGE_LENGTH));
    for character in message.chars() {
        if character == '\n' || character == '\t' || !character.is_control() {
            clean.push(character);
        } else {
            clean.push(' ');
        }
    }
    clean = URL_CREDENTIALS
        .get_or_init(|| {
            Regex::new(r#"(?i)\b((?:https?|ftp|rtsp)://)[^/\s:@]+:[^@\s/]+@"#)
                .expect("valid URL credential redaction regex")
        })
        .replace_all(&clean, "$1[redacted]@")
        .into_owned();
    clean = SECRETS
        .get_or_init(|| {
            Regex::new(
                r#"(?i)\b(authorization|proxy-authorization|cookie|set-cookie|token|access[_-]?token|refresh[_-]?token|api[_-]?key|password|passwd|secret|session[_-]?id)\b(\\?[\"']?\s*[:=]\s*\\?[\"']?)(?:\"[^\"]*\"|'[^']*'|(?:Bearer\s+)?[^\s,;}\]\)\r\n]+)"#,
            )
            .expect("valid secret redaction regex")
        })
        .replace_all(&clean, "$1$2[redacted]")
        .into_owned();
    clean = WINDOWS_PATHS
        .get_or_init(|| Regex::new(r#"(?i)\b[a-z]:\\[^\r\n\t,;\)\]]+"#).expect("valid path regex"))
        .replace_all(&clean, "[redacted local path]")
        .into_owned();
    clean = HOME_PATHS
        .get_or_init(|| {
            Regex::new(r#"(?i)(?:/home/|/users/)[^\s\"'<>]+"#).expect("valid home path regex")
        })
        .replace_all(&clean, "[redacted local path]")
        .into_owned();
    let truncated = clean.len() > MAX_MESSAGE_LENGTH;
    (truncate_utf8(&clean, MAX_MESSAGE_LENGTH), truncated)
}

fn sanitize_identifier(value: &str, limit: usize) -> String {
    let clean = value
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>();
    truncate_utf8(&sanitize_message(&clean).0, limit)
}

fn append_session_report(report: &mut String, path: &Path, limit: usize) -> Result<bool, String> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut wrote = false;
    let mut current_metadata: Option<SessionMetadata> = None;
    for line in BufReader::new(file).lines() {
        let line = line.map_err(|error| error.to_string())?;
        let Ok(parsed) = serde_json::from_str::<PersistentLine>(&line) else {
            continue;
        };
        match parsed {
            PersistentLine::Session { metadata } => {
                current_metadata = Some(metadata.clone());
                push_line(
                    report,
                    &format!(
                        "\n--- Session {} started {} ---",
                        sanitize_identifier(&metadata.session_id, 64),
                        format_timestamp(metadata.started_at)
                    ),
                    limit,
                );
                wrote = true;
            }
            PersistentLine::Metadata {
                webview_engine,
                webview_version,
            } => {
                if let Some(metadata) = current_metadata.as_mut() {
                    metadata.webview_engine = webview_engine;
                    metadata.webview_version = webview_version;
                }
            }
            PersistentLine::Record {
                timestamp,
                level,
                source,
                message,
                ..
            } => {
                let entry = LogEntry {
                    id: 0,
                    timestamp,
                    level,
                    source: sanitize_source(&sanitize_message(&source).0).0,
                    message: sanitize_message(&message).0,
                };
                push_record_line(report, &entry, limit);
                wrote = true;
            }
        }
        if report.len() >= limit {
            break;
        }
    }
    Ok(wrote)
}

fn append_server_report(report: &mut String, title: &str, content: Result<String, String>) {
    push_line(report, &format!("\n=== {title} ==="), REPORT_LIMIT_BYTES);
    match content {
        Ok(content) if content.trim().is_empty() => {
            push_line(report, "[No retained output.]", REPORT_LIMIT_BYTES)
        }
        Ok(content) => {
            let content = sanitize_report_block(&content, SERVER_REPORT_LIMIT_BYTES);
            push_line(report, &content, REPORT_LIMIT_BYTES);
        }
        Err(_) => push_line(
            report,
            "[Streaming-server output is unavailable.]",
            REPORT_LIMIT_BYTES,
        ),
    }
}

fn sanitize_report_block(value: &str, limit: usize) -> String {
    let mut output = String::with_capacity(value.len().min(limit));
    for line in value.lines() {
        push_line(&mut output, &sanitize_message(line).0, limit);
        if output.len() >= limit {
            break;
        }
    }
    output
}

fn push_record_line(report: &mut String, entry: &LogEntry, limit: usize) {
    push_line(
        report,
        &format!(
            "{} [{}] {}: {}",
            format_timestamp(entry.timestamp),
            entry.level.as_str().to_ascii_uppercase(),
            sanitize_source(&sanitize_message(&entry.source).0).0,
            sanitize_message(&entry.message).0
        ),
        limit,
    );
}

fn push_line(output: &mut String, line: &str, limit: usize) {
    if output.len() >= limit {
        return;
    }
    let remaining = limit.saturating_sub(output.len()).saturating_sub(1);
    output.push_str(&truncate_utf8(line, remaining));
    output.push('\n');
}

fn session_path(directory: &Path, started_at: u64, session_id: &str) -> PathBuf {
    directory.join(format!(
        "{SESSION_FILE_PREFIX}{started_at:020}-{session_id}.ndjson"
    ))
}

fn session_files(directory: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("failed to list diagnostic sessions: {error}")),
    };
    Ok(entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.starts_with(SESSION_FILE_PREFIX) && name.ends_with(".ndjson")
                })
        })
        .collect())
}

fn bound_existing_sessions(directory: &Path, limits: DiagnosticLimits) -> Result<(), String> {
    recover_replacement_files(directory)?;
    for path in session_files(directory)? {
        if fs::metadata(&path)
            .map(|metadata| metadata.len() <= limits.session_bytes)
            .unwrap_or(false)
        {
            continue;
        }
        let metadata = File::open(&path)
            .ok()
            .map(BufReader::new)
            .and_then(|reader| {
                reader.lines().map_while(Result::ok).find_map(|line| {
                    match serde_json::from_str::<PersistentLine>(&line) {
                        Ok(PersistentLine::Session { metadata }) => Some(metadata),
                        _ => None,
                    }
                })
            });
        let Some(metadata) = metadata else {
            remove_if_exists(&path).map_err(|error| {
                format!("failed to remove malformed diagnostic session: {error}")
            })?;
            continue;
        };
        let sink = PersistentSink {
            directory: directory.to_path_buf(),
            active_path: path.clone(),
            metadata,
            limits,
        };
        if sink.compact().is_err() {
            remove_if_exists(&path).map_err(|error| {
                format!("failed to remove oversized diagnostic session: {error}")
            })?;
        }
    }
    Ok(())
}

fn recover_replacement_files(directory: &Path) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("failed to inspect diagnostic replacements: {error}"))?;
    for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if name.ends_with(".ndjson.tmp") {
            remove_if_exists(&path)
                .map_err(|error| format!("failed to remove stale diagnostic file: {error}"))?;
            continue;
        }
        let Some(destination_name) = name.strip_suffix(".replace-backup") else {
            continue;
        };
        let destination = path.with_file_name(destination_name);
        if destination.exists() {
            remove_if_exists(&path).map_err(|error| {
                format!("failed to remove stale diagnostic replacement: {error}")
            })?;
        } else {
            fs::rename(&path, &destination)
                .map_err(|error| format!("failed to restore diagnostic replacement: {error}"))?;
        }
    }
    Ok(())
}

fn replace_file_safely(source: &Path, destination: &Path) -> std::io::Result<()> {
    match fs::rename(source, destination) {
        Ok(()) => return Ok(()),
        Err(error) if !destination.exists() => return Err(error),
        Err(_) => {}
    }

    let backup = destination.with_extension("ndjson.replace-backup");
    remove_if_exists(&backup)?;
    fs::rename(destination, &backup)?;
    if let Err(error) = fs::rename(source, destination) {
        let _ = fs::rename(&backup, destination);
        return Err(error);
    }
    remove_if_exists(&backup)
}

fn remove_if_exists(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn generate_session_id() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        let fallback = unix_timestamp_ms() ^ ((std::process::id() as u64) << 32);
        bytes[..8].copy_from_slice(&fallback.to_le_bytes());
        bytes[8..].copy_from_slice(&fallback.rotate_left(29).to_le_bytes());
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn unix_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn format_timestamp(timestamp_ms: u64) -> String {
    let seconds = timestamp_ms / 1_000;
    let milliseconds = timestamp_ms % 1_000;
    let days = (seconds / 86_400) as i64;
    let seconds_of_day = seconds % 86_400;
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{milliseconds:03}Z")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, i64, i64) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    (year, month, day)
}

fn truncate_utf8(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_string();
    }
    let marker = "... [truncated]";
    if limit <= marker.len() {
        return marker[..limit].to_string();
    }
    let mut end = limit - marker.len();
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{}", &value[..end], marker)
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
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

    fn test_dir(name: &str) -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "stremio-lightning-diagnostics-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn test_config(directory: PathBuf, session: &str) -> LoggingConfig {
        LoggingConfig {
            directory,
            app_version: "1.2.3".to_string(),
            platform: "test".to_string(),
            architecture: "test-arch".to_string(),
            shell: "test-shell".to_string(),
            webview_engine: "test-webview".to_string(),
            webview_version: Some("1.0".to_string()),
            session_id: session.to_string(),
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
    fn entries_serialize_with_compatible_fields() {
        let value = serde_json::to_value(entry(1)).unwrap();
        assert_eq!(value["id"], 1);
        assert_eq!(value["timestamp"], 1);
        assert_eq!(value["level"], "info");
        assert_eq!(value["source"], "native.test");
        assert_eq!(value["message"], "1");
    }

    #[test]
    fn buffer_evicts_oldest_entries_and_clear_preserves_future_ids() {
        let logger = Logger::default();
        for _ in 0..=MAX_ENTRIES {
            logger.push(LogLevel::Info, "native.test".into(), "message".into());
        }
        assert_eq!(logger.snapshot_after(0).len(), MAX_ENTRIES);
        let previous = logger.snapshot_after(0).last().unwrap().id;
        logger.clear().unwrap();
        let next = logger.push(LogLevel::Info, "native.test".into(), "next".into());
        assert!(next.id > previous);
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
        assert_eq!(
            entries.iter().map(|entry| entry.id).collect::<Vec<_>>(),
            (1..=400).collect::<Vec<_>>()
        );
    }

    #[test]
    fn sanitizer_preserves_urls_while_redacting_secrets_and_local_paths() {
        let message = sanitize_text(
            r#"GET https://example.test/a?token=secret) rtsp://user:pass@media.test/file data:text/plain,private Authorization: Bearer123 C:\Users\alice\file /home/alice/file {"token":"json-secret"} {\"token\":\"escaped-secret\"}"#,
        );
        assert!(
            message.contains("https://example.test/a?token=[redacted])"),
            "{message}"
        );
        assert!(message.contains("rtsp://[redacted]@media.test/file"));
        assert!(message.contains("data:text/plain,private"));
        assert!(!message.contains("Bearer123"));
        assert!(!message.contains("json-secret"));
        assert!(!message.contains("escaped-secret"));
        assert!(!message.contains("alice"));
    }

    #[test]
    fn sink_compacts_and_retains_three_sessions() {
        let directory = test_dir("retention");
        for index in 0..4 {
            let mut sink = PersistentSink::new(
                test_config(directory.clone(), &format!("session-{index}")),
                DiagnosticLimits {
                    session_bytes: 512,
                    sessions: 3,
                },
            )
            .unwrap();
            for record in 0..30 {
                sink.append_record(
                    record,
                    unix_timestamp_ms(),
                    LogLevel::Info,
                    "native.test",
                    &"x".repeat(80),
                    "native",
                )
                .unwrap();
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let files = session_files(&directory).unwrap();
        assert_eq!(files.len(), 3);
        assert!(files
            .iter()
            .all(|path| fs::metadata(path).unwrap().len() <= 512));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn startup_compacts_oversized_previous_sessions() {
        let directory = test_dir("startup-budget");
        let mut old = PersistentSink::new(
            test_config(directory.clone(), "old-large"),
            DiagnosticLimits {
                session_bytes: 16 * 1024,
                sessions: 3,
            },
        )
        .unwrap();
        for receipt in 0..50 {
            old.append_record(
                receipt,
                unix_timestamp_ms(),
                LogLevel::Info,
                "native.test",
                &"x".repeat(120),
                "native",
            )
            .unwrap();
        }
        assert!(fs::metadata(&old.active_path).unwrap().len() > 512);
        drop(old);

        let _current = PersistentSink::new(
            test_config(directory.clone(), "current"),
            DiagnosticLimits {
                session_bytes: 512,
                sessions: 3,
            },
        )
        .unwrap();

        assert!(session_files(&directory)
            .unwrap()
            .iter()
            .all(|path| fs::metadata(path).unwrap().len() <= 512));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn baseline_omits_debug_and_external_records_do_not_enter_native_ring() {
        let directory = test_dir("external");
        let logger = Logger::default();
        logger
            .initialize(test_config(directory.clone(), "external"))
            .unwrap();
        logger
            .submit_external(vec![
                ExternalLogEntry {
                    level: LogLevel::Debug,
                    source: "bridge.test".to_string(),
                    message: "debug".to_string(),
                    timestamp: Some(1),
                },
                ExternalLogEntry {
                    level: LogLevel::Error,
                    source: "bridge.test".to_string(),
                    message: "error".to_string(),
                    timestamp: Some(1),
                },
                ExternalLogEntry {
                    level: LogLevel::Error,
                    source: "rtsp://user:secret@private.example/source?token=hidden".to_string(),
                    message: "source redaction".to_string(),
                    timestamp: Some(1),
                },
            ])
            .unwrap();
        assert!(logger.snapshot_after(0).is_empty());
        let text = fs::read_to_string(session_files(&directory).unwrap()[0].clone()).unwrap();
        assert!(!text.contains("\"message\":\"debug\""));
        assert!(text.contains("\"message\":\"error\""));
        assert!(text.contains("browser.bridge.test"));
        assert!(!text.contains("private.example"));
        assert!(!text.contains("hidden"));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn clear_removes_retained_records_and_starts_a_fresh_segment() {
        let directory = test_dir("clear-persistent");
        let logger = Logger::default();
        logger
            .initialize(test_config(directory.clone(), "clear"))
            .unwrap();
        logger.persist(
            unix_timestamp_ms(),
            LogLevel::Error,
            "native.test",
            "before clear",
            "native",
        );

        logger.clear().unwrap();

        let files = session_files(&directory).unwrap();
        assert_eq!(files.len(), 1);
        assert!(!fs::read_to_string(&files[0])
            .unwrap()
            .contains("before clear"));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn clear_recovers_and_erases_files_after_the_sink_is_disabled() {
        let directory = test_dir("clear-disabled-sink");
        let logger = Logger::default();
        logger
            .initialize(test_config(directory.clone(), "clear-disabled"))
            .unwrap();
        logger.persist(
            unix_timestamp_ms(),
            LogLevel::Error,
            "native.test",
            "retained before failure",
            "native",
        );
        lock_unpoisoned(&logger.diagnostics).sink = None;

        logger.clear().unwrap();

        let files = session_files(&directory).unwrap();
        assert_eq!(files.len(), 1);
        assert!(!fs::read_to_string(&files[0])
            .unwrap()
            .contains("retained before failure"));
        assert!(lock_unpoisoned(&logger.diagnostics).sink.is_some());
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn external_rate_state_is_strictly_bounded_across_source_churn() {
        let mut limits = ExternalLimitState::default();
        let mut allowed = 0;
        for index in 0..2_000 {
            if matches!(
                limits.allow(
                    &format!("browser.source-{index}"),
                    LogLevel::Info,
                    &format!("message-{index}"),
                    1,
                ),
                LimitDecision::Allow { .. }
            ) {
                allowed += 1;
            }
        }
        assert!(allowed <= 500);
        assert!(limits.sources.len() <= 128);
        assert!(limits.fingerprints.len() <= 1_024);
    }

    #[test]
    fn disk_initialization_failure_keeps_the_memory_logger_available() {
        let directory = test_dir("disk-failure");
        let invalid_directory = directory.join("not-a-directory");
        fs::write(&invalid_directory, "file").unwrap();
        let logger = Logger::default();

        assert!(logger
            .initialize(test_config(invalid_directory, "failure"))
            .is_err());
        logger.push(LogLevel::Info, "native.test".into(), "still live".into());
        assert_eq!(
            logger.snapshot_after(0).last().unwrap().message,
            "still live"
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn report_is_bounded_and_redacted() {
        let directory = test_dir("report");
        let logger = Logger::default();
        logger
            .initialize(test_config(directory.clone(), "report"))
            .unwrap();
        logger.persist(
            unix_timestamp_ms(),
            LogLevel::Error,
            "native.test",
            &sanitize_text("failed https://secret.test/?token=abc"),
            "native",
        );
        let report = logger.report(DiagnosticReportRuntime {
            native_player_status: "available".to_string(),
            streaming_server_running: true,
            server_stdout: Ok("safe".to_string()),
            server_stderr: Ok("Authorization: secret".to_string()),
        });
        assert!(report.len() <= REPORT_LIMIT_BYTES);
        assert!(report.contains("https://secret.test/?token=[redacted]"));
        assert!(!report.contains("Authorization: secret"));
        assert!(report.contains("Session report"));
        let _ = fs::remove_dir_all(directory);
    }
}
