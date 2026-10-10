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
    let runtime = project_root
        .join("binaries")
        .join("stremio-runtime-x86_64-unknown-linux-gnu");
    let server = project_root.join("resources").join("server.cjs");
    let ffmpeg = project_root.join("resources").join("ffmpeg");
    let ffprobe = project_root.join("resources").join("ffprobe");

    let mut env = BTreeMap::new();
    env.insert("NO_CORS".to_string(), "0".to_string());
    env.insert(
        "FFMPEG_BIN".to_string(),
        ffmpeg.to_string_lossy().into_owned(),
    );
    env.insert(
        "FFPROBE_BIN".to_string(),
        ffprobe.to_string_lossy().into_owned(),
    );

    CommandSpec {
        program: runtime,
        args: vec![PathBuf::from("--max-old-space-size=192"), server],
        env,
        stdout_log: log_dir.join("stremio-server.stdout.log"),
        stderr_log: log_dir.join("stremio-server.stderr.log"),
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
        assert_eq!(
            spec.program,
            PathBuf::from("/repo/binaries/stremio-runtime-x86_64-unknown-linux-gnu")
        );
        assert_eq!(
            spec.args,
            vec![
                PathBuf::from("--max-old-space-size=192"),
                PathBuf::from("/repo/resources/server.cjs")
            ]
        );
        assert_eq!(spec.env.get("NO_CORS").unwrap(), "0");
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
}
