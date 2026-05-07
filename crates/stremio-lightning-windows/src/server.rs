use crate::resources::WindowsResourceLayout;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowsServerConfig {
    pub disabled: bool,
    pub runtime_path: PathBuf,
    pub script_path: PathBuf,
    pub ffmpeg_path: PathBuf,
    pub ffprobe_path: PathBuf,
    pub log_dir: PathBuf,
}

impl WindowsServerConfig {
    pub fn from_resources(layout: &WindowsResourceLayout) -> Self {
        Self {
            disabled: false,
            runtime_path: layout.stremio_runtime(),
            script_path: layout.server_script(),
            ffmpeg_path: layout.ffmpeg(),
            ffprobe_path: layout.ffprobe(),
            log_dir: default_log_dir(),
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn command_spec(&self) -> CommandSpec {
        command_spec(self)
    }
}

#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<PathBuf>,
    pub env: BTreeMap<String, String>,
    pub stdout_log: PathBuf,
    pub stderr_log: PathBuf,
}

pub trait ProcessSpawner: Send + Sync + 'static {
    type Child: ProcessChild;

    fn spawn(&self, spec: CommandSpec) -> Result<Self::Child, String>;
}

pub trait ProcessChild: Send + 'static {
    fn stop(&mut self) -> Result<(), String>;
    fn has_exited(&mut self) -> Result<bool, String>;
}

impl ProcessChild for Child {
    fn stop(&mut self) -> Result<(), String> {
        if self
            .try_wait()
            .map_err(|e| format!("Failed to inspect Windows streaming server: {e}"))?
            .is_some()
        {
            return Ok(());
        }

        self.kill()
            .map_err(|e| format!("Failed to stop Windows streaming server: {e}"))?;
        self.wait()
            .map_err(|e| format!("Failed to wait for Windows streaming server: {e}"))?;
        Ok(())
    }

    fn has_exited(&mut self) -> Result<bool, String> {
        self.try_wait()
            .map(|status| status.is_some())
            .map_err(|e| format!("Failed to inspect Windows streaming server: {e}"))
    }
}

#[derive(Debug, Default, Clone)]
pub struct RealProcessSpawner;

impl ProcessSpawner for RealProcessSpawner {
    type Child = Child;

    fn spawn(&self, spec: CommandSpec) -> Result<Self::Child, String> {
        spawn_real_process(spec)
    }
}

#[derive(Debug)]
pub struct WindowsStreamingServer<P: ProcessSpawner> {
    spawner: P,
    config: WindowsServerConfig,
    child: Mutex<Option<P::Child>>,
}

impl WindowsStreamingServer<RealProcessSpawner> {
    pub fn from_resources(layout: &WindowsResourceLayout, disabled: bool) -> Self {
        Self::new(
            RealProcessSpawner,
            WindowsServerConfig::from_resources(layout).disabled(disabled),
        )
    }
}

impl<P: ProcessSpawner> WindowsStreamingServer<P> {
    pub fn new(spawner: P, config: WindowsServerConfig) -> Self {
        Self {
            spawner,
            config,
            child: Mutex::new(None),
        }
    }

    pub fn start(&self) -> Result<(), String> {
        if self.config.disabled {
            return Ok(());
        }

        let mut child = self.child.lock().map_err(|e| e.to_string())?;
        if let Some(existing) = child.as_mut() {
            if existing.has_exited()? {
                *child = None;
            } else {
                return Ok(());
            }
        }

        let spawned = self.spawner.spawn(self.config.command_spec())?;
        *child = Some(spawned);
        Ok(())
    }

    pub fn stop(&self) -> Result<(), String> {
        let mut child = self.child.lock().map_err(|e| e.to_string())?;
        if let Some(mut child) = child.take() {
            child.stop()?;
        }
        Ok(())
    }

    pub fn restart(&self) -> Result<(), String> {
        self.stop()?;
        self.start()
    }

