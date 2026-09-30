use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const MAX_ENTRIES: usize = 2_000;
pub const MAX_EXTERNAL_BATCH_ENTRIES: usize = 50;
pub const MAX_EXTERNAL_BATCH_BYTES: usize = 256 * 1024;
pub const MAX_SOURCE_LENGTH: usize = 256;
pub const MAX_MESSAGE_LENGTH: usize = 16 * 1024;
pub const SESSION_LIMIT_BYTES: u64 = 2 * 1024 * 1024;
pub const RETAINED_SESSION_COUNT: usize = 3;
pub const REPORT_LIMIT_BYTES: usize = 10 * 1024 * 1024;
pub const APPLICATION_REPORT_LIMIT_BYTES: usize = 6 * 1024 * 1024;
pub const SERVER_REPORT_LIMIT_BYTES: usize = 2 * 1024 * 1024;
pub const DIAGNOSTIC_SCHEMA_VERSION: u32 = 1;
pub const SESSION_FILE_PREFIX: &str = "diagnostics-session-";

#[derive(Debug, Error)]
pub enum LoggingError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("Lock poisoned: {0}")]
    LockPoisoned(String),
    #[error("Batch validation error: {0}")]
    BatchLimitExceeded(String),
    #[error("Storage failure: {0}")]
    Storage(String),
}

impl From<String> for LoggingError {
    fn from(message: String) -> Self {
        Self::Storage(message)
    }
}

impl From<&str> for LoggingError {
    fn from(message: &str) -> Self {
        Self::Storage(message.to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }

    #[must_use]
    pub fn persists_at_baseline(self) -> bool {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetadata {
    pub schema_version: u32,
    pub session_id: String,
    pub started_at: u64,
    pub app_version: String,
    pub platform: String,
    pub architecture: String,
    pub shell: String,
    pub webview_engine: String,
    pub webview_version: Option<String>,
}

impl SessionMetadata {
    pub fn from_config(config: LoggingConfig) -> Self {
        Self {
            schema_version: DIAGNOSTIC_SCHEMA_VERSION,
            session_id: crate::logging::sanitizer::sanitize_identifier(&config.session_id, 64),
            started_at: unix_timestamp_ms(),
            app_version: crate::logging::sanitizer::sanitize_identifier(&config.app_version, 64),
            platform: crate::logging::sanitizer::sanitize_identifier(&config.platform, 32),
            architecture: crate::logging::sanitizer::sanitize_identifier(&config.architecture, 32),
            shell: crate::logging::sanitizer::sanitize_identifier(&config.shell, 64),
            webview_engine: crate::logging::sanitizer::sanitize_identifier(
                &config.webview_engine,
                64,
            ),
            webview_version: config
                .webview_version
                .map(|value| crate::logging::sanitizer::sanitize_identifier(&value, 128)),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PersistentLine {
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
pub struct DiagnosticLimits {
    pub session_bytes: u64,
    pub sessions: usize,
}

impl Default for DiagnosticLimits {
    fn default() -> Self {
        Self {
            session_bytes: SESSION_LIMIT_BYTES,
            sessions: RETAINED_SESSION_COUNT,
        }
    }
}

pub enum LimitDecision {
    Allow { summary: u64 },
    Suppress,
}

#[must_use]
pub fn generate_session_id() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        let fallback = unix_timestamp_ms() ^ ((std::process::id() as u64) << 32);
        bytes[..8].copy_from_slice(&fallback.to_le_bytes());
        bytes[8..].copy_from_slice(&fallback.rotate_left(29).to_le_bytes());
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[must_use]
pub fn unix_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[must_use]
pub fn format_timestamp(timestamp_ms: u64) -> String {
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

#[must_use]
pub fn truncate_utf8(value: &str, limit: usize) -> String {
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
