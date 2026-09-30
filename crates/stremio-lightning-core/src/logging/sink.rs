use std::collections::{hash_map::DefaultHasher, HashMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use super::types::{
    unix_timestamp_ms, DiagnosticLimits, LimitDecision, LogEntry, LogLevel,
    LoggingConfig, PersistentLine, SessionMetadata,
    DIAGNOSTIC_SCHEMA_VERSION, MAX_ENTRIES, SESSION_FILE_PREFIX,
};

#[derive(Debug, Default)]
pub struct LogBuffer {
    entries: VecDeque<LogEntry>,
}

impl LogBuffer {
    pub fn push(&mut self, entry: LogEntry) {
        if self.entries.len() == MAX_ENTRIES {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    pub fn snapshot_after(&self, after_id: u64) -> Vec<LogEntry> {
        self.entries
            .iter()
            .filter(|entry| entry.id > after_id)
            .cloned()
            .collect()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

#[derive(Debug)]
pub struct PersistentSink {
    pub directory: PathBuf,
    pub active_path: PathBuf,
    pub metadata: SessionMetadata,
    pub limits: DiagnosticLimits,
}

impl PersistentSink {
    pub fn new(config: LoggingConfig, limits: DiagnosticLimits) -> Result<Self, String> {
        let directory = config.directory.clone();
        fs::create_dir_all(&directory)
            .map_err(|error| format!("failed to create diagnostics directory: {error}"))?;
        bound_existing_sessions(&directory, limits)?;
        let metadata = SessionMetadata::from_config(config);
        Self::from_metadata(directory, metadata, limits)
    }

    pub fn from_metadata(
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

    pub fn append_record(
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

    pub fn update_webview_metadata(
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

    pub fn append(&self, line: &PersistentLine) -> Result<(), String> {
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

    pub fn write_fresh_header(&mut self) -> Result<(), String> {
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

    pub fn compact(&self) -> Result<(), String> {
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

    pub fn clear(&mut self) -> Result<(), String> {
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

    pub fn enforce_retention(&self) -> Result<(), String> {
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
pub struct ExternalLimitState {
    pub fingerprints: HashMap<u64, (u64, u64)>,
    pub sources: HashMap<String, SourceWindow>,
    pub global: Option<SourceWindow>,
}

#[derive(Debug, Clone, Copy)]
pub struct SourceWindow {
    pub started_at: u64,
    pub emitted: u32,
    pub suppressed: u64,
}

impl ExternalLimitState {
    pub fn allow(
        &mut self,
        source: &str,
        level: LogLevel,
        message: &str,
        now: u64,
    ) -> LimitDecision {
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

#[must_use]
pub fn session_path(directory: &Path, started_at: u64, session_id: &str) -> PathBuf {
    directory.join(format!(
        "{SESSION_FILE_PREFIX}{started_at:020}-{session_id}.ndjson"
    ))
}

pub fn session_files(directory: &Path) -> Result<Vec<PathBuf>, String> {
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

pub fn bound_existing_sessions(directory: &Path, limits: DiagnosticLimits) -> Result<(), String> {
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

pub fn recover_replacement_files(directory: &Path) -> Result<(), String> {
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

pub fn replace_file_safely(source: &Path, destination: &Path) -> std::io::Result<()> {
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

pub fn remove_if_exists(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
