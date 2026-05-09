use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};

pub const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:11470";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamingServerStatus {
    pub running: bool,
    pub disabled: bool,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamingServerDiagnostics {
    pub status: StreamingServerStatus,
    pub stdout_log: PathBuf,
    pub stderr_log: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamingServerConfig {
    pub disabled: bool,
    pub runtime_path: PathBuf,
    pub script_path: PathBuf,
    pub ffmpeg_path: PathBuf,
    pub ffprobe_path: PathBuf,
    pub log_dir: PathBuf,
    pub url: String,
}

impl StreamingServerConfig {
    pub fn from_project_root(project_root: impl AsRef<Path>) -> Self {
        let project_root = project_root.as_ref();
        Self {
            disabled: false,
            runtime_path: runtime_path(project_root),
            script_path: project_root.join("resources").join("server.cjs"),
            ffmpeg_path: project_root.join("resources").join("ffmpeg"),
            ffprobe_path: project_root.join("resources").join("ffprobe"),
            log_dir: default_log_dir(),
            url: DEFAULT_SERVER_URL.to_string(),
        }
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn with_log_dir(mut self, log_dir: impl Into<PathBuf>) -> Self {
        self.log_dir = log_dir.into();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
            .map_err(|e| format!("Failed to inspect streaming server: {e}"))?
            .is_some()
        {
            return Ok(());
        }

        self.kill()
            .map_err(|e| format!("Failed to stop streaming server: {e}"))?;
        self.wait()
            .map_err(|e| format!("Failed to wait for streaming server: {e}"))?;
        Ok(())
    }

    fn has_exited(&mut self) -> Result<bool, String> {
        self.try_wait()
            .map(|status| status.is_some())
            .map_err(|e| format!("Failed to inspect streaming server: {e}"))
    }
}

#[derive(Debug, Default, Clone)]
pub struct RealProcessSpawner;

impl ProcessSpawner for RealProcessSpawner {
    type Child = Child;

    fn spawn(&self, spec: CommandSpec) -> Result<Self::Child, String> {
        ensure_log_parent_exists(&spec.stdout_log)?;
        ensure_log_parent_exists(&spec.stderr_log)?;

        let stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&spec.stdout_log)
            .map_err(|e| format!("Failed to open macOS streaming server stdout log: {e}"))?;
        let stderr = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&spec.stderr_log)
            .map_err(|e| format!("Failed to open macOS streaming server stderr log: {e}"))?;

        let mut command = Command::new(&spec.program);
        command.args(&spec.args);
        command.envs(&spec.env);
        command.stdout(Stdio::from(stdout));
        command.stderr(Stdio::from(stderr));
        command.spawn().map_err(|e| {
            format!(
                "Failed to start macOS streaming server sidecar {}: {e}",
                spec.program.display()
            )
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct FakeProcessSpawner {
    spawned: Arc<Mutex<Vec<CommandSpec>>>,
    stopped: Arc<Mutex<Vec<usize>>>,
    fail_next_spawn: Arc<Mutex<Option<String>>>,
    next_child_exited: Arc<Mutex<bool>>,
}

impl FakeProcessSpawner {
    pub fn spawned(&self) -> Vec<CommandSpec> {
        self.spawned
            .lock()
            .expect("fake process spawner poisoned")
            .clone()
    }

    pub fn stopped(&self) -> Vec<usize> {
        self.stopped
            .lock()
            .expect("fake process spawner stopped list poisoned")
            .clone()
    }

    pub fn fail_next_spawn(&self, error: impl Into<String>) {
        *self
            .fail_next_spawn
            .lock()
            .expect("fake process spawner failure flag poisoned") = Some(error.into());
    }

    pub fn set_next_child_exited(&self, exited: bool) {
        *self
            .next_child_exited
            .lock()
            .expect("fake process spawner exit flag poisoned") = exited;
    }
}

impl ProcessSpawner for FakeProcessSpawner {
    type Child = FakeProcessChild;

    fn spawn(&self, spec: CommandSpec) -> Result<Self::Child, String> {
        if let Some(error) = self
            .fail_next_spawn
            .lock()
            .map_err(|e| e.to_string())?
            .take()
        {
            return Err(error);
        }

        let mut spawned = self.spawned.lock().map_err(|e| e.to_string())?;
        spawned.push(spec);
        let id = spawned.len();
        let exited = *self.next_child_exited.lock().map_err(|e| e.to_string())?;
        Ok(FakeProcessChild {
            id,
            stopped: self.stopped.clone(),
            exited,
        })
    }
}

#[derive(Debug)]
pub struct FakeProcessChild {
    id: usize,
    stopped: Arc<Mutex<Vec<usize>>>,
    exited: bool,
}

impl ProcessChild for FakeProcessChild {
    fn stop(&mut self) -> Result<(), String> {
        self.stopped
            .lock()
            .map_err(|e| e.to_string())?
            .push(self.id);
        self.exited = true;
        Ok(())
    }

    fn has_exited(&mut self) -> Result<bool, String> {
        Ok(self.exited)
    }
}

#[derive(Debug)]
pub struct StreamingServer<P: ProcessSpawner> {
    spawner: P,
    child: Mutex<Option<P::Child>>,
    config: StreamingServerConfig,
}

impl<P: ProcessSpawner> StreamingServer<P> {
    pub fn new(spawner: P) -> Self {
        Self::with_project_root(spawner, default_project_root())
    }

    pub fn with_project_root(spawner: P, project_root: PathBuf) -> Self {
        Self::with_config(
            spawner,
            StreamingServerConfig::from_project_root(project_root),
        )
    }

    pub fn with_config(spawner: P, config: StreamingServerConfig) -> Self {
        Self {
            spawner,
            child: Mutex::new(None),
            config,
        }
    }

    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.config.disabled = disabled;
        self
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

        *child = Some(self.spawner.spawn(command_spec(&self.config))?);
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

    pub fn status(&self) -> StreamingServerStatus {
        StreamingServerStatus {
            running: self.is_running(),
            disabled: self.config.disabled,
            url: self.config.url.clone(),
        }
    }

    pub fn url(&self) -> &str {
        &self.config.url
    }

    pub fn disabled(&self) -> bool {
        self.config.disabled
    }

    pub fn diagnostics(&self) -> StreamingServerDiagnostics {
        let spec = command_spec(&self.config);
        StreamingServerDiagnostics {
            status: self.status(),
            stdout_log: spec.stdout_log,
            stderr_log: spec.stderr_log,
        }
    }
}

impl<P: ProcessSpawner> Drop for StreamingServer<P> {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

pub fn command_spec(config: &StreamingServerConfig) -> CommandSpec {
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

fn runtime_path(project_root: &Path) -> PathBuf {
    project_root.join("binaries").join("stremio-runtime-macos")
}

fn ensure_log_parent_exists(path: &Path) -> Result<(), String> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent).map_err(|e| format!("Failed to create macOS server log dir: {e}"))
}

fn default_project_root() -> PathBuf {
    if let Some(path) = std::env::var_os("STREMIO_LIGHTNING_BUNDLE_DIR") {
        return PathBuf::from(path);
    }

    if let Ok(executable) = std::env::current_exe() {
        if let Some(resources) = bundled_resources_root_from_executable(&executable) {
            return resources;
        }
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

fn default_log_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("STREMIO_LIGHTNING_LOG_DIR") {
        return PathBuf::from(path);
    }

    if let Some(home) = std::env::var_os("HOME") {
        return Path::new(&home)
            .join("Library")
            .join("Logs")
            .join("Stremio Lightning");
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("stremio-lightning")
        .join("logs")
}

fn bundled_resources_root_from_executable(executable: &Path) -> Option<PathBuf> {
    let macos_dir = executable.parent()?;
    if macos_dir.file_name().and_then(|name| name.to_str()) != Some("MacOS") {
        return None;
    }
    let contents_dir = macos_dir.parent()?;
    if contents_dir.file_name().and_then(|name| name.to_str()) != Some("Contents") {
        return None;
    }
    Some(contents_dir.join("Resources"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> StreamingServerConfig {
        StreamingServerConfig::from_project_root("/repo").with_log_dir("/logs")
    }

    #[test]
    fn builds_macos_sidecar_command() {
        let spec = command_spec(&test_config());
        assert_eq!(
            spec.program,
            PathBuf::from("/repo/binaries/stremio-runtime-macos")
        );
        assert_eq!(spec.args, vec![PathBuf::from("/repo/resources/server.cjs")]);
        assert_eq!(spec.env.get("NO_CORS").unwrap(), "1");
        assert_eq!(
            spec.env.get("FFMPEG_BIN").unwrap(),
            "/repo/resources/ffmpeg"
        );
        assert_eq!(
            spec.env.get("FFPROBE_BIN").unwrap(),
            "/repo/resources/ffprobe"
        );
        assert_eq!(
            spec.stdout_log,
            PathBuf::from("/logs/stremio-server.stdout.log")
        );
        assert_eq!(
            spec.stderr_log,
            PathBuf::from("/logs/stremio-server.stderr.log")
        );
    }

    #[test]
    fn detects_packaged_app_resources_from_executable_path() {
        let resources = bundled_resources_root_from_executable(Path::new(
            "/Applications/Stremio Lightning.app/Contents/MacOS/stremio-lightning-macos",
        ))
        .unwrap();
        assert_eq!(
            resources,
            PathBuf::from("/Applications/Stremio Lightning.app/Contents/Resources")
        );
        assert_eq!(
            bundled_resources_root_from_executable(Path::new("/tmp/stremio-lightning-macos")),
            None
        );
    }

    #[test]
    fn fake_spawner_starts_once_while_running() {
        let spawner = FakeProcessSpawner::default();
        let server = StreamingServer::with_config(spawner.clone(), test_config());
        server.start().unwrap();
        server.start().unwrap();
        assert!(server.is_running());
        assert_eq!(spawner.spawned().len(), 1);
    }

    #[test]
    fn fake_spawner_stops_and_restarts() {
        let spawner = FakeProcessSpawner::default();
        let server = StreamingServer::with_config(spawner.clone(), test_config());
        server.start().unwrap();
        server.stop().unwrap();
        assert!(!server.is_running());
        server.restart().unwrap();
        assert!(server.is_running());
        assert_eq!(spawner.spawned().len(), 2);
        assert_eq!(spawner.stopped(), vec![1]);
    }

    #[test]
    fn status_reaps_exited_child_and_start_spawns_again() {
        let spawner = FakeProcessSpawner::default();
        spawner.set_next_child_exited(true);
        let server = StreamingServer::with_config(spawner.clone(), test_config());
        server.start().unwrap();
        assert!(!server.is_running());

        spawner.set_next_child_exited(false);
        server.start().unwrap();
        assert!(server.is_running());
        assert_eq!(spawner.spawned().len(), 2);
    }

    #[test]
    fn disabled_server_reports_status_without_spawning() {
        let spawner = FakeProcessSpawner::default();
        let server = StreamingServer::with_config(spawner.clone(), test_config().disabled(true));
        server.start().unwrap();
        assert_eq!(server.status().running, false);
        assert_eq!(server.status().disabled, true);
        assert_eq!(server.status().url, DEFAULT_SERVER_URL);
        assert!(spawner.spawned().is_empty());
    }

    #[test]
    fn diagnostics_include_server_status_and_log_paths() {
        let spawner = FakeProcessSpawner::default();
        let server = StreamingServer::with_config(spawner, test_config());
        let diagnostics = server.diagnostics();
        assert_eq!(diagnostics.status.running, false);
        assert_eq!(diagnostics.status.url, DEFAULT_SERVER_URL);
        assert_eq!(
            diagnostics.stdout_log,
            PathBuf::from("/logs/stremio-server.stdout.log")
        );
        assert_eq!(
            diagnostics.stderr_log,
            PathBuf::from("/logs/stremio-server.stderr.log")
        );
    }

    #[test]
    fn spawn_failure_is_returned() {
        let spawner = FakeProcessSpawner::default();
        spawner.fail_next_spawn("boom");
        let server = StreamingServer::with_config(spawner, test_config());
        assert_eq!(server.start().unwrap_err(), "boom");
        assert!(!server.is_running());
    }
}
