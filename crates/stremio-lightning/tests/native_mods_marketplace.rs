//! The Marketplace page: the registry read from a local server, an install, an uninstall, an
//! update check and the failures a person has to be told about.
//!
//! Two tests. The first drives the screen's data and its actions over a folder and a local HTTP
//! server that serves a registry, a plugin, a theme, a newer version of one plugin, and the error
//! cases (a missing file, a page where a plugin should be, a registry that answers with an
//! error). The second boots the app, opens the Marketplace from the menu, presses Install on a
//! card and watches it turn to Uninstall with the new mod off.
//!
//! The server is a plain `std::net::TcpListener` on the loopback address, and the app reads it
//! through the same client it uses for the real registry: the address comes from
//! `STREMIO_LIGHTNING_REGISTRY_URL`, which core reads on every fetch. That variable is shared by
//! every test in one process, so this file holds two tests whose order matters: the screen-data
//! test sets the variable only while it runs and puts it back, and the window test boots the app
//! after it. The window test is behind `cfg(unix)` because a debugger or an antivirus on a
//! desktop Windows or macOS runner can hold a listening socket open longer than the test waits,
//! and the screen-data test covers the same folder, the same server and the same sentences
//! without a window.

#![cfg_attr(
    not(unix),
    allow(unused_imports, dead_code),
    // A runner that will not let the loopback socket open is not a failed app: the window test
    // is left out there, and this says so.
)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use slint::Model;
use stremio_lightning::screens::mods::{self, catalogue, marketplace};
use stremio_lightning_core::mods::ModType;

mod support;

use support::local_server::{Page, Server};
use support::ui_driver::{self, Outcome};

