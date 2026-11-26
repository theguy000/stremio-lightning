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
pub mod calendar;
pub mod details;
pub mod discover;
pub mod language_names;
pub mod library;
pub mod mods;
pub mod player;
mod preview;
pub mod settings;
pub mod videos;

pub(crate) use preview::preview_of;
pub use preview::Preview;
use stremio_core::types::addon::ResourceRequest;
use stremio_core::types::library::LibraryItem;
use stremio_core::types::resource::{MetaItemPreview, PosterShape};
use stremio_core::types::streams::StreamsItemKey;
use url::Url;

use crate::AppModel;

/// One poster tile.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Card {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub poster: Option<String>,
    /// Watch progress percentage (0..=100) for library/Continue Watching items; 0 elsewhere.
    pub progress: f64,
    /// Whether the title is marked watched (Library draws the badge; Continue Watching leaves it false).
    pub watched: bool,
    /// Whether a Stream for the title's current Video is already cached in `ctx.streams`.
    pub playable: bool,
}

/// Web: whether a title can be played from its card (`ctx.streams` holds a Stream for its Video).
pub(crate) fn can_play(model: &AppModel, item: &LibraryItem) -> bool {
    let Some(video_id) = item.state.video_id.as_ref() else {
        return false;
    };
    model.ctx.streams.items.contains_key(&StreamsItemKey {
        meta_id: item.id.clone(),
        video_id: video_id.clone(),
    })
}

/// Display data for one dropdown menu: closed label, selected index, and `(label, selected)` rows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SelectMenu {
    pub title: String,
    pub selected: usize,
    pub options: Vec<(String, bool)>,
}

/// A dropdown menu paired with the typed value each option carries (`ResourceRequest`,
/// `LibraryRequest`, `addons::Choice`, etc.).
#[derive(Clone, Debug)]
pub struct SelectInput<R> {
    pub menu: SelectMenu,
    pub values: Vec<R>,
}

impl<R> Default for SelectInput<R> {
    fn default() -> Self {
        Self {
            menu: SelectMenu::default(),
            values: Vec::new(),
        }
    }
}

impl<R> SelectInput<R> {
    pub(crate) fn new(title: String, items: impl Iterator<Item = (String, bool, R)>) -> Self {
        let mut out = Self {
            menu: SelectMenu {
                title,
                ..SelectMenu::default()
            },
            values: Vec::new(),
        };
        for (index, (label, selected, value)) in items.enumerate() {
            if selected {
                out.menu.selected = index;
            }
            out.menu.options.push((label, selected));
            out.values.push(value);
        }
        out
    }
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
        // A catalog's card is not a library item, so nothing has resolved a Stream for it: the web
        // offers no play button or Play row on those either.
        playable: false,
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
/// Display label for an optional content-type filter (`None` -> `"All"`, `Some(t)` -> capitalised).
pub(crate) fn type_label(kind: &Option<String>) -> String {
    kind.as_deref().map_or_else(|| "All".to_owned(), capitalise)
}

/// Poster shape for a library item (web serializer folds Landscape into Square).
pub(crate) fn library_poster_shape(shape: &PosterShape) -> &'static str {
    match shape {
        PosterShape::Square | PosterShape::Landscape => "square",
        _ => "poster",
    }
}

/// Builds a poster `Card` from a `LibraryItem`. Only Library passes `include_watched = true`;
/// Continue Watching leaves `watched = false` to match stremio-web's preview serializer.
pub(crate) fn card_of_library_item(
    model: &AppModel,
    item: &LibraryItem,
    include_watched: bool,
) -> Card {
    Card {
        id: item.id.clone(),
        kind: item.r#type.clone(),
        title: item.name.clone(),
        poster: item.poster.as_ref().map(|url| url.to_string()),
        progress: item.progress(),
        watched: include_watched && item.watched(),
        playable: can_play(model, item),
    }
}

/// Resolves an addon's manifest display name from `profile.addons`, falling back to its host.
pub(crate) fn addon_name(model: &AppModel, transport_url: &Url) -> String {
    model
        .ctx
        .profile
        .addons
        .iter()
        .find(|addon| &addon.transport_url == transport_url)
        .map(|addon| addon.manifest.name.clone())
        .unwrap_or_else(|| transport_url.host_str().unwrap_or_default().to_owned())
}

/// The two bits the details page's action group draws: an item flagged `removed` is not in the
/// library, and `watched` is the core's answer for the title as a whole.
pub fn library_state(model: &AppModel, id: &str) -> (bool, bool) {
    model
        .ctx
        .library
        .items
        .get(id)
        .map(|item| (!item.removed, item.watched()))
        .unwrap_or_default()
}
