//! Plugins: the JavaScript files in the plugins folder, run in a small interpreter.
//!
//! The native app has no web page, so a plugin is not a patch of stremio-web: it is a script that
//! this module loads into a QuickJS instance and gives a few global functions. `docs/plugin-api.md`
//! is the contract a plugin author writes against; this file implements it.
//!
//! What a plugin file gets:
//!
//! - `console.log`, `console.warn` and `console.error`, and `logger.debug`, `logger.info`,
//!   `logger.warn` and `logger.error`. Every line becomes a `LogLine` the Mods screen shows.
//! - `getSetting(key)` and `saveSetting(key, value)`, reading and writing the plugin's own
//!   `<name>.plugin.json` through `stremio_lightning_core::settings`.
//! - `registerSettings(schema)`, which records the schema the Plugin settings page draws.
//! - `onSettingsSaved(callback)`, which the app calls after the page saved this plugin's values.
//! - `applyTheme(rules)`, which restyles the window the way a theme file does.
//! - `setTimeout`, `clearTimeout`, `setInterval` and `clearInterval`, which fire only while the
//!   plugin's interpreter is running its own code.
//! - `on(event, callback)`, which subscribes to what the app says about the outside world.
//!
//! Nothing here touches Slint or a window, so a test runs a real plugin file from a temp folder
//! with no app behind it.
//!
//! Each plugin runs on its own thread: a plugin that never returns has to be left running in the
//! interpreter, and the app's own thread has to go on drawing. Every call into a plugin (the file
//! as it loads, a settings callback, an event, the timers one of them fired) is given
//! [`BUDGET`] of wall-clock time. When the budget runs out the interpreter raises an error inside
//! the plugin and, if the plugin swallows it and carries on, the plugin is closed for good: its
//! thread is left to the interpreter, and every later call says so at once.

use std::path::Path;
use std::sync::mpsc::{RecvTimeoutError, Sender};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use serde_json::Value as Json;
use stremio_lightning_core::mods::{self, ModType};
use stremio_lightning_core::settings;

use crate::screens::mods::plugin_schema;

mod globals;
mod worker;

pub use worker::Running;

/// How long one call into a plugin may take before the interpreter stops it.
pub const BUDGET: Duration = Duration::from_millis(200);
/// How much memory one plugin may use before its allocations fail.
const MEMORY_LIMIT: usize = 8 * 1024 * 1024;
/// How much of the thread's stack the interpreter may use. QuickJS counts this from the stack
/// pointer it has when an evaluation starts, and throws `InternalError: stack overflow` at the
/// limit instead of running into the guard page and killing the process. The plugin's thread gets
/// 2 MiB (see `worker::STACK_SIZE`), so this leaves half of it for the Rust frames below the
/// interpreter. A plugin that recurses deeply is closed with that error, not fatal to the app.
const STACK_LIMIT: usize = 1024 * 1024;
/// The most timers one plugin may have waiting at once. A plugin that sets one from a timer is a
/// plugin that never finishes, however short each of its steps is.
const MAX_TIMERS: usize = 256;
/// The most lines the app keeps for one plugin, newest last. The Logs page reads these; a plugin
/// that runs for hours does not grow the list without end.
pub const LOG_LINES: usize = 200;
/// How long the app waits for the plugin's thread to answer a call. The interpreter stops its own
/// code after [`BUDGET`]; this waits for a thread that answers nothing at all.
const ANSWER_WAIT: Duration = Duration::from_millis(700);
/// What the screen says about a plugin whose thread stopped answering on its own.
const GONE: &str = "The interpreter the plugin ran in stopped without saying why.";

/// The name a plugin is known by outside JavaScript: its file name without `.plugin.js`. This is
/// the name its settings file uses, `<name>.plugin.json`, and the name the app hands its callbacks.
pub fn name_of(filename: &str) -> String {
    filename
        .strip_suffix(".plugin.js")
        .unwrap_or(filename)
        .to_owned()
}

/// Something worth one line on the Mods screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogLine {
    /// `debug`, `info`, `warn` or `error`.
    pub level: String,
    pub text: String,
}

impl LogLine {
    pub(crate) fn new(level: &str, text: impl Into<String>) -> Self {
        Self {
            level: level.to_owned(),
            text: text.into(),
        }
    }

    /// The line as the Logs page shows it: the level, then the text.
    pub fn shown(&self) -> String {
        format!("{}: {}", self.level, self.text)
    }
}

