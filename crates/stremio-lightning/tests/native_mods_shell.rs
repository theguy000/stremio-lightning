//! The Mods section: its six pages, and the two pages that work offline.
//!
//! Three tests. The first reads the screen's data over a folder of real plugin and theme files
//! and presses its actions, so what a page lists and what a switch writes down is checked without
//! a window. The second hands a pressed screen's lists to `settings.json` and back, so the choice
//! a person made survives a restart. The third boots the app, presses the Mods tab, walks the six
//! pages, and removes a theme one press at a time, so the window is checked to draw what the
//! screen says and to delete nothing until the second press.

use std::path::{Path, PathBuf};
use std::time::Duration;

use slint::Model;
use stremio_lightning::screens::mods::{self, catalogue, Page};
use stremio_lightning::themes;
use stremio_lightning::{MainWindow, Section};

mod support;

use support::ui_driver::{self, Outcome, Step};

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
    assert_eq!(
        data.notice.text, "alpha.plugin.js is enabled. It will run once plugin support arrives.",
        "the screen says what the press did"
    );
    assert_eq!(
        settings(&data_dir).plugins,
        ["alpha.plugin.js"],
        "the switch writes the file name down"
    );
    let plugins = cards_of(&data, Page::Plugins);
    assert!(plugins[0].enabled, "and the card reads it back");

    mods::set_enabled(&mut data, &data_dir, "alpha.plugin.js", false);
    assert!(
        settings(&data_dir).plugins.is_empty(),
        "switching it off takes the name back out"
    );

    // ---- a theme's switch keeps one on at a time ------------------------------------------
    data = data.with_page(Page::Themes);
    mods::set_enabled(&mut data, &data_dir, "green.theme.css", true);
    mods::set_enabled(&mut data, &data_dir, "red.theme.css", true);
    assert_eq!(
        settings(&data_dir).themes,
        ["red.theme.css"],
        "turning a second theme on takes the first one off"
    );
    let themes = cards_of(&data, Page::Themes);
    assert_eq!(
        themes
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
    assert_eq!(data.notice.text, "", "the screen has something to say");
    mods::remove(&mut data, &data_dir, "red.theme.css");
    assert!(!red_file.exists(), "remove deletes the file");
    assert_eq!(
        data.notice.text, "Removed red.theme.css.",
        "and the screen says so"
    );
    assert!(
        settings(&data_dir).themes.is_empty(),
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

/// The choices a person makes are in `settings.json`, so the next start reads them back, and only
/// the theme named there is the one the window draws with.
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

    // A second read of the same folder, as a restart makes.
    let again = mods::data(&data_dir);
    assert_eq!(
        settings(&data_dir),
        LocalExtrasReading {
            plugins: vec!["keep.plugin.js".to_owned()],
            themes: vec!["blue.theme.css".to_owned()],
        },
        "the file names are what the settings hold"
    );
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
        settings(&data_dir).themes.is_empty(),
        "no theme is named after it is turned off"
    );

    let _ = std::fs::remove_dir_all(&data_dir);
}

/// The bits of this shell's `settings.json` the Mods screen writes, as a plain value so two
/// readings can be compared without depending on the file's other fields.
#[derive(Debug, PartialEq, Eq)]
struct LocalExtrasReading {
    plugins: Vec<String>,
    themes: Vec<String>,
}

