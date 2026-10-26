// What the Addons screen shows: the installed addons from stremio-core's `InstalledAddonsWithFilters`,
// and the marketplace list the route's second section reads from `CatalogWithFilters<Descriptor>`.
// Based on stremio-web src/routes/Addons (Addons.js, Addon/Addon.js, useRemoteAddons.js,
// useSelectableInputs.js).
// Licence basis: ../../THIRD_PARTY_NOTICES.md.
use stremio_core::models::addon_details::Selected as AddonDetailsSelected;
use stremio_core::models::catalog_with_filters::Selected as CatalogSelected;
use stremio_core::models::common::Loadable;
use stremio_core::models::installed_addons_with_filters::{
    InstalledAddonsRequest, SelectableType, Selected as InstalledSelected,
};
use stremio_core::runtime::msg::{Action, ActionCtx, ActionLoad};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::addon::{Descriptor, ResourceRequest};
use url::Url;

use super::{capitalise, AppModel, Input};
use crate::app::{AppModelField, AppRuntime};

#[derive(Clone, Debug, PartialEq)]
pub struct AddonCard {
    /// What identifies this addon when its buttons are pressed: `Descriptor::transport_url`.
    pub transport_url: String,
    pub name: String,
    pub version: String,
    pub types: String,
    pub description: String,
    pub logo: Option<String>,
    /// Web: `Addon.js` shows Uninstall on an addon the account has and Install on one it does not
    /// -- which is every card in the marketplace list.
    pub installed: bool,
    /// `manifest.behaviorHints.configurationRequired`: the addon only works once the user has
    /// configured it on its own page, so `Addon.js` offers Configure in place of Install.
    pub configuration_required: bool,
    /// `manifest.behaviorHints.configurable`: an installed addon that keeps a settings page, which
    /// `Addon.js` offers as a second, icon-only button beside Uninstall.
    pub configurable: bool,
    /// The descriptor this card was drawn from. The web hangs it on the card element and installs
    /// from there, so a press is never at the mercy of the list reloading underneath it -- which is
    /// exactly what happens here when one install makes the catalog reload and the next press
    /// arrives while it is empty.
    pub descriptor: Descriptor,
}

/// What picking an option in one of the screen's two menus loads. The web carries that in the route
/// (`/addons/{type}/{transportUrl}/{catalogId}`); here it is an action.
#[derive(Clone)]
pub enum Choice {
    Installed(InstalledAddonsRequest),
    Remote(ResourceRequest),
}

#[derive(Clone, Default)]
pub struct Data {
    /// The catalog menu ("Installed", then every addon catalog an installed addon declares) and
    /// the type menu ("All", "Movie", ...).
    pub inputs: Vec<Input>,
    /// What each option of each menu loads, menu by menu, in the menus' order.
    pub choices: Vec<Vec<Choice>>,
    pub cards: Vec<AddonCard>,
    /// Whether the list on screen is a marketplace catalog rather than the installed addons.
    pub remote: bool,
    /// Whether the marketplace is still being read, so the empty message waits for it.
    pub loading: bool,
    pub message: String,
    /// The addon the user pasted a URL for, while its manifest is being fetched or shown.
    pub pending: Option<Pending>,
}

/// The "add addon" dialog: the URL the user pasted and, once the core has fetched it, the manifest
/// it answered with -- which is what the web's `AddonDetailsModal` shows before installing.
#[derive(Clone, Default)]
pub struct Pending {
    pub transport_url: String,
    pub name: String,
    pub version: String,
    pub types: String,
    pub description: String,
    pub logo: Option<String>,
    /// Whether the manifest has arrived, so the Install button can be pressed.
    pub ready: bool,
    /// Whether the account already has this addon.
    pub installed: bool,
    /// What went wrong, empty while nothing has.
    pub error: String,
}

/// "Movie", "Movie & Series", "Movie, Series & Channel", like `Addon.js`.
fn types_label(types: &[String]) -> String {
    match types {
        [] => String::new(),
        [only] => capitalise(only),
        [init @ .., last] => {
            let init: Vec<String> = init.iter().map(|kind| capitalise(kind)).collect();
            format!("{} & {}", init.join(", "), capitalise(last))
        }
    }
}

