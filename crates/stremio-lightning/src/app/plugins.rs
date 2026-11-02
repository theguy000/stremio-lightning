//! The plugins the app is running: which ones started, why any of them stopped, and the way the
//! rest of the app reaches them.
//!
//! One of these lives for as long as the window does. It is what turns `enabled_plugins` in the
//! shell's `settings.json` into interpreters: [`Runtime::start_enabled`] runs every file the list
//! names, a press on a card starts or stops one, and every line a plugin logs or asks for lands
//! here, on the app's thread, as plain data.
//!
//! Nothing in this file draws. The Mods screen reads a [`report`] of it and the window is shown
//! that, the same way every other screen gets its data.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde_json::Value as Json;
use stremio_lightning_core::mods::{self, ModType};

use crate::plumbing::plugin::{self, Event, Host, LogLine, Outcome, Running};
use crate::screens::mods::settings::{self, Running as State};

/// What one plugin's thread has reached out about, waiting for the app's thread to act on it.
#[derive(Clone, Debug)]
pub enum Asked {
    /// A line for the Logs page and the plugin's own state.
    Log(LogLine, bool),
    /// Rules the plugin wants the window drawn with.
    Theme(Vec<Json>),
    /// Something about the rest of the app changed and the Mods screen should be redrawn.
    Changed,
}

/// One plugin the app started, and everything the app needs to say about it.
pub struct Started {
    /// The thread's own answer to every call. Only the registry's thread touches it.
    pub running: Running,
    /// The last line the plugin logged at `warn` or `error`.
    last_error: String,
    /// Every line the plugin has logged, newest last, for the Logs page.
    pub lines: Vec<LogLine>,
}

impl Started {
    fn record(&mut self, outcome: &Outcome) {
        for line in &outcome.logs {
            self.lines.push(line.clone());
            if line.level == "warn" || line.level == "error" {
                self.last_error = line.text.clone();
            }
        }
        let keep = plugin::LOG_LINES;
        if self.lines.len() > keep {
            let drop = self.lines.len() - keep;
            self.lines.drain(..drop);
        }
    }
}

/// Everything the app runs and the way it reaches it.
pub struct Runtime {
    /// The app's data folder: the plugins folder and every settings file live under it.
    data_dir: PathBuf,
    /// The plugins that are running, by file name.
    started: Mutex<HashMap<String, Arc<Mutex<Started>>>>,
    /// Plugins switched on whose thread is gone, by file name, and why: what the Plugins page
    /// says about them until they are switched off and on again.
    failed: Mutex<HashMap<String, String>>,
    /// What plugins have asked the app to do, for the app's own thread to take.
    asked: Mutex<Vec<Asked>>,
    /// Counts the asks, so the screen can tell whether anything about a plugin changed without
    /// waking every listener.
    count: AtomicU64,
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("plugin::Runtime")
            .field(
                "started",
                &self
                    .started
                    .lock()
                    .map(|m| m.keys().cloned().collect::<Vec<_>>()),
            )
            .finish()
    }
}

