//! The Logs and About pages, and the one recorder under them.
//!
//! Two tests. The first reads the recorder on its own: a plugin record, a theme's warnings, the
//! cap, the filter and Clear, all without a window. The second boots the app with themes whose
//! warnings and switches record into the log, opens the Logs page and filters it, then reads the
//! About page's counts off the same booted app.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use slint::{ComponentHandle, Model};
use stremio_lightning::plumbing::logs::{Filter, Level, Logs, Source};
use stremio_lightning::screens::mods;
use stremio_lightning::{MainWindow, Section};

mod support;

use support::ui_driver::{self, Outcome};

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

/// Types into whatever has the focus, one key at a time, letting the window draw between them.
fn type_text(window: &MainWindow, text: &str) {
    for character in text.chars() {
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed {
                text: character.to_string().into(),
            });
        ui_driver::pump(Duration::from_millis(20));
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

    // ---- a built-in plugin's line, at the level it named --------------------
    let entry = logs.record(
        Source::Plugin,
        "builtin:aniskip",
        Level::Warn,
        "the shelf is empty",
    );
    assert_eq!(entry.source, Source::Plugin);
    assert_eq!(entry.origin, "builtin:aniskip");
    assert_eq!(entry.level, Level::Warn);
    assert_eq!(entry.source_label(), "Plugin builtin:aniskip");
    assert_eq!(Level::from_id("chatty"), None);
    assert_eq!(Level::from_id("log"), Some(Level::Info));

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
        "builtin:aniskip",
        Level::Info,
        "reading the shelf",
    );
    logs.record(
        Source::Plugin,
        "builtin:aniskip",
        Level::Warn,
        "the shelf is empty",
    );
    logs.record(Source::Server, "127.0.0.1:11470", Level::Info, "answering");
    logs.record(Source::App, "Core", Level::Error, "the shelf is gone");
    logs.record(
        Source::Plugin,
        "builtin:aniskip",
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
        search: "builtin:aniskip".to_owned(),
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
    let kept = logs.matching(&logs.filter());
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert_eq!(kept[0].origin, "builtin:aniskip");
    assert_eq!(kept[0].level, Level::Error);
    assert!(kept[0].text.contains("shelf"));
    logs.set_level("");
    assert_eq!(logs.matching(&logs.filter()).len(), 3);
    logs.set_search("builtin:aniskip");
    assert_eq!(logs.matching(&logs.filter()).len(), 3);
    logs.set_source("nothing-like-this");
    assert_eq!(
        logs.filter().source,
        "",
        "an unknown source is every source"
    );

    // ---- one record's full text ------------------------------------------------
    let logs = Logs::new();
    let entry = logs.record(
        Source::Plugin,
        "builtin:aniskip",
        Level::Error,
        "first\nsecond",
    );
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
fn the_window_lists_what_was_logged_and_filters_it() {
    // Mock time, not system time: the clock this runner reports is not monotonic, and Slint
    // asserts on a tick that goes backwards as soon as any animation is live.
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let data_dir = temp_dir("window");
    write_mod(
        &data_dir,
        "themes",
        "odd.theme.css",
        "/**\n * @name Odd\n * @description Has an unmatched selector.\n * @author Someone\n * @version 1.0.0\n */\n.nothing-matches-this {\n  color: #00ff00;\n}\n",
    );
    write_mod(
        &data_dir,
        "themes",
        "red.theme.css",
        "/**\n * @name Red\n * @description A colour change.\n * @author Someone\n * @version 1.0.0\n */\n.nav-tab-button {\n  color: #ff0000;\n}\n",
    );
    std::fs::write(
        data_dir.join("settings.json"),
        r#"{"enabled_themes":["odd.theme.css"]}"#,
    )
    .expect("the settings file");

    let booted = stremio_lightning::boot(data_dir.clone()).expect("the app boots");
    let window = &booted.window;
    // The size a person opens the app at, so the page's widths are the ones its numbers were
    // measured for rather than a test backend's default.
    window
        .window()
        .set_size(slint::PhysicalSize::new(1100, 720));
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
        "Themes opens",
        click(window, "Themes").and_then(|_| {
            wait(window, Duration::from_secs(10), |window| {
                let shown = window.get_mods_page().to_string();
                if shown == "themes" {
                    Ok(shown)
                } else {
                    Err(format!("the window holds {shown}"))
                }
            })
        }),
    );
    outcomes = step(
        outcomes,
        "enabling red.theme.css switches it on without showing a page notice",
        click(window, "Enable red.theme.css").and_then(|_| {
            wait(window, Duration::from_secs(10), |window| {
                let mods = window.get_mods();
                let enabled = (0..mods.themes.row_count())
                    .filter_map(|row| mods.themes.row_data(row))
                    .any(|card| card.filename.as_str() == "red.theme.css" && card.enabled);
                let notice = mods.notice.to_string();
                if enabled && notice.is_empty() {
                    Ok("red.theme.css enabled with no notice".to_owned())
                } else {
                    Err(format!("enabled={enabled}, notice={notice:?}"))
                }
            })
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
    outcomes = step(
        outcomes,
        "the boot theme warning and the theme switch are on the Logs page, newest first",
        wait(window, Duration::from_secs(10), |window| {
            let rows = rows_on(window);
            let warn_pos = rows.iter().position(|(source, level, _)| {
                source == "Theme odd.theme.css" && level == "Warning"
            });
            let info_pos = rows
                .iter()
                .position(|(source, level, _)| source == "Theme red.theme.css" && level == "Info");
            match (info_pos, warn_pos) {
                (Some(newer), Some(older)) if newer < older => {
                    Ok(format!("red info at {newer}, odd warning at {older}"))
                }
                _ => Err(format!("the page lists {rows:?}")),
            }
        }),
    );
    outcomes = step(outcomes, "no sentence above the list counts the records", {
        let texts = ui_driver::texts(window);
        let counting: Vec<&String> = texts
            .iter()
            .filter(|text| text.contains("The filter keeps") || text.contains("of up to 2000"))
            .collect();
        if counting.is_empty() {
            Ok("no count on screen".to_owned())
        } else {
            Err(format!("the page says {counting:?}"))
        }
    });
    outcomes = step(outcomes, "the two filter menus offer every choice", {
        let named = |options: &slint::ModelRc<stremio_lightning::SelectOption>| {
            (0..options.row_count())
                .filter_map(|row| options.row_data(row))
                .map(|option| option.label.to_string())
                .collect::<Vec<_>>()
        };
        let sources = named(&window.get_mods().log_source_options);
        let levels = named(&window.get_mods().log_level_options);
        let wanted_sources = ["Every source", "Plugin", "Theme", "Mods", "Server", "App"];
        let wanted_levels = [
            "Every level",
            "Debug and worse",
            "Info and worse",
            "Warnings and errors",
            "Errors only",
        ];
        if sources == wanted_sources && levels == wanted_levels {
            Ok(format!("{sources:?} / {levels:?}"))
        } else {
            Err(format!("the menus say {sources:?} / {levels:?}"))
        }
    });
    outcomes = step(outcomes, "each menu opens on a choice and says it", {
        let sources = window.get_mods().log_source_options;
        let levels = window.get_mods().log_level_options;
        let title = |options: &slint::ModelRc<stremio_lightning::SelectOption>, index: i32| {
            options
                .row_data(usize::try_from(index).unwrap_or(0))
                .map(|option| option.label.to_string())
                .unwrap_or_default()
        };
        let source = title(&sources, window.get_mods().log_source_index);
        let level = title(&levels, window.get_mods().log_level_index);
        let names = ui_driver::labels(window);
        // The closed menu is a button named for the choice it is showing.
        if source == "Every source"
            && level == "Every level"
            && names.iter().any(|name| name == "Every source")
            && names.iter().any(|name| name == "Every level")
        {
            Ok(format!("{source:?} / {level:?}"))
        } else {
            Err(format!(
                "the menus show {source:?} / {level:?}; on screen: {names:?}"
            ))
        }
    });
    verdict("the Logs page did not list what was logged", &outcomes);

    // ---- the level filter keeps fewer records -----------------------------------
    //
    // A menu's rows live in a popup, which is a window of its own and not part of the tree the
    // testing backend's element search walks (the same limitation `addons_live_ui.rs` records).
    // So the press is made the way the popup makes it: the window's own callback, with the row a
    // press on the menu would send.
    let rows_before = rows_on(window).len();
    let mut filtering: Vec<Outcome> = Vec::new();
    filtering = step(
        filtering,
        "Warnings and errors is picked, and only warnings and errors are left",
        {
            window.invoke_filter_mods_logs("level".into(), 3);
            wait(window, Duration::from_secs(10), |window| {
                // The app keeps recording while the page is open (a failed picture fetch on boot
                // is itself a record), so the count either side of the press is not the thing to
                // judge: what the filter keeps is.
                let rows = rows_on(window);
                if rows.is_empty() {
                    return Err("the filter keeps nothing".to_owned());
                }
                if rows
                    .iter()
                    .any(|(_, level, _)| level != "Warning" && level != "Error")
                {
                    return Err(format!(
                        "{} rows, and not all warnings/errors: {rows:?}",
                        rows.len()
                    ));
                }
                if !rows
                    .iter()
                    .any(|(source, level, _)| source == "Theme odd.theme.css" && level == "Warning")
                {
                    return Err(format!("the warning line is gone: {rows:?}"));
                }
                Ok(format!("{} rows, warnings and errors only", rows.len()))
            })
        },
    );
    filtering = step(
        filtering,
        "the level menu now says Warnings and errors, on screen",
        wait(window, Duration::from_secs(5), |window| {
            let levels = window.get_mods().log_level_options;
            let shown = levels
                .row_data(usize::try_from(window.get_mods().log_level_index).unwrap_or(0))
                .map(|option| option.label.to_string())
                .unwrap_or_default();
            let drawn = ui_driver::texts(window).contains(&shown);
            if shown == "Warnings and errors" && drawn {
                Ok(shown)
            } else {
                Err(format!(
                    "the model says {shown:?}, drawn: {drawn}; on screen: {:?}",
                    ui_driver::texts(window)
                ))
            }
        }),
    );
    filtering = step(
        filtering,
        "the info switch record is not among them",
        wait(window, Duration::from_secs(5), |window| {
            let rows = rows_on(window);
            if rows
                .iter()
                .any(|(source, _, _)| source == "Theme red.theme.css")
            {
                Err(format!("the info record is still listed: {rows:?}"))
            } else {
                Ok(format!("{} rows", rows.len()))
            }
        }),
    );
    filtering = step(
        filtering,
        "a source picked keeps only that source, and Every level brings the rest back",
        {
            window.invoke_filter_mods_logs("source".into(), 2);
            wait(window, Duration::from_secs(5), |window| {
                let rows = rows_on(window);
                if rows.is_empty() {
                    return Err("a source with records in it keeps none".to_owned());
                }
                if rows
                    .iter()
                    .any(|(source, _, _)| !source.starts_with("Theme"))
                {
                    return Err(format!("the list holds {rows:?}"));
                }
                Ok(format!("{} theme rows", rows.len()))
            })
            .and_then(|detail| {
                window.invoke_filter_mods_logs("source".into(), 0);
                window.invoke_filter_mods_logs("level".into(), 0);
                wait(window, Duration::from_secs(5), |window| {
                    let rows = rows_on(window).len();
                    if rows >= rows_before {
                        Ok(format!("{detail}; {rows} rows with every source and level"))
                    } else {
                        Err(format!("{detail}; {rows} of {rows_before} rows came back"))
                    }
                })
            })
        },
    );
    verdict("the level filter did not narrow the list", &filtering);

    // ---- the search box filters as it is typed, and clears in one press ------------------------
    let mut searching: Vec<Outcome> = Vec::new();
    searching = step(
        searching,
        "the search box takes the pointer",
        click(window, "Search the logs"),
    );
    searching = step(
        searching,
        "typing red.theme keeps only the red theme's records",
        wait(window, Duration::from_secs(5), |window| {
            if window.get_mods().log_search.is_empty() {
                type_text(window, "red.theme");
            }
            let rows = rows_on(window);
            if rows.is_empty() {
                return Err("nothing is listed while searching".to_owned());
            }
            if rows
                .iter()
                .any(|(source, _, _)| !source.contains("red.theme.css"))
            {
                return Err(format!("the list holds {rows:?}"));
            }
            Ok(format!("{} rows for red.theme", rows.len()))
        }),
    );
    searching = step(
        searching,
        "the clear button is on screen and empty the box",
        wait(window, Duration::from_secs(5), |window| {
            ui_driver::labels(window)
                .iter()
                .any(|name| name == "Clear the search")
                .then(|| "Clear the search".to_owned())
                .ok_or_else(|| "the clear button is not on screen".to_owned())
        })
        .and_then(|_| click(window, "Clear the search")),
    );
    searching = step(
        searching,
        "clearing restores every record the filter keeps",
        wait(window, Duration::from_secs(5), |window| {
            let rows = rows_on(window).len();
            let search = window.get_mods().log_search.to_string();
            if !search.is_empty() {
                return Err(format!("the filter still holds {search:?}"));
            }
            if rows < rows_before {
                return Err(format!("{rows} of {rows_before} rows came back"));
            }
            Ok(format!("{rows} rows, and the filter is empty"))
        }),
    );
    verdict("the search box did not filter or clear", &searching);

    // ---- a record written while the page is open lands on it ----------------------------------
    let mut live: Vec<Outcome> = Vec::new();
    live = step(
        live,
        "a line recorded from another thread appears without a page switch",
        {
            // The page is open, and the recorder is the app's own: this is the line a plugin's
            // timer or the stream server writes while someone is looking at the page.
            booted.logs.record(
                stremio_lightning::plumbing::logs::Source::App,
                "Core",
                stremio_lightning::plumbing::logs::Level::Error,
                "a line written while the page was open",
            );
            let page_before = window.get_mods_page().to_string();
            wait(window, Duration::from_secs(5), |window| {
                let rows = rows_on(window);
                if rows
                    .iter()
                    .any(|(_, _, text)| text == "a line written while the page was open")
                {
                    let page_after = window.get_mods_page().to_string();
                    if page_after == page_before {
                        Ok(format!("{page_after} still open, and the line is on it"))
                    } else {
                        Err(format!("the page became {page_after}"))
                    }
                } else {
                    Err(format!("the line is not listed yet: {rows:?}"))
                }
            })
        },
    );
    verdict(
        "a record written while the page was open did not appear",
        &live,
    );

    // ---- the detail panel opens, copies, closes ----------------------------------------------
    let mut detail: Vec<Outcome> = Vec::new();
    detail = step(
        detail,
        "the line this test recorded is pressed, and its text opens at once",
        {
            // The row says the time, source and text it drew, which is the name a press looks
            // for. The record the test wrote is the one pressed, not the newest row: the app keeps
            // recording (a boot failure, a server verdict), and a newer row would move the target
            // out from under the press. What the press did is judged before anything else can
            // happen, because a record landing a moment later would redraw the panel and hide a
            // page that never opened it.
            let written = (0..window.get_mods().log_rows.row_count())
                .filter_map(|at| window.get_mods().log_rows.row_data(at))
                .find(|row| row.text == "a line written while the page was open");
            match written {
                None => Err("the test's own line is not listed".to_owned()),
                Some(row) => {
                    let label = format!("Show {} {}: {}", row.time, row.source, row.text);
                    click(window, &label).and_then(|pressed| {
                        let heading = window.get_mods().log_detail_heading.to_string();
                        let open = window.get_mods().log_open_id;
                        let lines = window.get_mods().log_detail.row_count();
                        if open == row.id && !heading.is_empty() && lines > 0 {
                            Ok(format!("{pressed}; the panel is open on row {}", row.id))
                        } else {
                            Err(format!(
                                "nothing opened: heading {heading:?}, open {open}, {lines} lines"
                            ))
                        }
                    })
                }
            }
        },
    );
    detail = step(
        detail,
        "the panel is drawn with the record's name, and the row it belongs to is marked",
        wait(window, Duration::from_secs(5), |window| {
            let heading = window.get_mods().log_detail_heading.to_string();
            let lines: Vec<String> = (0..window.get_mods().log_detail.row_count())
                .filter_map(|row| window.get_mods().log_detail.row_data(row))
                .map(|line| line.to_string())
                .collect();
            if heading.is_empty() || lines.is_empty() {
                return Err(format!("the panel holds {heading:?} / {lines:?}"));
            }
            if window.get_mods().log_open_id == 0 {
                return Err("no row is marked as open".to_owned());
            }
            // What the section draws, not what the model holds: the heading is on screen and the
            // row it belongs to says it is the open one.
            let texts = ui_driver::texts(window);
            if !texts.contains(&heading) {
                return Err(format!("the panel draws no heading; on screen: {texts:?}"));
            }
            let names = ui_driver::labels(window);
            let marked: Vec<&String> = names
                .iter()
                .filter(|name| {
                    name.starts_with("Open ")
                        && name.contains("a line written while the page was open")
                })
                .collect();
            match marked.first() {
                Some(name) => Ok(format!("{heading}; the row says {name:?}")),
                None => Err(format!("no row is marked; on screen: {names:?}")),
            }
        }),
    );
    detail = step(
        detail,
        "Copy reports what the clipboard answered",
        click(window, "Copy the record").and_then(|_| {
            wait(window, Duration::from_secs(5), |window| {
                // The press is judged by its answer, not by what the machine's clipboard does with
                // it: Windows copies, Linux has no clipboard yet and says so.
                let notice = window.get_mods().notice.to_string();
                if notice.contains("Copied the record")
                    || notice.contains("clipboard")
                    || notice.contains("copying is not implemented")
                {
                    Ok(notice)
                } else {
                    Err(format!("the page says {notice:?}"))
                }
            })
        }),
    );
    detail = step(
        detail,
        "Close takes the panel away and unmarks the row",
        click(window, "Close the record").and_then(|_| {
            wait(window, Duration::from_secs(5), |window| {
                let heading = window.get_mods().log_detail_heading.to_string();
                let open = window.get_mods().log_open_id;
                if heading.is_empty() && open == 0 {
                    Ok("the panel is gone".to_owned())
                } else {
                    Err(format!("the panel still says {heading:?}, open {open}"))
                }
            })
        }),
    );
    verdict("the detail panel did not open, copy or close", &detail);

    // ---- dragging across rows selects multiple and copies them all ---------------------------
    let mut multi: Vec<Outcome> = Vec::new();
    multi = step(
        multi,
        "dragging across two rows selects both and enables batch Copy",
        {
            let rows = window.get_mods().log_rows;
            let row0 = rows.row_data(0);
            let row1 = rows.row_data(1);
            match (row0, row1) {
                (Some(first), Some(second)) => {
                    let label0 = format!("Show {} {}: {}", first.time, first.source, first.text);
                    let label1 = format!("Show {} {}: {}", second.time, second.source, second.text);
                    match (
                        ui_driver::find(window, &label0),
                        ui_driver::find(window, &label1),
                    ) {
                        (Some(h0), Some(h1)) => {
                            let p0 = h0.absolute_position();
                            let s0 = h0.size();
                            let p1 = h1.absolute_position();
                            let s1 = h1.size();
                            let start = slint::LogicalPosition::new(
                                p0.x + s0.width * 0.5,
                                p0.y + s0.height * 0.5,
                            );
                            let end = slint::LogicalPosition::new(
                                p1.x + s1.width * 0.5,
                                p1.y + s1.height * 0.5,
                            );
                            window.window().dispatch_event(
                                slint::platform::WindowEvent::PointerPressed {
                                    position: start,
                                    button: slint::platform::PointerEventButton::Left,
                                },
                            );
                            ui_driver::pump(Duration::from_millis(30));
                            window.window().dispatch_event(
                                slint::platform::WindowEvent::PointerMoved { position: end },
                            );
                            ui_driver::pump(Duration::from_millis(30));
                            window.window().dispatch_event(
                                slint::platform::WindowEvent::PointerReleased {
                                    position: end,
                                    button: slint::platform::PointerEventButton::Left,
                                },
                            );
                            wait(window, Duration::from_secs(5), |window| {
                                let count = window.get_mods().log_selected_count;
                                if count >= 2 {
                                    Ok(format!("{count} rows selected"))
                                } else {
                                    Err(format!("selected count is {count}"))
                                }
                            })
                        }
                        _ => Err("could not locate first two log rows on screen".to_owned()),
                    }
                }
                _ => Err("fewer than two log rows on screen".to_owned()),
            }
        },
    );
    multi = step(multi, "toolbar Copy copies all selected records", {
        let count = window.get_mods().log_selected_count;
        let copy_label = format!("Copy {count} selected records");
        click(window, &copy_label).and_then(|_| {
            wait(window, Duration::from_secs(5), |window| {
                let notice = window.get_mods().notice.to_string();
                if notice.contains(&format!("Copied {count} records"))
                    || notice.contains("clipboard")
                    || notice.contains("copying is not implemented")
                {
                    Ok(notice)
                } else {
                    Err(format!("the page says {notice:?}"))
                }
            })
        })
    });
    multi = step(
        multi,
        "Close clears the multi-selection and closes the panel",
        click(window, "Close the record").and_then(|_| {
            wait(window, Duration::from_secs(5), |window| {
                let count = window.get_mods().log_selected_count;
                let heading = window.get_mods().log_detail_heading.to_string();
                if count == 0 && heading.is_empty() {
                    Ok("selection cleared".to_owned())
                } else {
                    Err(format!("count={count}, heading={heading:?}"))
                }
            })
        }),
    );
    verdict("drag multi-select and batch copy failed", &multi);

    // ---- Reset filters & keyboard navigation (Down, Esc, Select All) -------------------------
    let mut ux: Vec<Outcome> = Vec::new();
    ux = step(
        ux,
        "Reset filters restores source, level, and search in one press",
        {
            window.invoke_filter_mods_logs("source".into(), 2);
            window.invoke_search_mods_logs("odd".into());
            wait(window, Duration::from_secs(5), |window| {
                let mods = window.get_mods();
                if mods.log_filtered && mods.log_count.contains(" of ") {
                    Ok(mods.log_count.to_string())
                } else {
                    Err(format!(
                        "filtered={}, count={:?}",
                        mods.log_filtered, mods.log_count
                    ))
                }
            })
            .and_then(|count_before| {
                click(window, "Reset filters").and_then(|_| {
                    wait(window, Duration::from_secs(5), |window| {
                        let mods = window.get_mods();
                        if !mods.log_filtered
                            && mods.log_source_index == 0
                            && mods.log_level_index == 0
                            && mods.log_search.is_empty()
                        {
                            Ok(format!("{count_before} -> {}", mods.log_count))
                        } else {
                            Err(format!(
                                "still filtered: filtered={}, src={}, lvl={}, search={:?}",
                                mods.log_filtered,
                                mods.log_source_index,
                                mods.log_level_index,
                                mods.log_search
                            ))
                        }
                    })
                })
            })
        },
    );
    ux = step(ux, "Down arrow opens the top row and Escape closes it", {
        window
            .window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed {
                text: slint::platform::Key::DownArrow.into(),
            });
        wait(window, Duration::from_secs(5), |window| {
            let mods = window.get_mods();
            if mods.log_selected_count == 1 && mods.log_active_index == 0 && mods.log_open_id != 0 {
                Ok(format!("active_index=0, open_id={}", mods.log_open_id))
            } else {
                Err(format!(
                    "selected={}, active_index={}, open_id={}",
                    mods.log_selected_count, mods.log_active_index, mods.log_open_id
                ))
            }
        })
        .and_then(|opened| {
            window
                .window()
                .dispatch_event(slint::platform::WindowEvent::KeyPressed {
                    text: slint::platform::Key::Escape.into(),
                });
            wait(window, Duration::from_secs(5), |window| {
                let mods = window.get_mods();
                if mods.log_selected_count == 0 && mods.log_open_id == 0 {
                    Ok(format!("{opened}; Escape cleared"))
                } else {
                    Err(format!(
                        "selected={}, open_id={}",
                        mods.log_selected_count, mods.log_open_id
                    ))
                }
            })
        })
    });
    ux = step(
        ux,
        "Select all picks every visible row and Escape clears them",
        {
            let total = slint::Model::row_count(&window.get_mods().log_rows);
            window.invoke_select_all_mods_logs();
            wait(window, Duration::from_secs(5), |window| {
                let count = window.get_mods().log_selected_count as usize;
                if count == total && total > 0 {
                    Ok(format!("selected all {count}"))
                } else {
                    Err(format!("selected {count} of {total}"))
                }
            })
            .and_then(|selected| {
                window
                    .window()
                    .dispatch_event(slint::platform::WindowEvent::KeyPressed {
                        text: slint::platform::Key::Escape.into(),
                    });
                wait(window, Duration::from_secs(5), |window| {
                    let count = window.get_mods().log_selected_count;
                    if count == 0 {
                        Ok(format!("{selected}; Escape cleared"))
                    } else {
                        Err(format!("still {count} selected"))
                    }
                })
            })
        },
    );
    verdict("logs filter reset or keyboard navigation failed", &ux);

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
                if rows == 0 {
                    Ok("the list is empty".to_owned())
                } else {
                    Err(format!("{rows} rows are still listed"))
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
    about = step(
        about,
        "it counts one built-in plugin enabled and two themes with one enabled",
        wait(window, Duration::from_secs(10), |window| {
            let counts = window.get_mods().about_counts.clone();
            let value = |label: &str| {
                counts
                    .iter()
                    .find(|line| line.label == label)
                    .map(|line| line.value.clone())
            };
            let said = (
                value("Built-in plugins"),
                value("Built-in plugins enabled"),
                value("Themes installed"),
                value("Themes enabled"),
            );
            match said {
                (Some(a), Some(b), Some(c), Some(d))
                    if a == "1" && b == "1" && c == "3" && d == "1" =>
                {
                    Ok("1 built-in plugin, 1 enabled, 3 themes, 1 enabled".to_owned())
                }
                _ => Err(format!("the counts are {said:?}")),
            }
        }),
    );
    about = step(
        about,
        "the enabled lists name built-in AniSkip and the active theme",
        {
            let enabled = &window.get_mods().about_enabled;
            let find = |label: &str| {
                (0..enabled.row_count())
                    .filter_map(|row| enabled.row_data(row))
                    .find(|line| line.label == label)
                    .map(|line| line.value.to_string())
            };
            match (
                find("Enabled plugins").as_deref(),
                find("Enabled themes").as_deref(),
            ) {
                (Some("builtin:aniskip"), Some("red.theme.css")) => {
                    Ok("builtin:aniskip / red.theme.css".to_owned())
                }
                other => Err(format!("the enabled lists are {other:?}")),
            }
        },
    );
    about = step(about, "the places name the themes folder", {
        let places = &window.get_mods().about_places;
        let listed: Vec<(String, String)> = (0..places.row_count())
            .filter_map(|row| places.row_data(row))
            .map(|line| (line.label.to_string(), line.value.to_string()))
            .collect();
        let themes_dir = stremio_lightning_core::mods::mods_dir(
            &data_dir,
            stremio_lightning_core::mods::ModType::Theme,
        );
        if listed
            .iter()
            .any(|(name, said)| name == "Themes folder" && Path::new(said) == themes_dir)
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
                    Ok(text) if text.contains("Built-in plugins") => Ok(written),
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
