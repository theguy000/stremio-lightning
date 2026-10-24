//! The Library: the account's titles, filtered by type and sorted by the core's own
//! `LibraryWithFilters`. Based on stremio-web src/routes/Library (Library.js,
//! useSelectableInputs.js) and src/components/MetaItem.
//!
//! The screen only describes what to draw. Selecting a type or sort is a request to load, and
//! every menu action is a core action; `shell` is where both go.

use stremio_core::models::library_with_filters::{LibraryRequest, Sort};
use stremio_core::types::resource::PosterShape;

use super::{capitalise, AppModel};

/// One poster tile in the grid. The web's `MetaItem` draws the progress bar, the watched mark
/// and the new-episodes badge from these same numbers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Card {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub poster: Option<String>,
    /// Web: a Landscape poster is drawn as a Square one in the Library grid.
    pub shape: &'static str,
    pub progress: f64,
    pub watched: bool,
    /// How many new Videos the notifications bucket holds for this title, capped at 99 like
    /// `LibItem.js`.
    pub new_videos: u32,
}

/// One of the six sort chips. The web renders these as chips rather than a menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SortChoice {
    pub label: String,
    pub selected: bool,
    pub request: LibraryRequest,
}

/// What the Library screen shows.
#[derive(Clone, Default)]
pub struct Data {
    /// The type menu's closed label ("All", "Movie", "Series", ...).
    pub type_title: String,
    /// The type menu's options, in the core's order.
    pub type_options: Vec<(String, bool)>,
    /// What each type option loads, parallel to `type_options`.
    pub type_requests: Vec<LibraryRequest>,
    pub sorts: Vec<SortChoice>,
    pub cards: Vec<Card>,
    /// Web: `LIBRARY_NOT_LOADED` / `LIBRARY_EMPTY`; empty while there is something to draw.
    pub message: String,
    /// Changes when the type or sort changes, but not when another page is appended. The grid
    /// resets its scroll to the top on that change, as `Library.js` does when page 1 arrives.
    pub grid_key: String,
    pub has_more: bool,
}

/// Web: `SORT_LAST_WATCHED`, `SORT_NAME`, ... The labels are title-cased by the chip's CSS, so
/// these already carry the words the live client shows.
pub(crate) fn sort_label(sort: &Sort) -> &'static str {
    match sort {
        Sort::LastWatched => "Last Watched",
        Sort::Name => "A-Z",
        Sort::NameReverse => "Z-A",
        Sort::TimesWatched => "Most Watched",
        Sort::Watched => "Watched",
        Sort::NotWatched => "Not Watched",
    }
}

fn type_label(r#type: &Option<String>) -> String {
    r#type
        .as_deref()
        .map_or_else(|| "All".to_owned(), capitalise)
}

fn shape_of(poster_shape: &PosterShape) -> &'static str {
    match poster_shape {
        PosterShape::Square | PosterShape::Landscape => "square",
        _ => "poster",
    }
}

/// The notification count the badge shows: `LibItem.js` clamps it to 0..=99.
fn new_videos(model: &AppModel, id: &str) -> u32 {
    model
        .ctx
        .notifications
        .items
        .get(id)
        .map_or(0, |items| items.len().min(99) as u32)
}

pub fn data(model: &AppModel) -> Data {
    let library = &model.library;
    let selectable = &library.selectable;
    let selected_type = selectable
        .types
        .iter()
        .position(|item| item.selected)
        .unwrap_or(0);
    let type_title = selectable
        .types
        .get(selected_type)
        .map_or_else(|| "All".to_owned(), |item| type_label(&item.r#type));
    let message = if library.selected.is_none() {
        "Library not loaded!".to_owned()
    } else if library.catalog.is_empty() {
        "Empty Library".to_owned()
    } else {
        String::new()
    };
    let grid_key = library.selected.as_ref().map_or_else(
        || "none".to_owned(),
        |selected| format!("{:?}|{:?}", selected.request.r#type, selected.request.sort),
    );
    Data {
        type_title,
        type_options: selectable
            .types
            .iter()
            .map(|item| (type_label(&item.r#type), item.selected))
            .collect(),
        type_requests: selectable
            .types
            .iter()
            .map(|item| item.request.clone())
            .collect(),
        sorts: selectable
            .sorts
            .iter()
            .map(|item| SortChoice {
                label: sort_label(&item.sort).to_owned(),
                selected: item.selected,
                request: item.request.clone(),
            })
            .collect(),
        cards: library
            .catalog
            .iter()
            .map(|item| Card {
                id: item.id.clone(),
                kind: item.r#type.clone(),
                title: item.name.clone(),
                poster: item.poster.as_ref().map(|url| url.to_string()),
                shape: shape_of(&item.poster_shape),
                progress: item.progress(),
                watched: item.watched(),
                new_videos: new_videos(model, &item.id),
            })
            .collect(),
        message,
        grid_key,
        has_more: selectable.next_page.is_some(),
    }
}

/// The posters the grid draws, in the order it draws them.
pub fn card_images(data: &Data) -> impl Iterator<Item = String> + '_ {
    data.cards.iter().filter_map(|card| card.poster.clone())
}

/// The sort options' labels, parallel to the core's `selectable.sorts`. Used by the wiring to
/// find the request for a clicked chip.
pub fn sort_requests(data: &Data) -> Vec<LibraryRequest> {
    data.sorts.iter().map(|sort| sort.request.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sort_labels_are_the_words_the_chips_show() {
        assert_eq!(sort_label(&Sort::LastWatched), "Last Watched");
        assert_eq!(sort_label(&Sort::Name), "A-Z");
        assert_eq!(sort_label(&Sort::NameReverse), "Z-A");
        assert_eq!(sort_label(&Sort::TimesWatched), "Most Watched");
        assert_eq!(sort_label(&Sort::Watched), "Watched");
        assert_eq!(sort_label(&Sort::NotWatched), "Not Watched");
    }

    #[test]
    fn a_landscape_poster_is_drawn_as_a_square_one_as_the_web_serializer_does() {
        assert_eq!(shape_of(&PosterShape::Landscape), "square");
        assert_eq!(shape_of(&PosterShape::Square), "square");
        assert_eq!(shape_of(&PosterShape::Poster), "poster");
    }
}
