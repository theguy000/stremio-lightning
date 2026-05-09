use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandSpec {
    pub program: PathBuf,
    pub args: Vec<PathBuf>,
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
        Command::new(&spec.program)
            .args(&spec.args)
            .spawn()
            .map_err(|e| {
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
}

impl FakeProcessSpawner {
    pub fn spawned(&self) -> Vec<CommandSpec> {
        self.spawned
            .lock()
            .expect("fake process spawner poisoned")
            .clone()
    }
}

impl ProcessSpawner for FakeProcessSpawner {
    type Child = FakeProcessChild;

    fn spawn(&self, spec: CommandSpec) -> Result<Self::Child, String> {
        self.spawned.lock().map_err(|e| e.to_string())?.push(spec);
        Ok(FakeProcessChild::default())
    }
}

#[derive(Debug, Default)]
pub struct FakeProcessChild {
    stopped: bool,
}

impl ProcessChild for FakeProcessChild {
    fn stop(&mut self) -> Result<(), String> {
        self.stopped = true;
        Ok(())
    }

    fn has_exited(&mut self) -> Result<bool, String> {
        Ok(self.stopped)
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

        *child = Some(self.spawner.spawn(command_spec(&self.project_root))?);
        Ok(())
    }

    pub fn stop(&self) -> Result<(), String> {
        let mut child = self.child.lock().map_err(|e| e.to_string())?;
        if let Some(mut child) = child.take() {
            child.stop()?;
        }
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.child
            .lock()
            .map(|child| child.is_some())
            .unwrap_or(false)
    }
}

impl<P: ProcessSpawner> Drop for StreamingServer<P> {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

pub fn command_spec(project_root: &Path) -> CommandSpec {
    CommandSpec {
        program: project_root.join("binaries").join("stremio-runtime-macos"),
        args: vec![project_root.join("resources").join("server.cjs")],
    }
}

fn default_project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}
