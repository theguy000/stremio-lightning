use crate::logging;
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub const STREAM_LOG_LIMIT_BYTES: u64 = 1024 * 1024;
pub const STREAM_LOG_LINE_LIMIT_BYTES: usize = 16 * 1024;

static WRITER_LOCKS: OnceLock<Mutex<HashMap<PathBuf, Weak<Mutex<()>>>>> = OnceLock::new();

#[derive(Clone, Debug)]
pub struct RotatingLogWriter {
    path: PathBuf,
    backup_path: PathBuf,
    lock: Arc<Mutex<()>>,
    limit_bytes: u64,
    line_limit_bytes: usize,
}

impl RotatingLogWriter {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self::with_limits(path, STREAM_LOG_LIMIT_BYTES, STREAM_LOG_LINE_LIMIT_BYTES)
    }

    pub fn with_limits(
        path: impl Into<PathBuf>,
        limit_bytes: u64,
        line_limit_bytes: usize,
    ) -> Self {
        let path = path.into();
        let writer = Self {
            backup_path: backup_path(&path),
            lock: writer_lock(&path),
            path,
            limit_bytes: limit_bytes.max(1),
            line_limit_bytes: line_limit_bytes.max(1),
        };
        // Old releases appended directly to these files. Bound any legacy files
        // immediately, without making a diagnostics failure fatal to startup.
        let _ = writer.trim_existing_files();
        writer
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn backup_path(&self) -> &Path {
        &self.backup_path
    }

    /// # Errors
    /// Returns the underlying I/O error when the line cannot be written.
    pub fn write_line(&self, line: &str) -> io::Result<()> {
        let line = sanitize_server_line_with_limit(line, self.line_limit_bytes);
        self.write_sanitized_line(&line)
    }

    /// # Errors
    /// Returns the underlying I/O error when the line cannot be written.
    pub fn write_sanitized_line(&self, line: &str) -> io::Result<()> {
        let maximum_line_bytes = self
            .line_limit_bytes
            .min(usize::try_from(self.limit_bytes.saturating_sub(1)).unwrap_or(usize::MAX));
        let mut record = truncate_utf8(line, maximum_line_bytes);
        record.push('\n');
        let record = record.as_bytes();
        let _guard = self.lock.lock().map_err(lock_error)?;

        ensure_parent_exists(&self.path)?;
        let current_size = fs::metadata(&self.path).map_or(0, |metadata| metadata.len());
        if current_size > 0 && current_size.saturating_add(record.len() as u64) > self.limit_bytes {
            self.rotate_locked()?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(record)
    }

    /// # Errors
    /// Returns the underlying I/O error when the log file cannot be cleared.
    pub fn clear(&self) -> io::Result<()> {
        let _guard = self.lock.lock().map_err(lock_error)?;
        remove_if_exists(&self.path)?;
        remove_if_exists(&self.backup_path)
    }

    fn rotate_locked(&self) -> io::Result<()> {
        remove_if_exists(&self.backup_path)?;
        if fs::metadata(&self.path).is_ok_and(|metadata| metadata.len() > self.limit_bytes) {
            let tail = read_tail(
                &self.path,
                usize::try_from(self.limit_bytes).unwrap_or(usize::MAX),
            )?;
            fs::write(&self.backup_path, tail)?;
            remove_if_exists(&self.path)?;
            return Ok(());
        }
        match fs::rename(&self.path, &self.backup_path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn trim_existing_files(&self) -> io::Result<()> {
        let _guard = self.lock.lock().map_err(lock_error)?;
        sanitize_existing_file(&self.path, self.limit_bytes, self.line_limit_bytes)?;
        sanitize_existing_file(&self.backup_path, self.limit_bytes, self.line_limit_bytes)
    }
}

#[derive(Clone, Debug)]
pub struct StreamingLogFiles {
    stdout: RotatingLogWriter,
    stderr: RotatingLogWriter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamingLogPaths {
    pub stdout: PathBuf,
    pub stderr: PathBuf,
    pub stdout_backup: PathBuf,
    pub stderr_backup: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamingLogTails {
    pub stdout: String,
    pub stderr: String,
}

impl StreamingLogFiles {
    pub fn new(stdout: impl Into<PathBuf>, stderr: impl Into<PathBuf>) -> Self {
        Self {
            stdout: RotatingLogWriter::new(stdout),
            stderr: RotatingLogWriter::new(stderr),
        }
    }

    #[must_use]
    pub fn stdout(&self) -> &RotatingLogWriter {
        &self.stdout
    }

    #[must_use]
    pub fn stderr(&self) -> &RotatingLogWriter {
        &self.stderr
    }

    #[must_use]
    pub fn paths(&self) -> StreamingLogPaths {
        StreamingLogPaths {
            stdout: self.stdout.path.clone(),
            stderr: self.stderr.path.clone(),
            stdout_backup: self.stdout.backup_path.clone(),
            stderr_backup: self.stderr.backup_path.clone(),
        }
    }

    /// # Errors
    /// Returns the underlying I/O error when the log files cannot be cleared.
    pub fn clear(&self) -> io::Result<()> {
        self.stdout.clear()?;
        self.stderr.clear()
    }

    /// # Errors
    /// Returns the underlying I/O error when the retained log tails cannot be read.
    pub fn tails(&self, max_bytes_per_stream: usize) -> io::Result<StreamingLogTails> {
        Ok(StreamingLogTails {
            stdout: read_sanitized_tail(&self.stdout, max_bytes_per_stream)?,
            stderr: read_sanitized_tail(&self.stderr, max_bytes_per_stream)?,
        })
    }
}

/// A real child whose output readers are always joined once it exits or is stopped.
#[derive(Debug)]
pub struct ManagedChild {
    child: Child,
    stdout_reader: Option<JoinHandle<()>>,
    stderr_reader: Option<JoinHandle<()>>,
}

impl ManagedChild {
    /// # Errors
    /// Returns an error when the log files cannot be prepared for the child process.
    pub fn spawn(command: &mut Command, logs: StreamingLogFiles) -> Result<Self, String> {
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        let child = command
            .spawn()
            .map_err(|error| format!("Failed to spawn streaming server: {error}"))?;
        Self::from_child(child, logs)
    }

    /// # Errors
    /// Returns an error when the child stdout and stderr pipes cannot be attached.
    pub fn from_child(mut child: Child, logs: StreamingLogFiles) -> Result<Self, String> {
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Streaming server stdout pipe was unavailable".to_string())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "Streaming server stderr pipe was unavailable".to_string())?;
        let forwarder = Arc::new(StderrForwarder::default());

        Ok(Self {
            child,
            stdout_reader: Some(spawn_reader(stdout, logs.stdout, None)),
            stderr_reader: Some(spawn_reader(stderr, logs.stderr, Some(forwarder))),
        })
    }

    /// # Errors
    /// Returns an error when the child process cannot be stopped.
    pub fn stop(&mut self) -> Result<(), String> {
        let mut result = Ok(());
        if self
            .child
            .try_wait()
            .map_err(|error| format!("Failed to inspect streaming server: {error}"))?
            .is_none()
        {
            if let Err(error) = self.child.kill() {
                result = Err(format!("Failed to stop streaming server: {error}"));
            }
            if let Err(error) = self.child.wait() {
                result = Err(format!("Failed to wait for streaming server: {error}"));
            }
        }
        self.join_readers();
        result
    }

    /// # Errors
    /// Returns an error when the child process state cannot be read.
    pub fn has_exited(&mut self) -> Result<bool, String> {
        self.try_wait()
            .map(|status| status.is_some())
            .map_err(|error| format!("Failed to inspect streaming server: {error}"))
    }

    /// # Errors
    /// Returns an error when the child process state cannot be read.
    pub fn process_has_exited(&mut self) -> Result<bool, String> {
        self.child
            .try_wait()
            .map(|status| status.is_some())
            .map_err(|error| format!("Failed to inspect streaming server: {error}"))
    }

    /// # Errors
    /// Returns an error when the child process cannot be waited on.
    pub fn wait_for_exit(&mut self) -> Result<(), String> {
        if self
            .child
            .try_wait()
            .map_err(|error| format!("Failed to inspect streaming server: {error}"))?
            .is_none()
        {
            self.child
                .wait()
                .map_err(|error| format!("Failed to wait for streaming server: {error}"))?;
        }
        self.join_readers();
        Ok(())
    }

    /// # Errors
    /// Returns the underlying I/O error when the child process state cannot be read.
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        let status = self.child.try_wait()?;
        if status.is_some() {
            self.join_readers();
        }
        Ok(status)
    }

    fn join_readers(&mut self) {
        if let Some(reader) = self.stdout_reader.take() {
            let _ = reader.join();
        }
        if let Some(reader) = self.stderr_reader.take() {
            let _ = reader.join();
        }
    }
}

impl Drop for ManagedChild {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[derive(Default)]
struct StderrForwarder {
    state: Mutex<StderrForwardState>,
}

struct StderrForwardState {
    window_started: Instant,
    emitted: usize,
    suppressed: usize,
    last_line: String,
    last_line_at: Instant,
}

impl Default for StderrForwardState {
    fn default() -> Self {
        Self {
            window_started: Instant::now(),
            emitted: 0,
            suppressed: 0,
            last_line: String::new(),
            last_line_at: Instant::now(),
        }
    }
}

impl StderrForwarder {
    fn forward(&self, line: String) {
        const WINDOW: Duration = Duration::from_secs(60);
        const MAX_LINES_PER_WINDOW: usize = 20;

        // Stack frames remain in the rotating file and exported report. Showing
        // every frame as an independent UI warning obscures the actual error.
        if line.trim_start().starts_with("at ") {
            return;
        }

        let mut messages = Vec::new();
        if let Ok(mut state) = self.state.lock() {
            if state.window_started.elapsed() >= WINDOW {
                if state.suppressed > 0 {
                    messages.push(format!(
                        "Suppressed {} streaming-server stderr lines in the previous minute",
                        state.suppressed
                    ));
                }
                state.window_started = Instant::now();
                state.emitted = 0;
                state.suppressed = 0;
            }
            if state.last_line == line && state.last_line_at.elapsed() < Duration::from_secs(10) {
                state.last_line_at = Instant::now();
                state.suppressed += 1;
                return;
            }
            state.last_line.clone_from(&line);
            state.last_line_at = Instant::now();
            if state.emitted < MAX_LINES_PER_WINDOW {
                state.emitted += 1;
                messages.push(line);
            } else {
                state.suppressed += 1;
            }
        }

        // The rotating-writer lock is released before this logger call.
        for message in messages {
            logging::warn("streaming-server.stderr", message);
        }
    }

    fn finish(&self) {
        let suppressed = self
            .state
            .lock()
            .map_or(0, |mut state| std::mem::take(&mut state.suppressed));
        if suppressed > 0 {
            logging::warn(
                "streaming-server.stderr",
                format!("Suppressed {suppressed} streaming-server stderr lines"),
            );
        }
    }
}

fn spawn_reader<R: Read + Send + 'static>(
    mut reader: R,
    writer: RotatingLogWriter,
    forwarder: Option<Arc<StderrForwarder>>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut bytes = [0_u8; 4096];
        let mut line = Vec::with_capacity(STREAM_LOG_LINE_LIMIT_BYTES);
        let mut truncated = false;
        loop {
            let read = match reader.read(&mut bytes) {
                Ok(0) => {
                    write_drained_line(&writer, forwarder.as_ref(), &line, truncated);
                    if let Some(forwarder) = forwarder {
                        forwarder.finish();
                    }
                    return;
                }
                Ok(read) => read,
                Err(_) => {
                    if let Some(forwarder) = forwarder {
                        forwarder.finish();
                    }
                    return;
                }
            };
            for byte in &bytes[..read] {
                if *byte == b'\n' {
                    write_drained_line(&writer, forwarder.as_ref(), &line, truncated);
                    line.clear();
                    truncated = false;
                } else if line.len() < STREAM_LOG_LINE_LIMIT_BYTES {
                    line.push(*byte);
                } else {
                    truncated = true;
                }
            }
        }
    })
}

fn write_drained_line(
    writer: &RotatingLogWriter,
    forwarder: Option<&Arc<StderrForwarder>>,
    line: &[u8],
    truncated: bool,
) {
    if line.is_empty() && !truncated {
        return;
    }
    let mut line = String::from_utf8_lossy(line).into_owned();
    if truncated {
        line.push_str(" [truncated server line]");
    }
    let sanitized = sanitize_server_line(&line);
    let _ = writer.write_sanitized_line(&sanitized);
    if let Some(forwarder) = forwarder {
        forwarder.forward(sanitized);
    }
}

/// Removes credentials and local paths from untrusted server output.
#[must_use]
pub fn sanitize_server_line(line: &str) -> String {
    sanitize_server_line_with_limit(line, STREAM_LOG_LINE_LIMIT_BYTES)
}

fn sanitize_server_line_with_limit(line: &str, limit: usize) -> String {
    let clean = logging::sanitize_text(line);
    truncate_utf8(clean.trim(), limit)
}

fn read_sanitized_tail(writer: &RotatingLogWriter, max_bytes: usize) -> io::Result<String> {
    if max_bytes == 0 {
        return Ok(String::new());
    }
    let _guard = writer.lock.lock().map_err(lock_error)?;
    let current = read_tail(&writer.path, max_bytes)?;
    let remaining = max_bytes.saturating_sub(current.len());
    let backup = if remaining > 0 {
        read_tail(&writer.backup_path, remaining)?
    } else {
        Vec::new()
    };
    let mut result = String::from_utf8_lossy(&backup).into_owned();
    result.push_str(&String::from_utf8_lossy(&current));
    Ok(result
        .lines()
        .map(sanitize_server_line)
        .collect::<Vec<_>>()
        .join("\n"))
    .map(|value| truncate_utf8(&value, max_bytes))
}

fn read_tail(path: &Path, max_bytes: usize) -> io::Result<Vec<u8>> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let length = file.metadata()?.len();
    let start = length.saturating_sub(max_bytes as u64);
    file.seek(SeekFrom::Start(start))?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail)?;
    Ok(tail)
}

