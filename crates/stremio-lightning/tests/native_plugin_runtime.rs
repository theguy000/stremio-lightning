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