fn card_of(addon: &Descriptor, installed: bool) -> AddonCard {
    let manifest = &addon.manifest;
    AddonCard {
        transport_url: addon.transport_url.to_string(),
        name: if manifest.name.is_empty() {
            manifest.id.clone()
        } else {
            manifest.name.clone()
        },
        version: format!("v.{}", manifest.version),
        types: types_label(&manifest.types),
        description: manifest.description.clone().unwrap_or_default(),
        logo: manifest.logo.as_ref().map(|url| url.to_string()),
        installed,
        configuration_required: manifest.behavior_hints.configuration_required,
        configurable: manifest.behavior_hints.configurable,
        descriptor: addon.clone(),
    }
}

/// Web: `Addons.js` keeps the `installed` flag on every remote card so the same card can offer
/// Install or Uninstall; it asks the account's addon list, by transport URL.
fn is_installed(model: &AppModel, addon: &Descriptor) -> bool {
    model
        .ctx
        .profile
        .addons
        .iter()
        .any(|installed| installed.transport_url == addon.transport_url)
}

/// Every addon a marketplace catalog has answered with, across the pages the core fetched.
fn market_addons(model: &AppModel) -> Vec<&Descriptor> {
    model
        .remote_addons
        .catalog
        .iter()
        .filter_map(|page| match &page.content {
            Some(Loadable::Ready(addons)) => Some(addons.iter()),
            _ => None,
        })
        .flatten()
        .collect()
}

fn type_label(item: &SelectableType) -> String {
    item.r#type
        .as_deref()
        .map_or_else(|| "All".to_owned(), capitalise)
}

