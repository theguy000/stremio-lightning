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
    /// A plugin whose file is being loaded: it is on its way to running, and it has nothing to
    /// say about itself yet.
    fn starting() -> Self {
        Self {
            running: Running::starting(),
            last_error: String::new(),
            lines: Vec::new(),
        }
    }

    fn record(&mut self, outcome: &Outcome) {
        // The outcome's lines reach the plugin's log only through here. A plugin's card reads the
        // runtime's state, and the Logs page reads `Runtime::lines`, which reads this.
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
    /// The lines of the plugins that stopped, by file name, so the Logs page can still show why
    /// they stopped. A start replaces them; nothing else removes them.
    stopped: Mutex<HashMap<String, Arc<Mutex<Started>>>>,
    /// What plugins have asked the app to do on their own thread, for the app's own thread to
    /// take. A call the app makes returns its asks directly; this holds what arrived without one,
    /// such as a line a plugin logged from a timer.
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
            stopped: Mutex::default(),
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
        // The plugin is put in the running set before its file is loaded, not after. A load can
        // take most of a second, and a second press on the switch inside that window used to
        // find nothing running and start a second interpreter for the same file. The place is
        // taken with a cell that says it is starting; the load fills it in or takes it out.
        let cell = Arc::new(Mutex::new(Started::starting()));
        {
            let mut started = lock(&self.started);
            started
                .entry(filename.to_owned())
                .or_insert_with(|| Arc::clone(&cell));
        }
        lock(&self.failed).remove(filename);
        lock(&self.stopped).remove(filename);
        let plugins_dir = self.plugins_dir();
        let name = plugin::name_of(filename);
        let path = plugins_dir.join(filename);
        let source = match std::fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => {
                self.withdraw(filename, &cell);
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
        let live = {
            let mut entry = lock(&cell);
            entry.running = running;
            entry.last_error = settings::last_error(&outcome.logs);
            entry.record(&outcome);
            entry.running.running()
        };
        let mut asked = Vec::new();
        if let Some(rules) = &outcome.theme {
            asked.push(Asked::Theme(rules.clone()));
        }
        for line in &outcome.logs {
            asked.push(Asked::Log(line.clone(), false));
        }
        asked.push(Asked::Changed);
        if live {
            self.bump();
        } else {
            self.withdraw(filename, &cell);
            self.failed(filename, outcome.run.said());
            asked.push(Asked::Log(LogLine::new("error", outcome.run.said()), true));
        }
        asked
    }

    /// Takes a plugin that never got going out of the running set, if the place it took is still
    /// the one it was given.
    fn withdraw(&self, filename: &str, cell: &Arc<Mutex<Started>>) {
        let mut started = lock(&self.started);
        if started
            .get(filename)
            .is_some_and(|held| Arc::ptr_eq(held, cell))
        {
            started.remove(filename);
        }
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
            let mut outcome = entry.running.settings_saved(values);
            if outcome.logs.is_empty() && outcome.run.failed() {
                let reason = outcome.run.said();
                outcome.logs.push(LogLine::new("error", reason));
            }
            entry.record(&outcome);
            asked_by(&outcome, &entry)
        };
        self.close(filename, &cell);
        self.finish(asked)
    }

    /// Whether any running plugin has a timer waiting. The event task asks every few tenths of a
    /// second whether to call in at all, and a plugin with no timers is never called into for it.
    pub fn holds_timers(&self) -> bool {
        lock(&self.started)
            .values()
            .any(|cell| lock(cell).running.holds_timers())
    }

    /// Lets every running plugin run the timers that have come due, and returns what they asked
    /// for while they ran. A plugin's timer code only runs inside a call into it, so this is the
    /// call that makes a `setTimeout` fire even when nothing in the app happens.
    pub fn advance(&self) -> Vec<Asked> {
        let cells: Vec<(String, Arc<Mutex<Started>>)> = lock(&self.started)
            .iter()
            .map(|(name, cell)| (name.clone(), Arc::clone(cell)))
            .collect();
        let mut asked = Vec::new();
        for (name, cell) in cells {
            {
                let mut entry = lock(&cell);
                let mut outcome = entry.running.advance();
                if outcome.logs.is_empty() && outcome.run.failed() {
                    let reason = outcome.run.said();
                    outcome.logs.push(LogLine::new("error", reason));
                }
                entry.record(&outcome);
                asked.extend(asked_by(&outcome, &entry));
            }
            self.close(&name, &cell);
        }
        if asked.is_empty() {
            return Vec::new();
        }
        self.finish(asked)
    }

    /// One thing that happened in the app, told to every plugin that listens for it. Returns
    /// everything the plugins asked for while they heard it, which is what the caller draws: a
    /// plugin that logs from an event callback or sets a setting there has its line on its card
    /// without waiting for the next load or save.
    pub fn deliver(&self, event: Event) -> Vec<Asked> {
        let cells: Vec<(String, Arc<Mutex<Started>>)> = lock(&self.started)
            .iter()
            .map(|(name, cell)| (name.clone(), Arc::clone(cell)))
            .collect();
        let mut asked = Vec::new();
        for (name, cell) in cells {
            {
                let mut entry = lock(&cell);
                let mut outcome = entry.running.deliver(event.clone());
                // A plugin that ran past its budget is answered by the channel rather than by
                // its own thread, so the reason it stopped is not in what its thread reported.
                // It travels with the outcome, which is the only thing the card reads.
                if outcome.logs.is_empty() && outcome.run.failed() {
                    let reason = outcome.run.said();
                    outcome.logs.push(LogLine::new("error", reason));
                }
                entry.record(&outcome);
                asked.extend(asked_by(&outcome, &entry));
            }
            self.close(&name, &cell);
        }
        self.finish(asked)
    }

    /// Moves a plugin that stopped out of the running set, so every page says why rather than
    /// going on claiming it runs. A plugin that answered its call is still running, and a reason
    /// one of its earlier calls recorded is no longer true, so it is taken away.
    ///
    /// A plugin that stopped keeps its lines: the call that stopped it may have been answered by
    /// the closed channel rather than by its own thread, and its lines are the only reason the
    /// Logs page ever gets. They live beside the reason until the plugin is started again.
    fn close(&self, filename: &str, cell: &Arc<Mutex<Started>>) {
        let running = lock(cell).running.running();
        if running {
            lock(&self.failed).remove(filename);
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
        self.retain(filename, cell);
        self.failed(filename, reason);
    }

    /// Keeps the lines of a plugin that stopped, where `lines` can still find them. They are held
    /// with the plugin's cell, which nothing starts again until they are replaced.
    fn retain(&self, filename: &str, cell: &Arc<Mutex<Started>>) {
        lock(&self.stopped).insert(filename.to_owned(), Arc::clone(cell));
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

    /// What the settings page says about one plugin. A plugin still loading is running: its
    /// switch is on, and the line it gives once it has loaded replaces this one.
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

    /// The lines every plugin has logged, oldest first and by plugin, as the Logs page shows
    /// them: the plugin's name, then the line. A plugin that stopped is here too, which is where
    /// the reason it stopped is read from. This is the Logs page's only source; nothing else
    /// keeps a plugin's lines, and the list is in memory for this run of the app.
    pub fn lines(&self) -> Vec<(String, LogLine)> {
        let mut all: Vec<(String, LogLine)> = Vec::new();
        {
            let stopping = lock(&self.stopped);
            for (filename, cell) in lock(&self.started).iter().chain(stopping.iter()) {
                let lines = lock(cell)
                    .lines
                    .iter()
                    .map(|line| (filename.clone(), line.clone()))
                    .collect::<Vec<_>>();
                all.extend(lines);
            }
        }
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

    fn register_settings(&mut self, _plugin: &str, schema: &Json) -> Result<(), String> {
        let name = plugin::name_of(&self.plugin);
        let recorded = plugin::record_schema(&self.plugins_dir(), &name, schema);
        // Even a schema the page cannot draw is news: the Plugin settings page says so.
        self.ask(Asked::Changed);
        recorded
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
