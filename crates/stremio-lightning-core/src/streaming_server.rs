//! Streaming-server process supervision shared by every native shell.
//!
//! A shell supplies a [`CommandSpec`] (what to run, with which environment and log files)
//! and a [`ProcessSpawner`] (how to run it). [`StreamingServerSupervisor`] owns the rest:
//! start-once, stop, restart, reaping an exited child, and the rotating log files.

use crate::streaming_logs::{
    ManagedChild, StreamingLogFiles, StreamingLogPaths, StreamingLogTails,
};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};

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

    /// # Errors
    /// Returns an error when the process cannot be spawned.
    fn spawn(&self, spec: CommandSpec) -> Result<Self::Child, String>;
}

pub trait ProcessChild: Send + 'static {
    /// # Errors
    /// Returns an error when the process cannot be stopped.
    fn stop(&mut self) -> Result<(), String>;

    /// # Errors
    /// Returns an error when the process state cannot be read.
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

impl ProcessChild for ManagedChild {
    fn stop(&mut self) -> Result<(), String> {
        ManagedChild::stop(self)
    }

    fn has_exited(&mut self) -> Result<bool, String> {
        ManagedChild::has_exited(self)
    }
}

/// Spawns the server as a plain child process whose output goes to the rotating logs.
#[derive(Debug, Default, Clone)]
pub struct RealProcessSpawner;

impl ProcessSpawner for RealProcessSpawner {
    type Child = ManagedChild;

    fn spawn(&self, spec: CommandSpec) -> Result<Self::Child, String> {
        let mut command = Command::new(&spec.program);
        command.args(&spec.args);
        command.envs(&spec.env);
        ManagedChild::spawn(
            &mut command,
            StreamingLogFiles::new(spec.stdout_log, spec.stderr_log),
        )
    }
}

pub struct StreamingServerSupervisor<P: ProcessSpawner> {
    spawner: P,
    child: Mutex<Option<P::Child>>,
    spec: CommandSpec,
    log_files: StreamingLogFiles,
    disabled: bool,
}

impl<P: ProcessSpawner> StreamingServerSupervisor<P> {
    #[must_use]
    pub fn new(spawner: P, spec: CommandSpec) -> Self {
        let log_files = StreamingLogFiles::new(&spec.stdout_log, &spec.stderr_log);
        Self {
            spawner,
            child: Mutex::new(None),
            spec,
            log_files,
            disabled: false,
        }
    }

    #[must_use]
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    #[must_use]
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// Starts the server unless it is disabled or already running.
    ///
    /// # Errors
    /// Returns an error when the child state cannot be read or the process cannot be spawned.
    pub fn start(&self) -> Result<(), String> {
        if self.disabled {
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

        *child = Some(self.spawner.spawn(self.spec.clone())?);
        Ok(())
    }

    /// # Errors
    /// Returns an error when the process cannot be stopped.
    pub fn stop(&self) -> Result<(), String> {
        let mut child = self.child.lock().map_err(|e| e.to_string())?;
        if let Some(mut child) = child.take() {
            child.stop()?;
        }
        Ok(())
    }

    /// # Errors
    /// Returns an error when stopping or starting fails.
    pub fn restart(&self) -> Result<(), String> {
        self.stop()?;
        self.start()
    }

    #[must_use]
    pub fn is_running(&self) -> bool {
        self.refresh_running_state().unwrap_or(false)
    }

    /// Reports whether the child is alive, dropping it from the supervisor once it has exited.
    ///
    /// # Errors
    /// Returns an error when the child state cannot be read.
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

    #[must_use]
    pub fn log_paths(&self) -> StreamingLogPaths {
        self.log_files.paths()
    }

    /// # Errors
    /// Returns an error when the log files cannot be read.
    pub fn log_tails(&self, max_bytes_per_stream: usize) -> Result<StreamingLogTails, String> {
        self.log_files
            .tails(max_bytes_per_stream)
            .map_err(|error| format!("Failed to read streaming server log tails: {error}"))
    }

    /// # Errors
    /// Returns an error when the log files cannot be cleared.
    pub fn clear_logs(&self) -> Result<(), String> {
        self.log_files
            .clear()
            .map_err(|error| format!("Failed to clear streaming server logs: {error}"))
    }
}

impl<P: ProcessSpawner + std::fmt::Debug> std::fmt::Debug for StreamingServerSupervisor<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StreamingServerSupervisor")
            .field("spawner", &self.spawner)
            .field("spec", &self.spec)
            .field("disabled", &self.disabled)
            .finish_non_exhaustive()
    }
}