fn writer_lock(path: &Path) -> Arc<Mutex<()>> {
    let locks = WRITER_LOCKS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut locks = match locks.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(lock) = locks.get(path).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(Mutex::new(()));
    locks.insert(path.to_path_buf(), Arc::downgrade(&lock));
    lock
}

fn backup_path(path: &Path) -> PathBuf {
    let mut backup = path.as_os_str().to_os_string();
    backup.push(".1");
    PathBuf::from(backup)
}

fn ensure_parent_exists(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn remove_if_exists(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn sanitize_existing_file(
    path: &Path,
    limit_bytes: u64,
    line_limit_bytes: usize,
) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let tail = read_tail(path, usize::try_from(limit_bytes).unwrap_or(usize::MAX))?;
    let sanitized = String::from_utf8_lossy(&tail)
        .lines()
        .map(|line| sanitize_server_line_with_limit(line, line_limit_bytes))
        .collect::<Vec<_>>()
        .join("\n");
    let mut sanitized = truncate_utf8(
        &sanitized,
        usize::try_from(limit_bytes.saturating_sub(1)).unwrap_or(usize::MAX),
    );
    if !sanitized.is_empty() {
        sanitized.push('\n');
    }
    fs::write(path, sanitized)?;
    Ok(())
}

fn lock_error<T>(_: std::sync::PoisonError<T>) -> io::Error {
    io::Error::other("streaming log writer lock poisoned")
}

fn truncate_utf8(value: &str, limit: usize) -> String {
    if value.len() <= limit {
        return value.to_string();
    }
    let marker = " [truncated]";
    if limit <= marker.len() {
        return marker[..limit].to_string();
    }
    let mut end = limit.saturating_sub(marker.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{}", &value[..end], marker)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn test_dir(name: &str) -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "stremio-lightning-streaming-logs-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn rotates_current_log_into_one_backup() {
        let dir = test_dir("rotation");
        let writer = RotatingLogWriter::with_limits(dir.join("server.log"), 16, 16);
        writer.write_line("first-line").unwrap();
        writer.write_line("second-line").unwrap();

        assert_eq!(
            fs::read_to_string(writer.backup_path()).unwrap(),
            "first-line\n"
        );
        assert_eq!(fs::read_to_string(writer.path()).unwrap(), "second-line\n");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn sanitizes_sensitive_server_output_and_caps_lines() {
        let sanitized = sanitize_server_line(
            "GET https://alice:secret@example.test/media?token=private Authorization: Bearer abc",
        );
        assert!(sanitized.contains("https://[redacted]@example.test/media?token=[redacted]"));
        assert!(!sanitized.contains("secret"));
        assert!(!sanitized.contains("private"));

        let request_path = sanitize_server_line("GET /stream/movie/tt-private?token=secret 500");
        assert!(request_path.contains("/stream/movie/tt-private?token=[redacted]"));

        let path =
            sanitize_server_line("at onFinished (C:\\Users\\alice\\project\\server.cjs:81050:145)");
        assert!(path.contains("[redacted local path]"));
        assert!(!path.contains("alice"));

        let long = sanitize_server_line(&"x".repeat(STREAM_LOG_LINE_LIMIT_BYTES + 20));
        assert!(long.len() <= STREAM_LOG_LINE_LIMIT_BYTES);
        assert!(long.ends_with("[truncated]"));
    }

    #[test]
    fn clear_is_safe_while_a_writer_is_active() {
        let dir = test_dir("clear");
        let writer = RotatingLogWriter::new(dir.join("server.log"));
        let active_writer = writer.clone();
        let thread = thread::spawn(move || {
            for _ in 0..200 {
                active_writer.write_line("live output").unwrap();
            }
        });
        writer.clear().unwrap();
        thread.join().unwrap();
        writer.clear().unwrap();
        assert!(!writer.path().exists());
        assert!(!writer.backup_path().exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn managed_child_drains_and_joins_on_natural_exit() {
        let dir = test_dir("child");
        let logs = StreamingLogFiles::new(dir.join("stdout.log"), dir.join("stderr.log"));
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("cmd");
            command.args(["/C", "echo stdout-line & echo stderr-line 1>&2"]);
            command
        };
        #[cfg(not(windows))]
        let mut command = {
            let mut command = Command::new("sh");
            command.args(["-c", "printf 'stdout-line\\n'; printf 'stderr-line\\n' >&2"]);
            command
        };
        let mut child = ManagedChild::spawn(&mut command, logs.clone()).unwrap();
        for _ in 0..100 {
            if child.has_exited().unwrap() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(child.has_exited().unwrap());
        let tails = logs.tails(1024).unwrap();
        assert!(tails.stdout.contains("stdout-line"));
        assert!(tails.stderr.contains("stderr-line"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn managed_child_stops_and_joins_readers() {
        let dir = test_dir("stopped-child");
        let logs = StreamingLogFiles::new(dir.join("stdout.log"), dir.join("stderr.log"));
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new("cmd");
            command.args([
                "/C",
                concat!(
                    "echo stdout-before-stop & echo stderr-before-stop 1>&2 & ",
                    "ping -n 30 127.0.0.1 > NUL",
                ),
            ]);
            command
        };
        #[cfg(not(windows))]
        let mut command = {
            let mut command = Command::new("sh");
            command.args([
                "-c",
                "printf 'stdout-before-stop\\n'; printf 'stderr-before-stop\\n' >&2; sleep 30",
            ]);
            command
        };
        let mut child = ManagedChild::spawn(&mut command, logs.clone()).unwrap();
        for _ in 0..100 {
            if logs
                .tails(1024)
                .is_ok_and(|tails| tails.stdout.contains("stdout-before-stop"))
            {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        child.stop().unwrap();
        assert!(child.has_exited().unwrap());
        let tails = logs.tails(1024).unwrap();
        assert!(tails.stdout.contains("stdout-before-stop"));
        assert!(tails.stderr.contains("stderr-before-stop"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn tails_are_bounded_and_sanitized_again_after_rotation() {
        let dir = test_dir("tails");
        let stdout = RotatingLogWriter::with_limits(dir.join("stdout.log"), 32, 32);
        let stderr = RotatingLogWriter::with_limits(dir.join("stderr.log"), 32, 32);
        let logs = StreamingLogFiles { stdout, stderr };

        logs.stdout().write_line("first safe line").unwrap();
        logs.stdout()
            .write_line("https://example.test/private")
            .unwrap();
        let tails = logs.tails(20).unwrap();

        assert!(tails.stdout.len() <= 20);
        assert!(!tails.stdout.contains("https://example.test/private"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn legacy_files_are_trimmed_when_writers_start() {
        let dir = test_dir("legacy-limit");
        let path = dir.join("stdout.log");
        fs::write(&path, "x".repeat(64)).unwrap();

        let writer = RotatingLogWriter::with_limits(&path, 16, 16);

        assert!(fs::metadata(writer.path()).unwrap().len() <= 16);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn legacy_files_are_sanitized_when_writers_start() {
        let dir = test_dir("legacy-sanitize");
        let path = dir.join("stdout.log");
        fs::write(&path, "GET https://example.test/media?token=private\n").unwrap();

        let writer = RotatingLogWriter::new(&path);
        let retained = fs::read_to_string(writer.path()).unwrap();

        assert!(
            retained.contains("https://example.test/media?token=[redacted]"),
            "{retained}"
        );
        assert!(!retained.contains("private"));
        let _ = fs::remove_dir_all(dir);
    }
}
