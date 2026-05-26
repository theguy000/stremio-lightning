use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

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
        if let Some(parent) = spec.stdout_log.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create streaming server log dir: {e}"))?;
        }
        if let Some(parent) = spec.stderr_log.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create streaming server log dir: {e}"))?;
        }

        let stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&spec.stdout_log)
            .map_err(|e| format!("Failed to open streaming server stdout log: {e}"))?;
        let stderr = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&spec.stderr_log)
            .map_err(|e| format!("Failed to open streaming server stderr log: {e}"))?;

        let mut command = Command::new(&spec.program);
        command.args(&spec.args);
        command.envs(&spec.env);
        command.stdout(Stdio::from(stdout));
        command.stderr(Stdio::from(stderr));
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
    log_dir: PathBuf,
}

impl<P: ProcessSpawner> StreamingServer<P> {
    pub fn new(spawner: P) -> Self {
        Self::with_paths(spawner, default_project_root(), default_log_dir())
    }

    pub fn with_project_root(spawner: P, project_root: PathBuf) -> Self {
        Self::with_paths(spawner, project_root, default_log_dir())
    }

    pub fn with_paths(spawner: P, project_root: PathBuf, log_dir: PathBuf) -> Self {
        Self {
            spawner,
            child: Mutex::new(None),
            project_root,
            log_dir,
        }
    }

    pub fn start(&self) -> Result<(), String> {
        let mut child = self.child.lock().map_err(|e| e.to_string())?;
        if let Some(existing) = child.as_mut() {
            if existing.has_exited()? {
                *child = None;
            } else {
                return Ok(());
            }
        }

        let spec = command_spec(&self.project_root, &self.log_dir);
        let spawned = self.spawner.spawn(spec)?;
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

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }
}

impl<P: ProcessSpawner> Drop for StreamingServer<P> {
    fn drop(&mut self) {
        let _ = self.stop();
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