    pub fn is_running(&self) -> bool {
        self.refresh_running_state().unwrap_or(false)
    }

    pub fn refresh_running_state(&self) -> Result<bool, String> {
        let mut child = self.child.lock().map_err(|e| e.to_string())?;
        if let Some(existing) = child.as_mut() {
            if existing.has_exited()? {
                *child = None;
                return Ok(false);
            }
            return Ok(true);
        }
        Ok(false)
    }

    pub fn disabled(&self) -> bool {
        self.config.disabled
    }
}

impl<P: ProcessSpawner> Drop for WindowsStreamingServer<P> {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

pub fn command_spec(config: &WindowsServerConfig) -> CommandSpec {
    let mut env = BTreeMap::new();
    env.insert("NO_CORS".to_string(), "1".to_string());
    env.insert(
        "FFMPEG_BIN".to_string(),
        config.ffmpeg_path.to_string_lossy().into_owned(),
    );
    env.insert(
        "FFPROBE_BIN".to_string(),
        config.ffprobe_path.to_string_lossy().into_owned(),
    );

    CommandSpec {
        program: config.runtime_path.clone(),
        args: vec![config.script_path.clone()],
        env,
        stdout_log: config.log_dir.join("stremio-server.stdout.log"),
        stderr_log: config.log_dir.join("stremio-server.stderr.log"),
    }
}

fn spawn_real_process(spec: CommandSpec) -> Result<Child, String> {
    ensure_log_parent_exists(&spec.stdout_log)?;
    ensure_log_parent_exists(&spec.stderr_log)?;

    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&spec.stdout_log)
        .map_err(|e| format!("Failed to open Windows streaming server stdout log: {e}"))?;
    let stderr = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&spec.stderr_log)
        .map_err(|e| format!("Failed to open Windows streaming server stderr log: {e}"))?;

    let mut command = Command::new(&spec.program);
    command.args(&spec.args);
    command.envs(&spec.env);
    command.stdout(Stdio::from(stdout));
    command.stderr(Stdio::from(stderr));
    configure_windows_command(&mut command);

    let mut child = command
        .spawn()
        .map_err(|e| format!("Failed to spawn Windows streaming server: {e}"))?;

    if let Err(error) = assign_child_to_job(&child) {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }

    Ok(child)
}

fn ensure_log_parent_exists(path: &Path) -> Result<(), String> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };

    fs::create_dir_all(parent)
        .map_err(|e| format!("Failed to create Windows streaming server log dir: {e}"))
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
fn assign_child_to_job(child: &Child) -> Result<(), String> {
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    unsafe {
        let job = CreateJobObjectW(None, None)
            .map_err(|e| format!("Failed to create Windows streaming server job object: {e}"))?;
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as *const _,
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
        .map_err(|e| {
            let _ = CloseHandle(job);
            format!("Failed to configure Windows streaming server job object: {e}")
        })?;

        let process = HANDLE(child.as_raw_handle());
        AssignProcessToJobObject(job, process).map_err(|e| {
            let _ = CloseHandle(job);
            format!("Failed to assign Windows streaming server to job object: {e}")
        })?;

        // Intentionally leave the job handle open; the OS closes it on app exit,
        // triggering kill-on-close for the assigned server process.
        let _job_handle_kept_until_process_exit = job;
    }
    Ok(())
}