/// What the app tells a plugin about the outside world.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// A Video started playing: the core's own id for it.
    PlaybackStarted {
        video_id: String,
    },
    PlaybackStopped,
    /// The window went fullscreen or left it.
    Fullscreen {
        on: bool,
    },
    /// The streaming server started or stopped.
    ServerRunning {
        running: bool,
    },
    /// A catalogue arrived from the core.
    CatalogLoaded {
        id: String,
        count: usize,
    },
}

impl Event {
    /// The name a plugin subscribes with, and the value its callback is handed.
    pub fn named(&self) -> (&'static str, Json) {
        match self {
            Self::PlaybackStarted { video_id } => (
                "playback-started",
                serde_json::json!({ "videoId": video_id }),
            ),
            Self::PlaybackStopped => ("playback-stopped", Json::Null),
            Self::Fullscreen { on } => ("fullscreen", serde_json::json!({ "on": on })),
            Self::ServerRunning { running } => {
                ("server-running", serde_json::json!({ "running": running }))
            }
            Self::CatalogLoaded { id, count } => (
                "catalog-loaded",
                serde_json::json!({ "id": id, "count": count }),
            ),
        }
    }

    /// Every name a plugin may subscribe to.
    pub const NAMES: [&'static str; 5] = [
        "playback-started",
        "playback-stopped",
        "fullscreen",
        "server-running",
        "catalog-loaded",
    ];
}

/// Where a plugin runs, and what it may reach.
///
/// The shell implements this with the running app, so a plugin's save lands in the same
/// `settings.json` the Plugin settings page draws. Tests use `Folder`, which only touches the
/// plugin's own folder.
pub trait Host {
    /// A value from outside the interpreter, in the shapes JavaScript produces: strings, numbers,
    /// booleans, null, arrays and objects.
    fn get_setting(&mut self, plugin: &str, key: &str) -> Json;
    fn save_setting(&mut self, plugin: &str, key: &str, value: &Json);
    fn register_settings(&mut self, plugin: &str, schema: &Json);
    /// The plugin asked for the window to be restyled, in the same terms a theme file uses: a list
    /// of `{ "selector": ".class", "property": "color", "value": "#ff0000" }` objects.
    fn apply_theme(&mut self, plugin: &str, rules: &[Json]);
    /// A plugin line. `stopped` is true for the line that says why the plugin was closed.
    fn log(&mut self, plugin: &str, line: LogLine, stopped: bool);
}

/// How one call into a plugin ended.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Run {
    /// The plugin's code returned.
    #[default]
    Clean,
    /// The code threw. The plugin has stopped.
    Threw(String),
    /// The code ran past its budget and did not stop when the interpreter asked. It has been
    /// closed, and the interpreter's thread with it.
    RanTooLong,
}

impl Run {
    pub fn failed(&self) -> bool {
        !matches!(self, Self::Clean)
    }

    /// What the screen says about this run, empty when nothing went wrong.
    pub fn said(&self) -> String {
        match self {
            Self::Clean => String::new(),
            Self::Threw(error) => error.clone(),
            Self::RanTooLong => {
                "It kept running past the time one plugin step is given, so it was stopped."
                    .to_owned()
            }
        }
    }
}

/// What one call into a plugin did: what it logged, and whether it stopped.
#[derive(Clone, Debug, Default)]
pub struct Outcome {
    pub run: Run,
    pub logs: Vec<LogLine>,
    /// The rules the plugin asked to restyle the window with in this call, when it asked.
    pub theme: Option<Vec<Json>>,
}

impl Outcome {
    fn merge(&mut self, other: Self) {
        self.logs.extend(other.logs);
        if other.theme.is_some() {
            self.theme = other.theme;
        }
        if other.run.failed() {
            self.run = other.run;
        }
    }
}

/// The plugin's own folder. This is the host a plugin runs with in a test: the settings file is
/// real, and everything else lands in the `Outcome`.
#[derive(Debug, Default)]
pub struct Folder {
    /// The app's data folder, which holds `settings.json` and `plugins/`.
    pub data_dir: std::path::PathBuf,
}

impl Folder {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            data_dir: data_dir.to_path_buf(),
        }
    }

    fn plugins_dir(&self) -> std::path::PathBuf {
        mods::mods_dir(&self.data_dir, ModType::Plugin)
    }
}

impl Host for Folder {
    fn get_setting(&mut self, plugin: &str, key: &str) -> Json {
        settings::setting(&self.plugins_dir(), &name_of(plugin), key).unwrap_or(Json::Null)
    }

    fn save_setting(&mut self, plugin: &str, key: &str, value: &Json) {
        let _ = settings::save_setting(&self.plugins_dir(), &name_of(plugin), key, value.clone());
    }

