//! The Community marketplace's addons, installed through the window and then asked for the one
//! thing each of them exists to provide.
//!
//! Nothing here reaches for the core: every install is a press on a label a person could read
//! ("Add addon", "Install the addon"), and every expectation is read back from the window or from
//! the profile the app itself wrote to disk. The app runs on a profile of its own in a temp folder
//! with nobody signed in, so the addons of whoever runs this are untouched.
//!
//! The network is real -- what the addons answer is the point -- so this test is `#[ignore]`d and
//! run on purpose:
//!
//! ```text
//! cargo test -p stremio-lightning --test addons_live_ui -- --ignored --nocapture
//! ```
//!
//! The catalog dropdown itself is a `PopupWindow`, and its rows live in an item tree the testing
//! backend's element search does not reach (it walks the component's own tree). So the marketplace
//! list is judged where it can be: the menu must offer the Community catalog, and the addons come
//! from Stremio's own community endpoint, the same JSON that catalog answers with. The installs go
//! through the app's Add-addon dialog, which is the same `InstallAddon` action a marketplace card
//! presses.

mod support;

use std::path::{Path, PathBuf};
use std::time::Duration;

use slint::Model;
use stremio_lightning::{AddonData, MainWindow, Section};

use support::ui_driver::{self, Outcome, Step};

/// What the Community catalog is served from: Cinemeta declares it as one of its `addonCatalogs`,
/// and the core requests it by exactly this path.
const COMMUNITY_LIST: &str = "https://v3-cinemeta.strem.io/addon_catalog/all/community.json";

/// The first ten the Community catalog lists, in the order it lists them. Stremio publishes no
/// install count, so the order of its own list is the only ordering there is.
const TOP_OF_THE_LIST: usize = 10;

/// Ten more people actually install, kept because between them they offer every kind of resource a
/// card can be pressed for: catalogs, metadata, streams, subtitles, and the ones that have to be
/// configured before they work.
const ALSO_WORTH_PRESSING: [&str; 10] = [
    "Anime Kitsu",
    "IMDb Ratings",
    "TMDB Collections",
    "Cinemeta (with ratings)",
    "Streaming Catalogs",
    "The Movie Database Addon",
    "Community OpenSubtitles",
    "SubDL Subtitles",
    "Trakt Tv",
    "Watch Calendar",
];

fn cards(window: &MainWindow) -> Vec<AddonData> {
    let model = window.get_addons();
    (0..model.row_count())
        .filter_map(|row| model.row_data(row))
        .collect()
}

fn named(window: &MainWindow, name: &str) -> Option<AddonData> {
    cards(window).into_iter().find(|card| card.name == name)
}

fn is_installed(window: &MainWindow, name: &str) -> bool {
    named(window, name).is_some_and(|card| card.installed)
}

