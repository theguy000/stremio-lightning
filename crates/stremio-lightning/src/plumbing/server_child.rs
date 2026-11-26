//! Child process handle for the local streaming server: pipes stdout/stderr into rotating
//! sanitized log files and forwards rate-limited stderr warnings into `plumbing::logs::Logs`.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::logs::{Level, Logs, Source};
use super::rotating_log::{RotatingLogWriter, LINE_LIMIT_BYTES};
use super::sanitize::{sanitize_text, truncate_utf8};

const SERVER_LOG_LIMIT_BYTES: u64 = 1024 * 1024;
const STDERR_WINDOW: Duration = Duration::from_secs(60);
const STDERR_MAX_PER_WINDOW: usize = 20;
const STDERR_DEDUPE_WINDOW: Duration = Duration::from_secs(10);

pub(crate) struct ManagedChild {
    child: Child,
    stdout_reader: Option<JoinHandle<()>>,
    stderr_reader: Option<JoinHandle<()>>,
}

impl ManagedChild {
    pub(crate) fn spawn(
        command: &mut Command,
        stdout_log: PathBuf,
        stderr_log: PathBuf,
        logs: Arc<Logs>,
    ) -> Result<Self, String> {
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|error| format!("Failed to spawn streaming server: {error}"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "Streaming server stdout pipe was unavailable".to_owned())?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| "Streaming server stderr pipe was unavailable".to_owned())?;
        let forwarder = Arc::new(StderrForwarder::new(logs));
        Ok(Self {
            child,
            stdout_reader: Some(spawn_reader(
                stdout,
                RotatingLogWriter::new(stdout_log, SERVER_LOG_LIMIT_BYTES),
                None,
            )),
            stderr_reader: Some(spawn_reader(
                stderr,
                RotatingLogWriter::new(stderr_log, SERVER_LOG_LIMIT_BYTES),
                Some(forwarder),
            )),
        })
    }

    pub(crate) fn stop(&mut self) -> Result<(), String> {
        let mut result = Ok(());
        if self
            .child
            .try_wait()
            .map_err(|e| format!("Failed to inspect streaming server: {e}"))?
            .is_none()
        {
            if let Err(e) = self.child.kill() {
                result = Err(format!("Failed to stop streaming server: {e}"));
            }
            if let Err(e) = self.child.wait() {
                result = Err(format!("Failed to wait for streaming server: {e}"));
            }
        }
        self.join_readers();
        result
    }

    pub(crate) fn has_exited(&mut self) -> Result<bool, String> {
        let exited = self
            .child
            .try_wait()
            .map(|status| status.is_some())
            .map_err(|e| format!("Failed to inspect streaming server: {e}"))?;
        if exited {
            self.join_readers();
        }
        Ok(exited)
    }

    fn join_readers(&mut self) {
        if let Some(handle) = self.stdout_reader.take() {
            let _ = handle.join();
        }
        if let Some(handle) = self.stderr_reader.take() {
            let _ = handle.join();
        }
    }
}

struct ForwarderState {
    window_started: Instant,
    emitted: usize,
    suppressed: usize,
    last_line: String,
    last_line_at: Instant,
}

struct StderrForwarder {
    logs: Arc<Logs>,
    state: Mutex<ForwarderState>,
}

impl StderrForwarder {
    fn new(logs: Arc<Logs>) -> Self {
        let now = Instant::now();
        Self {
            logs,
            state: Mutex::new(ForwarderState {
                window_started: now,
                emitted: 0,
                suppressed: 0,
                last_line: String::new(),
                last_line_at: now - STDERR_DEDUPE_WINDOW,
            }),
        }
    }

    fn forward(&self, line: String) {
        if line.trim_start().starts_with("at ") {
            return;
        }
        let mut messages = Vec::new();
        if let Ok(mut state) = self.state.lock() {
            if state.window_started.elapsed() >= STDERR_WINDOW {
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
            if state.last_line == line && state.last_line_at.elapsed() < STDERR_DEDUPE_WINDOW {
                state.last_line_at = Instant::now();
                state.suppressed += 1;
                return;
            }
            state.last_line.clone_from(&line);
            state.last_line_at = Instant::now();
            if state.emitted < STDERR_MAX_PER_WINDOW {
                state.emitted += 1;
                messages.push(line);
            } else {
                state.suppressed += 1;
            }
        }
        for message in messages {
            self.logs
                .record(Source::Server, "stderr", Level::Warn, message);
        }
    }

    fn finish(&self) {
        let suppressed = self
            .state
            .lock()
            .map_or(0, |mut state| std::mem::take(&mut state.suppressed));
        if suppressed > 0 {
            self.logs.record(
                Source::Server,
                "stderr",
                Level::Warn,
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
        let mut buf = [0_u8; 4096];
        let mut line = Vec::with_capacity(LINE_LIMIT_BYTES);
        let mut truncated = false;
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    flush_line(&writer, forwarder.as_ref(), &line, truncated);
                    if let Some(f) = forwarder {
                        f.finish();
                    }
                    return;
                }
                Ok(n) => {
                    for &byte in &buf[..n] {
                        if byte == b'\n' {
                            flush_line(&writer, forwarder.as_ref(), &line, truncated);
                            line.clear();
                            truncated = false;
                        } else if line.len() < LINE_LIMIT_BYTES {
                            line.push(byte);
                        } else {
                            truncated = true;
                        }
                    }
                }
                Err(_) => {
                    if let Some(f) = forwarder {
                        f.finish();
                    }
                    return;
                }
            }
        }
    })
}

fn flush_line(
    writer: &RotatingLogWriter,
    forwarder: Option<&Arc<StderrForwarder>>,
    raw: &[u8],
    truncated: bool,
) {
    if raw.is_empty() && !truncated {
        return;
    }
    let mut text = String::from_utf8_lossy(raw).into_owned();
    if truncated {
        text.push_str(" [truncated server line]");
    }
    let sanitized = truncate_utf8(sanitize_text(&text).trim(), LINE_LIMIT_BYTES);
    let _ = writer.write_sanitized_line(&sanitized);
    if let Some(f) = forwarder {
        f.forward(sanitized);
    }
}
