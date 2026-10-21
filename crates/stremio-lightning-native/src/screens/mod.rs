//! One module per screen. Each holds that screen's data (plain structs and the functions that
//! build them from the app state) and, beside it, the actions the screen can ask for.
//!
//! Screens never touch the window. They are given a model and return what to show; the wiring
//! in `shell` turns that into Slint properties.
//!
//! This file holds the vocabulary the screens share: a card, a filter, a preview, and the
//! handful of conversions several screens need.

pub(crate) mod addons;
pub(crate) mod auth;
pub(crate) mod board;
pub(crate) mod details;
pub(crate) mod discover;
pub(crate) mod settings;

use stremio_core::constants::IMDB_LINK_CATEGORY;
use stremio_core::types::addon::ResourceRequest;
use stremio_core::types::resource::{MetaItemPreview, PosterShape};

use crate::{AppModel, SelectInputData, SelectOption};

/// One poster tile.
#[derive(Clone)]
pub(crate) struct Card {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) title: String,
    pub(crate) poster: Option<String>,
}

/// One filter in a screen's bar (type, catalog, or an extra such as genre): the closed label,
/// the options, and the request each option loads.
#[derive(Clone, Default)]
pub(crate) struct Input {
    pub(crate) title: String,
    pub(crate) selected: usize,
    pub(crate) options: Vec<(String, bool)>,
    pub(crate) requests: Vec<ResourceRequest>,
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
pub(crate) struct Preview {
    pub(crate) title: String,
    pub(crate) runtime: String,
    pub(crate) release: String,
    pub(crate) rating: String,
    pub(crate) description: String,
    pub(crate) groups: Vec<(String, Vec<String>)>,
    pub(crate) has_trailer: bool,
    pub(crate) logo: Option<String>,
    pub(crate) background: Option<String>,
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
    }
}

pub(crate) fn shape_of<'a>(items: impl IntoIterator<Item = &'a MetaItemPreview>) -> &'static str {
    match items.into_iter().next().map(|item| &item.poster_shape) {
        Some(PosterShape::Square) => "square",
        Some(PosterShape::Landscape) => "landscape",
        _ => "poster",
    }
}

/// Builds the preview panel's content from a catalog item, like stremio-web's `MetaPreview`.
pub(crate) fn preview_of(item: &MetaItemPreview) -> Preview {
    let mut rating = String::new();
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for link in &item.links {
        match link.category.as_str() {
            IMDB_LINK_CATEGORY => rating = link.name.clone(),
            category
                if category.eq_ignore_ascii_case("share")
                    || category.eq_ignore_ascii_case("writers") => {}
            category => match groups.iter_mut().find(|(label, _)| label == category) {
                Some((_, names)) => names.push(link.name.clone()),
                None => groups.push((category.to_owned(), vec![link.name.clone()])),
            },
        }
    }
    for (label, _) in &mut groups {
        *label = label.to_uppercase();
    }
    Preview {
        title: item.name.clone(),
        runtime: item.runtime.clone().unwrap_or_default(),
        release: item
            .release_info
            .clone()
            .or_else(|| item.released.map(|date| date.format("%Y").to_string()))
            .unwrap_or_default(),
        rating,
        description: item.description.clone().unwrap_or_default(),
        groups,
        has_trailer: !item.trailer_streams.is_empty(),
        logo: item
            .logo
            .as_ref()
            .map(|url| format!("{url}{}", crate::plumbing::images::LOGO_SUFFIX)),
        background: item
            .background
            .as_ref()
            .or(item.poster.as_ref())
            .map(|url| format!("{url}{}", crate::plumbing::images::BLUR_SUFFIX)),
    }
}

/// One filter of a screen's bar as Slint wants it.
pub(crate) fn select_input_data(input: Input) -> SelectInputData {
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
