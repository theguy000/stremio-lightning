//! The local streaming server: where its files live, how it is supervised, and what state it is
//! in once started. Reports plain `State` values; Settings owns the wording.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use super::logs::Logs;
use super::server_child::ManagedChild;

pub(crate) const ADDRESS: &str = "127.0.0.1:11470";

/// What the server is doing. `PortTaken` is normal when another Stremio instance already holds the
/// port: our child exits because it cannot bind, and the app uses the running server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum State {
    NotStarted(String),
    Ready,
    PortTaken,
    NotAnswering,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandSpec {
    pub(crate) program: PathBuf,
    pub(crate) args: Vec<PathBuf>,
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) stdout_log: PathBuf,
    pub(crate) stderr_log: PathBuf,
}

trait ProcessChild: Send + 'static {
    fn stop(&mut self) -> Result<(), String>;
    fn has_exited(&mut self) -> Result<bool, String>;
}

impl ProcessChild for ManagedChild {
    fn stop(&mut self) -> Result<(), String> {
        ManagedChild::stop(self)
    }

    fn has_exited(&mut self) -> Result<bool, String> {
        ManagedChild::has_exited(self)
    }
}

trait ProcessSpawner: Send + Sync + 'static {
    type Child: ProcessChild;
    fn spawn(&self, spec: CommandSpec) -> Result<Self::Child, String>;
}

struct RealSpawner {
    logs: Arc<Logs>,
}

impl ProcessSpawner for RealSpawner {
    type Child = ManagedChild;

    fn spawn(&self, spec: CommandSpec) -> Result<Self::Child, String> {
        let mut command = Command::new(&spec.program);
        command.args(&spec.args).envs(&spec.env);
        ManagedChild::spawn(
            &mut command,
            spec.stdout_log,
            spec.stderr_log,
            self.logs.clone(),
        )
    }
}

pub(crate) struct Supervisor {
    inner: Supervised<RealSpawner>,
}

impl Supervisor {
    pub(crate) fn new(bundle: &Path, data_dir: &Path, logs: Arc<Logs>) -> Self {
        Self {
            inner: Supervised::new(RealSpawner { logs }, spec(bundle, data_dir)),
        }
    }

    pub(crate) fn start(&self) -> Result<(), String> {
        self.inner.start()
    }

    pub(crate) fn stop(&self) -> Result<(), String> {
        self.inner.stop()
    }

    pub(crate) fn is_running(&self) -> bool {
        self.inner.is_running()
    }
}

struct Supervised<P: ProcessSpawner> {
    spawner: P,
    child: Mutex<Option<P::Child>>,
    spec: CommandSpec,
}

impl<P: ProcessSpawner> Supervised<P> {
    fn new(spawner: P, spec: CommandSpec) -> Self {
        Self {
            spawner,
            child: Mutex::new(None),
            spec,
        }
    }

    fn start(&self) -> Result<(), String> {
        let mut child = self.child.lock().map_err(|e| e.to_string())?;
        if let Some(existing) = child.as_mut() {
            if existing.has_exited()? {
                *child = None;
            } else {
                return Ok(());
            }
        }
        *child = Some(self.spawner.spawn(self.spec.clone())?);
        Ok(())
    }

    fn stop(&self) -> Result<(), String> {
        let mut child = self.child.lock().map_err(|e| e.to_string())?;
        if let Some(mut existing) = child.take() {
            existing.stop()?;
        }
        Ok(())
    }

    fn is_running(&self) -> bool {
        let Ok(mut child) = self.child.lock() else {
            return false;
        };
        if let Some(existing) = child.as_mut() {
            if existing.has_exited().unwrap_or(true) {
                *child = None;
                return false;
            }
            return true;
        }
        false
    }
}

impl<P: ProcessSpawner> Drop for Supervised<P> {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// The folder holding `resources/`: the executable's folder unless `STREMIO_LIGHTNING_BUNDLE_DIR`
/// says otherwise.
pub(crate) fn bundle_dir() -> PathBuf {
    std::env::var_os("STREMIO_LIGHTNING_BUNDLE_DIR")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(Path::to_path_buf))
        })
        .unwrap_or_default()
}