#[cfg(not(windows))]
fn assign_child_to_job(_child: &Child) -> Result<(), String> {
    Ok(())
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

#[derive(Debug, Default, Clone)]
pub struct FakeProcessSpawner {
    calls: Arc<Mutex<Vec<CommandSpec>>>,
    stopped: Arc<Mutex<Vec<usize>>>,
    next_child_exited: Arc<Mutex<bool>>,
}

impl FakeProcessSpawner {
    pub fn calls(&self) -> Vec<CommandSpec> {
        self.calls.lock().expect("fake spawner poisoned").clone()
    }

    pub fn stopped(&self) -> Vec<usize> {
        self.stopped
            .lock()
            .expect("fake spawner stopped list poisoned")
            .clone()
    }

    pub fn set_next_child_exited(&self, exited: bool) {
        *self
            .next_child_exited
            .lock()
            .expect("fake spawner exit flag poisoned") = exited;
    }
}

#[derive(Debug)]
pub struct FakeProcessChild {
    id: usize,
    stopped: Arc<Mutex<Vec<usize>>>,
    exited: bool,
}

impl ProcessSpawner for FakeProcessSpawner {
    type Child = FakeProcessChild;

    fn spawn(&self, spec: CommandSpec) -> Result<Self::Child, String> {
        let mut calls = self.calls.lock().expect("fake spawner poisoned");
        calls.push(spec);
        let id = calls.len();
        let exited = *self
            .next_child_exited
            .lock()
            .expect("fake spawner exit flag poisoned");
        Ok(FakeProcessChild {
            id,
            stopped: self.stopped.clone(),
            exited,
        })
    }
}

impl ProcessChild for FakeProcessChild {
    fn stop(&mut self) -> Result<(), String> {
        self.stopped
            .lock()
            .expect("fake child stopped list poisoned")
            .push(self.id);
        self.exited = true;
        Ok(())
    }

    fn has_exited(&mut self) -> Result<bool, String> {
        Ok(self.exited)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> WindowsServerConfig {
        WindowsServerConfig {
            disabled: false,
            runtime_path: PathBuf::from("C:/app/resources/stremio-runtime.exe"),
            script_path: PathBuf::from("C:/app/resources/server.cjs"),
            ffmpeg_path: PathBuf::from("C:/app/resources/ffmpeg.exe"),
            ffprobe_path: PathBuf::from("C:/app/resources/ffprobe.exe"),
            log_dir: PathBuf::from("C:/logs"),
        }
    }

    #[test]
    fn command_spec_passes_runtime_script_and_media_tools() {
        let spec = command_spec(&test_config());

        assert_eq!(
            spec.program,
            PathBuf::from("C:/app/resources/stremio-runtime.exe")
        );
        assert_eq!(
            spec.args,
            vec![PathBuf::from("C:/app/resources/server.cjs")]
        );
        assert_eq!(spec.env.get("NO_CORS").unwrap(), "1");
        assert_eq!(
            spec.env.get("FFMPEG_BIN").unwrap(),
            "C:/app/resources/ffmpeg.exe"
        );
        assert_eq!(
            spec.env.get("FFPROBE_BIN").unwrap(),
            "C:/app/resources/ffprobe.exe"
        );
        assert_eq!(
            spec.stdout_log,
            PathBuf::from("C:/logs/stremio-server.stdout.log")
        );
        assert_eq!(
            spec.stderr_log,
            PathBuf::from("C:/logs/stremio-server.stderr.log")
        );
    }

    #[test]
    fn start_is_idempotent_while_process_is_running() {
        let spawner = FakeProcessSpawner::default();
        let server = WindowsStreamingServer::new(spawner.clone(), test_config());

        server.start().unwrap();
        server.start().unwrap();

        assert_eq!(spawner.calls().len(), 1);
        assert!(server.is_running());
    }

    #[test]
    fn restart_stops_existing_process_and_starts_again() {
        let spawner = FakeProcessSpawner::default();
        let server = WindowsStreamingServer::new(spawner.clone(), test_config());

        server.start().unwrap();
        server.restart().unwrap();

        assert_eq!(spawner.stopped(), vec![1]);
        assert_eq!(spawner.calls().len(), 2);
    }

    #[test]
    fn disabled_server_does_not_spawn() {
        let spawner = FakeProcessSpawner::default();
        let server = WindowsStreamingServer::new(spawner.clone(), test_config().disabled(true));

        server.start().unwrap();

        assert!(server.disabled());
        assert!(spawner.calls().is_empty());
        assert!(!server.is_running());
    }
}
