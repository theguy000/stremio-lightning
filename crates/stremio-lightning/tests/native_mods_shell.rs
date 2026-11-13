//! The Mods section: built-in plugins, local `.theme.css` themes, settings, logs, and about.
//!
//! Four tests. The first reads the screen's data over built-in plugins and a folder of real theme
//! files and presses its actions, so what a page lists and what a switch writes down is checked
//! without a window. The second reads the choices back from `settings.json`, so a choice survives
//! a restart and only the theme it names is the one that colours the window. The third checks the
//! built-in plugin settings page edits and persistence. The fourth boots the app, opens every
//! menu page plus AniSkip's settings subpage, and removes a theme in two presses.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::json;
use slint::Model;
use stremio_lightning::screens::mods::{self, catalogue, Page};
use stremio_lightning::screens::settings::LocalExtras;
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
        "stremio-lightning-mods-{name}-{}",
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

fn theme(name: &str) -> String {
    format!(
        "/**\n * @name {name}\n * @description A colour change.\n * @author Someone\n * @version 1.0.0\n */\n.nav-tab-button {{\n  color: #ff0000;\n}}\n"
    )
}

fn cards_of(data: &mods::Data, page: Page) -> Vec<mods::ModCard> {
    data.clone().with_page(page).cards().to_vec()
}

fn names(cards: &[mods::ModCard]) -> Vec<String> {
    cards.iter().map(|card| card.name.clone()).collect()
}

fn file_names(cards: &[mods::ModCard]) -> Vec<String> {
    cards.iter().map(|card| card.filename.clone()).collect()
}

#[derive(Debug, PartialEq, Eq)]
struct SavedState {
    skip_intros: bool,
    themes: Vec<String>,
}

