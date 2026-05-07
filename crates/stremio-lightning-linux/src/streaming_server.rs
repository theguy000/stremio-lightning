use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<PathBuf>,
    pub env: BTreeMap<String, String>,
}

pub trait ProcessSpawner: Send + Sync + 'static {
    type Child: Send + 'static;

    fn spawn(&self, spec: CommandSpec) -> Result<Self::Child, String>;
}

#[derive(Debug, Default, Clone)]
pub struct RealProcessSpawner;

impl ProcessSpawner for RealProcessSpawner {
    type Child = Child;

    fn spawn(&self, spec: CommandSpec) -> Result<Self::Child, String> {
        let mut command = Command::new(&spec.program);
        command.args(&spec.args);
        command.envs(&spec.env);
        command
            .spawn()
            .map_err(|e| format!("Failed to spawn streaming server: {e}"))
    }
}

#[derive(Debug)]
pub struct StreamingServer<P: ProcessSpawner> {
    spawner: P,
    child: Mutex<Option<P::Child>>,
    project_root: PathBuf,
}

impl<P: ProcessSpawner> StreamingServer<P> {
    pub fn new(spawner: P) -> Self {
        Self::with_project_root(spawner, default_project_root())
    }

    pub fn with_project_root(spawner: P, project_root: PathBuf) -> Self {
        Self {
            spawner,
            child: Mutex::new(None),
            project_root,
        }
    }

    pub fn start(&self) -> Result<(), String> {
        let mut child = self.child.lock().map_err(|e| e.to_string())?;
        if child.is_some() {
            return Ok(());
        }

        let spec = command_spec(&self.project_root);
        let spawned = self.spawner.spawn(spec)?;
        *child = Some(spawned);
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.child
            .lock()
            .map(|child| child.is_some())
            .unwrap_or(false)
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }
}

pub fn command_spec(project_root: &Path) -> CommandSpec {
    let tauri_dir = project_root.join("src-tauri");
    let runtime = tauri_dir
        .join("binaries")
        .join("stremio-runtime-x86_64-unknown-linux-gnu");
    let server = tauri_dir.join("resources").join("server.cjs");
    let ffmpeg = tauri_dir.join("resources").join("ffmpeg");
    let ffprobe = tauri_dir.join("resources").join("ffprobe");

    let mut env = BTreeMap::new();
    env.insert("NO_CORS".to_string(), "1".to_string());
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
        args: vec![server],
        env,
    }
}

fn default_project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[derive(Debug, Default, Clone)]
pub struct FakeProcessSpawner {
    calls: Arc<Mutex<Vec<CommandSpec>>>,
}

impl FakeProcessSpawner {
    pub fn calls(&self) -> Vec<CommandSpec> {
        self.calls.lock().expect("fake spawner poisoned").clone()
    }
}

impl ProcessSpawner for FakeProcessSpawner {
    type Child = u64;

    fn spawn(&self, spec: CommandSpec) -> Result<Self::Child, String> {
        let mut calls = self.calls.lock().map_err(|e| e.to_string())?;
        calls.push(spec);
        Ok(calls.len() as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_linux_sidecar_command() {
        let root = PathBuf::from("/repo");
        let spec = command_spec(&root);
        assert_eq!(
            spec.program,
            PathBuf::from("/repo/src-tauri/binaries/stremio-runtime-x86_64-unknown-linux-gnu")
        );
        assert_eq!(
            spec.args,
            vec![PathBuf::from("/repo/src-tauri/resources/server.cjs")]
        );
        assert_eq!(spec.env.get("NO_CORS").unwrap(), "1");
        assert_eq!(
            spec.env.get("FFMPEG_BIN").unwrap(),
            "/repo/src-tauri/resources/ffmpeg"
        );
        assert_eq!(
            spec.env.get("FFPROBE_BIN").unwrap(),
            "/repo/src-tauri/resources/ffprobe"
        );
    }

    #[test]
    fn fake_spawner_starts_without_real_sidecar() {
        let spawner = FakeProcessSpawner::default();
        let server = StreamingServer::with_project_root(spawner.clone(), PathBuf::from("/repo"));
        server.start().unwrap();
        server.start().unwrap();
        assert!(server.is_running());
        assert_eq!(spawner.calls().len(), 1);
    }
}