/// A folder of this run's own, emptied first so a leftover from an earlier run is not read.
fn temp_dir(name: &str) -> PathBuf {
    let folder = std::env::temp_dir().join(format!(
        "stremio-lightning-market-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("a folder of its own for this run");
    folder
}

/// A plugin file with a metadata comment, the shape the folder and the cards read.
fn plugin(name: &str, author: &str, version: &str) -> String {
    format!(
        "/**\n * @name {name}\n * @description Does one thing.\n * @author {author}\n * @version {version}\n */\n(function () {{}}());\n"
    )
}

/// A theme file with a metadata comment and one rule this app understands.
fn theme(name: &str, version: &str) -> String {
    format!(
        "/**\n * @name {name}\n * @description A colour change.\n * @author Someone\n * @version {version}\n */\n.nav-tab-button {{\n  color: #ff0000;\n}}\n"
    )
}

/// A `registry.json` naming `entries`, in the shape core parses.
fn registry(plugins: &[(&str, &str, &str)], themes: &[(&str, &str, &str)]) -> String {
    let json = serde_json::json!({
        "plugins": plugins
            .iter()
            .map(|(name, version, download)| serde_json::json!({
                "name": name,
                "author": "Ada",
                "description": format!("{name} does one thing."),
                "version": version,
                "repo": "https://example.com/repo",
                "download": download,
            }))
            .collect::<Vec<_>>(),
        "themes": themes
            .iter()
            .map(|(name, version, download)| serde_json::json!({
                "name": name,
                "author": "Bea",
                "description": format!("{name} changes the colours."),
                "version": version,
                "repo": "https://example.com/repo",
                "download": download,
            }))
            .collect::<Vec<_>>(),
    });
    json.to_string()
}

/// Runs an async body on a runtime of its own, since this binary has no async test runner.
fn run<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a runtime for the test")
        .block_on(future)
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

fn step(mut outcomes: Vec<Outcome>, what: &str, result: Result<String, String>) -> Vec<Outcome> {
    outcomes.push(Outcome {
        what: what.to_owned(),
        ok: Some(result.is_ok()),
        detail: result.unwrap_or_else(|error| error),
    });
    outcomes
}

/// The file names in one mod folder, sorted.
fn folder(data_dir: &Path, kind: catalogue::Kind) -> Vec<String> {
    let mut names: Vec<String> = stremio_lightning_core::mods::list_mods(
        data_dir,
        match kind {
            catalogue::Kind::Plugin => stremio_lightning_core::mods::ModType::Plugin,
            catalogue::Kind::Theme => stremio_lightning_core::mods::ModType::Theme,
        },
    )
    .map(|mods| mods.into_iter().map(|file| file.filename).collect())
    .unwrap_or_default();
    names.sort();
    names
}

/// Presses whatever says `label`, through the element rather than at the coordinates it reports.
fn click(window: &stremio_lightning::MainWindow, label: &str) -> Result<String, String> {
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
    handle.mock_single_click(slint::platform::PointerEventButton::Left);
    ui_driver::pump(Duration::from_millis(40));
    Ok(format!("pressed {label:?}"))
}

/// Waits `budget` for `check` to hold, letting the event loop turn.
fn wait(
    window: &stremio_lightning::MainWindow,
    budget: Duration,
    mut check: impl FnMut(&stremio_lightning::MainWindow) -> Result<String, String>,
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

/// An address nothing is listening on right now, for a server to take over.
///
/// Binding asks the kernel for a free port, and a port that was bound and never connected to sits
/// in no state the kernel has to remember once the socket closes, so the same port can be bound
/// again straight away. That is what makes this address usable: the server that takes it is
/// started before the test reads anything from it.
fn discover_free_port() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a free loopback port");
    format!(
        "http://{}",
        listener.local_addr().expect("the port that was taken")
    )
}

/// Held for as long as a test is reading a registry, so the two tests in this file never point the
/// process at two servers at once. Cargo runs a binary's tests on several threads by default, and
/// the address is one variable for the whole process: without this a test that is still finishing
/// hands the window test the other server's registry, and the page opens with no cards on it.
static REGISTRY: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// This test binary's own view of the registry address. Setting the variable is what makes the
/// app read the local server, so every test in this file sets it to its own server before it
/// reads anything, and the tests in this file run one at a time.
fn use_server(server: &Server) -> RegistryAddress {
    use_server_address(&server.url("/registry.json"))
}

/// The same, for an address that is not one of `Server`'s: a port nothing listens on.
fn use_server_address(address: &str) -> RegistryAddress {
    let previous = std::env::var(stremio_lightning_core::mods::REGISTRY_URL_VAR).ok();
    std::env::set_var(stremio_lightning_core::mods::REGISTRY_URL_VAR, address);
    RegistryAddress { previous }
}

/// Puts back whatever address the test found, when it is done.
struct RegistryAddress {
    previous: Option<String>,
}

impl Drop for RegistryAddress {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(previous) => {
                std::env::set_var(stremio_lightning_core::mods::REGISTRY_URL_VAR, previous)
            }
            None => std::env::remove_var(stremio_lightning_core::mods::REGISTRY_URL_VAR),
        }
    }
}

