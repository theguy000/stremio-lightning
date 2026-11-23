//! Addons screen: installed addons (`InstalledAddonsWithFilters`) and marketplace catalogs
//! (`CatalogWithFilters<Descriptor>`), plus the Add-Addon modal preview.

mod actions;

pub(crate) use actions::{
    configure_url, install, install_card, open_details, select, uninstall, InstallOutcome,
};

use stremio_core::models::addon_details::AddonDetails;
use stremio_core::models::common::Loadable;
use stremio_core::models::installed_addons_with_filters::InstalledAddonsRequest;
use stremio_core::types::addon::{Descriptor, Manifest, ResourceRequest};

use super::{capitalise, type_label, AppModel, SelectInput};

#[derive(Clone, Debug, PartialEq)]
pub struct AddonCard {
    pub transport_url: String,
    pub name: String,
    pub version: String,
    pub types: String,
    pub description: String,
    pub logo: Option<String>,
    pub installed: bool,
    pub configuration_required: bool,
    pub configurable: bool,
    pub descriptor: Descriptor,
}

/// What picking an option in one of the screen's two menus loads.
#[derive(Clone)]
pub enum Choice {
    Installed(InstalledAddonsRequest),
    Remote(ResourceRequest),
}

#[derive(Clone, Default)]
pub struct Data {
    /// Catalog menu ("Installed", Official, Community, ...) and type menu ("All", "Movie", ...),
    /// each paired with the `Choice` every option loads.
    pub inputs: Vec<SelectInput<Choice>>,
    pub cards: Vec<AddonCard>,
    pub remote: bool,
    pub loading: bool,
    pub message: String,
    pub pending: Option<Pending>,
}

/// The "add addon" dialog preview (`AddonDetailsModal`).
#[derive(Clone, Default)]
pub struct Pending {
    pub transport_url: String,
    pub name: String,
    pub version: String,
    pub types: String,
    pub description: String,
    pub logo: Option<String>,
    pub ready: bool,
    pub configuration_required: bool,
    pub installed: bool,
    pub error: String,
}

struct ManifestSummary {
    name: String,
    version: String,
    types: String,
    description: String,
    logo: Option<String>,
    configuration_required: bool,
    configurable: bool,
}

impl From<&Manifest> for ManifestSummary {
    fn from(manifest: &Manifest) -> Self {
        Self {
            name: if manifest.name.is_empty() {
                manifest.id.clone()
            } else {
                manifest.name.clone()
            },
            version: format!("v.{}", manifest.version),
            types: types_label(&manifest.types),
            description: manifest.description.clone().unwrap_or_default(),
            logo: manifest.logo.as_ref().map(|url| url.to_string()),
            configuration_required: manifest.behavior_hints.configuration_required,
            configurable: manifest.behavior_hints.configurable,
        }
    }
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
    let s = ManifestSummary::from(&addon.manifest);
    AddonCard {
        transport_url: addon.transport_url.to_string(),
        name: s.name,
        version: s.version,
        types: s.types,
        description: s.description,
        logo: s.logo,
        installed,
        configuration_required: s.configuration_required,
        configurable: s.configurable,
        descriptor: addon.clone(),
    }
}

fn is_installed(model: &AppModel, addon: &Descriptor) -> bool {
    model
        .ctx
        .profile
        .addons
        .iter()
        .any(|installed| installed.transport_url == addon.transport_url)
}

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

pub fn data(model: &AppModel) -> Data {
    let installed = &model.installed_addons;
    let remote = &model.remote_addons;
    let remote_selected = remote.selected.is_some();

    let mut catalog_items = vec![(
        "Installed".to_owned(),
        !remote_selected,
        Choice::Installed(InstalledAddonsRequest { r#type: None }),
    )];
    let mut seen: Vec<(String, String)> = Vec::new();
    for sel in &remote.selectable.catalogs {
        let key = (sel.request.base.to_string(), sel.request.path.id.clone());
        if !seen.contains(&key) {
            seen.push(key);
            catalog_items.push((
                sel.catalog.clone(),
                sel.selected,
                Choice::Remote(sel.request.clone()),
            ));
        }
    }
    let catalog_title = catalog_items
        .iter()
        .find(|(_, selected, _)| *selected)
        .map_or_else(|| "Installed".to_owned(), |(label, _, _)| label.clone());
    let catalog = SelectInput::new(catalog_title, catalog_items.into_iter());

    let kinds = if remote_selected {
        let types = &remote.selectable.types;
        let title = types
            .iter()
            .find(|t| t.selected)
            .or_else(|| types.first())
            .map_or_else(String::new, |t| capitalise(&t.r#type));
        SelectInput::new(
            title,
            types.iter().map(|t| {
                (
                    capitalise(&t.r#type),
                    t.selected,
                    Choice::Remote(t.request.clone()),
                )
            }),
        )
    } else {
        let types = &installed.selectable.types;
        let title = types
            .iter()
            .find(|t| t.selected)
            .or_else(|| types.first())
            .map_or_else(|| "All".to_owned(), |t| type_label(&t.r#type));
        SelectInput::new(
            title,
            types.iter().map(|t| {
                (
                    type_label(&t.r#type),
                    t.selected,
                    Choice::Installed(t.request.clone()),
                )
            }),
        )
    };

    let loading = remote_selected
        && remote
            .catalog
            .iter()
            .any(|p| !matches!(p.content, Some(Loadable::Ready(_)) | Some(Loadable::Err(_))));
    let cards = if remote_selected {
        let mut cards: Vec<AddonCard> = Vec::new();
        for addon in market_addons(model) {
            if !cards
                .iter()
                .any(|c| c.transport_url == addon.transport_url.as_str())
            {
                cards.push(card_of(addon, is_installed(model, addon)));
            }
        }
        cards
    } else {
        installed.catalog.iter().map(|a| card_of(a, true)).collect()
    };
    let message = if cards.is_empty() && !loading {
        "No addons for this type.".to_owned()
    } else {
        String::new()
    };
    Data {
        inputs: vec![catalog, kinds],
        cards,
        remote: remote_selected,
        loading,
        message,
        pending: pending(&model.addon_details),
    }
}

fn pending(details: &AddonDetails) -> Option<Pending> {
    let selected = details.selected.as_ref()?;
    let mut out = Pending {
        transport_url: selected.transport_url.to_string(),
        installed: details.local_addon.is_some(),
        ..Pending::default()
    };
    match details.remote_addon.as_ref().map(|a| &a.content) {
        Some(Loadable::Ready(addon)) => {
            let s = ManifestSummary::from(&addon.manifest);
            out.name = s.name;
            out.version = s.version;
            out.types = s.types;
            out.description = s.description;
            out.logo = s.logo;
            out.configuration_required = s.configuration_required;
            out.ready = true;
        }
        Some(Loadable::Err(error)) => out.error = format!("{error}"),
        _ => {}
    }
    Some(out)
}

/// Cards whose name or description contains `query` (case-insensitive).
pub(crate) fn visible<'a>(cards: &'a [AddonCard], query: &str) -> Vec<&'a AddonCard> {
    let query = query.to_lowercase();
    cards
        .iter()
        .filter(|c| {
            query.is_empty()
                || c.name.to_lowercase().contains(&query)
                || c.description.to_lowercase().contains(&query)
        })
        .collect()
}

#[cfg(test)]
mod tests;
