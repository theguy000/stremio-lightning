//! The Logs and About pages, and the one recorder under them.
//!
//! Three tests. The first reads the recorder on its own: a line a plugin logs, a theme's warnings,
//! the cap, the filter and Clear, all without a window. The second boots the app with a plugin
//! that logs, opens the Logs page, and reads the rows back, so the list a person sees is the list
//! the recorder holds. The third reads the About page's counts off the same booted app.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use slint::Model;
use stremio_lightning::plumbing::logs::{Filter, Level, Logs, Source};
use stremio_lightning::screens::mods;
use stremio_lightning::{MainWindow, Section};

mod support;

use support::ui_driver::{self, Outcome};

/// The plugin this run writes: it logs one line at each level, including one that says what it is,
/// so a filtered list differs from the whole list.
const TALKATIVE: &str = r#"/**
 * @name Talkative
 * @description Says what it is doing.
 * @author Ada
 * @version 1.0.0
 */
(function () {
  console.debug("native-mods-logs quiet");
  console.info("native-mods-logs reading the shelf");
  console.warn("native-mods-logs the shelf is empty");
  console.error("native-mods-logs the shelf could not be read");
})();
"#;

/// How many lines that plugin writes.
const TALKATIVE_LINES: usize = 4;

/// One line as the plugin runtime hands it to the recorder.
fn line(level: &str, text: &str) -> stremio_lightning::plumbing::plugin::LogLine {
    stremio_lightning::plumbing::plugin::LogLine {
        level: level.to_owned(),
        text: text.to_owned(),
    }
}

/// Presses whatever says `label`, through the element rather than at the coordinates it reports.
fn click(window: &MainWindow, label: &str) -> Result<String, String> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut found = None;
    while found.is_none() && Instant::now() < deadline {
        ui_driver::pump(Duration::from_millis(50));
        found = ui_driver::find(window, label);
    }
    let Some(handle) = found else {
        return Err(format!(
            "nothing says this; on screen: {:?}",
            ui_driver::labels(window)
        ));
    };
    let at = handle.absolute_position();
    let size = handle.size();
    handle.mock_single_click(slint::platform::PointerEventButton::Left);
    ui_driver::pump(Duration::from_millis(40));
    Ok(format!(
        "pressed at ({:.0}, {:.0}) size {:.0}x{:.0}",
        at.x, at.y, size.width, size.height
    ))
}

/// Waits `budget` for `check` to hold, letting the event loop turn.
fn wait(
    window: &MainWindow,
    budget: Duration,
    mut check: impl FnMut(&MainWindow) -> Result<String, String>,
) -> Result<String, String> {
    let deadline = Instant::now() + budget;
    loop {
        ui_driver::pump(Duration::from_millis(50));
        match check(window) {
            Ok(detail) => return Ok(detail),
            Err(error) if Instant::now() >= deadline => return Err(format!("gave up: {error}")),
            Err(_) => {}
        }
    }
}

fn step(mut outcomes: Vec<Outcome>, what: &str, result: Result<String, String>) -> Vec<Outcome> {
    outcomes.push(Outcome {
        what: what.to_owned(),
        ok: Some(result.is_ok()),
        detail: result.unwrap_or_else(|error| error),
    });
    outcomes
}

fn verdict(what: &str, outcomes: &[Outcome]) {
    print!("{}", ui_driver::report(outcomes));
    assert!(
        outcomes.iter().all(|outcome| outcome.ok != Some(false)),
        "{what}: {:?}",
        outcomes
            .iter()
            .filter(|outcome| outcome.ok == Some(false))
            .map(|outcome| outcome.line())
            .collect::<Vec<_>>()
    );
}

fn temp_dir(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "stremio-lightning-mods-logs-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("a folder of its own for this run");
    folder
}

fn write_mod(data_dir: &Path, folder: &str, filename: &str, body: &str) -> PathBuf {
    let path = data_dir
        .join("stremio-lightning")
        .join(folder)
        .join(filename);
    std::fs::create_dir_all(path.parent().unwrap()).expect("the mods folder");
    std::fs::write(&path, body).expect("the mod file");
    path
}