pub fn data(model: &AppModel) -> Data {
    let installed = &model.installed_addons;
    let remote = &model.remote_addons;
    let remote_selected = remote.selected.is_some();

    // Menu 0: "Installed", then one entry per addon catalog the installed addons declare. Cinemeta
    // declares "Official", "Community" and "Podcasts" once per type; the menu shows each name once,
    // and its option loads the catalog for the type the next menu is on.
    let mut catalog_options = vec![("Installed".to_owned(), !remote_selected)];
    let mut catalog_choices = vec![Choice::Installed(InstalledAddonsRequest { r#type: None })];
    let mut seen: Vec<(String, String)> = Vec::new();
    for selectable in &remote.selectable.catalogs {
        let key = (
            selectable.request.base.to_string(),
            selectable.request.path.id.clone(),
        );
        if seen.contains(&key) {
            continue;
        }
        seen.push(key);
        catalog_options.push((selectable.catalog.clone(), selectable.selected));
        catalog_choices.push(Choice::Remote(selectable.request.clone()));
    }
    let catalog = Input {
        title: catalog_options
            .iter()
            .find(|(_, selected)| *selected)
            .map_or_else(|| "Installed".to_owned(), |(label, _)| label.clone()),
        selected: catalog_options
            .iter()
            .position(|(_, selected)| *selected)
            .unwrap_or(0),
        options: catalog_options,
        requests: vec![],
    };

    // Menu 1: the types of whichever list is on screen.
    let (type_options, type_choices, type_title) = if remote_selected {
        let types = &remote.selectable.types;
        let selected = types.iter().position(|item| item.selected).unwrap_or(0);
        (
            types
                .iter()
                .map(|item| (capitalise(&item.r#type), item.selected))
                .collect::<Vec<_>>(),
            types
                .iter()
                .map(|item| Choice::Remote(item.request.clone()))
                .collect::<Vec<_>>(),
            types
                .get(selected)
                .map_or_else(String::new, |item| capitalise(&item.r#type)),
        )
    } else {
        let types = &installed.selectable.types;
        let selected = types.iter().position(|item| item.selected).unwrap_or(0);
        (
            types
                .iter()
                .map(|item| (type_label(item), item.selected))
                .collect::<Vec<_>>(),
            types
                .iter()
                .map(|item| Choice::Installed(item.request.clone()))
                .collect::<Vec<_>>(),
            types
                .get(selected)
                .map_or_else(|| "All".to_owned(), type_label),
        )
    };
    let kinds = Input {
        title: type_title,
        selected: type_options
            .iter()
            .position(|(_, selected)| *selected)
            .unwrap_or(0),
        options: type_options,
        requests: vec![],
    };

    let loading = remote_selected
        && remote.catalog.iter().any(|page| {
            !matches!(
                page.content,
                Some(Loadable::Ready(_)) | Some(Loadable::Err(_))
            )
        });
    let cards = if remote_selected {
        let mut cards: Vec<AddonCard> = Vec::new();
        for addon in market_addons(model) {
            if cards
                .iter()
                .any(|card| card.transport_url == addon.transport_url.as_str())
            {
                continue;
            }
            cards.push(card_of(addon, is_installed(model, addon)));
        }
        cards
    } else {
        installed
            .catalog
            .iter()
            .map(|addon| card_of(addon, true))
            .collect()
    };
    // stremio-web's NO_ADDONS_FOR_TYPE, shown only once the list has actually answered. The
    // translated text is not available here, so this wording is a stand-in.
    let message = if cards.is_empty() && !loading {
        "No addons for this type.".to_owned()
    } else {
        String::new()
    };
    Data {
        inputs: vec![catalog, kinds],
        choices: vec![catalog_choices, type_choices],
        cards,
        remote: remote_selected,
        loading,
        message,
        pending: pending(&model.addon_details),
    }
}

/// Web: picking an option in either menu navigates, which loads that list. Installed addons and a
/// marketplace catalog are two different models.
pub(crate) fn select(runtime: &AppRuntime, choice: &Choice) {
    let action = match choice {
        Choice::Installed(request) => {
            // Web: `useRemoteAddons` unloads the marketplace whenever no marketplace catalog is
            // named, which is what puts the account's own list back on the screen.
            runtime.dispatch(RuntimeAction {
                field: Some(AppModelField::RemoteAddons),
                action: Action::Unload,
            });
            Action::Load(ActionLoad::InstalledAddonsWithFilters(InstalledSelected {
                request: request.clone(),
            }))
        }
        Choice::Remote(request) => {
            Action::Load(ActionLoad::CatalogWithFilters(Some(CatalogSelected {
                request: request.clone(),
            })))
        }
    };
    runtime.dispatch(RuntimeAction {
        field: None,
        action,
    });
}

/// Web: `onAddonInstall` for a marketplace card -- the descriptor the card was drawn from is
/// exactly what `InstallAddon` takes, and an addon that has to be configured first opens its own
/// page instead.
pub(crate) fn install_card(runtime: &AppRuntime, card: &AddonCard) {
    if card.installed {
        return;
    }
    if card.configuration_required {
        configure(&card.transport_url);
        return;
    }
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Ctx(ActionCtx::InstallAddon(card.descriptor.clone())),
    });
}

/// Web: `AddonDetailsModal` -- the manifest behind the pasted URL, its Install button, and the
/// errors the fetch can answer with. `std::option` is the dialog being closed: nothing pasted.
fn pending(details: &stremio_core::models::addon_details::AddonDetails) -> Option<Pending> {
    let selected = details.selected.as_ref()?;
    let transport_url = selected.transport_url.to_string();
    let installed = details.local_addon.is_some();
    let mut pending = Pending {
        transport_url,
        installed,
        ..Pending::default()
    };
    match details.remote_addon.as_ref().map(|addon| &addon.content) {
        Some(Loadable::Ready(addon)) => {
            let manifest = &addon.manifest;
            pending.name = if manifest.name.is_empty() {
                manifest.id.clone()
            } else {
                manifest.name.clone()
            };
            pending.version = format!("v.{}", manifest.version);
            pending.types = types_label(&manifest.types);
            pending.description = manifest.description.clone().unwrap_or_default();
            pending.logo = manifest.logo.as_ref().map(|url| url.to_string());
            pending.ready = true;
        }
        Some(Loadable::Err(error)) => pending.error = format!("{error}"),
        // Still loading, or nothing asked for yet.
        _ => {}
    }
    Some(pending)
}

/// Web: `Addons.js` parses the pasted text as a URL -- `stremio://` is stremio's own scheme for
/// the same link, so it is read as `https://` -- and hands the transport URL to `AddonDetails`.
pub(crate) fn open_details(runtime: &AppRuntime, text: &str) -> Result<(), String> {
    let url = parse_addon_url(text)?;
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Load(ActionLoad::AddonDetails(AddonDetailsSelected {
            transport_url: url,
        })),
    });
    Ok(())
}

fn parse_addon_url(text: &str) -> Result<Url, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Paste the addon's manifest URL first.".to_owned());
    }
    let text = if let Some(rest) = text.strip_prefix("stremio://") {
        format!("https://{rest}")
    } else {
        text.to_owned()
    };
    let url = Url::parse(&text).map_err(|error| format!("Failed to parse addon url: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("An addon URL starts with https:// or stremio://.".to_owned());
    }
    Ok(url)
}

/// Web: the dialog's Install button dispatches `InstallAddon` with the manifest the core fetched.
pub(crate) fn install(runtime: &AppRuntime) {
    let Ok(model) = runtime.model() else {
        return;
    };
    let addon = model
        .addon_details
        .remote_addon
        .as_ref()
        .and_then(|addon| match &addon.content {
            Loadable::Ready(addon) => Some(addon.clone()),
            _ => None,
        });
    drop(model);
    let Some(addon) = addon else {
        return;
    };
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Ctx(ActionCtx::InstallAddon(addon)),
    });
}

/// Web: `Addons.js`'s Configure button opens the addon's own configuration page -- its manifest
/// URL with `manifest.json` swapped for `configure` -- in the user's browser.
pub(crate) fn configure(transport_url: &str) {
    let target = transport_url.replace("manifest.json", "configure");
    crate::plumbing::links::open(&target);
}

/// Web: the card's Uninstall button dispatches `UninstallAddon` with the installed descriptor.
pub(crate) fn uninstall(runtime: &AppRuntime, transport_url: &str) {
    let Ok(model) = runtime.model() else {
        return;
    };
    let addon = model
        .ctx
        .profile
        .addons
        .iter()
        .find(|addon| addon.transport_url.as_str() == transport_url)
        .cloned();
    drop(model);
    let Some(addon) = addon else {
        return;
    };
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Ctx(ActionCtx::UninstallAddon(addon)),
    });
}