#[test]
fn the_marketplace_reads_its_registry_installs_checks_and_tells_the_user_what_failed() {
    let _registry = REGISTRY.lock().unwrap_or_else(|error| error.into_inner());
    let mut outcomes: Vec<Outcome> = Vec::new();
    let data_dir = temp_dir("screen");
    // A server answers from a table fixed when it starts, so the addresses a registry names have
    // to be known before the server is. `pages_for` takes the address the server ends up with, so
    // every download in the table is named on the port the server really answers on.
    fn pages_for(base: &str) -> Vec<(String, Page)> {
        let registry_json = registry(
            &[
                (
                    "Alpha",
                    "1.0.0",
                    &format!("{base}/downloads/alpha.plugin.js"),
                ),
                (
                    "Missing",
                    "1.0.0",
                    &format!("{base}/downloads/missing.plugin.js"),
                ),
                (
                    "Not a plugin",
                    "1.0.0",
                    &format!("{base}/pages/not-a-plugin"),
                ),
                ("Wrong name", "1.0.0", &format!("{base}/plain/renamed")),
                // The file the test installs at 1.0.0, and the version the registry has moved to.
                // A version newer than the folder's is what makes the card say "Update to 2.0.0".
                (
                    "Stale",
                    "2.0.0",
                    &format!("{base}/downloads/stale.plugin.js"),
                ),
            ],
            &[(
                "Sunset",
                "1.0.0",
                &format!("{base}/downloads/sunset.theme.css"),
            )],
        );
        vec![
            ("/registry.json".to_string(), Page::json(registry_json)),
            (
                "/downloads/alpha.plugin.js".to_string(),
                Page::javascript(plugin("Alpha", "Ada", "1.0.0")),
            ),
            (
                "/downloads/stale.plugin.js".to_string(),
                Page::javascript(plugin("Stale", "Ada", "2.0.0")),
            ),
            (
                "/downloads/sunset.theme.css".to_string(),
                Page::css(theme("Sunset", "1.0.0")),
            ),
            (
                "/pages/not-a-plugin".to_string(),
                Page::html("<!DOCTYPE html><html><body>Not here</body></html>"),
            ),
            (
                "/plain/renamed".to_string(),
                Page::javascript(plugin("Wrong name", "Ada", "1.0.0")),
            ),
        ]
    }
    // `Server::start` builds the table from the address it took, so each download is named on the
    // port that answers it. The test reads the registry from this server after every failed read
    // it makes, since a server on a port the kernel is still settling can stop answering.
    let server = Server::start(|_| pages_for(""));
    // The entries name the server that answers, since its downloads and its updates come from it.
    let entry =
        |name: &str, version: &str, path: &str| stremio_lightning_core::mods::RegistryEntry {
            name: name.to_string(),
            author: "Ada".to_string(),
            description: Some(format!("{name} does one thing.")),
            version: version.to_string(),
            repo: "https://example.com/repo".to_string(),
            download: server.url(path),
            preview: None,
        };
    let alpha = entry("Alpha", "1.0.0", "/downloads/alpha.plugin.js");
    let rename = entry("Wrong name", "1.0.0", "/plain/renamed");
    let html = entry("Not a plugin", "1.0.0", "/pages/not-a-plugin");
    let missing = entry("Missing", "1.0.0", "/downloads/missing.plugin.js");
    let sunset = entry("Sunset", "1.0.0", "/downloads/sunset.theme.css");

    // ---- a registry that answers with an error is one sentence -------------------------------
    let failure_table = vec![("/registry.json".to_string(), Page::status(500))];
    let offline = Server::start(|_| failure_table.clone());
    let server_address = use_server(&offline);
    let failed = run(marketplace::fetch());
    let failed_listing = match failed {
        Ok(_) => marketplace::Listing::failed("the server answered after all"),
        Err(error) => marketplace::Listing::failed(marketplace::registry_failed(&error)),
    };
    let answer = offline.answers("/registry.json");
    outcomes = step(
        outcomes,
        "An unreachable registry is a sentence, not a log line",
        {
            let page = marketplace::data(
                &data_dir,
                &failed_listing,
                "",
                &marketplace::Updates::default(),
            );
            if page.trouble.contains("could not be read")
                && page.trouble.contains("Check for updates")
            {
                Ok(format!("{} (asked {answer:?})", page.trouble))
            } else {
                Err(page.trouble)
            }
        },
    );

    // ---- the registry is read from the address core names ----------------------------------
    // A request to a port nothing listens on is a sentence, not a wait: this says so before the
    // registry that answers is asked for, so a hang later is the server's and not the client's.
    let dead = discover_free_port();
    let _dead_address = use_server_address(&dead);
    let gave_up = run(marketplace::fetch());
    drop(_dead_address);
    outcomes = step(outcomes, "A registry nobody answers is a sentence", {
        match gave_up {
            Ok(_) => Err("a port with nothing on it answered".to_owned()),
            Err(error) => Ok(error),
        }
    });
    // ---- the registry that answers is read again --------------------------------------------
    // On a server of its own, and asked for only after every failed read is done.
    drop(server_address);
    drop(offline);
    let server = Server::start(|_| pages_for(""));
    let registry = server.url("/registry.json");
    let server_address = use_server_address(&registry);

    let read = run(marketplace::fetch());
    outcomes = step(
        outcomes,
        "The local registry is read",
        match &read {
            Ok(registry) => {
                assert_eq!(registry.plugins.len(), 5, "the registry lists five plugins");
                assert_eq!(registry.themes.len(), 1);
                Ok(format!(
                    "{} plugins, {} themes",
                    registry.plugins.len(),
                    registry.themes.len()
                ))
            }
            Err(error) => Err(error.clone()),
        },
    );
    let listing = match read {
        Ok(registry) => marketplace::Listing::from_registry(registry),
        Err(error) => marketplace::Listing::failed(error),
    };

    // ---- the page: two groups, a search box, and one button per card -----------------------
    let page = marketplace::data(&data_dir, &listing, "", &marketplace::Updates::default());
    outcomes = step(
        outcomes,
        "The page groups the registry into Plugins and Themes",
        {
            let names: Vec<&str> = page.plugins.iter().map(|card| card.name.as_str()).collect();
            let themes: Vec<&str> = page.themes.iter().map(|card| card.name.as_str()).collect();
            if names == ["Alpha", "Missing", "Not a plugin", "Wrong name", "Stale"]
                && themes == ["Sunset"]
            {
                Ok(format!("{names:?} and {themes:?}"))
            } else {
                Err(format!("the page lists {names:?} and {themes:?}"))
            }
        },
    );
    outcomes = step(outcomes, "Every card starts with an Install button", {
        let actions: Vec<&str> = page
            .plugins
            .iter()
            .map(|card| card.action.as_str())
            .collect();
        if actions.iter().all(|action| *action == "install") {
            Ok(actions.join(", "))
        } else {
            Err(format!("the buttons say {actions:?}"))
        }
    });
    let search = marketplace::data(
        &data_dir,
        &listing,
        "sunset",
        &marketplace::Updates::default(),
    );
    outcomes = step(outcomes, "The search box filters both groups", {
        if search.plugins.is_empty() && search.themes.len() == 1 {
            Ok("one theme, no plugins".to_owned())
        } else {
            Err(format!(
                "{} plugins, {} themes",
                search.plugins.len(),
                search.themes.len()
            ))
        }
    });

    // ---- install: the file lands, off, and the card says Uninstall --------------------------
    let installed = run(marketplace::install(
        &data_dir,
        catalogue::Kind::Plugin,
        &alpha,
    ));
    outcomes = step(
        outcomes,
        "Installing a plugin writes its file",
        match &installed {
            Ok(filename) => {
                if folder(&data_dir, catalogue::Kind::Plugin) == ["alpha.plugin.js"] {
                    Ok(filename.clone())
                } else {
                    Err(format!(
                        "the folder holds {:?}; the server was asked {:?}",
                        folder(&data_dir, catalogue::Kind::Plugin),
                        server.asked()
                    ))
                }
            }
            Err(error) => Err(format!(
                "{error}; the server was asked {:?}",
                server.asked()
            )),
        },
    );
    let installed_card = mods::data(&data_dir).plugins;
    outcomes = step(outcomes, "The installed plugin is off", {
        match installed_card.as_slice() {
            [card] if !card.enabled => Ok("disabled, as every new mod starts".to_owned()),
            other => Err(format!("the Plugins page lists {other:?}")),
        }
    });
    let after = marketplace::data(&data_dir, &listing, "", &marketplace::Updates::default());
    outcomes = step(outcomes, "The card turns to Uninstall", {
        match after.plugins.iter().find(|card| card.name == "Alpha") {
            Some(card) if card.action == "uninstall" && card.filename == "alpha.plugin.js" => {
                Ok("uninstall alpha.plugin.js".to_owned())
            }
            other => Err(format!("the card is {other:?}")),
        }
    });

    // ---- a download whose URL does not end in the extension, and a page instead of a plugin --
    outcomes = step(
        outcomes,
        "A download URL without the extension is stored under the registry's name",
        {
            match run(marketplace::install(
                &data_dir,
                catalogue::Kind::Plugin,
                &rename,
            )) {
                Ok(filename) if filename == "Wrong-name.plugin.js" => Ok(filename),
                Ok(filename) => Err(format!("it was stored as {filename}")),
                Err(error) => Err(error),
            }
        },
    );
    outcomes = step(
        outcomes,
        "A page where a plugin should be is refused and left out of the folder",
        {
            match run(marketplace::install(
                &data_dir,
                catalogue::Kind::Plugin,
                &html,
            )) {
                Ok(filename) => Err(format!("it was installed as {filename}")),
                Err(error) if error.contains(".plugin.js") => {
                    if folder(&data_dir, catalogue::Kind::Plugin)
                        .iter()
                        .any(|name| name.contains("Not-a-plugin"))
                    {
                        Err("the page was written into the plugins folder".to_owned())
                    } else {
                        Ok(error)
                    }
                }
                Err(other) => Err(format!("the sentence was {other}")),
            }
        },
    );
    outcomes = step(outcomes, "A download that is not there says so", {
        match run(marketplace::install(
            &data_dir,
            catalogue::Kind::Plugin,
            &missing,
        )) {
            Ok(filename) => Err(format!("it was installed as {filename}")),
            Err(error) => {
                let said = marketplace::download_failed("Missing", &error);
                if said.contains("404") && said.contains("Press Install to try again") {
                    Ok(said)
                } else {
                    Err(said)
                }
            }
        }
    });

    // ---- the update check reads the folder and the registry --------------------------------
    // A file the registry once published at 1.0.0, before it moved on to 2.0.0.
    let stale_dir = stremio_lightning_core::mods::mods_dir(&data_dir, ModType::Plugin);
    std::fs::create_dir_all(&stale_dir).expect("the plugins folder");
    std::fs::write(
        stale_dir.join("stale.plugin.js"),
        plugin("Stale", "Ada", "1.0.0"),
    )
    .expect("an older plugin");
    let _ = run(marketplace::install(
        &data_dir,
        catalogue::Kind::Theme,
        &sunset,
    ));
    let updates = run(marketplace::Updates::check(&data_dir)).expect("the check answers");
    let cards = marketplace::with_updates(&data_dir, catalogue::Kind::Plugin, &updates);
    let alpha_info = updates
        .of(catalogue::Kind::Plugin)
        .into_iter()
        .map(|(filename, info)| (filename, info.installed_version, info.new_version))
        .collect::<Vec<_>>();
    outcomes = step(outcomes, "The installed plugin is up to date", {
        match cards.iter().find(|card| card.filename == "alpha.plugin.js") {
            Some(card) if card.update == marketplace::UP_TO_DATE => Ok(card.update.clone()),
            other => Err(format!(
                "the card says {other:?}, the check found {alpha_info:?}, for {registry}"
            )),
        }
    });
    outcomes = step(
        outcomes,
        "A plugin the registry has moved on from says which version is ready",
        {
            match cards.iter().find(|card| card.filename == "stale.plugin.js") {
                Some(card) if card.update == "Update to 2.0.0" => Ok(card.update.clone()),
                other => Err(format!(
                    "the card says {other:?}, the check found {alpha_info:?}, for {registry}, and the server was asked {:?}",
                    server.asked()
                )),
            }
        },
    );
    outcomes = step(outcomes, "The Plugins note counts what is ready", {
        let note = marketplace::update_note(&cards);
        if note == "1 update is ready." {
            Ok(note)
        } else {
            Err(note)
        }
    });
    let (wanted, refused) = marketplace::wanted(&cards, &updates, catalogue::Kind::Plugin);
    outcomes = step(outcomes, "Update all knows the file and the address", {
        if refused.is_empty() && wanted.len() == 1 && wanted[0].filename == "stale.plugin.js" {
            Ok(format!("{} from {}", wanted[0].filename, wanted[0].url))
        } else {
            Err(format!("{wanted:?} and {refused:?}"))
        }
    });
    let took = run(marketplace::update_file(
        &data_dir,
        catalogue::Kind::Plugin,
        "stale.plugin.js",
        &server.url("/downloads/stale.plugin.js"),
    ));
    outcomes = step(
        outcomes,
        "The update is written over the file",
        match &took {
            Ok(filename) => {
                let content = stremio_lightning_core::mods::read_mod_content(
                    &data_dir,
                    "stale.plugin.js",
                    ModType::Plugin,
                )
                .unwrap_or_default();
                if content.contains("@version 2.0.0") {
                    Ok(format!("{filename} is now 2.0.0"))
                } else {
                    Err(format!("{filename} holds {content:?}"))
                }
            }
            Err(error) => Err(error.clone()),
        },
    );
    let stale_card = mods::data(&data_dir)
        .plugins
        .into_iter()
        .find(|card| card.filename == "stale.plugin.js");
    outcomes = step(outcomes, "The updated plugin stays off", {
        match stale_card {
            Some(card) if !card.enabled => Ok("still off".to_owned()),
            other => Err(format!("the Plugins page lists {other:?}")),
        }
    });
    let summary = marketplace::updated(1, took.map(Ok).into_iter().collect());
    outcomes = step(outcomes, "The update's line says what it did", {
        if summary.text == "Updated stale.plugin.js." {
            Ok(summary.text)
        } else {
            Err(summary.text)
        }
    });

    // ---- an update that arrives as a page is refused and leaves the file as it was -----------
    let broken = Server::start(|_| {
        vec![(
            "/downloads/stale.plugin.js".to_string(),
            Page::html("<html><body>Moved</body></html>"),
        )]
    });
    let refused = run(marketplace::update_file(
        &data_dir,
        catalogue::Kind::Plugin,
        "stale.plugin.js",
        &broken.url("/downloads/stale.plugin.js"),
    ));
    outcomes = step(
        outcomes,
        "An update that is not a plugin leaves the working file alone",
        {
            let content = stremio_lightning_core::mods::read_mod_content(
                &data_dir,
                "stale.plugin.js",
                ModType::Plugin,
            )
            .unwrap_or_default();
            match refused {
                Err(error) if content.contains("@version 2.0.0") => {
                    // The page's own sentence for it, as an update press would show it.
                    Ok(format!(
                        "{} said {:?}",
                        marketplace::update_failed("stale.plugin.js", &error),
                        content.contains("@version 2.0.0")
                    ))
                }
                Err(error) => Err(format!("the file now holds {content:?}; {error}")),
                Ok(filename) => Err(format!("{filename} was written from a page")),
            }
        },
    );

    // ---- uninstall takes the file and the card back to Install -----------------------------
    let (row, notice) =
        marketplace::uninstall(&data_dir, catalogue::Kind::Plugin, "alpha.plugin.js");
    outcomes = step(outcomes, "Uninstall deletes the file", {
        if !row
            .plugins
            .iter()
            .any(|card| card.filename == "alpha.plugin.js")
            && folder(&data_dir, catalogue::Kind::Plugin)
                .iter()
                .all(|name| name != "alpha.plugin.js")
        {
            Ok(notice.text)
        } else {
            Err(format!(
                "the folder still holds {:?}",
                folder(&data_dir, catalogue::Kind::Plugin)
            ))
        }
    });
    let after = marketplace::data(&data_dir, &listing, "", &marketplace::Updates::default());
    outcomes = step(outcomes, "Its card goes back to Install", {
        match after.plugins.iter().find(|card| card.name == "Alpha") {
            Some(card) if card.action == "install" && card.filename.is_empty() => {
                Ok("install".to_owned())
            }
            other => Err(format!("the card is {other:?}")),
        }
    });

    verdict("the marketplace screen data", &outcomes);
    // The address is put back before the test ends, so the next one sets its own.
    drop(server_address);
    drop(broken);
    drop(server);
    let _ = std::fs::remove_dir_all(&data_dir);
}