/// Reads `settings.json` the way the app writes it. The screen's own readers are in the crate,
/// so the file is read here as the app's format: JSON, with the two lists by the names the screen
/// uses.
fn settings(data_dir: &Path) -> LocalExtrasReading {
    let path = data_dir.join("settings.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let json: serde_json::Value = serde_json::from_str(&text).expect("settings.json is JSON");
    let list = |key: &str| {
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
    LocalExtrasReading {
        plugins: list("enabled_plugins"),
        themes: list("enabled_themes"),
    }
}

#[test]
fn the_window_opens_mods_lists_its_pages_and_removes_a_theme_in_two_presses() {
    i_slint_backend_testing::init_integration_test_with_system_time();
    let data_dir = temp_dir("window");
    write_mod(&data_dir, "themes", "red.theme.css", &theme("Red"));
    let red = data_dir
        .join("stremio-lightning")
        .join("themes")
        .join("red.theme.css");
    assert!(red.exists(), "the theme is installed before the app opens");

    let booted = stremio_lightning::boot(data_dir.clone()).expect("the app boots");
    let window = &booted.window;

    let page_names = || {
        Box::new(|window: &MainWindow| {
            let nav = window.get_mods_nav();
            let names: Vec<String> = (0..nav.row_count())
                .filter_map(|row| nav.row_data(row))
                .map(|item| item.label.to_string())
                .collect();
            if names
                == [
                    "Plugins",
                    "Themes",
                    "Marketplace",
                    "Plugin settings",
                    "Logs",
                    "About",
                ]
            {
                Ok(names.join(", "))
            } else {
                Err(format!("the menu says {names:?}"))
            }
        }) as Box<dyn Fn(&MainWindow) -> Result<String, String>>
    };
    let page_is = |page: &'static str| {
        Box::new(move |window: &MainWindow| {
            // The page the window holds, and the page the screen is drawing: the menu marks the
            // row whose id is the screen's own page, so the two have to agree.
            let shown = window.get_mods_page().to_string();
            let drawn = window.get_mods().page.to_string();
            if shown == page && drawn == page {
                Ok(page.to_owned())
            } else {
                Err(format!(
                    "the window holds {shown} and the screen draws {drawn}"
                ))
            }
        }) as Box<dyn Fn(&MainWindow) -> Result<String, String>>
    };
    let armed_is = |armed: bool| {
        Box::new(move |window: &MainWindow| {
            let themes = window.get_mods().themes;
            match themes.row_data(0) {
                Some(card) if card.armed == armed => Ok(format!("armed: {}", card.armed)),
                Some(card) => Err(format!("the card says armed: {}", card.armed)),
                None => Err("no theme card on the page".to_owned()),
            }
        }) as Box<dyn Fn(&MainWindow) -> Result<String, String>>
    };

    let mut outcomes: Vec<Outcome> = ui_driver::run(
        window,
        vec![
            Step::Click("Mods".to_owned()),
            Step::Expect(
                "the Mods tab opens the Mods screen",
                Box::new(|window: &MainWindow| match window.get_section() {
                    Section::Mods => Ok("Mods".to_owned()),
                    other => Err(format!("the section is {other:?}")),
                }),
            ),
            Step::Expect("the menu lists the six pages", page_names()),
            Step::Expect("the Plugins page is on screen first", page_is("plugins")),
            // A second press on the row of the page already on screen, after the page has been
            // drawn again: a row is only ever pressed through the frame behind it.
            Step::Click("Plugins".to_owned()),
            Step::Expect(
                "a second press on its own row still opens it",
                page_is("plugins"),
            ),
            Step::Click("Themes".to_owned()),
            Step::Expect("Themes opens", page_is("themes")),
            Step::Click("Themes".to_owned()),
            Step::Expect(
                "and is still on screen after a second press",
                page_is("themes"),
            ),
            Step::Wait {
                what: "the theme the folder holds has a card",
                budget: Duration::from_secs(15),
                check: Box::new(|window: &MainWindow| {
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
            },
            Step::Expect("the card starts out unarmed", armed_is(false)),
        ],
    );
    print!("{}", ui_driver::report(&outcomes));

    // ---- the first press arms the card and removes nothing ------------------------------------
    outcomes = ui_driver::run(
        window,
        vec![
            Step::Click("Remove red.theme.css".to_owned()),
            Step::Expect("the first press arms the card", armed_is(true)),
            Step::Wait {
                what: "and the button now asks for a confirmation",
                budget: Duration::from_secs(5),
                check: Box::new(|window: &MainWindow| {
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
            },
        ],
    );
    print!("{}", ui_driver::report(&outcomes));
    assert!(
        red.exists(),
        "one press deletes nothing, and the file is still there"
    );
    assert_eq!(
        window.get_mods().themes.row_count(),
        1,
        "the card is still on the page"
    );

    // ---- the second press on the same card deletes the file -----------------------------------
    outcomes = ui_driver::run(
        window,
        vec![
            Step::Click("Confirm removing red.theme.css".to_owned()),
            Step::Wait {
                what: "the second press removes the file",
                budget: Duration::from_secs(10),
                check: Box::new(move |window: &MainWindow| {
                    if window.get_mods().themes.row_count() == 0 {
                        Ok("the page is empty".to_owned())
                    } else {
                        Err("the card is still on the page".to_owned())
                    }
                }),
            },
            Step::Expect(
                "and the screen says what happened",
                Box::new(|window: &MainWindow| {
                    let notice = window.get_mods().notice.to_string();
                    if notice == "Removed red.theme.css." {
                        Ok(notice)
                    } else {
                        Err(format!("the screen says {notice:?}"))
                    }
                }),
            ),
        ],
    );
    print!("{}", ui_driver::report(&outcomes));
    assert!(!red.exists(), "the second press deletes the file");

    let failed: Vec<&Outcome> = outcomes
        .iter()
        .filter(|outcome| outcome.ok == Some(false))
        .collect();
    assert!(
        failed.is_empty(),
        "the window did not do what was expected: {:?}",
        failed
            .iter()
            .map(|outcome| outcome.line())
            .collect::<Vec<_>>()
    );

    let _ = std::fs::remove_dir_all(&data_dir);
}
