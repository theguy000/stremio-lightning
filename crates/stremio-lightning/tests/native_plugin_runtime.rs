//! The plugin interpreter, run against real plugin files in temp folders with no window behind it.
//!
//! Each test writes a `plugins/<name>.plugin.js` file and starts it the way the app does. What a
//! test checks is what the screen would show: the outcome of the load, the lines the plugin logged,
//! the schema in its settings file, and why it stopped.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use serde_json::{json, Value as Json};
use stremio_lightning::plumbing::plugin::{self, Folder, LogLine, Run};
use stremio_lightning::screens::mods::plugin_schema;
use stremio_lightning_core::mods::{self, ModType};
use stremio_lightning_core::settings;

/// A folder of its own for one test, removed when the test ends.
struct Temp {
    path: PathBuf,
}

impl Temp {
    fn new(name: &str) -> Self {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let unique = COUNT.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "stremio-lightning-plugin-{}-{name}-{unique}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(mods::mods_dir(&path, ModType::Plugin))
            .expect("the folder could be made");
        Self { path }
    }

    fn plugins(&self) -> PathBuf {
        mods::mods_dir(&self.path, ModType::Plugin)
    }

    /// Writes a plugin file and returns the file name the app knows it by.
    fn write(&self, name: &str, source: &str) -> String {
        let file = format!("{name}.plugin.js");
        fs::write(self.plugins().join(&file), source).expect("the plugin could be written");
        file
    }

    fn folder(&self) -> Folder {
        Folder::new(&self.path)
    }

    /// The plugin's settings file, as the Plugin settings page reads it.
    fn stored(&self, name: &str) -> Json {
        let path = settings::plugin_settings_path(&self.plugins(), name).expect("a valid name");
        settings::load_settings_file(&path).unwrap_or(Json::Null)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn lines(logs: &[LogLine]) -> Vec<String> {
    logs.iter().map(LogLine::shown).collect()
}

#[test]
fn a_plugin_loads_logs_and_saves_a_setting() {
    let temp = Temp::new("loads");
    let file = temp.write(
        "greet",
        r#"
        logger.info("hello from", 1 + 1);
        console.warn("and a warning");
        saveSetting("greeting", "hi there");
        "#,
    );

    let (running, outcome) = started(&temp, &file);

    assert!(running.running(), "stopped: {}", running.stopped());
    assert_eq!(
        lines(&outcome.logs),
        vec![
            "info: hello from 2".to_owned(),
            "warn: and a warning".to_owned(),
        ]
    );
    assert_eq!(temp.stored("greet")["greeting"], json!("hi there"));
}

/// Starts a plugin file the way the app does, and says what the load did.
fn started(temp: &Temp, file: &str) -> (plugin::Running, plugin::Outcome) {
    let source = fs::read_to_string(temp.plugins().join(file)).expect("the plugin was written");
    plugin::Running::start(source, file, temp.folder())
}

#[test]
fn a_plugin_registers_the_schema_the_settings_page_draws() {
    let temp = Temp::new("schema");
    let file = temp.write(
        "drawn",
        r#"
        registerSettings([
          { key: "enabled", label: "Turn it on", type: "toggle", default: true },
          { key: "quality", label: "Quality", type: "select", options: ["720p", "1080p"], default: "720p" }
        ]);
        "#,
    );

    let (running, outcome) = started(&temp, &file);

    assert!(running.running(), "stopped: {}", running.stopped());
    assert!(!outcome.run.failed(), "stopped: {}", outcome.run.said());
    let stored = temp.stored("drawn");
    let fields = plugin_schema::parse(&stored[plugin_schema::SCHEMA_KEY]).expect("readable");
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].key(), "enabled");
    assert_eq!(fields[1].default(), json!("720p"));
}