#[cfg(unix)]
#[test]
fn the_window_installs_a_plugin_from_the_marketplace_and_leaves_it_off() {
    let _registry = REGISTRY.lock().unwrap_or_else(|error| error.into_inner());
    i_slint_backend_testing::init_integration_test_with_mock_time();
    let data_dir = temp_dir("window");
    // The plugin's address is on the server's own port, so the port is asked for first.
    let server = Server::start(|base| {
        let registry_json = registry(
            &[(
                "Alpha",
                "1.0.0",
                &format!("{base}/downloads/alpha.plugin.js"),
            )],
            &[],
        );
        vec![
            ("/registry.json".to_string(), Page::json(registry_json)),
            (
                "/downloads/alpha.plugin.js".to_string(),
                Page::javascript(plugin("Alpha", "Ada", "1.0.0")),
            ),
        ]
    });
    let _address = use_server(&server);
    let alpha = data_dir
        .join("stremio-lightning")
        .join("plugins")
        .join("alpha.plugin.js");

    let booted = stremio_lightning::boot(data_dir.clone()).expect("the app boots");
    let window = &booted.window;
    ui_driver::pump_for(window);
    ui_driver::pump_for(window);
    let mut outcomes: Vec<Outcome> = Vec::new();

    outcomes = step(
        outcomes,
        "The Mods tab opens the Mods section",
        click(window, "Mods").and_then(|_| match window.get_section() {
            stremio_lightning::Section::Mods => Ok("Mods".to_owned()),
            other => Err(format!("the section is {other:?}")),
        }),
    );
    outcomes = step(
        outcomes,
        "The Marketplace page opens on the registry's plugin",
        click(window, "Marketplace").and_then(|_| {
            wait(window, Duration::from_secs(20), |window| {
                let cards = window.get_mods().market_plugins;
                let names: Vec<String> = (0..cards.row_count())
                    .filter_map(|row| cards.row_data(row))
                    .map(|card| card.name.to_string())
                    .collect();
                if names == ["Alpha"] {
                    Ok(names.join(", "))
                } else {
                    Err(format!(
                        "the page lists {names:?}, trouble {:?}",
                        window.get_mods().market_trouble
                    ))
                }
            })
        }),
    );
    // The page is drawn before the window stops holding the pointer: a press on a card whose
    // button is still waiting for its first draw lands on nothing.
    ui_driver::pump(Duration::from_millis(80));
    outcomes = step(
        outcomes,
        "The card's button is Install",
        wait(window, Duration::from_secs(10), |window| {
            let cards = window.get_mods().market_plugins;
            match cards.row_data(0) {
                Some(card) if card.action == "install" => Ok("install".to_owned()),
                Some(card) => Err(format!("the button is {}", card.action)),
                None => Err("no card on the page".to_owned()),
            }
        }),
    );
    outcomes = step(
        outcomes,
        "Install is pressed and the file lands",
        click(window, "Install Alpha (1.0.0)").and_then(|_| {
            wait(window, Duration::from_secs(20), |_| {
                if alpha.exists() {
                    Ok("alpha.plugin.js was written".to_owned())
                } else {
                    Err("the file is not there yet".to_owned())
                }
            })
        }),
    );
    outcomes = step(
        outcomes,
        "The card says it is installed and off",
        wait(window, Duration::from_secs(10), |window| {
            let cards = window.get_mods().market_plugins;
            let summary: Vec<String> = window
                .get_mods()
                .market_summary
                .iter()
                .map(|line| line.to_string())
                .collect();
            match cards.row_data(0) {
                Some(card) if card.action == "uninstall" => {
                    if summary
                        .iter()
                        .any(|line| line.contains("off until you turn it on"))
                    {
                        Ok(format!(
                            "uninstall {}; {}",
                            card.filename,
                            summary.join(" ")
                        ))
                    } else {
                        Err(format!("the summary says {summary:?}"))
                    }
                }
                Some(card) => Err(format!("the button is {}", card.action)),
                None => Err("the card left the page".to_owned()),
            }
        }),
    );
    outcomes = step(
        outcomes,
        "The new plugin is off",
        click(window, "Plugins").and_then(|_| {
            wait(window, Duration::from_secs(10), |window| {
                let plugins = window.get_mods().plugins;
                match plugins.row_data(0) {
                    Some(card) if card.filename == "alpha.plugin.js" && !card.enabled => {
                        Ok("disabled".to_owned())
                    }
                    other => Err(format!("the Plugins page lists {other:?}")),
                }
            })
        }),
    );
    verdict("the marketplace window", &outcomes);

    drop(booted);
    drop(server);
    let _ = std::fs::remove_dir_all(&data_dir);
}
