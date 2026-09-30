use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use super::sink::{session_files, ExternalLimitState, PersistentSink};
use super::types::{
    unix_timestamp_ms, DiagnosticLimits, DiagnosticReportRuntime,
    ExternalLogEntry, LimitDecision, LogEntry, LogLevel, LoggingConfig, MAX_ENTRIES,
    REPORT_LIMIT_BYTES,
};
use super::{lock_unpoisoned, sanitize_text, Logger};

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
    let message = sanitize_text(concat!(
        r#"GET https://example.test/a?token=secret) rtsp://user:pass@media.test/file "#,
        r#"data:text/plain,private Authorization: Bearer123 C:\Users\alice\file "#,
        r#"/home/alice/file {"token":"json-secret"} {\"token\":\"escaped-secret\"}"#,
    ));
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