fn saved(data_dir: &Path) -> SavedState {
    let path = data_dir.join("settings.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let json: serde_json::Value = serde_json::from_str(&text).expect("settings.json is JSON");
    let skip_intros = json
        .get("skip_intros")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    let themes = json
        .get("enabled_themes")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    SavedState {
        skip_intros,
        themes,
    }
}

#[test]
fn the_screen_lists_what_is_installed_and_its_actions_reach_the_disk() {
    let data_dir = temp_dir("screen");
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
        ["builtin:aniskip"],
        "built-in plugins are listed on the Plugins page"
    );
    assert_eq!(names(&plugins), ["AniSkip"]);
    assert!(plugins[0].enabled, "AniSkip starts enabled by default");
    assert!(plugins[0].has_settings, "AniSkip exposes settings");
    assert!(!plugins[0].removable, "a built-in plugin cannot be removed");

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

    // ---- a built-in plugin's switch ------------------------------------------------------
    data = data.with_page(Page::Plugins);
    mods::set_enabled(&mut data, &data_dir, "builtin:aniskip", false);
    assert!(!saved(&data_dir).skip_intros);
    assert!(!cards_of(&data, Page::Plugins)[0].enabled);

    mods::set_enabled(&mut data, &data_dir, "builtin:aniskip", true);
    assert!(saved(&data_dir).skip_intros);
    assert!(cards_of(&data, Page::Plugins)[0].enabled);

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
    for page in [Page::Settings, Page::Logs, Page::About] {
        let page_data = data.clone().with_page(page);
        assert!(page_data.cards().is_empty(), "{} lists no cards", page.id());
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
    write_mod(&data_dir, "themes", "blue.theme.css", &theme("Blue"));

    let mut data = mods::data(&data_dir).with_page(Page::Plugins);
    mods::set_enabled(&mut data, &data_dir, "builtin:aniskip", false);
    let mut data = data.with_page(Page::Themes);
    mods::set_enabled(&mut data, &data_dir, "blue.theme.css", true);

    assert_eq!(
        saved(&data_dir),
        SavedState {
            skip_intros: false,
            themes: vec!["blue.theme.css".to_owned()],
        },
        "the choices are what the settings hold"
    );

    // A second read of the same folder, as a restart makes.
    let again = mods::data(&data_dir);
    assert!(
        !cards_of(&again, Page::Plugins)[0].enabled,
        "AniSkip stays off across restart"
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

#[test]
fn the_settings_page_saves_builtin_settings_only_on_save() {
    let data_dir = temp_dir("settings");
    let mut extras = LocalExtras::default();

    let listed = mods::settings::plugins(&data_dir, &extras).expect("built-in plugins list");
    let ids: Vec<&str> = listed.iter().map(|p| p.filename.as_str()).collect();
    assert_eq!(ids, ["builtin:aniskip"]);
    assert_eq!(listed[0].name, "AniSkip");
    assert_eq!(listed[0].fields, 3);

    let mut edits = mods::settings::edits(&data_dir, "builtin:aniskip", &extras);
    let keys: Vec<&str> = edits.controls.iter().map(|c| c.field.key()).collect();
    assert_eq!(keys, ["provider_name", "submitter_id", "submit_button"]);
    assert!(!edits.dirty(), "nothing is typed yet");

    // ---- typing changes the page and not the settings --------------------------------------
    mods::settings::set_value(&mut edits, "provider_name", &json!("tester"));
    mods::settings::set_value(&mut edits, "submit_button", &json!(false));
    assert!(edits.dirty());
    assert!(extras.builtin_settings.is_empty(), "editing writes nothing");

    // ---- leaving drops unsaved edits -------------------------------------------------------
    edits = mods::settings::Edits::default();
    assert!(!edits.dirty());

    // ---- Save writes into LocalExtras and persists to disk ---------------------------------
    edits = mods::settings::edits(&data_dir, "builtin:aniskip", &extras);
    mods::settings::set_value(&mut edits, "provider_name", &json!("tester"));
    mods::settings::set_value(&mut edits, "submit_button", &json!(false));
    let saved_out = mods::settings::save(&data_dir, &mut edits, &mut extras);
    assert!(saved_out.written, "save succeeded: {}", saved_out.error);
    assert!(!edits.dirty(), "the page and saved state agree after Save");
    std::fs::write(
        data_dir.join("settings.json"),
        serde_json::to_string_pretty(&extras).expect("LocalExtras serializes"),
    )
    .expect("settings.json writes");

    let raw = std::fs::read_to_string(data_dir.join("settings.json")).expect("settings.json reads");
    let reloaded: LocalExtras = serde_json::from_str(&raw).expect("LocalExtras deserializes");
    let reloaded_edits = mods::settings::edits(&data_dir, "builtin:aniskip", &reloaded);
    assert_eq!(reloaded_edits.controls[0].stored, json!("tester"));
    assert_eq!(reloaded_edits.controls[2].stored, json!(false));

    let _ = std::fs::remove_dir_all(&data_dir);
}

#[test]
fn the_window_opens_every_page_and_removes_a_theme_in_two_presses() {
    // Mock time, not system time: the clock this runner reports is not monotonic, and Slint
    // asserts on a tick that goes backwards as soon as any animation is live.
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let data_dir = temp_dir("window");
    write_mod(&data_dir, "themes", "red.theme.css", &theme("Red"));
    let red = data_dir
        .join("stremio-lightning")
        .join("themes")
        .join("red.theme.css");
    assert!(red.exists(), "the theme is installed before the app opens");

    let booted = stremio_lightning::boot(data_dir.clone()).expect("the app boots");
    let window = &booted.window;
    ui_driver::pump_for(window);

    let menu = [
        ("plugins", "Plugins"),
        ("themes", "Themes"),
        ("logs", "Logs"),
        ("about", "About"),
    ];
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
        "the Plugins page lists built-in AniSkip",
        wait(window, Duration::from_secs(10), |window| {
            let plugins = window.get_mods().plugins;
            let ids: Vec<String> = (0..plugins.row_count())
                .filter_map(|row| plugins.row_data(row))
                .map(|card| card.filename.to_string())
                .collect();
            if ids == ["builtin:aniskip"] {
                Ok(ids.join(", "))
            } else {
                Err(format!("the page lists {ids:?}"))
            }
        }),
    );
    outcomes = step(
        outcomes,
        "AniSkip's gear opens the Plugin settings subpage and Back returns to Plugins",
        click(window, "Settings for builtin:aniskip")
            .and_then(|_| wait(window, Duration::from_secs(10), |_| page_is("settings")))
            .and_then(|_| click(window, "Back to plugins"))
            .and_then(|_| wait(window, Duration::from_secs(10), |_| page_is("plugins"))),
    );
    outcomes = step(outcomes, "the menu lists the four pages", {
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
    for (id, label) in menu {
        outcomes = step(
            outcomes,
            &format!("{label} opens"),
            click(window, label)
                .and_then(|_| wait(window, Duration::from_secs(10), |_| page_is(id))),
        );
    }
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