impl Runtime {
    pub fn new(data_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            data_dir,
            started: Mutex::default(),
            failed: Mutex::default(),
            asked: Mutex::default(),
            count: AtomicU64::default(),
        })
    }

    /// Starts every plugin `settings.json` names, in the order it names them. A file the list
    /// names that is no longer in the folder is reported as a plugin that is not there, not
    /// silently dropped: the list is the user's, and only they take a name out of it.
    pub fn start_enabled(self: &Arc<Self>, enabled: &[String]) -> Vec<Asked> {
        enabled
            .iter()
            .flat_map(|filename| self.start(filename))
            .collect()
    }

    /// Starts `filename` if it is not running: reads its file, hands the thread a host, and
    /// remembers whatever the load did. Returns what the app was asked to look at because of it.
    pub fn start(self: &Arc<Self>, filename: &str) -> Vec<Asked> {
        if lock(&self.started).contains_key(filename) {
            return Vec::new();
        }
        lock(&self.failed).remove(filename);
        let plugins_dir = self.plugins_dir();
        let name = plugin::name_of(filename);
        let path = plugins_dir.join(filename);
        let source = match std::fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => {
                let said = format!("{} could not be read: {error}", path.display());
                self.failed(filename, said.clone());
                return vec![
                    Asked::Log(LogLine::new("error", said), true),
                    Asked::Changed,
                ];
            }
        };
        let host = Hosted {
            dir: self.data_dir.clone(),
            plugin: filename.to_owned(),
            app: Arc::clone(self),
        };
        let (running, outcome) = Running::start(source, &name, host);
        let mut started = Started {
            running,
            last_error: settings::last_error(&outcome.logs),
            lines: Vec::new(),
        };
        started.record(&outcome);
        let mut asked = Vec::new();
        if let Some(rules) = &outcome.theme {
            asked.push(Asked::Theme(rules.clone()));
        }
        for line in &outcome.logs {
            asked.push(Asked::Log(line.clone(), false));
        }
        asked.push(Asked::Changed);
        if started.running.running() {
            lock(&self.started).insert(filename.to_owned(), Arc::new(Mutex::new(started)));
        } else {
            self.failed(filename, outcome.run.said());
            asked.push(Asked::Log(LogLine::new("error", outcome.run.said()), true));
        }
        self.bump();
        asked
    }

    /// Stops `filename`, if it is running: the plugin's thread is let go, and the state that said
    /// it was running goes with it. Nothing is deleted and nothing is forgotten.
    pub fn stop(&self, filename: &str) -> Vec<Asked> {
        let gone = lock(&self.started).remove(filename);
        lock(&self.failed).remove(filename);
        if gone.is_none() {
            return Vec::new();
        }
        self.bump();
        vec![Asked::Changed]
    }

    /// Whether `filename` is running now.
    pub fn running(&self, filename: &str) -> bool {
        lock(&self.started).contains_key(filename)
    }

    /// The plugin's `onSettingsSaved` callbacks, told the values its settings file now holds.
    pub fn settings_saved(&self, filename: &str, values: Json) -> Vec<Asked> {
        let Some(cell) = lock(&self.started).get(filename).cloned() else {
            return Vec::new();
        };
        let asked = {
            let mut entry = lock(&cell);
            let outcome = entry.running.settings_saved(values);
            entry.record(&outcome);
            asked_by(&outcome, &entry)
        };
        self.close(filename, &cell);
        self.finish(asked)
    }

    /// One thing that happened in the app, told to every plugin that listens for it.
    pub fn deliver(&self, event: Event) -> Vec<Asked> {
        let cells: Vec<(String, Arc<Mutex<Started>>)> = lock(&self.started)
            .iter()
            .map(|(name, cell)| (name.clone(), Arc::clone(cell)))
            .collect();
        let mut asked = Vec::new();
        for (name, cell) in cells {
            {
                let mut entry = lock(&cell);
                let outcome = entry.running.deliver(event.clone());
                entry.record(&outcome);
                asked.extend(asked_by(&outcome, &entry));
            }
            self.close(&name, &cell);
        }
        self.finish(asked)
    }

    /// Moves a plugin that stopped out of the running set, so every page says why rather than
    /// going on claiming it runs.
    fn close(&self, filename: &str, cell: &Arc<Mutex<Started>>) {
        let running = lock(cell).running.running();
        if running {
            return;
        }
        let reason = lock(cell).running.stopped().to_owned();
        let mut started = lock(&self.started);
        if let Some(current) = started.get(filename) {
            if Arc::ptr_eq(current, cell) {
                started.remove(filename);
            }
        }
        drop(started);
        self.failed(filename, reason);
    }

    fn finish(&self, mut asked: Vec<Asked>) -> Vec<Asked> {
        self.bump();
        asked.push(Asked::Changed);
        asked
    }

    fn failed(&self, filename: &str, reason: String) {
        lock(&self.failed).insert(filename.to_owned(), reason);
        self.bump();
    }

    /// Everything waiting for the app's thread, in the order it was asked for.
    pub fn take(&self) -> Vec<Asked> {
        std::mem::take(&mut *lock(&self.asked))
    }

    /// What the settings page says about one plugin.
    pub fn state(&self, filename: &str, enabled: bool) -> State {
        if let Some(cell) = lock(&self.started).get(filename) {
            return State {
                running: true,
                stopped: String::new(),
                last_error: lock(cell).last_error.clone(),
            };
        }
        let stopped = lock(&self.failed)
            .get(filename)
            .cloned()
            .unwrap_or_default();
        State {
            running: false,
            stopped: if enabled { stopped } else { String::new() },
            last_error: String::new(),
        }
    }

    /// The lines every running plugin has logged, oldest first, as the Logs page shows them: the
    /// plugin's name, then the line.
    pub fn lines(&self) -> Vec<(String, LogLine)> {
        let started = lock(&self.started);
        let mut all: Vec<(String, LogLine)> = started
            .iter()
            .flat_map(|(filename, cell)| {
                lock(cell)
                    .lines
                    .iter()
                    .map(|line| (filename.clone(), line.clone()))
                    .collect::<Vec<_>>()
            })
            .collect();
        all.sort_by(|a, b| a.0.cmp(&b.0));
        all
    }

    /// A number that changes whenever any plugin started, stopped or logged.
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::SeqCst)
    }

    fn bump(&self) {
        self.count.fetch_add(1, Ordering::SeqCst);
    }

    fn plugins_dir(&self) -> PathBuf {
        mods::mods_dir(&self.data_dir, ModType::Plugin)
    }
}

