use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use super::sanitizer::{sanitize_identifier, sanitize_message, sanitize_source};
use super::sink::session_files;
use super::types::{
    format_timestamp, truncate_utf8, unix_timestamp_ms, DiagnosticReportRuntime,
    LogEntry, PersistentLine, SessionMetadata, APPLICATION_REPORT_LIMIT_BYTES,
    DIAGNOSTIC_SCHEMA_VERSION, REPORT_LIMIT_BYTES, RETAINED_SESSION_COUNT,
    SERVER_REPORT_LIMIT_BYTES,
};

pub fn build_report(
    metadata: Option<&SessionMetadata>,
    directory: Option<&Path>,
    runtime: DiagnosticReportRuntime,
    extended: &AtomicBool,
    truncated_records: &AtomicU64,
    dropped_records: &AtomicU64,
    suppressed_records: &AtomicU64,
    in_memory_records: &[LogEntry],
) -> String {
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
    if let Some(metadata) = metadata {
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
            extended.load(Ordering::Relaxed)
        ),
        REPORT_LIMIT_BYTES,
    );
    push_line(
        &mut report,
        &format!(
            "truncated-records: {}",
            truncated_records.load(Ordering::Relaxed)
        ),
        REPORT_LIMIT_BYTES,
    );
    push_line(
        &mut report,
        &format!(
            "dropped-records: {}",
            dropped_records.load(Ordering::Relaxed)
        ),
        REPORT_LIMIT_BYTES,
    );
    push_line(
        &mut report,
        &format!(
            "suppressed-records: {}",
            suppressed_records.load(Ordering::Relaxed)
        ),
        REPORT_LIMIT_BYTES,
    );
    push_line(
        &mut report,
        concat!(
            "redaction-notice: credentials, secret values, request bodies, ",
            "identifiers, and local paths are excluded or redacted; URLs are retained.",
        ),
        REPORT_LIMIT_BYTES,
    );

    push_line(
        &mut report,
        "\n=== Application sessions ===",
        REPORT_LIMIT_BYTES,
    );
    let mut wrote_session = false;
    if let Some(dir) = directory {
        match session_files(dir) {
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
        for entry in in_memory_records {
            push_record_line(&mut report, entry, APPLICATION_REPORT_LIMIT_BYTES);
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

pub fn append_session_report(
    report: &mut String,
    path: &Path,
    limit: usize,
) -> Result<bool, String> {
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

pub fn append_server_report(report: &mut String, title: &str, content: Result<String, String>) {
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

pub fn sanitize_report_block(value: &str, limit: usize) -> String {
    let mut output = String::with_capacity(value.len().min(limit));
    for line in value.lines() {
        push_line(&mut output, &sanitize_message(line).0, limit);
        if output.len() >= limit {
            break;
        }
    }
    output
}

pub fn push_record_line(report: &mut String, entry: &LogEntry, limit: usize) {
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

pub fn push_line(output: &mut String, line: &str, limit: usize) {
    if output.len() >= limit {
        return;
    }
    let remaining = limit.saturating_sub(output.len()).saturating_sub(1);
    output.push_str(&truncate_utf8(line, remaining));
    output.push('\n');
}