impl<P: ProcessSpawner> Drop for StreamingServerSupervisor<P> {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

/// In-memory spawner that records specs and stops instead of running processes.
#[derive(Debug, Default, Clone)]
pub struct FakeProcessSpawner {
    spawned: Arc<Mutex<Vec<CommandSpec>>>,
    stopped: Arc<Mutex<Vec<usize>>>,
    fail_next_spawn: Arc<Mutex<Option<String>>>,
    next_child_exited: Arc<Mutex<bool>>,
}

impl FakeProcessSpawner {
    /// # Panics
    /// Panics if the recorded-spawns lock is poisoned.
    #[must_use]
    pub fn spawned(&self) -> Vec<CommandSpec> {
        self.spawned
            .lock()
            .expect("fake process spawner poisoned")
            .clone()
    }

    /// # Panics
    /// Panics if the stopped-children lock is poisoned.
    #[must_use]
    pub fn stopped(&self) -> Vec<usize> {
        self.stopped
            .lock()
            .expect("fake process spawner stopped list poisoned")
            .clone()
    }

    /// # Panics
    /// Panics if the failure-flag lock is poisoned.
    pub fn fail_next_spawn(&self, error: impl Into<String>) {
        *self
            .fail_next_spawn
            .lock()
            .expect("fake process spawner failure flag poisoned") = Some(error.into());
    }

    /// # Panics
    /// Panics if the exit-flag lock is poisoned.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_spec() -> CommandSpec {
        CommandSpec {
            program: PathBuf::from("/bin/server"),
            args: vec![PathBuf::from("--flag")],
            env: BTreeMap::new(),
            stdout_log: PathBuf::from("/logs/out.log"),
            stderr_log: PathBuf::from("/logs/err.log"),
        }
    }

    fn supervisor(spawner: &FakeProcessSpawner) -> StreamingServerSupervisor<FakeProcessSpawner> {
        StreamingServerSupervisor::new(spawner.clone(), test_spec())
    }

    #[test]
    fn start_spawns_the_spec_once_while_running() {
        let spawner = FakeProcessSpawner::default();
        let server = supervisor(&spawner);

        server.start().unwrap();
        server.start().unwrap();

        assert!(server.is_running());
        assert_eq!(spawner.spawned(), vec![test_spec()]);
    }

    #[test]
    fn stop_then_restart_spawns_a_new_child() {
        let spawner = FakeProcessSpawner::default();
        let server = supervisor(&spawner);

        server.start().unwrap();
        server.stop().unwrap();
        assert!(!server.is_running());
        server.restart().unwrap();

        assert!(server.is_running());
        assert_eq!(spawner.spawned().len(), 2);
        assert_eq!(spawner.stopped(), vec![1]);
    }

    #[test]
    fn exited_child_is_reaped_and_start_spawns_again() {
        let spawner = FakeProcessSpawner::default();
        spawner.set_next_child_exited(true);
        let server = supervisor(&spawner);
        server.start().unwrap();
        assert!(!server.is_running());

        spawner.set_next_child_exited(false);
        server.start().unwrap();

        assert!(server.is_running());
        assert_eq!(spawner.spawned().len(), 2);
    }

    #[test]
    fn disabled_server_never_spawns() {
        let spawner = FakeProcessSpawner::default();
        let server = supervisor(&spawner).disabled(true);

        server.start().unwrap();

        assert!(server.is_disabled());
        assert!(!server.is_running());
        assert!(spawner.spawned().is_empty());
    }

    #[test]
    fn spawn_failure_is_returned_and_leaves_server_stopped() {
        let spawner = FakeProcessSpawner::default();
        spawner.fail_next_spawn("boom");
        let server = supervisor(&spawner);

        assert_eq!(server.start().unwrap_err(), "boom");
        assert!(!server.is_running());
    }

    #[test]
    fn dropping_the_supervisor_stops_the_child() {
        let spawner = FakeProcessSpawner::default();
        {
            let server = supervisor(&spawner);
            server.start().unwrap();
        }

        assert_eq!(spawner.stopped(), vec![1]);
    }

    #[test]
    fn log_paths_come_from_the_spec() {
        let spawner = FakeProcessSpawner::default();
        let paths = supervisor(&spawner).log_paths();

        assert_eq!(paths.stdout, PathBuf::from("/logs/out.log"));
        assert_eq!(paths.stderr, PathBuf::from("/logs/err.log"));
    }
}
