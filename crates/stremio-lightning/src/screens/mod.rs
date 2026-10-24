//! One module per screen. Each holds that screen's data (plain structs and the functions that
//! build them from the app state) and, beside it, the actions the screen can ask for.
//!
//! Screens never touch the window. They are given a model and return what to show; the wiring
//! in `shell` turns that into Slint properties.
//!
//! This file holds the vocabulary the screens share: a card, a filter, a preview, and the
//! handful of conversions several screens need.

pub mod addons;
pub mod auth;
pub mod board;
pub mod details;
pub mod discover;
pub mod library;
pub mod player;
pub mod settings;

use stremio_core::constants::IMDB_LINK_CATEGORY;
use stremio_core::types::addon::ResourceRequest;
use stremio_core::types::resource::{MetaItemPreview, PosterShape};

use crate::{AppModel, SelectInputData, SelectOption};

/// One poster tile.
#[derive(Clone)]
pub struct Card {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub poster: Option<String>,
    /// How far into the title the viewer is, as a percentage: the Continue Watching row draws it as
    /// a bar, and every other row's cards are zero.
    pub progress: f64,
    /// Whether the account has watched the title: the Continue Watching row's cards are library
    /// items, and the poster carries the check mark the web draws for one.
    pub watched: bool,
}

/// One filter in a screen's bar (type, catalog, or an extra such as genre): the closed label,
/// the options, and the request each option loads.
#[derive(Clone, Default)]
pub struct Input {
    pub title: String,
    pub selected: usize,
    pub options: Vec<(String, bool)>,
    pub requests: Vec<ResourceRequest>,
}

impl Input {
    fn new(title: String, options: impl Iterator<Item = (String, bool, ResourceRequest)>) -> Self {
        let mut input = Self {
            title,
            ..Self::default()
        };
        for (index, (label, selected, request)) in options.enumerate() {
            if selected {
                input.selected = index;
            }
            input.options.push((label, selected));
            input.requests.push(request);
        }
        input
    }
}

/// What a preview panel shows for one card. `logo` and `background` are image-cache keys.
#[derive(Clone, Default)]
pub struct Preview {
    pub title: String,
    pub runtime: String,
    pub release: String,
    pub rating: String,
    pub description: String,
    pub groups: Vec<(String, Vec<String>)>,
    pub has_trailer: bool,
    pub logo: Option<String>,
    pub background: Option<String>,
}

/// "Popular - Movie": the catalog's name (the addon's name when the catalog has none, as
/// stremio-core-web does) and the capitalised type, like stremio-web's `catalogTitle` without
/// its translations.
pub(crate) fn catalog_title(model: &AppModel, request: &ResourceRequest) -> String {
    let name = model
        .ctx
        .profile
        .addons
        .iter()
        .find(|addon| addon.transport_url == request.base)
        .and_then(|addon| {
            addon
                .manifest
                .catalogs
                .iter()
                .find(|catalog| {
                    catalog.id == request.path.id && catalog.r#type == request.path.r#type
                })
                .map(|catalog| {
                    catalog
                        .name
                        .clone()
                        .unwrap_or_else(|| addon.manifest.name.clone())
                })
        });
    let Some(name) = name else {
        return String::new();
    };
    format!("{name} - {}", capitalise(&request.path.r#type))
}

pub(crate) fn capitalise(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

pub(crate) fn card_of(item: &MetaItemPreview) -> Card {
    Card {
        id: item.id.clone(),
        kind: item.r#type.clone(),
        title: item.name.clone(),
        poster: item.poster.as_ref().map(|url| url.to_string()),
        progress: 0.0,
        watched: false,
    }
}

pub(crate) fn shape_of<'a>(items: impl IntoIterator<Item = &'a MetaItemPreview>) -> &'static str {
    match items.into_iter().next().map(|item| &item.poster_shape) {
        Some(PosterShape::Square) => "square",
        Some(PosterShape::Landscape) => "landscape",
        _ => "poster",
    }
}

/// Cinemeta writes punctuation as HTML entities in the names it returns -- a cast member comes
/// back as `Josh O&apos;Connor` and the panel showed exactly that. Nothing in a name is markup, so
/// undoing the entities that actually appear in them is all this has to do.
pub(crate) fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    const ENTITIES: [(&str, &str); 6] = [
        ("&apos;", "'"),
        ("&quot;", "\""),
        ("&amp;", "&"),
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&#39;", "'"),
    ];
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        match ENTITIES.iter().find(|(entity, _)| after.starts_with(entity)) {
            Some((entity, replacement)) => {
                out.push_str(replacement);
                rest = &after[entity.len()..];
            }
            None => {
                out.push('&');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Builds the preview panel's content from a catalog item, like stremio-web's `MetaPreview`.
pub(crate) fn preview_of(item: &MetaItemPreview) -> Preview {
    let mut rating = String::new();
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for link in &item.links {
        let name = decode_entities(&link.name);
        match link.category.as_str() {
            IMDB_LINK_CATEGORY => rating = name,
            category
                if category.eq_ignore_ascii_case("share")
                    || category.eq_ignore_ascii_case("writers") => {}
            category => match groups.iter_mut().find(|(label, _)| label == category) {
                Some((_, names)) => names.push(name),
                None => groups.push((category.to_owned(), vec![name])),
            },
        }
    }
    for (label, _) in &mut groups {
        *label = label.to_uppercase();
    }
    Preview {
        title: decode_entities(&item.name),
        runtime: item.runtime.clone().unwrap_or_default(),
        release: item
            .release_info
            .clone()
            .or_else(|| item.released.map(|date| date.format("%Y").to_string()))
            .unwrap_or_default(),
        rating,
        description: item
            .description
            .as_deref()
            .map(decode_entities)
            .unwrap_or_default(),
        groups,
        has_trailer: !item.trailer_streams.is_empty(),
        logo: item
            .logo
            .as_ref()
            .map(|url| format!("{url}{}", crate::plumbing::images::LOGO_SUFFIX)),
        // Web: the panel's backdrop is the *poster*, blurred -- `renderMetaPreview` passes
        // `background={selectedMetaItem.poster}` and never the wide artwork, which is what the
        // details page paints across the screen instead.
        background: item
            .poster
            .as_ref()
            .map(|url| format!("{url}{}", crate::plumbing::images::BLUR_SUFFIX)),
    }
}

/// One filter of a screen's bar as Slint wants it.
pub fn select_input_data(input: Input) -> SelectInputData {
    SelectInputData {
        title: input.title.into(),
        selected_index: i32::try_from(input.selected).unwrap_or(0),
        options: slint::ModelRc::new(slint::VecModel::from(
            input
                .options
                .into_iter()
                .map(|(label, selected)| SelectOption {
                    label: label.into(),
                    selected,
                })
                .collect::<Vec<_>>(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::decode_entities;

    #[test]
    fn entities_in_a_name_are_undone() {
        assert_eq!(decode_entities("Josh O&apos;Connor"), "Josh O'Connor");
        assert_eq!(decode_entities("Dungeons &amp; Dragons"), "Dungeons & Dragons");
        assert_eq!(decode_entities("&quot;Weird&quot; Al"), "\"Weird\" Al");
        // One pass, so an escaped ampersand does not become an entity.
        assert_eq!(decode_entities("&amp;apos;"), "&apos;");
        // Anything that is not one of these entities is left alone.
        assert_eq!(decode_entities("Fish & Chips"), "Fish & Chips");
        assert_eq!(decode_entities("&notanentity;"), "&notanentity;");
        assert_eq!(decode_entities("No ampersand"), "No ampersand");
    }
}
