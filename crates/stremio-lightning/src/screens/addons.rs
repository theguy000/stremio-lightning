// What the Addons screen shows: the installed addons from stremio-core's `InstalledAddonsWithFilters`.
// Based on stremio-web src/routes/Addons (Addons.js, Addon/Addon.js, useSelectableInputs.js).
// Licence basis: ../../THIRD_PARTY_NOTICES.md.
use stremio_core::models::addon_details::Selected as AddonDetailsSelected;
use stremio_core::models::common::Loadable;
use stremio_core::models::installed_addons_with_filters::{InstalledAddonsRequest, SelectableType};
use stremio_core::runtime::msg::{Action, ActionCtx, ActionLoad};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::addon::Descriptor;
use url::Url;

use super::{capitalise, AppModel, Input};
use crate::app::AppRuntime;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AddonCard {
    /// What identifies this addon when its buttons are pressed: `Descriptor::transport_url`.
    pub transport_url: String,
    pub name: String,
    pub version: String,
    pub types: String,
    pub description: String,
    pub logo: Option<String>,
}

#[derive(Clone, Default)]
pub struct Data {
    /// The catalog menu ("Installed") and the type menu ("All", "Movie", ...).
    pub inputs: Vec<Input>,
    /// What each option of the type menu loads, in the menu's order.
    pub type_requests: Vec<InstalledAddonsRequest>,
    pub cards: Vec<AddonCard>,
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

fn card_of(addon: &Descriptor) -> AddonCard {
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
    }
}

fn type_label(item: &SelectableType) -> String {
    item.r#type
        .as_deref()
        .map_or_else(|| "All".to_owned(), capitalise)
}

pub fn data(model: &AppModel) -> Data {
    let installed = &model.installed_addons;
    let types = &installed.selectable.types;
    let selected = types.iter().position(|item| item.selected).unwrap_or(0);
    let title = types
        .get(selected)
        .map_or_else(|| "All".to_owned(), type_label);
    let catalog = Input {
        title: "Installed".to_owned(),
        selected: 0,
        options: vec![("Installed".to_owned(), true)],
        requests: vec![],
    };
    let kinds = Input {
        title,
        selected,
        options: types
            .iter()
            .map(|item| (type_label(item), item.selected))
            .collect(),
        requests: vec![],
    };
    // stremio-web's NO_ADDONS_FOR_TYPE. The translated text is not available here, so this wording is a stand-in.
    let message = if installed.selected.is_some() && installed.catalog.is_empty() {
        "No addons for this type.".to_owned()
    } else {
        String::new()
    };
    Data {
        inputs: vec![catalog, kinds],
        type_requests: types.iter().map(|item| item.request.clone()).collect(),
        cards: installed.catalog.iter().map(card_of).collect(),
        message,
        pending: pending(&model.addon_details),
    }
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
        AddonCard {
            name: name.to_owned(),
            description: description.to_owned(),
            ..AddonCard::default()
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