    fn register_settings(&mut self, plugin: &str, schema: &Json) {
        let _ = record_schema(&self.plugins_dir(), &name_of(plugin), schema);
    }

    fn apply_theme(&mut self, _plugin: &str, _rules: &[Json]) {
        // A folder has no window to restyle.
    }

    fn log(&mut self, _plugin: &str, _line: LogLine, _stopped: bool) {
        // Nobody reads a folder's log; the lines are in the `Outcome`.
    }
}

/// Stores a schema under the plugin's settings. A schema this app cannot read at all is stored as
/// no fields, so the page draws nothing rather than the schema that worked last time.
pub fn record_schema(plugins_dir: &Path, name: &str, schema: &Json) -> Result<(), String> {
    let fields = plugin_schema::parse(schema)?;
    settings::save_setting(
        plugins_dir,
        name,
        plugin_schema::SCHEMA_KEY,
        plugin_schema::fields_json(&fields),
    )
}

/// The command the app sends the plugin's thread. Each carries the channel its answer goes back
/// on, so an answer can only reach the call that asked for it.
enum Call {
    /// The first call: the thread loads the plugin and says whether that worked.
    Started(Sender<Outcome>),
    /// The Plugin settings page saved values: the plugin's `onSettingsSaved` callbacks hear it.
    SettingsSaved(Json, Sender<Outcome>),
    Event(String, Json, Sender<Outcome>),
}

impl Call {
    /// Answers a call the thread's interpreter never started to run. The app may have stopped
    /// listening, in which case the answer is dropped with the channel.
    fn answer(self, reason: String) {
        let outcome = Outcome {
            run: Run::Threw(reason),
            ..Outcome::default()
        };
        let reply = match self {
            Self::Started(reply) => reply,
            Self::SettingsSaved(_, reply) => reply,
            Self::Event(_, _, reply) => reply,
        };
        let _ = reply.send(outcome);
    }
}

/// Calls into one plugin, each run on the plugin's thread. The thread holds the interpreter for
/// as long as the plugin runs: a plugin that never returns is left there, and every later call is
/// answered without waiting.
struct Channel {
    calls: Option<Sender<Call>>,
    /// Why the plugin takes no call, once it has stopped taking them.
    closed: Option<String>,
}

impl Channel {
    /// Sends one call and waits for its answer. `call` builds the call around the channel the
    /// answer comes back on, so the answer cannot be read by a later call.
    fn ask(&mut self, call: impl FnOnce(Sender<Outcome>) -> Call) -> Outcome {
        let trouble = |reason: &str| Outcome {
            run: Run::Threw(reason.to_owned()),
            ..Outcome::default()
        };
        if let Some(reason) = &self.closed {
            return trouble(reason);
        }
        let Some(calls) = &self.calls else {
            return Outcome::default();
        };
        let (replies, answer) = std::sync::mpsc::channel();
        if calls.send(call(replies)).is_err() {
            self.close(GONE.to_owned());
            return trouble(GONE);
        }
        match answer.recv_timeout(ANSWER_WAIT) {
            Ok(outcome) => {
                if outcome.run.failed() {
                    self.close(outcome.run.said());
                }
                outcome
            }
            Err(RecvTimeoutError::Timeout) => {
                // The thread is inside the plugin and is not coming back.
                self.close(Run::RanTooLong.said());
                Outcome {
                    run: Run::RanTooLong,
                    ..Outcome::default()
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                self.close(GONE.to_owned());
                trouble(GONE)
            }
        }
    }

    fn close(&mut self, reason: String) {
        self.calls = None;
        self.closed = Some(reason);
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic inside the plugin cannot leave half a value here: every field is replaced whole, so
    // the recoverable lock is the right one.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One timer a plugin set and has not cleared.
struct Timer {
    due: Instant,
    /// `None` for `setTimeout`, `Some(period)` for `setInterval`.
    period: Option<Duration>,
    code: globals::Held,
}

/// What a failed evaluation says on screen. The plugin author reads the same words the engine
/// threw, so they can find the line the error came from. An exception carries its own sentence,
/// which the interpreter's pending exception still holds when the evaluation ends.
fn describe(error: rquickjs::Error, exception: Option<String>) -> String {
    match error {
        rquickjs::Error::Exception => match exception {
            Some(said) if !said.is_empty() => said,
            _ => "The plugin threw an error.".to_owned(),
        },
        rquickjs::Error::UnrelatedRuntime => {
            "The plugin's callback came from another interpreter.".to_owned()
        }
        other => format!("The plugin stopped: {other}"),
    }
}