/// The rows the window is drawing now, newest first: (source, level, text).
fn rows_on(window: &MainWindow) -> Vec<(String, String, String)> {
    let rows = window.get_mods().log_rows;
    (0..rows.row_count())
        .filter_map(|row| rows.row_data(row))
        .map(|row| {
            (
                row.source.to_string(),
                row.level.to_string(),
                row.text.to_string(),
            )
        })
        .collect()
}

#[test]
fn the_recorder_keeps_plugin_lines_theme_warnings_and_the_cap() {
    let logs = Logs::new();
    assert!(logs.is_empty());

    // ---- a plugin's line, at the level it named --------------------------------
    let entry = logs.plugin("talkative.plugin.js", &line("warn", "the shelf is empty"));
    assert_eq!(entry.source, Source::Plugin);
    assert_eq!(entry.origin, "talkative.plugin.js");
    assert_eq!(entry.level, Level::Warn);
    assert_eq!(entry.source_label(), "Plugin talkative.plugin.js");
    // A level this app does not know is information, not a dropped line.
    logs.plugin("talkative.plugin.js", &line("chatty", "hello"));
    assert_eq!(logs.entries()[1].level, Level::Info);
    // `console.log` arrives as `log`, which is also information.
    logs.plugin("talkative.plugin.js", &line("log", "quiet"));
    assert_eq!(logs.entries()[2].level, Level::Info);

    // ---- a theme's warnings, one record each ----------------------------------
    logs.theme(
        "odd.theme.css",
        &[
            "unknown property 'colour'".to_owned(),
            "unmatched".to_owned(),
        ],
    );
    let themes: Vec<_> = logs
        .entries()
        .into_iter()
        .filter(|entry| entry.source == Source::Theme)
        .collect();
    assert_eq!(themes.len(), 2, "one record per warning");
    assert_eq!(themes[0].level, Level::Warn);
    assert_eq!(themes[0].source_label(), "Theme odd.theme.css");
    // A theme that cannot be read at all is an error, not a warning.
    logs.theme_unreadable("odd.theme.css", "the file could not be read");
    assert_eq!(logs.entries().last().unwrap().level, Level::Error);

    // ---- the list is newest first, and the cap drops the oldest ----------------
    let logs = Logs::new();
    for index in 0..stremio_lightning::plumbing::logs::CAPACITY + 5 {
        logs.record(Source::App, "", Level::Info, format!("line {index}"));
    }
    assert_eq!(logs.len(), stremio_lightning::plumbing::logs::CAPACITY);
    let matching = logs.matching(&Filter::default());
    assert_eq!(matching.len(), stremio_lightning::plumbing::logs::CAPACITY);
    assert_eq!(matching[0].text, "line 2004", "the newest is first");
    assert_eq!(matching.last().unwrap().text, "line 5", "the oldest went");
    assert!(
        matching.iter().all(|entry| entry.text != "line 0"),
        "a record past the cap is dropped as it is added"
    );

    // ---- the filter: one source, a least level, and a search -------------------
    let logs = Logs::new();
    logs.record(
        Source::Plugin,
        "a.plugin.js",
        Level::Info,
        "reading the shelf",
    );
    logs.record(
        Source::Plugin,
        "a.plugin.js",
        Level::Warn,
        "the shelf is empty",
    );
    logs.record(Source::Server, "127.0.0.1:11470", Level::Info, "answering");
    logs.record(Source::App, "Core", Level::Error, "the shelf is gone");
    // The one record that is a plugin's, an error, and about the shelf: what the three parts of
    // the filter are checked against below.
    logs.record(
        Source::Plugin,
        "a.plugin.js",
        Level::Error,
        "the shelf could not be read",
    );

    let plugins = Filter {
        source: "plugin".to_owned(),
        ..Filter::default()
    };
    let kept = logs.matching(&plugins);
    assert_eq!(kept.len(), 3);
    assert!(kept.iter().all(|entry| entry.source == Source::Plugin));

    let warnings = Filter {
        level: "warn".to_owned(),
        ..Filter::default()
    };
    let kept = logs.matching(&warnings);
    assert_eq!(kept.len(), 3, "warnings and errors: {kept:?}");
    assert!(kept.iter().all(|entry| entry.level >= Level::Warn));

    let errors = Filter {
        level: "error".to_owned(),
        ..Filter::default()
    };
    assert_eq!(logs.matching(&errors).len(), 2);

    let searched = Filter {
        search: "SHELF".to_owned(),
        ..Filter::default()
    };
    assert_eq!(
        logs.matching(&searched).len(),
        4,
        "the search ignores case and reads the text"
    );
    let by_origin = Filter {
        search: "a.plugin.js".to_owned(),
        ..Filter::default()
    };
    assert_eq!(logs.matching(&by_origin).len(), 3);

    // The filter the page is showing lives in the recorder, so a page redrawn by an
    // unrelated event draws the same list. A part this app does not know is the "any" answer.
    assert_eq!(logs.filter(), Filter::default());
    logs.set_source("plugin");
    logs.set_level("error");
    logs.set_search("shelf");
    assert_eq!(
        logs.filter(),
        Filter {
            source: "plugin".to_owned(),
            level: "error".to_owned(),
            search: "shelf".to_owned(),
        }
    );
    // Three parts at once, each true of one record: the plugin's own error is the only record
    // that is a plugin's, an error, and about the shelf. A filter that read only part of itself
    // would keep the warning or the app's error.
    let kept = logs.matching(&logs.filter());
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(kept[0].origin, "a.plugin.js");
    assert_eq!(kept[0].level, Level::Error);
    assert!(kept[0].text.contains("shelf"));
    // The search reads the origin as well as the text: every one of the plugin's lines names it.
    logs.set_level("");
    assert_eq!(logs.matching(&logs.filter()).len(), 3);
    logs.set_search("a.plugin.js");
    assert_eq!(logs.matching(&logs.filter()).len(), 3);
    logs.set_source("nothing-like-this");
    assert_eq!(
        logs.filter().source,
        "",
        "an unknown source is every source"
    );

    // ---- one record's full text ------------------------------------------------
    let logs = Logs::new();
    let entry = logs.record(Source::Plugin, "a.plugin.js", Level::Error, "first\nsecond");
    assert!(logs.set_detail(Some(entry.id)));
    assert_eq!(logs.detail().unwrap().text, "first\nsecond");
    assert!(
        !logs.set_detail(Some(entry.id + 100)),
        "no such record opens"
    );
    assert!(logs.detail().is_none());
    assert!(logs.set_detail(Some(entry.id)));

    // ---- Clear empties the list and deletes nothing -----------------------------
    let before = logs.count();
    assert_eq!(logs.clear(), 1);
    assert!(logs.is_empty());
    assert!(
        logs.detail().is_none(),
        "the open record goes with the list"
    );
    assert!(logs.count() > before, "a clear is a change to redraw for");
    assert_eq!(logs.clear(), 0, "clearing an empty list removes nothing");
    let after = logs.record(Source::App, "", Level::Info, "still recording");
    assert_eq!(
        after.id,
        entry.id + 1,
        "ids keep counting after a clear, so an old row opens nothing"
    );
}