/// The stream server and its ffmpeg live in `<bundle>/resources`.
pub(crate) fn spec(bundle: &Path, data_dir: &Path) -> CommandSpec {
    let resources = bundle.join("resources");
    let program = resources.join(if cfg!(windows) {
        "stream-server.exe"
    } else {
        "stream-server"
    });
    let mut search = vec![resources];
    search.extend(std::env::split_paths(
        &std::env::var_os("PATH").unwrap_or_default(),
    ));
    let path = std::env::join_paths(search)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let logs = data_dir.join("logs");
    CommandSpec {
        program,
        args: if cfg!(windows) {
            vec![PathBuf::from("--no-tray")]
        } else {
            Vec::new()
        },
        env: [("PATH".to_owned(), path)].into(),
        stdout_log: logs.join("stremio-server.stdout.log"),
        stderr_log: logs.join("stremio-server.stderr.log"),
    }
}

/// Waits for something to answer on the server port and reports which state landed.
pub(crate) async fn wait_until_answering(
    server: &Supervisor,
    started: Result<(), String>,
) -> State {
    let Err(error) = started else {
        let mut answering = false;
        for _ in 0..60 {
            if tokio::net::TcpStream::connect(ADDRESS).await.is_ok() {
                answering = true;
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        // Give a child that cannot bind the port time to exit before judging.
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        return match (answering, server.is_running()) {
            (true, true) => State::Ready,
            (true, false) => State::PortTaken,
            (false, _) => State::NotAnswering,
        };
    };
    State::NotStarted(error)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default, Clone)]
    struct FakeSpawner {
        spawned: Arc<Mutex<Vec<CommandSpec>>>,
        stopped: Arc<Mutex<Vec<usize>>>,
        exited: Arc<Mutex<bool>>,
    }

    struct FakeChild {
        id: usize,
        stopped: Arc<Mutex<Vec<usize>>>,
        exited: bool,
    }

    impl ProcessChild for FakeChild {
        fn stop(&mut self) -> Result<(), String> {
            self.stopped.lock().unwrap().push(self.id);
            self.exited = true;
            Ok(())
        }
        fn has_exited(&mut self) -> Result<bool, String> {
            Ok(self.exited)
        }
    }

    impl ProcessSpawner for FakeSpawner {
        type Child = FakeChild;
        fn spawn(&self, spec: CommandSpec) -> Result<Self::Child, String> {
            let mut list = self.spawned.lock().unwrap();
            list.push(spec);
            Ok(FakeChild {
                id: list.len(),
                stopped: self.stopped.clone(),
                exited: *self.exited.lock().unwrap(),
            })
        }
    }

    #[test]
    fn spec_places_bundle_resources_first_on_path() {
        let bundle = Path::new("bundle");
        let s = spec(bundle, Path::new("data"));
        let resources = bundle.join("resources");
        assert_eq!(s.program.parent(), Some(resources.as_path()));
        let path = s.env.get("PATH").expect("PATH is set");
        assert_eq!(std::env::split_paths(path).next(), Some(resources));
        assert_eq!(s.stdout_log.parent(), Some(Path::new("data/logs")));
    }

    #[test]
    fn start_once_reap_exited_and_stop_on_drop() {
        let spawner = FakeSpawner::default();
        {
            let sup = Supervised::new(spawner.clone(), spec(Path::new("b"), Path::new("d")));
            sup.start().unwrap();
            sup.start().unwrap();
            assert!(sup.is_running());
            assert_eq!(spawner.spawned.lock().unwrap().len(), 1);
            sup.stop().unwrap();
            assert!(!sup.is_running());
            *spawner.exited.lock().unwrap() = true;
            sup.start().unwrap();
            assert!(!sup.is_running());
            *spawner.exited.lock().unwrap() = false;
            sup.start().unwrap();
        }
        assert_eq!(*spawner.stopped.lock().unwrap(), vec![1, 3]);
    }
}