#[test]
fn a_plugin_that_throws_says_why_and_stops() {
    let temp = Temp::new("throws");
    let file = temp.write("bad", "throw new Error('no good');");
    let (mut running, _) = started(&temp, &file);

    assert!(!running.running());
    assert!(
        running.stopped().contains("no good"),
        "the screen says: {}",
        running.stopped()
    );
    // A plugin that stopped stays stopped and says why on every later call.
    let outcome = running.settings_saved(json!({}));
    assert!(outcome.run.failed());
    assert_eq!(running.stopped(), outcome.run.said());
}

#[test]
fn a_plugin_that_never_returns_is_stopped_at_its_budget() {
    let temp = Temp::new("loop");
    let file = temp.write("spin", "while (true) {}");
    let at = Instant::now();
    let (running, _) = started(&temp, &file);

    assert!(!running.running());
    assert_eq!(running.stopped(), Run::RanTooLong.said());
    assert!(
        at.elapsed() < Duration::from_secs(5),
        "the load took {:?}",
        at.elapsed()
    );
}

#[test]
fn settings_round_trip_through_the_page_and_back_to_the_plugin() {
    let temp = Temp::new("round-trip");
    let file = temp.write(
        "rounded",
        r#"
        registerSettings([{ key: "name", label: "Name", type: "text", default: "nobody" }]);
        onSettingsSaved(function (values) {
          logger.info("saved for", values.name, "was", getSetting("name"));
        });
        "#,
    );
    let (mut running, _) = started(&temp, &file);
    assert!(running.running(), "stopped: {}", running.stopped());

    // What the Plugin settings page does on its save press: write the key, then hand the file to
    // the plugin.
    settings::save_setting(&temp.plugins(), "rounded", "name", json!("ada")).unwrap();
    let outcome = running.settings_saved(temp.stored("rounded"));

    assert!(!outcome.run.failed(), "stopped: {}", outcome.run.said());
    let said = lines(&outcome.logs);
    assert!(
        said.iter()
            .any(|line| line == "info: saved for ada was ada"),
        "the plugin saw: {said:?}"
    );
    assert_eq!(temp.stored("rounded")["name"], json!("ada"));
}

#[test]
fn a_plugin_that_recurses_without_end_is_closed_and_the_app_survives() {
    let temp = Temp::new("deep");
    // The thread the interpreter runs on is the app's own code: if the recursion were to run off
    // the stack, this test would take the whole process with it, not just fail.
    let file = temp.write("deep", "function f() { f(); }\nf();");

    let (running, outcome) = started(&temp, &file);

    assert!(!running.running(), "the plugin is closed");
    assert!(
        running.stopped().to_lowercase().contains("stack"),
        "the screen says: {}",
        running.stopped()
    );
    assert!(outcome.run.failed());
    // The process is still here and can start another plugin.
    let next = temp.write("after", "logger.info('still here');");
    let (after, outcome) = started(&temp, &next);
    assert!(after.running(), "stopped: {}", after.stopped());
    assert_eq!(lines(&outcome.logs), vec!["info: still here".to_owned()]);
}

#[test]
fn a_plugin_hearing_that_playback_started_logs_its_own_line() {
    let temp = Temp::new("events");
    let file = temp.write(
        "listener",
        r#"
        on("playback-started", function (event) {
          logger.info("now playing " + event.videoId);
        });
        on("fullscreen", function (event) {
          logger.info("fullscreen is " + event.on);
        });
        "#,
    );
    // `started` waits out the load, and the listener it armed is in this plugin.
    let (mut running, outcome) = started(&temp, &file);
    assert!(running.running(), "stopped: {}", running.stopped());
    assert!(outcome.logs.is_empty(), "loading logs nothing");

    // What the wiring says when mpv reports the file it was given has opened.
    let heard = running.deliver(plugin::Event::PlaybackStarted {
        video_id: "tt1:1:1".to_owned(),
    });
    assert_eq!(
        lines(&heard.logs),
        vec!["info: now playing tt1:1:1".to_owned()],
        "the plugin heard the event"
    );
    assert_eq!(heard.run, Run::Clean);

    // One the plugin does not listen for is delivered to it as nothing at all.
    let unlistened = running.deliver(plugin::Event::CatalogLoaded {
        id: "top".to_owned(),
        count: 3,
    });
    assert!(unlistened.logs.is_empty());
    assert_eq!(unlistened.run, Run::Clean);

    let stopped = running.deliver(plugin::Event::PlaybackStopped);
    assert!(
        stopped.logs.is_empty(),
        "it listens for the start, not the stop"
    );
}

