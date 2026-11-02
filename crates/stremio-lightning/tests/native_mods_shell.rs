//! The Mods section: its six pages, and the two pages that work offline.
//!
//! Three tests. The first reads the screen's data over a folder of real plugin and theme files
//! and presses its actions, so what a page lists and what a switch writes down is checked without
//! a window. The second reads the two lists back from `settings.json`, so a choice survives a
//! restart and only the theme it names is the one that colours the window. The third boots the
//! app, opens every page from the menu, and removes a theme one press at a time, so the window is
//! checked to draw what the screen says and to delete nothing until the second press.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::json;
use slint::Model;
use stremio_lightning::screens::mods::{self, catalogue, Page};
use stremio_lightning::themes;
use stremio_lightning::{MainWindow, Section};

mod support;

use support::ui_driver::{self, Outcome};

/// The theme cards the page holds now, by name.
fn on_screen(window: &MainWindow) -> Vec<String> {
    window
        .get_mods()
        .themes
        .iter()
        .map(|card| card.name.to_string())
        .collect()
}

/// Presses whatever says `label`, through the element rather than at the coordinates it reports:
/// the element maps the press to the window itself, so a control inside a scrolled list is
/// pressed where it is drawn however far the list has scrolled. `Err` says what is on screen.
fn click(window: &MainWindow, label: &str) -> Result<String, String> {
    // What is on the model and what is drawn are not the same thing: a card the model has just
    // gained is a row the window builds on a later turn, and the row's buttons only exist once it
    // has. Waiting for the label is waiting for what a person waits for: the control being there
    // to press. The budget is generous and the loop is bounded, so a control that never appears
    // fails the line rather than the run.
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

/// Waits `budget` for `check` to hold, letting the event loop turn: what a press asks the app
/// for is written by other threads and lands a moment later.
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

/// Adds one line to the report: what was done or expected, and how it went.
fn step(mut outcomes: Vec<Outcome>, what: &str, result: Result<String, String>) -> Vec<Outcome> {
    outcomes.push(Outcome {
        what: what.to_owned(),
        ok: Some(result.is_ok()),
        detail: result.unwrap_or_else(|error| error),
    });
    outcomes
}

/// Fails the test with every line that did not hold, so a red run says the whole story.
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

/// A folder of this run's own, emptied first so a leftover from an earlier run is not read.
fn temp_dir(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "stremio-lightning-mods-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("a folder of its own for this run");
    folder
}

/// Writes one mod file where `stremio_lightning_core::mods` looks for it.
fn write_mod(data_dir: &Path, folder: &str, filename: &str, body: &str) -> PathBuf {
    let path = data_dir
        .join("stremio-lightning")
        .join(folder)
        .join(filename);
    std::fs::create_dir_all(path.parent().unwrap()).expect("the mods folder");
    std::fs::write(&path, body).expect("the mod file");
    path
}

/// A plugin file with a metadata comment, the shape the catalogue reads.
fn plugin(name: &str, author: &str, version: &str) -> String {
    format!(
        "/**\n * @name {name}\n * @description Does one thing.\n * @author {author}\n * @version {version}\n */\n(function () {{}}());\n"
    )
}

/// A theme file with a metadata comment and one rule this app understands.
fn theme(name: &str) -> String {
    format!(
        "/**\n * @name {name}\n * @description A colour change.\n * @author Someone\n * @version 1.0.0\n */\n.nav-tab-button {{\n  color: #ff0000;\n}}\n"
    )
}

/// The cards of a page, as the window would be handed them.
fn cards_of(data: &mods::Data, page: Page) -> Vec<mods::ModCard> {
    data.clone().with_page(page).cards().to_vec()
}

fn names(cards: &[mods::ModCard]) -> Vec<String> {
    cards.iter().map(|card| card.name.clone()).collect()
}

fn file_names(cards: &[mods::ModCard]) -> Vec<String> {
    cards.iter().map(|card| card.filename.clone()).collect()
}

/// The two lists this shell writes into `settings.json`, read back the way the app's own reader
/// reads them: by the names the screen uses, a list of file names each.
#[derive(Debug, PartialEq, Eq)]
struct SavedLists {
    plugins: Vec<String>,
    themes: Vec<String>,
}

fn saved(data_dir: &Path) -> SavedLists {
    let path = data_dir.join("settings.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let json: serde_json::Value = serde_json::from_str(&text).expect("settings.json is JSON");
    let list = |key: &str| -> Vec<String> {
        json.get(key)
            .and_then(|value| value.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    SavedLists {
        plugins: list("enabled_plugins"),
        themes: list("enabled_themes"),
    }
}

#[test]
fn the_screen_lists_what_is_installed_and_its_actions_reach_the_disk() {
    let data_dir = temp_dir("screen");
    write_mod(
        &data_dir,
        "plugins",
        "alpha.plugin.js",
        &plugin("Alpha", "Ada", "1.2.0"),
    );
    // A file with no metadata comment: its card says so and shows the file name.
    write_mod(
        &data_dir,
        "plugins",
        "bare.plugin.js",
        "// nothing in here\n",
    );
    write_mod(&data_dir, "themes", "red.theme.css", &theme("Red"));
    write_mod(&data_dir, "themes", "green.theme.css", &theme("Green"));
    // A theme this app cannot read all of: the card must say so before anyone turns it on.
    write_mod(
        &data_dir,
        "themes",
        "odd.theme.css",
        &format!(
            "{}\n.nothing-matches-this {{ color: #00ff00 }}\n",
            theme("Odd")
        ),
    );

    // ---- what the two pages list -------------------------------------------------------------
    let mut data = mods::data(&data_dir);
    let plugins = cards_of(&data, Page::Plugins);
    assert_eq!(
        file_names(&plugins),
        ["alpha.plugin.js", "bare.plugin.js"],
        "every file in the folder gets a card, sorted by file name"
    );
    assert_eq!(names(&plugins)[0], "Alpha");
    assert_eq!(catalogue::byline(&plugins[0]), "Ada v1.2.0");
    assert_eq!(
        names(&plugins)[1],
        "bare.plugin.js",
        "a file with no metadata shows its file name"
    );
    assert!(
        !catalogue::unreadable(&plugins[1]).is_empty(),
        "and says why it has no name of its own"
    );
    assert!(
        catalogue::unreadable(&plugins[0]).is_empty(),
        "a file that names itself says nothing about its name"
    );
    assert!(
        plugins.iter().all(|card| !card.enabled),
        "nothing is on until someone turns it on"
    );

    let themes = cards_of(&data, Page::Themes);
    assert_eq!(names(&themes), ["Green", "Odd", "Red"]);
    let odd = catalogue::card(&themes, "odd.theme.css").expect("the third theme");
    assert!(
        odd.warnings
            .iter()
            .any(|warning| warning.contains("nothing-matches-this")),
        "a rule that reaches nothing is on the card: {:?}",
        odd.warnings
    );
    let red = catalogue::card(&themes, "red.theme.css").expect("the red theme");
    assert!(red.warnings.is_empty(), "{:?}", red.warnings);

    // ---- a plugin's switch ---------------------------------------------------------------
    data = data.with_page(Page::Plugins);
    mods::set_enabled(&mut data, &data_dir, "alpha.plugin.js", true);
    assert!(
        data.notice.text.contains("alpha.plugin.js is enabled"),
        "the screen says what the press did: {:?}",
        data.notice.text
    );
    assert_eq!(
        saved(&data_dir).plugins,
        ["alpha.plugin.js"],
        "the switch writes the file name down"
    );
    assert!(
        cards_of(&data, Page::Plugins)[0].enabled,
        "and the card reads it back"
    );

    mods::set_enabled(&mut data, &data_dir, "alpha.plugin.js", false);
    assert!(
        saved(&data_dir).plugins.is_empty(),
        "switching it off takes the name back out"
    );

    // ---- a theme's switch keeps one on at a time ------------------------------------------
    data = data.with_page(Page::Themes);
    mods::set_enabled(&mut data, &data_dir, "green.theme.css", true);
    mods::set_enabled(&mut data, &data_dir, "red.theme.css", true);
    assert_eq!(
        saved(&data_dir).themes,
        ["red.theme.css"],
        "turning a second theme on takes the first one off"
    );
    assert_eq!(
        cards_of(&data, Page::Themes)
            .iter()
            .filter(|card| card.enabled)
            .map(|card| card.name.as_str())
            .collect::<Vec<_>>(),
        ["Red"],
        "exactly one theme card says it is on"
    );

    // ---- remove ----------------------------------------------------------------------------
    let red_file = data_dir
        .join("stremio-lightning")
        .join("themes")
        .join("red.theme.css");
    assert!(red_file.exists(), "the file is there before the press");
    data = data.with_notice(mods::Notice::default());
    mods::remove(&mut data, &data_dir, "red.theme.css");
    assert!(!red_file.exists(), "remove deletes the file");
    assert_eq!(
        data.notice.text, "Removed red.theme.css.",
        "and the screen says so"
    );
    assert!(
        saved(&data_dir).themes.is_empty(),
        "the settings no longer name a file that is gone"
    );
    assert_eq!(
        file_names(&cards_of(&data, Page::Themes)),
        ["green.theme.css", "odd.theme.css"],
        "the card is gone from the page"
    );

    // ---- pages that do not list cards say so ----------------------------------------------
    for page in [Page::Marketplace, Page::Settings, Page::Logs, Page::About] {
        let page_data = data.clone().with_page(page);
        assert!(
            page_data.cards().is_empty(),
            "{} lists no mods yet",
            page.id()
        );
        assert!(
            !mods::switchable(&page_data),
            "{} has no switch to press",
            page.id()
        );
        assert_eq!(page_data.page, page.id());
    }

    let _ = std::fs::remove_dir_all(&data_dir);
}

/// The choices a person makes are in `settings.json`, so a second reading of the same folder sees
/// them, and the theme it names is the one whose rules reach the window.
#[test]
fn a_choice_survives_a_restart() {
    let data_dir = temp_dir("restart");
    write_mod(
        &data_dir,
        "plugins",
        "keep.plugin.js",
        &plugin("Keep", "Kay", "0.1.0"),
    );
    write_mod(&data_dir, "themes", "blue.theme.css", &theme("Blue"));

    let mut data = mods::data(&data_dir).with_page(Page::Plugins);
    mods::set_enabled(&mut data, &data_dir, "keep.plugin.js", true);
    let mut data = data.with_page(Page::Themes);
    mods::set_enabled(&mut data, &data_dir, "blue.theme.css", true);

    assert_eq!(
        saved(&data_dir),
        SavedLists {
            plugins: vec!["keep.plugin.js".to_owned()],
            themes: vec!["blue.theme.css".to_owned()],
        },
        "the file names are what the settings hold"
    );

    // A second read of the same folder, as a restart makes.
    let again = mods::data(&data_dir);
    assert!(
        cards_of(&again, Page::Plugins)[0].enabled,
        "the plugin comes back on"
    );
    assert!(
        cards_of(&again, Page::Themes)[0].enabled,
        "the theme comes back on"
    );

    // The enabled theme file is the one `plumbing::theme` colours the window with.
    let css = std::fs::read_to_string(
        data_dir
            .join("stremio-lightning")
            .join("themes")
            .join("blue.theme.css"),
    )
    .expect("the theme file");
    let style = themes::inspect(&css)
        .style_of("nav-tab-button-container")
        .expect("the nav tab is a component this app has");
    assert_eq!(
        style.color,
        Some(themes::Rgba(255, 0, 0, 255)),
        "the rule the theme names reaches the nav tab"
    );

    // Turning the theme off leaves nothing on, which is the app's own colours.
    let mut data = data.with_page(Page::Themes);
    mods::set_enabled(&mut data, &data_dir, "blue.theme.css", false);
    assert!(
        saved(&data_dir).themes.is_empty(),
        "no theme is named after it is turned off"
    );

    let _ = std::fs::remove_dir_all(&data_dir);
}

/// A plugin whose file registers two controls, one of each kind the page has more than one of: a
/// range and a select. Anything else it wants to do is `body`.
fn configuring(name: &str, body: &str) -> String {
    format!(
        "/**\n * @name {name}\n * @description Settings.\n * @author Ada\n * @version 1.0.0\n */\nregisterSettings([\n  {{ key: \"rows\", label: \"Rows\", type: \"range\", min: 1, max: 9, step: 1, default: 3 }},\n  {{ key: \"mode\", label: \"Mode\", type: \"select\", options: [\"fast\", \"slow\"], default: \"slow\" }}\n]);\n{body}\n"
    )
}

/// The plugins folder under `data_dir`.
fn plugins_dir(data_dir: &Path) -> PathBuf {
    stremio_lightning_core::mods::mods_dir(data_dir, stremio_lightning_core::mods::ModType::Plugin)
}

#[test]
fn the_settings_page_saves_only_on_save_and_a_stopped_plugin_says_why() {
    let data_dir = temp_dir("settings");
    write_mod(
        &data_dir,
        "plugins",
        "tidy.plugin.js",
        &configuring("Tidy", ""),
    );
    write_mod(
        &data_dir,
        "plugins",
        "broken.plugin.js",
        &format!("{}\n", plugin("Broken", "Bo", "1.0.0")),
    );

    // The plugin runs once, the way the app starts it, and writes its schema into its own file.
    let source = std::fs::read_to_string(plugins_dir(&data_dir).join("tidy.plugin.js"))
        .expect("the plugin was written");
    let (running, outcome) = stremio_lightning::plumbing::plugin::Running::start(
        source,
        "tidy",
        stremio_lightning::plumbing::plugin::Folder::new(&data_dir),
    );
    assert!(running.running(), "stopped: {}", running.stopped());
    assert!(!outcome.run.failed(), "stopped: {}", outcome.run.said());

    // ---- the left list says what each plugin has -------------------------------------------
    let folder = plugins_dir(&data_dir);
    let extras = stremio_lightning::screens::settings::LocalExtras::default();
    let listed = mods::settings::plugins(&data_dir, &extras).expect("the plugins folder reads");
    let file_names: Vec<&str> = listed.iter().map(|p| p.filename.as_str()).collect();
    assert_eq!(
        file_names,
        ["broken.plugin.js", "tidy.plugin.js"],
        "every installed plugin is on the left list, by file name"
    );
    let tidy = listed
        .iter()
        .find(|p| p.filename == "tidy.plugin.js")
        .unwrap();
    assert_eq!(tidy.fields, 2, "it registered two controls");
    assert_eq!(tidy.name, "Tidy", "and is named by its own metadata");
    assert_eq!(
        listed
            .iter()
            .find(|p| p.filename == "broken.plugin.js")
            .unwrap()
            .fields,
        0,
        "a plugin that registered nothing has no controls"
    );

    // ---- the schema is drawn from the file, not from a process that has to run first --------
    let mut edits = mods::settings::edits(&data_dir, "tidy.plugin.js");
    let keys: Vec<&str> = edits.controls.iter().map(|c| c.field.key()).collect();
    assert_eq!(keys, ["rows", "mode"], "the page reads the schema back");
    assert_eq!(
        edits.controls[0].edited,
        json!(3),
        "a control with no saved value holds its default"
    );
    assert!(!edits.dirty(), "nothing is typed yet");

    // ---- typing changes the page and not the file -------------------------------------------
    let schema_only = mods::settings::file_of(&folder, "tidy");
    mods::settings::set_value(&mut edits, "rows", &json!(7));
    assert!(edits.dirty(), "a typed value is a change to save");
    assert_eq!(
        mods::settings::file_of(&folder, "tidy"),
        schema_only,
        "typing writes nothing"
    );

    // ---- Revert throws the typing away and leaves the file alone ---------------------------
    mods::settings::revert(&mut edits);
    assert!(!edits.dirty(), "revert leaves nothing typed");
    assert_eq!(edits.controls[0].edited, json!(3));
    assert_eq!(
        mods::settings::file_of(&folder, "tidy"),
        schema_only,
        "revert writes nothing"
    );

    // ---- Save writes what changed, keeps the schema, and hands the file back ----------------
    mods::settings::set_value(&mut edits, "rows", &json!(7));
    mods::settings::set_value(&mut edits, "mode", &json!("fast"));
    let saved = mods::settings::save(&data_dir, &mut edits);
    assert!(saved.written, "the file took the values: {}", saved.error);
    assert_eq!(
        stremio_lightning_core::settings::setting(&folder, "tidy", "rows").expect("a saved value"),
        json!(7)
    );
    assert_eq!(
        stremio_lightning_core::settings::setting(&folder, "tidy", "mode").expect("a saved value"),
        json!("fast")
    );
    assert!(
        saved.values.get("__schema").is_some(),
        "a save keeps the schema the page draws from"
    );
    assert_eq!(
        saved.values.get("mode"),
        Some(&json!("fast")),
        "the plugin is handed what its file now holds"
    );
    assert!(!edits.dirty(), "the page and the file agree after Save");

    // ---- nothing on the page deletes a value the schema no longer names --------------------
    stremio_lightning_core::settings::save_setting(&folder, "tidy", "left_over", json!("keep me"))
        .expect("a value the schema does not name");
    let mut edits = mods::settings::edits(&data_dir, "tidy.plugin.js");
    mods::settings::set_value(&mut edits, "rows", &json!(4));
    assert!(mods::settings::save(&data_dir, &mut edits).written);
    assert_eq!(
        stremio_lightning_core::settings::setting(&folder, "tidy", "left_over")
            .expect("still there"),
        json!("keep me"),
        "a save writes the controls and leaves the rest of the file alone"
    );

    // ---- what the page says about a plugin that runs, and one that stopped -----------------
    let up = mods::settings::Running {
        running: true,
        stopped: String::new(),
        last_error: "slow".to_owned(),
    };
    assert_eq!(up.said(true), "Running. Last warning: slow");
    let stopped = mods::settings::Running {
        running: false,
        stopped: "it threw".to_owned(),
        ..up
    };
    assert_eq!(stopped.said(true), "Stopped: it threw");
    assert_eq!(
        stopped.said(false),
        "",
        "a plugin that is off does not claim to have stopped"
    );

    let _ = std::fs::remove_dir_all(&data_dir);
}

#[test]
fn the_window_opens_every_page_and_removes_a_theme_in_two_presses() {
    // Mock time, not system time: the clock this runner reports is not monotonic, and Slint
    // asserts on a tick that goes backwards as soon as any animation is live. The clock only
    // moves when the driver steps it, so every wait in this test steps it too.
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let data_dir = temp_dir("window");
    write_mod(&data_dir, "themes", "red.theme.css", &theme("Red"));
    // One plugin that fails at load, switched on before the app starts, so the Plugins page has to
    // say what became of it rather than only that the file is on.
    write_mod(
        &data_dir,
        "plugins",
        "throws.plugin.js",
        &format!(
            "{}\nthrow new Error(\"no\");\n",
            plugin("Throws", "Tia", "1.0.0")
        ),
    );
    std::fs::write(
        data_dir.join("settings.json"),
        r#"{"enabled_plugins":["throws.plugin.js"]}"#,
    )
    .expect("the settings file");
    // The page reads the same file, so a file it cannot parse would leave the plugin off and the
    // card with nothing to say. Prove the switch is on before the app opens.
    let installed = stremio_lightning::screens::mods::data(&data_dir).plugins;
    assert_eq!(
        installed
            .iter()
            .map(|card| (card.filename.as_str(), card.enabled))
            .collect::<Vec<_>>(),
        [("throws.plugin.js", true)],
        "the settings file names the plugin and the card says it is on"
    );
    let red = data_dir
        .join("stremio-lightning")
        .join("themes")
        .join("red.theme.css");
    assert!(red.exists(), "the theme is installed before the app opens");

    let booted = stremio_lightning::boot(data_dir.clone()).expect("the app boots");
    let window = &booted.window;
    println!(
        "the plugins folder holds {:?}",
        std::fs::read_dir(plugins_dir(&data_dir)).map(|dir| dir
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name())
            .collect::<Vec<_>>())
    );
    ui_driver::pump_for(window);
    // ---- the Mods tab, the menu, and every page opening from its own row ---------------------
    let menu = [
        ("plugins", "Plugins"),
        ("themes", "Themes"),
        ("marketplace", "Marketplace"),
        ("settings", "Plugin settings"),
        ("logs", "Logs"),
        ("about", "About"),
    ];
    // The page the window holds and the page the screen is drawing have to agree; the menu's own
    // mark follows the screen's page, so the row for that page is the marked one by construction.
    let page_is = |page: &'static str| {
        let shown = window.get_mods_page().to_string();
        let drawn = window.get_mods().page.to_string();
        if shown == page && drawn == page {
            Ok(page.to_owned())
        } else {
            Err(format!(
                "the window holds {shown} and the screen draws {drawn}"
            ))
        }
    };

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
        "The Plugins page says why a plugin that failed is not running",
        wait(window, Duration::from_secs(20), |window| {
            let plugins = window.get_mods().plugins;
            let cards: Vec<(String, Vec<String>)> = (0..plugins.row_count())
                .filter_map(|row| plugins.row_data(row))
                .map(|card| {
                    (
                        card.filename.to_string(),
                        card.warnings.iter().map(|w| w.to_string()).collect(),
                    )
                })
                .collect();
            let Some((_, said)) = cards.iter().find(|(file, _)| file == "throws.plugin.js") else {
                return Err(format!("the page lists {cards:?}"));
            };
            if said.iter().any(|line| line.starts_with("Stopped: ")) {
                Ok(said.join(" / "))
            } else {
                Err(format!(
                    "the card says {said:?}, all cards {cards:?}, notice {:?}",
                    window.get_mods().notice
                ))
            }
        }),
    );
    outcomes = step(outcomes, "the menu lists the six pages", {
        let nav = window.get_mods_nav();
        let shown: Vec<String> = (0..nav.row_count())
            .filter_map(|row| nav.row_data(row))
            .map(|item| item.label.to_string())
            .collect();
        let wanted: Vec<String> = menu.iter().map(|(_, label)| (*label).to_owned()).collect();
        if shown == wanted {
            Ok(shown.join(", "))
        } else {
            Err(format!("the menu says {shown:?}"))
        }
    });
    // Every row of the menu, pressed in turn. A row is drawn inside the menu column, which is on
    // screen on every page, so this is the press a person makes from anywhere in the section. The
    // press and what it changes are not one instant: the page is drawn from a snapshot the app
    // posts from its own thread, so each is waited for the way a person waits for the page.
    for (id, label) in menu {
        outcomes = step(
            outcomes,
            &format!("{label} opens"),
            click(window, label)
                .and_then(|_| wait(window, Duration::from_secs(10), |_| page_is(id))),
        );
    }
    // And again on the page already on screen, after the page has been drawn again.
    outcomes = step(
        outcomes,
        "Themes opens once more",
        click(window, "Themes")
            .and_then(|_| wait(window, Duration::from_secs(10), |_| page_is("themes"))),
    );
    outcomes = step(
        outcomes,
        "the theme the folder holds has a card",
        wait(window, Duration::from_secs(15), |window| {
            let themes = window.get_mods().themes;
            let names: Vec<String> = (0..themes.row_count())
                .filter_map(|row| themes.row_data(row))
                .map(|card| card.name.to_string())
                .collect();
            if names == ["Red"] {
                Ok(names.join(", "))
            } else {
                Err(format!("the page shows {names:?}"))
            }
        }),
    );
    outcomes = step(outcomes, "the card starts out unarmed", {
        let themes = window.get_mods().themes;
        match themes.row_data(0) {
            Some(card) if !card.armed => Ok("armed: false".to_owned()),
            Some(card) => Err(format!("the card says armed: {}", card.armed)),
            None => Err("no theme card on the page".to_owned()),
        }
    });
    verdict("the Mods pages did not open", &outcomes);

    // ---- the first press arms the card and removes nothing -----------------------------------
    let mut first: Vec<Outcome> = Vec::new();
    first = step(
        first,
        "the Remove button is pressed, and nothing is deleted",
        click(window, "Remove red.theme.css").and_then(|_| {
            wait(window, Duration::from_secs(5), |window| {
                if !red.exists() {
                    return Err("the first press deleted the file".to_owned());
                }
                match window.get_mods().themes.row_data(0) {
                    Some(card) if card.armed => Ok("armed, the file is still there".to_owned()),
                    Some(card) => Err(format!("the card says armed: {}", card.armed)),
                    None => Err("the card left the page".to_owned()),
                }
            })
        }),
    );
    first = step(
        first,
        "and the button now asks for a confirmation",
        wait(window, Duration::from_secs(5), |window| {
            let names = ui_driver::labels(window);
            if names
                .iter()
                .any(|name| name == "Confirm removing red.theme.css")
            {
                Ok("Confirm removing red.theme.css".to_owned())
            } else {
                Err(format!("the buttons say {names:?}"))
            }
        }),
    );
    verdict("the first press did not arm the card", &first);

    // ---- the second press on the same card deletes the file ----------------------------------
    // What the page holds is read when the press lands, not after it. The file is deleted inside
    // the press, so a snapshot built afterwards cannot list the card whether the window was told
    // to draw it gone or not; the check is that the card that was there is not there now.
    let before: Vec<String> = on_screen(window);
    let pressed = click(window, "Confirm removing red.theme.css");
    let mut second: Vec<Outcome> = Vec::new();
    second = step(
        second,
        "the Confirm remove button is pressed, and the file goes",
        pressed.and_then(|_| {
            wait(window, Duration::from_secs(10), |_| {
                if red.exists() {
                    return Err("the file is still there".to_owned());
                }
                Ok("the file is gone".to_owned())
            })
        }),
    );
    second = step(
        second,
        "the card leaves the page",
        wait(window, Duration::from_secs(10), move |window| {
            let now = on_screen(window);
            if before == ["Red"] && now.is_empty() {
                Ok("the page is empty".to_owned())
            } else {
                Err(format!(
                    "the page listed {before:?} before the press and {now:?} after"
                ))
            }
        }),
    );
    second = step(
        second,
        "and the screen says what happened",
        wait(window, Duration::from_secs(10), |window| {
            let notice = window.get_mods().notice.to_string();
            if notice == "Removed red.theme.css." {
                Ok(notice)
            } else {
                Err(format!("the screen says {notice:?}"))
            }
        }),
    );
    verdict("the second press did not remove the theme", &second);

    let _ = std::fs::remove_dir_all(&data_dir);
}