/// One plugin's way back into the app. Each plugin's thread owns one for as long as it runs, so
/// every line, setting and theme it asks for arrives with the plugin's name attached.
struct Hosted {
    dir: PathBuf,
    plugin: String,
    app: Arc<Runtime>,
}

impl Hosted {
    fn plugins_dir(&self) -> PathBuf {
        mods::mods_dir(&self.dir, ModType::Plugin)
    }

    fn ask(&self, asked: Asked) {
        lock(&self.app.asked).push(asked);
        self.app.bump();
    }
}

impl Host for Hosted {
    fn get_setting(&mut self, _plugin: &str, key: &str) -> Json {
        stremio_lightning_core::settings::setting(
            &self.plugins_dir(),
            &plugin::name_of(&self.plugin),
            key,
        )
        .unwrap_or(Json::Null)
    }

    fn save_setting(&mut self, _plugin: &str, key: &str, value: &Json) {
        let name = plugin::name_of(&self.plugin);
        let _ = stremio_lightning_core::settings::save_setting(
            &self.plugins_dir(),
            &name,
            key,
            value.clone(),
        );
        self.ask(Asked::Changed);
    }

    fn register_settings(&mut self, _plugin: &str, schema: &Json) {
        let name = plugin::name_of(&self.plugin);
        let _ = plugin::record_schema(&self.plugins_dir(), &name, schema);
        self.ask(Asked::Changed);
    }

    fn apply_theme(&mut self, _plugin: &str, rules: &[Json]) {
        self.ask(Asked::Theme(rules.to_vec()));
    }

    fn log(&mut self, _plugin: &str, line: LogLine, stopped: bool) {
        self.ask(Asked::Log(line, stopped));
    }
}

/// What one call into a plugin asked for, as asks for the app's thread. A plugin that stopped
/// during the call adds the sentence that says why.
fn asked_by(outcome: &Outcome, entry: &Started) -> Vec<Asked> {
    let mut asked: Vec<Asked> = outcome
        .logs
        .iter()
        .map(|line| Asked::Log(line.clone(), false))
        .collect();
    if let Some(rules) = &outcome.theme {
        asked.push(Asked::Theme(rules.clone()));
    }
    if !entry.running.running() {
        asked.push(Asked::Log(
            LogLine::new("error", entry.running.stopped()),
            true,
        ));
    }
    asked
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
