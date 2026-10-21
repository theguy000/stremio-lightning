// What the Addons screen shows: the installed addons from stremio-core's `InstalledAddonsWithFilters`.
// Based on stremio-web src/routes/Addons (Addons.js, Addon/Addon.js, useSelectableInputs.js).
// Licence basis: ../../THIRD_PARTY_NOTICES.md.
use stremio_core::models::installed_addons_with_filters::{InstalledAddonsRequest, SelectableType};
use stremio_core::types::addon::Descriptor;

use super::{capitalise, AppModel, Input};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AddonCard {
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
    }
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
}
