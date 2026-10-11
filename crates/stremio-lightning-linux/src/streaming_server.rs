use std::collections::BTreeMap;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use stremio_lightning_core::streaming_server::StreamingServerSupervisor;
pub use stremio_lightning_core::streaming_server::{
    CommandSpec, ProcessChild, ProcessSpawner, RealProcessSpawner,
};

#[derive(Debug)]
pub struct StreamingServer<P: ProcessSpawner> {
    supervisor: StreamingServerSupervisor<P>,
}

impl<P: ProcessSpawner> StreamingServer<P> {
    pub fn new(spawner: P) -> Self {
        Self::with_paths(spawner, default_project_root(), default_log_dir())
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.supervisor = self.supervisor.disabled(disabled);
        self
    }

    pub fn with_project_root(spawner: P, project_root: PathBuf) -> Self {
        Self::with_paths(spawner, project_root, default_log_dir())
    }

    pub fn with_paths(spawner: P, project_root: PathBuf, log_dir: PathBuf) -> Self {
        Self {
            supervisor: StreamingServerSupervisor::new(
                spawner,
                command_spec(&project_root, &log_dir),
            ),
        }
    }
}

impl<P: ProcessSpawner> Deref for StreamingServer<P> {
    type Target = StreamingServerSupervisor<P>;

    fn deref(&self) -> &Self::Target {
        &self.supervisor
    }
}

pub fn command_spec(project_root: &Path, log_dir: &Path) -> CommandSpec {
    let resources = project_root.join("resources");

    let mut env = BTreeMap::new();
    // stream-server resolves ffmpeg/ffprobe from PATH, not FFMPEG_BIN, so put
    // the directory holding the bundled media tools first.
    env.insert("PATH".to_string(), path_with_first(&resources));

    CommandSpec {
        program: resources.join("stream-server"),
        args: Vec::new(),
        env,
        stdout_log: log_dir.join("stremio-server.stdout.log"),
        stderr_log: log_dir.join("stremio-server.stderr.log"),
    }
}

fn path_with_first(dir: &Path) -> String {
    match std::env::var("PATH") {
        Ok(existing) if !existing.is_empty() => format!("{}:{existing}", dir.display()),
        _ => dir.display().to_string(),
    }
}

fn default_project_root() -> PathBuf {
    if let Some(path) = std::env::var_os("STREMIO_LIGHTNING_BUNDLE_DIR") {
        return PathBuf::from(path);
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn default_log_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(path).join("stremio-lightning").join("logs");
    }

    if let Some(home) = std::env::var_os("HOME") {
        return Path::new(&home)
            .join(".local")
            .join("share")
            .join("stremio-lightning")
            .join("logs");
    }

    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("stremio-lightning")
        .join("logs")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_linux_sidecar_command() {
        let root = PathBuf::from("/repo");
        let log_dir = PathBuf::from("/logs");
        let spec = command_spec(&root, &log_dir);
        assert_eq!(spec.program, PathBuf::from("/repo/resources/stream-server"));
        assert!(spec.args.is_empty());
        assert!(spec.env.get("PATH").unwrap().starts_with("/repo/resources"));
        assert!(!spec.env.contains_key("FFMPEG_BIN"));
        assert!(!spec.env.contains_key("FFPROBE_BIN"));
        assert_eq!(
            spec.stdout_log,
            PathBuf::from("/logs/stremio-server.stdout.log")
        );
        assert_eq!(
            spec.stderr_log,
            PathBuf::from("/logs/stremio-server.stderr.log")
        );
    }
}
