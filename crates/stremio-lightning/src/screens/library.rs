//! The Library: the account's titles, filtered by type and sorted by the core's
//! `LibraryWithFilters`. Based on stremio-web src/routes/Library and src/components/MetaItem.

use std::ops::Deref;

use stremio_core::models::library_with_filters::{LibraryRequest, Sort};

use super::{card_of_library_item, library_poster_shape, type_label, AppModel, SelectInput};

/// One poster tile in the Library grid: a shared `Card` plus Library-specific shape and badge count.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Card {
    pub card: super::Card,
    /// Web: a Landscape poster is drawn as a Square one in the Library grid.
    pub shape: &'static str,
    /// How many new Videos the notifications bucket holds for this title, capped at 99.
    pub new_videos: u32,
}

impl Deref for Card {
    type Target = super::Card;
    fn deref(&self) -> &Self::Target {
        &self.card
    }
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
    /// The type filter dropdown and the `LibraryRequest` each option loads.
    pub types: SelectInput<LibraryRequest>,
    pub sorts: Vec<SortChoice>,
    pub cards: Vec<Card>,
    /// Web: `LIBRARY_NOT_LOADED` / `LIBRARY_EMPTY`; empty while there is something to draw.
    pub message: String,
    /// Changes when the type or sort changes, resetting scroll to the top.
    pub grid_key: String,
    pub has_more: bool,
}

fn sort_label(sort: &Sort) -> &'static str {
    match sort {
        Sort::LastWatched => "Last Watched",
        Sort::Name => "A-Z",
        Sort::NameReverse => "Z-A",
        Sort::TimesWatched => "Most Watched",
        Sort::Watched => "Watched",
        Sort::NotWatched => "Not Watched",
    }
}

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
    let type_title = selectable
        .types
        .iter()
        .find(|item| item.selected)
        .or_else(|| selectable.types.first())
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
        types: SelectInput::new(
            type_title,
            selectable.types.iter().map(|item| {
                (
                    type_label(&item.r#type),
                    item.selected,
                    item.request.clone(),
                )
            }),
        ),
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
                card: card_of_library_item(model, item, true),
                shape: library_poster_shape(&item.poster_shape),
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

#[cfg(test)]
mod tests {
    use super::*;
    use stremio_core::types::resource::PosterShape;

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
        assert_eq!(library_poster_shape(&PosterShape::Landscape), "square");
        assert_eq!(library_poster_shape(&PosterShape::Square), "square");
        assert_eq!(library_poster_shape(&PosterShape::Poster), "poster");
    }
}