/// The addons the app itself wrote to its profile, which is what "installed" means on disk.
fn installed_on_disk(folder: &Path) -> Vec<String> {
    let text = std::fs::read_to_string(folder.join("profile.json")).unwrap_or_default();
    let profile: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
    profile
        .get("addons")
        .and_then(|addons| addons.as_array())
        .map(|addons| {
            addons
                .iter()
                .filter_map(|addon| addon.get("transportUrl").and_then(|url| url.as_str()))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

/// One addon the list offers, as the list describes it.
struct Offered {
    name: String,
    transport_url: String,
    configuration_required: bool,
}

fn fetch_text(booted: &stremio_lightning::Booted, url: &str) -> Result<String, String> {
    booted
        .tokio
        .block_on(async {
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(25))
                .build()?;
            let response = client.get(url).send().await?;
            let status = response.status();
            let body = response.text().await?;
            Ok::<_, reqwest::Error>((status, body))
        })
        .map(|(status, body)| {
            if status.is_success() {
                Ok(body)
            } else {
                Err(format!("HTTP {status}"))
            }
        })
        .unwrap_or_else(|error| Err(format!("the network did not answer: {error}")))
}

/// What the Community catalog offers, by name.
fn community_addons(booted: &stremio_lightning::Booted) -> Result<Vec<Offered>, String> {
    let body = fetch_text(booted, COMMUNITY_LIST)?;
    let value: serde_json::Value =
        serde_json::from_str(&body).map_err(|error| format!("not JSON: {error}"))?;
    let addons = value
        .get("addons")
        .and_then(|addons| addons.as_array())
        .ok_or("the catalog has no addons")?;
    Ok(addons
        .iter()
        .filter_map(|addon| {
            let manifest = addon.get("manifest")?;
            Some(Offered {
                name: manifest.get("name")?.as_str()?.to_owned(),
                transport_url: addon.get("transportUrl")?.as_str()?.to_owned(),
                configuration_required: manifest
                    .get("behaviorHints")
                    .and_then(|hints| hints.get("configurationRequired"))
                    .and_then(|required| required.as_bool())
                    .unwrap_or(false),
            })
        })
        .collect())
}

/// The dialog's one button, by what it currently offers. The addon's name is left out on purpose:
/// an addon may answer with a manifest whose name is not the one the catalog listed it under.
fn dialog_button(window: &MainWindow, offers: &str) -> Result<String, String> {
    ui_driver::labels(window)
        .into_iter()
        .find(|label| label.starts_with(offers))
        .ok_or_else(|| format!("the dialog is not offering {offers:?}"))
}

/// The button that takes the addon -- Install, or Reinstall once the account already has it.
fn dialog_install_label(window: &MainWindow) -> Result<String, String> {
    dialog_button(window, "Install the addon: ")
        .or_else(|_| dialog_button(window, "Reinstall the addon: "))
}

/// Waits until the dialog is holding the manifest for `url` -- the window's own `pending`, not a
/// label, because a label can flip between the moment a press lands and the moment it is read.
fn read_for(window: &MainWindow, url: &str) -> Result<(), String> {
    let pending = window.get_addon_pending();
    if !pending.ready {
        return Err(format!("still reading {:?}", pending.transport_url));
    }
    if pending.transport_url != url {
        return Err(format!("the dialog holds {:?}", pending.transport_url));
    }
    Ok(())
}

/// Where an addon that has to be configured keeps its page: the web opens this, not an install.
fn configure_page(transport_url: &str) -> String {
    transport_url.replace("manifest.json", "configure")
}

/// Whether an addon's manifest answers, and with what.
fn manifest_of(booted: &stremio_lightning::Booted, url: &str) -> Result<String, String> {
    let body = fetch_text(booted, url)?;
    let value: serde_json::Value =
        serde_json::from_str(&body).map_err(|error| format!("not a manifest: {error}"))?;
    value
        .get("id")
        .and_then(|id| id.as_str())
        .map(|id| format!("{} bytes, id {id:?}", body.len()))
        .ok_or_else(|| format!("{} bytes with no id", body.len()))
}


/// One request to an addon, built the way `AddonHTTPTransport` builds it: the manifest path is
/// replaced by `/<resource>/<type>/<id>.json`, and any extra becomes the query string.
fn resource_url(
    transport_url: &str,
    resource: &str,
    kind: &str,
    id: &str,
    extra: &[(String, String)],
) -> Option<String> {
    // A legacy `/stremio/v1` transport speaks a different protocol than these paths.
    let base = transport_url.strip_suffix("manifest.json")?;
    let mut url = format!("{base}{resource}/{kind}/{id}.json");
    if !extra.is_empty() {
        let query: Vec<String> = extra
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect();
        url.push('?');
        url.push_str(&query.join("&"));
    }
    Some(url)
}

/// What an addon answered, and how long it took to say it.
fn fetch_json(
    booted: &stremio_lightning::Booted,
    url: &str,
) -> Result<(serde_json::Value, u128), String> {
    let started = std::time::Instant::now();
    let body = fetch_text(booted, url)?;
    let took = started.elapsed().as_millis();
    let value: serde_json::Value =
        serde_json::from_str(&body).map_err(|error| format!("not JSON: {error}"))?;
    Ok((value, took))
}

/// The content a resource was asked for, and how much of it there is.
fn counted(value: &serde_json::Value, key: &str) -> Option<usize> {
    match value.get(key)? {
        serde_json::Value::Array(items) => Some(items.len()),
        serde_json::Value::Null => Some(0),
        _ => Some(1),
    }
}

/// One resource a manifest declares: its name, the types it serves, and the id prefixes it
/// accepts (a resource's own, or the manifest's, or none at all, which means any id will do).
struct Declared {
    name: String,
    types: Vec<String>,
    prefixes: Vec<String>,
}

impl Declared {
    /// Whether this resource would be asked for an id at all. A kitsu catalog has never heard of
    /// a `tt` id, and asking a music addon about a film gets an answer of 500.
    fn accepts(&self, id: &str) -> bool {
        self.prefixes.is_empty() || self.prefixes.iter().any(|prefix| id.starts_with(prefix))
    }
}

fn strings(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(|value| value.as_array())
        .map(|items| items.iter().filter_map(|item| item.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

/// Every resource a manifest declares, as `resources` writes them: strings, or objects carrying
/// their own `types` and `idPrefixes`.
fn declared_resources(manifest: &serde_json::Value) -> Vec<Declared> {
    let manifest_types = strings(manifest.get("types"));
    let manifest_prefixes = strings(manifest.get("idPrefixes"));
    manifest
        .get("resources")
        .and_then(|resources| resources.as_array())
        .map(|resources| {
            resources
                .iter()
                .filter_map(|resource| match resource {
                    serde_json::Value::String(name) => Some(Declared {
                        name: name.clone(),
                        types: manifest_types.clone(),
                        prefixes: manifest_prefixes.clone(),
                    }),
                    serde_json::Value::Object(fields) => {
                        let name = fields.get("name")?.as_str()?.to_owned();
                        let types = strings(fields.get("types"));
                        let prefixes = strings(fields.get("idPrefixes"));
                        Some(Declared {
                            name,
                            types: if types.is_empty() { manifest_types.clone() } else { types },
                            prefixes: if prefixes.is_empty() {
                                manifest_prefixes.clone()
                            } else {
                                prefixes
                            },
                        })
                    }
                    _ => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The extras a catalog declares, each set to its first option: what a screen sends when nobody
/// has picked one.
fn catalog_extra(catalog: &serde_json::Value) -> Vec<(String, String)> {
    catalog
        .get("extra")
        .and_then(|extra| extra.as_array())
        .map(|extra| {
            extra
                .iter()
                .filter_map(|one| {
                    let name = one.get("name")?.as_str()?;
                    let value = one
                        .get("options")
                        .and_then(|options| options.as_array())
                        .and_then(|options| options.first())
                        .and_then(|option| option.as_str())
                        .or_else(|| one.get("default").and_then(|default| default.as_str()))?;
                    Some((name.to_owned(), value.to_owned()))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// One title to ask an addon about, by the type it serves. `meta` takes a title's own id, while
/// streams and subtitles take a video's: a series is `tt…:season:episode` there.
fn title_for(kind: &str, video: bool) -> Option<&'static str> {
    match (kind, video) {
        ("movie", _) => Some("tt0111161"),
        ("series", true) => Some("tt0944947:1:1"),
        ("series", false) => Some("tt0944947"),
        _ => None,
    }
}

#[test]
#[ignore = "opens the network, so it is run on purpose"]
fn the_community_list_installs_and_every_addon_answers() {
    // The app posts its updates from tokio threads, so this backend needs a real event loop.
    i_slint_backend_testing::init_integration_test_with_system_time();

    let folder: PathBuf =
        std::env::temp_dir().join(format!("stremio-lightning-addons-ui-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&folder);
    std::fs::create_dir_all(&folder).expect("a folder of its own for this run");

    let booted = stremio_lightning::boot(folder.clone()).expect("the app boots");
    let window = &booted.window;
    let before = installed_on_disk(&folder).len();

    // ---- the screen, in front of the user ----------------------------------------------------
    let mut outcomes = ui_driver::run(
        window,
        vec![
            Step::Expect(
                "the app opens on the Board",
                Box::new(|window: &MainWindow| match window.get_section() {
                    Section::Board => Ok("Board".to_owned()),
                    other => Err(format!("it opened on {other:?}")),
                }),
            ),
            Step::Click("Addons".to_owned()),
            Step::Expect(
                "the Addons tab opens the Addons screen",
                Box::new(|window: &MainWindow| match window.get_section() {
                    Section::Addons => Ok("Addons".to_owned()),
                    other => Err(format!("the section is {other:?}")),
                }),
            ),
            Step::Wait {
                what: "the addons the app already has arrive",
                budget: Duration::from_secs(60),
                check: Box::new(move |window: &MainWindow| {
                    let count = cards(window).len();
                    if count == 0 {
                        Err("no cards yet".to_owned())
                    } else {
                        Ok(format!("{count} cards"))
                    }
                }),
            },
            Step::Wait {
                what: "the catalog menu offers the Community list",
                budget: Duration::from_secs(60),
                check: Box::new(|window: &MainWindow| {
                    let inputs = window.get_addon_inputs();
                    let input = inputs.row_data(0).ok_or("no catalog menu")?;
                    let options: Vec<String> = (0..input.options.row_count())
                        .filter_map(|row| input.options.row_data(row))
                        .map(|option| option.label.to_string())
                        .collect();
                    if options.iter().any(|label| label == "Community") {
                        Ok(format!("{} catalogs listed", options.len()))
                    } else {
                        Err(format!("the menu says {options:?}"))
                    }
                }),
            },
        ],
    );

    // ---- the list, from the same place the app reads it --------------------------------------
    let offered = community_addons(&booted);
    let mut targets: Vec<Offered> = Vec::new();
    match offered {
        Ok(list) => {
            outcomes.push(Outcome {
                what: "Stremio's community list answers".to_owned(),
                ok: Some(true),
                detail: format!("{} addons", list.len()),
            });
            for addon in list.iter().filter(|addon| !addon.name.is_empty()).take(TOP_OF_THE_LIST) {
                targets.push(Offered {
                    name: addon.name.clone(),
                    transport_url: addon.transport_url.clone(),
                    configuration_required: addon.configuration_required,
                });
            }
            for name in ALSO_WORTH_PRESSING {
                if let Some(addon) = list.iter().find(|addon| addon.name == name) {
                    if !targets.iter().any(|kept| kept.name == addon.name) {
                        targets.push(Offered {
                            name: addon.name.clone(),
                            transport_url: addon.transport_url.clone(),
                            configuration_required: addon.configuration_required,
                        });
                    }
                } else {
                    outcomes.push(Outcome {
                        what: format!("the list still offers {name:?}"),
                        ok: None,
                        detail: "it is not in the Community catalog any more".to_owned(),
                    });
                }
            }
        }
        Err(error) => outcomes.push(Outcome {
            what: "Stremio's community list answers".to_owned(),
            ok: None,
            detail: error,
        }),
    }

    // ---- does each addon stand up at all? ----------------------------------------------------
    // An addon whose own server does not answer is no mark against the app, so that is a skip;
    // one that answers is what the installs below are worth trying.
    let mut live: Vec<&Offered> = Vec::new();
    for addon in &targets {
        match manifest_of(&booted, &addon.transport_url) {
            Ok(detail) => {
                outcomes.push(Outcome {
                    what: format!("{} answers its manifest", addon.name),
                    ok: Some(true),
                    detail,
                });
                live.push(addon);
            }
            Err(error) => outcomes.push(Outcome {
                what: format!("{} answers its manifest", addon.name),
                ok: None,
                detail: error,
            }),
        }
    }

    // ---- press Add addon, paste the URL, press Install ---------------------------------------
    // Nothing on the machine running this opens: the app's one door for a web page is watched.
    static OPENED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    stremio_lightning::links::watch(Some(Box::new(|url| {
        OPENED.lock().unwrap().push(url.to_owned())
    })));

    let mut steps: Vec<Step> = Vec::new();
    let mut previous = String::new();
    for addon in &live {
        let name = addon.name.clone();
        let url = addon.transport_url.clone();
        let folder = folder.clone();
        steps.push(Step::Click("Add addon".to_owned()));
        steps.push(Step::Click("Addon URL".to_owned()));
        if !previous.is_empty() {
            // The dialog keeps what was last pasted into it, so the field is emptied first.
            steps.push(Step::Erase(previous.chars().count()));
        }
        previous = url.clone();
        steps.push(Step::Type(url.clone()));
        steps.push(Step::Click("Read the addon's url".to_owned()));
        let reads = name.clone();
        let url_for_check = url.clone();
        let wanted_url = url.clone();
        let written = name.clone();
        if addon.configuration_required {
            // Web: `Addon.js` offers Configure, not Install, for an addon that asks to be set up
            // first -- and pressing it opens the addon's own page, where the settings and the
            // install link are. Nothing joins the account from here.
            steps.push(Step::Wait {
                what: "the dialog offers Configure, not Install",
                budget: Duration::from_secs(40),
                check: Box::new(move |window: &MainWindow| {
                    let asked = wanted_url.clone();
                    read_for(window, &asked)?;
                    match dialog_button(window, "Configure the addon: ") {
                        Ok(label) => Ok(format!("{reads} reads as {label:?}")),
                        Err(error) => Err(error),
                    }
                }),
            });
            steps.push(Step::ClickLabel(
                "the dialog's Configure is pressed",
                Box::new(|window: &MainWindow| dialog_button(window, "Configure the addon: ")),
            ));
            let page = configure_page(&url);
            steps.push(Step::Pause(400));
            steps.push(Step::Expect(
                "its own page opens, where it is set up",
                Box::new(move |_: &MainWindow| {
                    if OPENED.lock().unwrap().iter().any(|opened| *opened == page) {
                        Ok(page.clone())
                    } else {
                        Err(format!("nothing opened {page}"))
                    }
                }),
            ));
            steps.push(Step::Expect(
                "it is not added to the account unconfigured",
                Box::new(move |_: &MainWindow| {
                    if installed_on_disk(&folder).contains(&url_for_check) {
                        Err(format!("{written} was installed anyway"))
                    } else {
                        Ok("not installed, as the web does".to_owned())
                    }
                }),
            ));
        } else {
            steps.push(Step::Wait {
                what: "the manifest arrives and the dialog offers Install",
                budget: Duration::from_secs(40),
                check: Box::new(move |window: &MainWindow| {
                    let asked = wanted_url.clone();
                    read_for(window, &asked)?;
                    match dialog_install_label(window) {
                        Ok(label) => Ok(format!("{reads} reads as {label:?}")),
                        Err(error) => Err(error),
                    }
                }),
            });
            steps.push(Step::ClickLabel(
                "the dialog's Install is pressed",
                Box::new(dialog_install_label),
            ));
            steps.push(Step::Wait {
                what: "the addon joins the profile on disk",
                budget: Duration::from_secs(40),
                check: Box::new(move |_: &MainWindow| {
                    if installed_on_disk(&folder).contains(&url_for_check) {
                        Ok(format!("{written} installed"))
                    } else {
                        Err(format!("{written} is not in the profile yet"))
                    }
                }),
            });
        }
    }
    if steps.is_empty() {
        outcomes.push(Outcome {
            what: "the addons are installed".to_owned(),
            ok: None,
            detail: "nothing was live to install".to_owned(),
        });
    } else {
        outcomes.extend(ui_driver::run(window, steps));
    }
    stremio_lightning::links::watch(None);

    // ---- what the app wrote down -------------------------------------------------------------
    let after = installed_on_disk(&folder);
    let added = after.len().saturating_sub(before);
    outcomes.push(Outcome {
        what: "the profile on disk holds the new addons".to_owned(),
        ok: Some(added > 0),
        detail: format!("{before} before, {} after ({added} added)", after.len()),
    });
    for addon in &live {
        let name = addon.name.clone();
        let url = addon.transport_url.clone();
        let held = after.contains(&url);
        outcomes.push(Outcome {
            what: if addon.configuration_required {
                format!("{name} stays out until it is configured")
            } else {
                format!("{name} is installed")
            },
            ok: Some(held != addon.configuration_required),
            detail: if addon.configuration_required {
                if held {
                    "it was installed anyway".to_owned()
                } else {
                    "the dialog offered its configuration page instead".to_owned()
                }
            } else if held {
                url.clone()
            } else {
                "it never reached the profile".to_owned()
            },
        });
    }
    // The list behind the dialog is the account's own; a fresh install has to show up in it.
    if let Some(addon) = live.iter().find(|addon| !addon.configuration_required) {
        let name = addon.name.clone();
        outcomes.extend(ui_driver::run(
            window,
            vec![Step::Wait {
                what: "the Addons screen lists a new addon",
                budget: Duration::from_secs(40),
                check: Box::new(move |window: &MainWindow| {
                    if is_installed(window, &name) {
                        Ok(format!("{name} says Uninstall"))
                    } else {
                        Err(format!("no card for {name} yet"))
                    }
                }),
            }],
        ));
    }

    // ---- ask the installed addons for the thing they exist for ---------------------------------
    // An addon that answers its manifest but not its own resources is no use, and the shapes
    // differ enough that only asking finds out. Only the addons that actually joined the account
    // are asked: an addon that had to be configured first is not part of what the app does, and
    // its own server's state is not the app's business -- so this sweep prints what each addon
    // does and never counts against the app. What the app must get right is above it: the list,
    // the install, the dialog, and the card that then says Uninstall.
    //
    // Each request is built the way `AddonHTTPTransport` builds it -- the manifest path becomes
    // `/<resource>/<type>/<id>.json` -- and is only made when the manifest says it can be
    // answered: the type has to be one the resource serves, and the id has to start with one of
    // its `idPrefixes` (a kitsu catalog has never heard of a `tt` id, and a music addon answers
    // 500 to a film).
    let installed_urls: Vec<String> = after.clone();
    // The ids the catalogs hand over, by type: the ones a type nothing here knows is asked with
    // (kitsu, tv, music).
    let mut known: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    // The names of the catalogs those addons declare, which is what the Board's rows are called.
    let mut drawn_catalogs: Vec<String> = Vec::new();
    for addon in live
        .iter()
        .filter(|addon| installed_urls.contains(&addon.transport_url))
    {
        let name = addon.name.clone();
        let transport_url = addon.transport_url.clone();
        let manifest = match fetch_json(&booted, &transport_url) {
            Ok((manifest, _)) => manifest,
            Err(error) => {
                outcomes.push(Outcome {
                    what: format!("{name} answers its manifest again"),
                    ok: Some(false),
                    detail: error,
                });
                continue;
            }
        };
        let resources = declared_resources(&manifest);

        // A catalog first: it is the only way to learn the ids of the types nobody here knows
        // (kitsu, tv, music), and those ids are what everything else is asked for.
        if let Some(catalog_resource) = resources.iter().find(|resource| resource.name == "catalog") {
            let first = manifest
                .get("catalogs")
                .and_then(|catalogs| catalogs.as_array())
                .and_then(|catalogs| catalogs.first());
            match first {
                None => outcomes.push(Outcome {
                    what: format!("{name} serves a catalog"),
                    ok: None,
                    detail: "it declares the resource but no catalog".to_owned(),
                }),
                Some(catalog) => {
                    let kind = catalog.get("type").and_then(|one| one.as_str()).unwrap_or("");
                    let id = catalog.get("id").and_then(|one| one.as_str()).unwrap_or("");
                    // Extras the catalog declares are filled with their first option, which is
                    // what a screen sends when nobody has picked one.
                    let extra = catalog_extra(catalog);
                    let label = format!("{name} serves catalog {kind}/{id}");
                    match resource_url(&transport_url, "catalog", kind, id, &extra) {
                        None => outcomes.push(Outcome {
                            what: label,
                            ok: None,
                            detail: "a legacy transport, not these paths".to_owned(),
                        }),
                        Some(url) => match fetch_json(&booted, &url) {
                            Ok((value, took)) => {
                                if let Some(metas) = value.get("metas").and_then(|metas| metas.as_array()) {
                                    for meta in metas {
                                        if let (Some(one), Some(kind)) = (
                                            meta.get("id").and_then(|one| one.as_str()),
                                            meta.get("type").and_then(|one| one.as_str()),
                                        ) {
                                            known.entry(kind.to_owned()).or_insert_with(|| one.to_owned());
                                        }
                                    }
                                }
                                if let Some(named) = catalog.get("name").and_then(|one| one.as_str()) {
                                    drawn_catalogs.push(named.to_owned());
                                }
                                let count = counted(&value, "metas");
                                outcomes.push(Outcome {
                                    what: label,
                                    ok: None,
                                    detail: match count {
                                        Some(count) => format!(
                                            "{count} titles, {took} ms{}",
                                            if extra.is_empty() {
                                                String::new()
                                            } else {
                                                format!(", asked with {extra:?}")
                                            }
                                        ),
                                        None => format!("no metas, {took} ms"),
                                    },
                                });
                            }
                            Err(error) if !extra.is_empty() => outcomes.push(Outcome {
                                what: label,
                                ok: None,
                                detail: format!("{error}; the app sends the extra this probe guessed"),
                            }),
                            Err(error) => outcomes.push(Outcome {
                                what: label,
                                ok: None,
                                detail: format!("its own server said {error}"),
                            }),
                        },
                    }
                }
            }
            let _ = catalog_resource;
        }

        for resource in ["meta", "stream", "subtitles"] {
            let Some(declared) = resources.iter().find(|one| one.name == resource) else {
                continue;
            };
            let video = resource != "meta";
            let asked: Vec<(String, String)> = declared
                .types
                .iter()
                .filter_map(|kind| {
                    // A known title of that type first, then the id this addon's own catalog
                    // handed over -- whichever of them this resource accepts.
                    let candidates = [
                        title_for(kind, video).map(str::to_owned),
                        known.get(kind).cloned(),
                    ];
                    let id = candidates
                        .into_iter()
                        .flatten()
                        .find(|id| declared.accepts(id))?;
                    Some((kind.clone(), id))
                })
                .collect();
            if asked.is_empty() {
                outcomes.push(Outcome {
                    what: format!("{name} serves {resource}"),
                    ok: None,
                    detail: format!(
                        "no title of a type it serves ({:?}) and prefix it accepts ({:?}) is at hand",
                        declared.types, declared.prefixes
                    ),
                });
                continue;
            }
            for (kind, id) in asked {
                let answer = match resource {
                    "meta" => "meta",
                    "stream" => "streams",
                    _ => "subtitles",
                };
                let label = format!("{name} serves {resource} for {id}");
                let Some(url) = resource_url(&transport_url, resource, &kind, &id, &[]) else {
                    outcomes.push(Outcome {
                        what: label,
                        ok: None,
                        detail: "a legacy transport, not these paths".to_owned(),
                    });
                    continue;
                };
                match fetch_json(&booted, &url) {
                    Ok((value, took)) => {
                        let count = counted(&value, answer);
                        outcomes.push(Outcome {
                            what: label,
                            ok: None,
                            detail: match count {
                                Some(count) => format!("{count} for {kind}, {took} ms"),
                                None => format!("answered without {answer:?}, {took} ms"),
                            },
                        });
                    }
                    Err(error) => outcomes.push(Outcome {
                        what: label,
                        ok: None,
                        detail: format!("its own server said {error}"),
                    }),
                }
            }
        }
    }

    // ---- the Board draws what the new catalogs answer, and one card opens -------------------
    // The sweep above proves the addons answer. This is the half that proves the app copes: the
    // Board asks every installed addon's catalogs, draws them as rows, and opening one of their
    // cards lands on a Details screen that the addon's own metadata fills.
    //
    // A Board row is named by its catalog and the type it serves -- "Kitsu Trending - Anime" -- so
    // the row is found by the catalog name it starts with.
    drawn_catalogs.sort();
    drawn_catalogs.dedup();
    let wanted_rows = drawn_catalogs.clone();
    outcomes.extend(ui_driver::run(
        window,
        vec![
            Step::Click("Board".to_owned()),
            Step::Wait {
                what: "the Board asks every installed addon for its catalogs",
                budget: Duration::from_secs(90),
                check: Box::new({
                    let wanted = wanted_rows.clone();
                    move |window: &MainWindow| {
                        let rows = window.get_rows();
                        let titled: Vec<(String, usize)> = (0..rows.row_count())
                            .filter_map(|index| rows.row_data(index))
                            .map(|row| (row.title.to_string(), row.cards.row_count()))
                            .collect();
                        match titled.iter().find(|(title, _)| {
                            wanted
                                .iter()
                                .any(|catalog| title.starts_with(&format!("{catalog} - ")))
                        }) {
                            Some((title, cards)) if *cards > 0 => {
                                Ok(format!("{title:?} with {cards} cards"))
                            }
                            Some((title, _)) => Err(format!("{title:?} drew no cards")),
                            None => Err(format!(
                                "no row of {wanted:?} among {:?}",
                                titled.iter().map(|(title, _)| title).collect::<Vec<_>>()
                            )),
                        }
                    }
                }),
            },
        ],
    ));

    // A card is pressed from a row that is on the screen, because that is what a person can press:
    // the Board is a `Flickable`, and Slint only builds the rows near the viewport. The wheel
    // cannot be turned from here -- this shell reads it from winit, which the testing backend
    // never speaks -- so the run takes the first row, and the first card of it, that the window is
    // drawing. The rows the addons just added are on screen further down; what they prove is the
    // row above, which is their data arriving at the Board.
    let drawn = ui_driver::labels(window);
    let rows = window.get_rows();
    let picked: Option<(String, String)> = (0..rows.row_count())
        .filter_map(|index| rows.row_data(index))
        .filter(|row| drawn.iter().any(|label| label == row.title.as_str()))
        .find_map(|row| {
            let cards = (0..row.cards.row_count())
                .filter_map(|index| row.cards.row_data(index))
                .find(|card| drawn.iter().any(|label| label == card.title.as_str()))?;
            Some((row.title.to_string(), cards.title.to_string()))
        });
    match picked {
        None => outcomes.push(Outcome {
            what: "a card on the Board".to_owned(),
            ok: None,
            detail: "no drawn row had a drawn card".to_owned(),
        }),
        Some((row_title, card_title)) => {
            let opened = card_title.clone();
            let which = row_title.clone();
            outcomes.extend(ui_driver::run(
                window,
                vec![
                    Step::Click(card_title.clone()),
                    Step::Wait {
                        what: "that card opens the Details screen",
                        budget: Duration::from_secs(60),
                        check: Box::new(move |window: &MainWindow| {
                            if !window.get_details_open() {
                                return Err("the Details screen is not open".to_owned());
                            }
                            let preview = window.get_details_preview();
                            if preview.title.is_empty() {
                                Err("the panel has no title yet".to_owned())
                            } else if preview.title != opened {
                                Err(format!("the panel says {:?}", preview.title))
                            } else {
                                Ok(format!("{:?}", preview.title))
                            }
                        }),
                    },
                    Step::Wait {
                        what: "the streams for that card settle",
                        budget: Duration::from_secs(60),
                        check: Box::new(move |window: &MainWindow| {
                            if !window.get_details_open() {
                                return Err("the Details screen closed".to_owned());
                            }
                            if window.get_streams_loading() > 0 {
                                return Err(format!(
                                    "{} addon(s) still answering",
                                    window.get_streams_loading()
                                ));
                            }
                            Ok(format!(
                                "{} stream(s) for a card of {which:?}",
                                window.get_streams().row_count()
                            ))
                        }),
                    },
                ],
            ));
        }
    }

    let held = ui_driver::report(&outcomes);
    println!("{} of {} addons were live", live.len(), targets.len());
    println!("the app's profile for this run: {}", folder.display());
    assert!(held, "some expectations did not hold");
}
