//! Thread-safe, sanitized, size-bounded log file writer with single `.1` rotation.
//!
//! Shared by `plumbing::logs` (`app.log`) and `plumbing::stream_server` (`stremio-server.*.log`)
//! so file rotation, locking, and secret redaction live in one place.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::sanitize::{sanitize_text, truncate_utf8};

pub(crate) const LINE_LIMIT_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct RotatingLogWriter {
    path: PathBuf,
    backup_path: PathBuf,
    lock: Arc<Mutex<()>>,
    limit_bytes: u64,
    line_limit_bytes: usize,
}

impl RotatingLogWriter {
    pub(crate) fn new(path: impl Into<PathBuf>, limit_bytes: u64) -> Self {
        let path = path.into();
        Self {
            backup_path: backup_path(&path),
            lock: Arc::new(Mutex::new(())),
            path,
            limit_bytes: limit_bytes.max(1),
            line_limit_bytes: LINE_LIMIT_BYTES,
        }
    }

    /// Sanitizes and appends one line, rotating `path` to `<path>.1` when exceeding `limit_bytes`.
    pub(crate) fn write_line(&self, line: &str) -> io::Result<()> {
        let clean = sanitize_text(line);
        self.write_sanitized_line(clean.trim_end_matches(['\r', '\n']))
    }

    pub(crate) fn write_sanitized_line(&self, line: &str) -> io::Result<()> {
        let max_line = self
            .line_limit_bytes
            .min(usize::try_from(self.limit_bytes.saturating_sub(1)).unwrap_or(usize::MAX));
        let mut record = truncate_utf8(line, max_line);
        record.push('\n');
        let bytes = record.as_bytes();

        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let current_size = fs::metadata(&self.path).map_or(0, |meta| meta.len());
        if current_size > 0 && current_size.saturating_add(bytes.len() as u64) > self.limit_bytes {
            self.rotate_locked()?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(bytes)
    }

    fn rotate_locked(&self) -> io::Result<()> {
        remove_if_exists(&self.backup_path)?;
        match fs::rename(&self.path, &self.backup_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }
}

fn backup_path(path: &Path) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(".1");
    PathBuf::from(os)
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "stremio-lightning-rotlog-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        dir.join("test.log")
    }

    #[test]
    fn rotates_and_redacts_secrets_on_write() {
        let path = temp_path("rotate");
        let writer = RotatingLogWriter::new(&path, 48);
        writer.write_line("first line token=secret123").unwrap();
        writer
            .write_line("second line triggers rotation to backup")
            .unwrap();

        let current = fs::read_to_string(&path).unwrap();
        let backup = fs::read_to_string(backup_path(&path)).unwrap();
        assert!(backup.contains("token=[redacted]"));
        assert!(!backup.contains("secret123"));
        assert!(current.contains("second line"));
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