#[test]
fn a_step_that_arms_too_many_timers_says_why_it_stopped() {
    let temp = Temp::new("timers");
    // More than one step may arm, in one step. The timers are due a minute from now, so none of
    // them has fired when the step ends.
    let file = temp.write(
        "army",
        "for (let i = 0; i < 5000; i++) { setTimeout(function () {}, 60000); }",
    );

    let (running, outcome) = started(&temp, &file);

    assert!(!running.running(), "the plugin is closed");
    assert!(
        running.stopped().contains("timers"),
        "the card says: {}",
        running.stopped()
    );
    assert!(
        lines(&outcome.logs)
            .iter()
            .any(|line| line.contains("arm at most")),
        "the log says: {:?}",
        lines(&outcome.logs)
    );
}

#[test]
fn the_documented_way_a_value_is_logged_is_the_way_it_is_logged() {
    let temp = Temp::new("logged");
    let file = temp.write(
        "printer",
        r#"
        console.log("rows", [1, 2], { a: true }, null);
        logger.warn(42, "and a half", 1.5);
        console.log(undefined, function () {});
        "#,
    );

    let (_, outcome) = started(&temp, &file);

    assert_eq!(
        lines(&outcome.logs),
        vec![
            "info: rows [1,2] {\"a\":true} null".to_owned(),
            "warn: 42 and a half 1.5".to_owned(),
            "info: null null".to_owned(),
        ]
    );
}

#[test]
fn a_timer_takes_a_function_and_not_a_string() {
    let temp = Temp::new("strings");
    let file = temp.write(
        "strings",
        r#"
        const id = setTimeout("logger.info('never')", 1);
        logger.info("id is " + id);
        "#,
    );

    let (running, outcome) = started(&temp, &file);

    assert!(running.running(), "stopped: {}", running.stopped());
    assert_eq!(lines(&outcome.logs), vec!["info: id is 0".to_owned()]);
}

#[test]
fn saving_settings_calls_only_a_callback_that_is_a_function() {
    let temp = Temp::new("saved");
    let file = temp.write(
        "saved",
        r#"
        onSettingsSaved("not a function");
        logger.info("loaded");
        "#,
    );
    let (mut running, outcome) = started(&temp, &file);
    assert!(running.running(), "stopped: {}", running.stopped());
    assert_eq!(lines(&outcome.logs), vec!["info: loaded".to_owned()]);

    // The save runs with nothing to call, and says so by doing nothing: no error, no line.
    let saved = running.settings_saved(json!({ "announce": false }));
    assert_eq!(saved.run, Run::Clean);
    assert!(saved.logs.is_empty());
}

#[test]
fn a_schema_that_is_not_a_list_throws_inside_the_plugin() {
    let temp = Temp::new("schema-shape");
    let file = temp.write(
        "wrong",
        r#"
        try {
          registerSettings({ key: "one" });
          logger.error("no error");
        } catch (error) {
          logger.warn("threw: " + error.message);
        }
        "#,
    );

    let (running, outcome) = started(&temp, &file);

    // Caught inside the plugin, so the plugin goes on, and the message is the app's own.
    assert!(running.running(), "stopped: {}", running.stopped());
    let logged = lines(&outcome.logs);
    assert_eq!(logged.len(), 1, "{logged:?}");
    assert!(logged[0].contains("wants a list of fields"), "{logged:?}");
}