#[test]
fn the_window_lists_what_a_plugin_logged_and_filters_it() {
    // Mock time, not system time: the clock this runner reports is not monotonic, and Slint
    // asserts on a tick that goes backwards as soon as any animation is live.
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let data_dir = temp_dir("window");
    write_mod(&data_dir, "plugins", "talkative.plugin.js", TALKATIVE);
    // A second plugin that fails at load and is switched on too, so the About page has a failure
    // to count and a second enabled name.
    write_mod(
        &data_dir,
        "plugins",
        "throws.plugin.js",
        "/**\n * @name Throws\n * @description Fails.\n * @author Bo\n * @version 1.0.0\n */\nthrow new Error(\"no\");\n",
    );
    write_mod(
        &data_dir,
        "themes",
        "red.theme.css",
        "/**\n * @name Red\n * @description A colour change.\n * @author Someone\n * @version 1.0.0\n */\n.nav-tab-button {\n  color: #ff0000;\n}\n",
    );
    std::fs::write(
        data_dir.join("settings.json"),
        r#"{"enabled_plugins":["talkative.plugin.js","throws.plugin.js"],"enabled_themes":["red.theme.css"]}"#,
    )
    .expect("the settings file");

    let booted = stremio_lightning::boot(data_dir.clone()).expect("the app boots");
    let window = &booted.window;
    ui_driver::pump_for(window);

    let mut outcomes: Vec<Outcome> = Vec::new();
    outcomes = step(
        outcomes,
        "the Mods tab opens the Mods screen",
        click(window, "Mods").and_then(|_| match window.get_section() {
            Section::Mods => Ok("Mods".to_owned()),
            other => Err(format!("the section is {other:?}")),
        }),
    );
    outcomes = step(
        outcomes,
        "Logs opens",
        click(window, "Logs").and_then(|_| {
            wait(window, Duration::from_secs(10), |window| {
                let shown = window.get_mods_page().to_string();
                if shown == "logs" {
                    Ok(shown)
                } else {
                    Err(format!("the window holds {shown}"))
                }
            })
        }),
    );
    // The plugin runs on its own thread as the app starts, so the line arrives after the page.
    outcomes = step(
        outcomes,
        "a line the plugin logged is on the page, named by its plugin",
        wait(window, Duration::from_secs(20), |window| {
            let rows = rows_on(window);
            let found: Vec<&(String, String, String)> = rows
                .iter()
                .filter(|(_, _, text)| text == "native-mods-logs the shelf is empty")
                .collect();
            match found.first() {
                Some((source, level, _)) => Ok(format!("{source} / {level}")),
                None => Err(format!("the page lists {rows:?}")),
            }
        }),
    );
    outcomes = step(
        outcomes,
        "the source column names the plugin file",
        wait(window, Duration::from_secs(10), |window| {
            let rows = rows_on(window);
            match rows
                .iter()
                .find(|(_, _, text)| text == "native-mods-logs the shelf is empty")
            {
                Some((source, level, _))
                    if source == "Plugin talkative.plugin.js" && level == "Warning" =>
                {
                    Ok(format!("{source} / {level}"))
                }
                other => Err(format!("the row says {other:?}")),
            }
        }),
    );
    outcomes = step(
        outcomes,
        "every line the plugin logged is there, newest first",
        wait(window, Duration::from_secs(10), |window| {
            let said: Vec<String> = rows_on(window)
                .into_iter()
                .filter(|(_, _, text)| text.starts_with("native-mods-logs "))
                .map(|(_, _, text)| text)
                .collect();
            if said.len() == TALKATIVE_LINES
                && said.first().map(String::as_str)
                    == Some("native-mods-logs the shelf could not be read")
            {
                Ok(format!("{} lines", said.len()))
            } else {
                Err(format!("the plugin's lines are {said:?}"))
            }
        }),
    );
    let all = rows_on(window);
    // What the header counts is the recorder's list, not the rows on screen: an error the app
    // recorded from another part is a row too.
    outcomes = step(outcomes, "the page draws a header for its list", {
        let texts = ui_driver::texts(window);
        let header: Vec<&String> = texts
            .iter()
            .filter(|text| text.contains("records.") && text.contains("The filter keeps"))
            .collect();
        match header.first() {
            Some(text) => Ok((*text).clone()),
            None => Err(format!("the page says {texts:?}")),
        }
    });
    outcomes = step(
        outcomes,
        "the header holds every record and keeps every one",
        {
            let rows = rows_on(window).len();
            let texts = ui_driver::texts(window);
            let header = texts
                .iter()
                .find(|text| text.contains("records.") && text.contains("The filter keeps"))
                .cloned()
                .unwrap_or_default();
            let kept = format!("The filter keeps {rows} of them.");
            if rows > 0 && header.contains("of up to 2000 records") && header.contains(&kept) {
                Ok(format!("{rows} rows, and {header}"))
            } else {
                Err(format!("{rows} rows, and the header says {header:?}"))
            }
        },
    );
    verdict(
        "the Logs page did not list what the plugin logged",
        &outcomes,
    );

    // ---- the level filter keeps fewer records -----------------------------------
    let mut filtering: Vec<Outcome> = Vec::new();
    filtering = step(
        filtering,
        "Errors only is pressed, and the list gets shorter",
        click(window, "Errors only").and_then(|_| {
            wait(window, Duration::from_secs(10), |window| {
                let rows = rows_on(window);
                if rows.iter().any(|(_, level, _)| level != "Error") {
                    return Err(format!("{} rows, and not all errors: {rows:?}", rows.len()));
                }
                if rows.len() >= all.len() {
                    return Err(format!("{} rows before, {} after", all.len(), rows.len()));
                }
                if !rows
                    .iter()
                    .any(|(_, _, text)| text == "native-mods-logs the shelf could not be read")
                {
                    return Err(format!("the error line is gone: {rows:?}"));
                }
                Ok(format!("{} rows, all errors", rows.len()))
            })
        }),
    );
    filtering = step(
        filtering,
        "the header says how many the filter keeps",
        wait(window, Duration::from_secs(5), |window| {
            let held = window.get_mods().log_held.to_string();
            if held.contains("The filter keeps") {
                Ok(held)
            } else {
                Err(held)
            }
        }),
    );
    filtering = step(
        filtering,
        "the plugin's warning is not among them",
        wait(window, Duration::from_secs(5), |window| {
            let rows = rows_on(window);
            if rows
                .iter()
                .any(|(_, _, text)| text == "native-mods-logs the shelf is empty")
            {
                Err(format!("the warning is still listed: {rows:?}"))
            } else {
                Ok(format!("{} rows", rows.len()))
            }
        }),
    );
    verdict("the level filter did not narrow the list", &filtering);

    // ---- Clear takes two presses and empties the list ---------------------------
    let mut clearing: Vec<Outcome> = Vec::new();
    clearing = step(
        clearing,
        "Clear is pressed once, and the list stays",
        click(window, "Clear the log records").and_then(|_| {
            wait(window, Duration::from_secs(5), |window| {
                let armed = window.get_mods().log_clear_armed;
                if armed {
                    Ok("armed".to_owned())
                } else {
                    Err("the button did not arm".to_owned())
                }
            })
        }),
    );
    clearing = step(
        clearing,
        "the button now asks for a confirmation",
        wait(window, Duration::from_secs(5), |window| {
            let names = ui_driver::labels(window);
            if names
                .iter()
                .any(|name| name == "Confirm clearing the log records")
            {
                Ok("Confirm clearing the log records".to_owned())
            } else {
                Err(format!("the buttons say {names:?}"))
            }
        }),
    );
    clearing = step(
        clearing,
        "the armed button is pressed, and the list empties",
        click(window, "Confirm clearing the log records").and_then(|_| {
            wait(window, Duration::from_secs(5), |window| {
                let rows = slint::Model::row_count(&window.get_mods().log_rows);
                let held = window.get_mods().log_held.to_string();
                if rows == 0 && held.contains("holds no records") {
                    Ok(held)
                } else {
                    Err(format!("{rows} rows, and the header says {held:?}"))
                }
            })
        }),
    );
    clearing = step(
        clearing,
        "and the page says why it is empty",
        wait(window, Duration::from_secs(5), |window| {
            let empty = window.get_mods().log_empty.to_string();
            if empty.contains("Nothing has been recorded") || empty.contains("No record matches") {
                Ok(empty)
            } else {
                Err(format!("the page says {empty:?}"))
            }
        }),
    );
    verdict("Clear did not empty the list in two presses", &clearing);

    // ---- the About page reads the same app --------------------------------------
    let mut about: Vec<Outcome> = Vec::new();
    about = step(
        about,
        "About opens",
        click(window, "About").and_then(|_| {
            wait(window, Duration::from_secs(10), |window| {
                let shown = window.get_mods_page().to_string();
                if shown == "about" {
                    Ok(shown)
                } else {
                    Err(format!("the window holds {shown}"))
                }
            })
        }),
    );
    about = step(about, "it names the version and the build", {
        let build = &window.get_mods().about_build;
        let labels: Vec<String> = (0..build.row_count())
            .filter_map(|row| build.row_data(row))
            .map(|line| line.label.to_string())
            .collect();
        let version = (0..build.row_count())
            .filter_map(|row| build.row_data(row))
            .find(|line| line.label == "Version")
            .map(|line| line.value.to_string());
        if labels == ["Version", "Build", "Platform"]
            && version.as_deref() == Some(mods::about::VERSION)
        {
            Ok(labels.join(", "))
        } else {
            Err(format!("the build lines are {labels:?}"))
        }
    });
    // The counts are drawn from the page's own reading of the folder, and the failed plugin is
    // only known once the runtime has tried it. The check is the window's own data, once it says
    // what the folder and the runtime say.
    about = step(
        about,
        "it counts three plugins installed (including built-in AniSkip) and three on, and one of each theme",
        wait(window, Duration::from_secs(20), |window| {
            let counts = window.get_mods().about_counts.clone();
            let value = |label: &str| {
                counts
                    .iter()
                    .find(|line| line.label == label)
                    .map(|line| line.value.clone())
            };
            let said = (
                value("Plugins installed"),
                value("Plugins enabled"),
                value("Plugins running"),
                value("Plugins failed"),
                value("Themes installed"),
                value("Themes enabled"),
            );
            match said {
                (Some(a), Some(b), Some(c), Some(d), Some(e), Some(f))
                    if a == "3" && b == "3" && c == "1" && d == "1" && e == "1" && f == "1" =>
                {
                    Ok("3 plugins installed, 3 on, 1 running, 1 failed, 1 theme".to_owned())
                }
                _ => Err(format!("the counts are {said:?}")),
            }
        }),
    );
    about = step(about, "the enabled lists name files", {
        let enabled = &window.get_mods().about_enabled;
        let plugins = (0..enabled.row_count())
            .filter_map(|row| enabled.row_data(row))
            .find(|line| line.label == "Enabled plugins")
            .map(|line| line.value.to_string());
        match plugins.as_deref() {
            Some("builtin:aniskip, talkative.plugin.js, throws.plugin.js") => {
                Ok(plugins.unwrap_or_default())
            }
            other => Err(format!("the enabled plugins are {other:?}")),
        }
    });
    about = step(about, "the places name the two folders and the registry", {
        let places = &window.get_mods().about_places;
        let listed: Vec<(String, String)> = (0..places.row_count())
            .filter_map(|row| places.row_data(row))
            .map(|line| (line.label.to_string(), line.value.to_string()))
            .collect();
        let plugins_dir = stremio_lightning_core::mods::mods_dir(
            &data_dir,
            stremio_lightning_core::mods::ModType::Plugin,
        );
        let has = |label: &str, value: &Path| {
            listed
                .iter()
                .any(|(name, said)| name == label && Path::new(said) == value)
        };
        if has("Plugins folder", &plugins_dir) && listed.iter().any(|(name, _)| name == "Registry")
        {
            Ok(format!("{listed:?}"))
        } else {
            Err(format!("the places are {listed:?}"))
        }
    });
    about = step(about, "it ends with the licence line", {
        let license = window.get_mods().about_license.to_string();
        if license == mods::about::LICENSE && !license.is_empty() {
            Ok(license)
        } else {
            Err(format!("the page says {license:?}"))
        }
    });
    // Export writes a file into the app's data folder and shows where it went. It uploads
    // nothing: the only thing to check is that the file is there and says what the page says.
    let pressed = click(window, "Export diagnostics");
    about = step(
        about,
        "Export diagnostics writes a file and shows where",
        pressed.and_then(|_| {
            wait(window, Duration::from_secs(10), |window| {
                let written = window.get_mods().about_export.to_string();
                if written.is_empty() {
                    return Err("the page shows no path yet".to_owned());
                }
                match std::fs::read_to_string(&written) {
                    Ok(text) if text.contains("Plugins installed") => Ok(written),
                    Ok(_) => Err(format!("{written} says nothing about the counts")),
                    Err(error) => Err(format!("{written}: {error}")),
                }
            })
        }),
    );
    about = step(about, "and the file is under the app's data folder", {
        let written = PathBuf::from(window.get_mods().about_export.to_string());
        let folder = written.parent().map(Path::to_path_buf);
        let wanted = data_dir.join(mods::about::FOLDER);
        if folder.as_deref() == Some(wanted.as_path()) && written.exists() {
            Ok(written.display().to_string())
        } else {
            Err(format!(
                "{} is not in {}",
                written.display(),
                wanted.display()
            ))
        }
    });
    verdict("the About page did not read the app's counts", &about);

    let _ = std::fs::remove_dir_all(&data_dir);
}