/// The cards whose name or description contains `query`, ignoring case, like `Addons.js`.
pub(crate) fn visible<'a>(cards: &'a [AddonCard], query: &str) -> Vec<&'a AddonCard> {
    let query = query.to_lowercase();
    cards
        .iter()
        .filter(|card| {
            query.is_empty()
                || card.name.to_lowercase().contains(&query)
                || card.description.to_lowercase().contains(&query)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(name: &str, description: &str) -> AddonCard {
        let manifest: stremio_core::types::addon::Manifest = serde_json::from_value(serde_json::json!({
            "id": "example",
            "version": "1.0.0",
            "name": name,
            "description": description,
            "resources": ["catalog"],
            "types": ["movie"],
        }))
        .expect("a manifest");
        AddonCard {
            transport_url: "https://example.com/manifest.json".to_owned(),
            name: name.to_owned(),
            version: String::new(),
            types: String::new(),
            description: description.to_owned(),
            logo: None,
            installed: false,
            configuration_required: false,
            configurable: false,
            descriptor: Descriptor {
                transport_url: Url::parse("https://example.com/manifest.json").expect("a url"),
                manifest,
                flags: Default::default(),
            },
        }
    }

    #[test]
    fn types_are_joined_like_the_web() {
        let types = |list: &[&str]| list.iter().map(|t| (*t).to_owned()).collect::<Vec<_>>();
        assert_eq!(types_label(&types(&[])), "");
        assert_eq!(types_label(&types(&["movie"])), "Movie");
        assert_eq!(types_label(&types(&["movie", "series"])), "Movie & Series");
        assert_eq!(
            types_label(&types(&["movie", "series", "channel"])),
            "Movie, Series & Channel"
        );
    }

    #[test]
    fn search_matches_name_or_description_ignoring_case() {
        let cards = vec![
            card("Cinemeta", "The official addon"),
            card("YouTube", "Watch channels"),
        ];
        assert_eq!(visible(&cards, "").len(), 2);
        assert_eq!(visible(&cards, "CINE")[0].name, "Cinemeta");
        assert_eq!(visible(&cards, "channels")[0].name, "YouTube");
        assert!(visible(&cards, "zzz").is_empty());
    }

    #[test]
    fn an_addon_url_is_read_the_way_the_web_reads_it() {
        // `Addons.js` parses the pasted text as a URL; stremio's own scheme is the same link.
        assert_eq!(
            parse_addon_url("https://v3-cinemeta.strem.io/manifest.json")
                .expect("an https url")
                .as_str(),
            "https://v3-cinemeta.strem.io/manifest.json"
        );
        assert_eq!(
            parse_addon_url("  stremio://v3-cinemeta.strem.io/manifest.json  ")
                .expect("a stremio url")
                .as_str(),
            "https://v3-cinemeta.strem.io/manifest.json"
        );
        assert!(parse_addon_url("").is_err(), "nothing pasted");
        assert!(
            parse_addon_url("file:///etc/passwd").is_err(),
            "only http(s) and stremio are addon urls"
        );
        assert!(parse_addon_url("not a url").is_err());
    }
}
