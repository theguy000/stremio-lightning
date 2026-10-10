use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use stremio_lightning_core::streaming_server::{CommandSpec, StreamingServerSupervisor};
pub use stremio_lightning_core::streaming_server::{
    FakeProcessSpawner, ProcessSpawner, RealProcessSpawner,
};

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

#[derive(Debug)]
pub struct StreamingServer<P: ProcessSpawner> {
    supervisor: StreamingServerSupervisor<P>,
    url: String,
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
        let supervisor = StreamingServerSupervisor::new(spawner, command_spec(&config))
            .disabled(config.disabled);
        Self {
            supervisor,
            url: config.url,
        }
    }

    pub fn with_disabled(mut self, disabled: bool) -> Self {
        self.supervisor = self.supervisor.disabled(disabled);
        self
    }

    pub fn status(&self) -> StreamingServerStatus {
        StreamingServerStatus {
            running: self.is_running(),
            disabled: self.is_disabled(),
            url: self.url.clone(),
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn diagnostics(&self) -> StreamingServerDiagnostics {
        let paths = self.log_paths();
        StreamingServerDiagnostics {
            status: self.status(),
            stdout_log: paths.stdout,
            stderr_log: paths.stderr,
        }
    }
}

impl<P: ProcessSpawner> Deref for StreamingServer<P> {
    type Target = StreamingServerSupervisor<P>;

    fn deref(&self) -> &Self::Target {
        &self.supervisor
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
            .join("Application Support")
            .join("stremio-lightning")
            .join("logs");
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
            &PathBuf::from("/repo")
                .join("resources")
                .join("ffmpeg")
                .to_string_lossy()
        );
        assert_eq!(
            spec.env.get("FFPROBE_BIN").unwrap(),
            &PathBuf::from("/repo")
                .join("resources")
                .join("ffprobe")
                .to_string_lossy()
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
    fn disabled_server_reports_status_without_spawning() {
        let spawner = FakeProcessSpawner::default();
        let server = StreamingServer::with_config(spawner.clone(), test_config().disabled(true));
        server.start().unwrap();
        assert!(!server.status().running);
        assert!(server.status().disabled);
        assert_eq!(server.status().url, DEFAULT_SERVER_URL);
        assert!(spawner.spawned().is_empty());
    }

    #[test]
    fn diagnostics_include_server_status_and_log_paths() {
        let spawner = FakeProcessSpawner::default();
        let server = StreamingServer::with_config(spawner, test_config());
        let diagnostics = server.diagnostics();
        assert!(!diagnostics.status.running);
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
}
