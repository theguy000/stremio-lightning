#![cfg_attr(windows, allow(unsafe_code))]

use crate::resources::WindowsResourceLayout;
use std::collections::BTreeMap;
use std::ops::Deref;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use stremio_lightning_core::streaming_logs::{ManagedChild, StreamingLogFiles};
use stremio_lightning_core::streaming_server::StreamingServerSupervisor;
pub use stremio_lightning_core::streaming_server::{CommandSpec, ProcessChild, ProcessSpawner};
use thiserror::Error;

/// Optional environment override for the stream-server executable path.
pub const STREAM_SERVER_BIN_ENV: &str = "STREMIO_LIGHTNING_STREAM_SERVER_BIN";

#[derive(Debug, Error)]
pub enum ServerError {
    #[error("Failed to start streaming server: {0}")]
    Start(String),
    #[error("Failed to stop streaming server: {0}")]
    Stop(String),
    #[error("Streaming server mutex poisoned: {0}")]
    LockPoisoned(String),
    #[error("Streaming server job error: {0}")]
    Job(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

impl From<String> for ServerError {
    fn from(error: String) -> Self {
        Self::Other(error)
    }
}

impl From<&str> for ServerError {
    fn from(error: &str) -> Self {
        Self::Other(error.to_string())
    }
}

impl From<ServerError> for String {
    fn from(error: ServerError) -> Self {
        error.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsServerConfig {
    pub disabled: bool,
    pub stream_server_path: PathBuf,
    pub ffmpeg_path: PathBuf,
    pub log_dir: PathBuf,
}

impl WindowsServerConfig {
    pub fn from_resources(layout: &WindowsResourceLayout) -> Self {
        Self {
            disabled: false,
            stream_server_path: std::env::var_os(STREAM_SERVER_BIN_ENV)
                .map_or_else(|| layout.stream_server(), PathBuf::from),
            ffmpeg_path: layout.ffmpeg(),
            log_dir: default_log_dir(),
        }
    }

    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    #[must_use]
    pub fn command_spec(&self) -> CommandSpec {
        command_spec(self)
    }
}

#[derive(Debug)]
pub struct WindowsProcessChild {
    #[cfg(windows)]
    job: Option<WindowsJob>,
    inner: ManagedChild,
}

impl ProcessChild for WindowsProcessChild {
    fn stop(&mut self) -> Result<(), String> {
        #[cfg(windows)]
        if let Some(job) = self.job.as_ref() {
            job.terminate()?;
            let result = self.inner.wait_for_exit();
            self.job.take();
            return result;
        }
        self.inner.stop()
    }

    fn has_exited(&mut self) -> Result<bool, String> {
        if !self.inner.process_has_exited()? {
            return Ok(false);
        }
        self.inner.stop()?;
        #[cfg(windows)]
        self.job.take();
        Ok(true)
    }
}

#[derive(Debug, Default, Clone)]
pub struct RealProcessSpawner;

impl ProcessSpawner for RealProcessSpawner {
    type Child = WindowsProcessChild;

    fn spawn(&self, spec: CommandSpec) -> Result<Self::Child, String> {
        spawn_real_process(spec)
    }
}

#[derive(Debug)]
pub struct WindowsStreamingServer<P: ProcessSpawner> {
    supervisor: StreamingServerSupervisor<P>,
}

impl WindowsStreamingServer<RealProcessSpawner> {
    #[must_use]
    pub fn from_resources(layout: &WindowsResourceLayout, disabled: bool) -> Self {
        Self::new(
            RealProcessSpawner,
            &WindowsServerConfig::from_resources(layout).disabled(disabled),
        )
    }
}

impl<P: ProcessSpawner> WindowsStreamingServer<P> {
    pub fn new(spawner: P, config: &WindowsServerConfig) -> Self {
        Self {
            supervisor: StreamingServerSupervisor::new(spawner, config.command_spec())
                .disabled(config.disabled),
        }
    }
}

impl<P: ProcessSpawner> Deref for WindowsStreamingServer<P> {
    type Target = StreamingServerSupervisor<P>;

    fn deref(&self) -> &Self::Target {
        &self.supervisor
    }
}

#[must_use]
pub fn command_spec(config: &WindowsServerConfig) -> CommandSpec {
    let stdout_log = config.log_dir.join("stremio-server.stdout.log");
    let stderr_log = config.log_dir.join("stremio-server.stderr.log");

    let mut env = BTreeMap::new();
    // stream-server resolves ffmpeg/ffprobe from PATH, not FFMPEG_BIN, so
    // prepend the directory holding the bundled media tools.
    env.insert("PATH".to_string(), stream_server_path_env(config));

    CommandSpec {
        program: config.stream_server_path.clone(),
        args: vec![PathBuf::from("--no-tray")],
        env,
        stdout_log,
        stderr_log,
    }
}

fn stream_server_path_env(config: &WindowsServerConfig) -> String {
    let tools_dir = match config.ffmpeg_path.parent() {
        Some(parent) => parent.to_path_buf(),
        None => config.ffmpeg_path.clone(),
    };

    match std::env::var("PATH") {
        Ok(existing) if !existing.is_empty() => format!("{};{existing}", tools_dir.display()),
        _ => tools_dir.display().to_string(),
    }
}

fn spawn_real_process(spec: CommandSpec) -> Result<WindowsProcessChild, String> {
    let mut command = Command::new(&spec.program);
    command.args(&spec.args);
    command.envs(&spec.env);
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    configure_windows_command(&mut command);

    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to spawn Windows streaming server: {e}"))?;

    #[cfg(windows)]
    let job = match assign_child_to_job(&child) {
        Ok(job) => job,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };

    let inner = ManagedChild::from_child(
        child,
        StreamingLogFiles::new(spec.stdout_log, spec.stderr_log),
    )?;
    Ok(WindowsProcessChild {
        #[cfg(windows)]
        job: Some(job),
        inner,
    })
}

#[cfg(windows)]
fn configure_windows_command(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    use windows::Win32::System::Threading::CREATE_NO_WINDOW;

    command.creation_flags(CREATE_NO_WINDOW.0);
}

#[cfg(not(windows))]
fn configure_windows_command(_command: &mut Command) {}

#[cfg(windows)]
#[derive(Debug)]
struct WindowsJob(windows::Win32::Foundation::HANDLE);

// SAFETY: A Windows job object handle can be closed from any thread. Access is
// synchronized by the server mutex.
#[cfg(windows)]
unsafe impl Send for WindowsJob {}

#[cfg(windows)]
impl Drop for WindowsJob {
    fn drop(&mut self) {
        use windows::Win32::Foundation::CloseHandle;

        // SAFETY: self.0 is an owned Win32 job object handle closed exactly once on drop.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(windows)]
impl WindowsJob {
    fn terminate(&self) -> Result<(), String> {
        use windows::Win32::System::JobObjects::TerminateJobObject;

        // SAFETY: self.0 is an owned valid Win32 job object handle.
        unsafe { TerminateJobObject(self.0, 1) }
            .map_err(|error| format!("Failed to terminate Windows streaming server job: {error}"))
    }
}

#[cfg(windows)]
// The Win32 ABI takes this length as a u32; the struct is a fixed small constant.
#[allow(clippy::cast_possible_truncation)]
fn assign_child_to_job(child: &Child) -> Result<WindowsJob, String> {
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    // SAFETY: CreateJobObjectW creates a new job object; limits points to a valid
    // JOBOBJECT_EXTENDED_LIMIT_INFORMATION struct; child.as_raw_handle() is a valid
    // live process handle.
    unsafe {
        let job =
            WindowsJob(CreateJobObjectW(None, None).map_err(|e| {
                format!("Failed to create Windows streaming server job object: {e}")
            })?);
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            (&raw const limits).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .map_err(|e| format!("Failed to configure Windows streaming server job object: {e}"))?;

        let process = HANDLE(child.as_raw_handle());
        AssignProcessToJobObject(job.0, process)
            .map_err(|e| format!("Failed to assign Windows streaming server to job object: {e}"))?;

        Ok(job)
    }
}

fn default_log_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(path).join("stremio-lightning").join("logs");
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("stremio-lightning")
        .join("logs")
}

#[cfg(test)]
mod tests {
    use super::*;
    use stremio_lightning_core::streaming_server::FakeProcessSpawner;

    fn test_config() -> WindowsServerConfig {
        WindowsServerConfig {
            disabled: false,
            stream_server_path: PathBuf::from("C:/app/resources/stream-server.exe"),
            ffmpeg_path: PathBuf::from("C:/app/resources/ffmpeg.exe"),
            log_dir: PathBuf::from("C:/logs"),
        }
    }

    #[test]
    fn command_spec_uses_stream_server_binary_without_ffmpeg_bin() {
        let spec = command_spec(&test_config());

        assert_eq!(
            spec.program,
            PathBuf::from("C:/app/resources/stream-server.exe")
        );
        assert_eq!(spec.args, vec![PathBuf::from("--no-tray")]);
        assert!(spec
            .env
            .get("PATH")
            .unwrap()
            .starts_with("C:/app/resources"));
        assert!(!spec.env.contains_key("FFMPEG_BIN"));
        assert!(!spec.env.contains_key("FFPROBE_BIN"));
        assert_eq!(
            spec.stdout_log,
            PathBuf::from("C:/logs/stremio-server.stdout.log")
        );
    }
    #[test]
    fn disabled_server_does_not_spawn() {
        let spawner = FakeProcessSpawner::default();
        let server = WindowsStreamingServer::new(spawner.clone(), &test_config().disabled(true));

        server.start().unwrap();

        assert!(server.is_disabled());
        assert!(spawner.spawned().is_empty());
        assert!(!server.is_running());
    }
}
