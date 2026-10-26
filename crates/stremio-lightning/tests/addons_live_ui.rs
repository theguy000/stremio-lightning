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
        // The dialog is cleared of the last addon's manifest before this one is read, so waiting
        // for that state first is what tells the two apart.
        steps.push(Step::Wait {
            what: "the dialog starts reading the url",
            budget: Duration::from_secs(20),
            check: Box::new(|window: &MainWindow| {
                if ui_driver::labels(window)
                    .iter()
                    .any(|label| label == "Read the addon's url")
                {
                    Ok("reading".to_owned())
                } else {
                    Err("the last addon is still in the dialog".to_owned())
                }
            }),
        });
        let reads = name.clone();
        let url_for_check = url.clone();
        let written = name.clone();
        if addon.configuration_required {
            // Web: `Addon.js` offers Configure, not Install, for an addon that asks to be set up
            // first -- and pressing it opens the addon's own page, where the settings and the
            // install link are. Nothing joins the account from here.
            steps.push(Step::Wait {
                what: "the dialog offers Configure, not Install",
                budget: Duration::from_secs(40),
                check: Box::new(move |window: &MainWindow| {
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

    let held = ui_driver::report(&outcomes);
    println!("{} of {} addons were live", live.len(), targets.len());
    println!("the app's profile for this run: {}", folder.display());
    assert!(held, "some expectations did not hold");
}
