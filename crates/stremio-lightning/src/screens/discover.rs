//! Discover: the filter bar and the grid of cards the chosen catalog returned, plus the
//! preview panel for the selected card. Based on stremio-web src/routes/Discover
//! (Discover.js, useSelectableInputs.js) and MetaPreview.

use stremio_core::models::common::Loadable;

use super::{capitalise, card_of, preview_of, shape_of, Card, Input, Preview};
use crate::AppModel;

/// What the Discover screen shows: the labels of its first three inputs and every card loaded
/// so far.
#[derive(Clone, Default)]
pub struct Data {
    pub shape: &'static str,
    pub cards: Vec<Card>,
    pub inputs: Vec<Input>,
    pub previews: Vec<Preview>,
}

pub fn data(model: &AppModel) -> Data {
    let discover = &model.discover;
    let mut items: Vec<&stremio_core::types::resource::MetaItemPreview> = Vec::new();
    for page in &discover.catalog {
        if let Some(Loadable::Ready(page_items)) = &page.content {
            items.extend(page_items);
        }
    }
    let selectable = &discover.selectable;
    let mut inputs = Vec::new();
    if !selectable.types.is_empty() {
        let title = selectable
            .types
            .iter()
            .find(|item| item.selected)
            .map_or_else(|| "Select type".to_owned(), |item| capitalise(&item.r#type));
        inputs.push(Input::new(
            title,
            selectable.types.iter().map(|item| {
                (
                    capitalise(&item.r#type),
                    item.selected,
                    item.request.clone(),
                )
            }),
        ));
    }
    if !selectable.catalogs.is_empty() {
        let title = selectable
            .catalogs
            .iter()
            .find(|item| item.selected)
            .map_or_else(|| "Select catalog".to_owned(), |item| item.catalog.clone());
        inputs.push(Input::new(
            title,
            selectable
                .catalogs
                .iter()
                .map(|item| (item.catalog.clone(), item.selected, item.request.clone())),
        ));
    }
    for extra in &selectable.extra {
        let title = extra
            .options
            .iter()
            .find(|option| option.selected)
            .and_then(|option| option.value.clone())
            .unwrap_or_else(|| capitalise(&extra.name));
        inputs.push(Input::new(
            title,
            extra.options.iter().map(|option| {
                (
                    option.value.clone().unwrap_or_else(|| "None".to_owned()),
                    option.selected,
                    option.request.clone(),
                )
            }),
        ));
    }
    // The filter bar has room for three.
    inputs.truncate(3);
    Data {
        shape: shape_of(items.iter().copied()),
        cards: items.iter().copied().map(card_of).collect(),
        previews: items.iter().copied().map(preview_of).collect(),
        inputs,
    }
}

/// The image keys the preview panel needs for the card at `index`.
pub fn preview_images(data: &Data, index: usize) -> Vec<String> {
    data.previews
        .get(index)
        .into_iter()
        .flat_map(|preview| {
            preview
                .logo
                .iter()
                .chain(preview.background.iter())
                .cloned()
        })
        .collect()
}

/// Where the grid is scrolled to, in cards rather than pixels: the first card on screen and how
/// many of them are. `scroll`, `height`, `pitch` and `columns` come from the grid itself, which
/// reports them whenever it moves.
///
/// A row of slack is kept at each end, because a card arriving just out of sight is what a scroll
/// would show next; on an unsized grid (`height` of nothing, which is what the first report from a
/// window that has not been laid out looks like) a screenful is assumed so the screen is not left
/// blank.
pub fn visible_range(scroll: f32, height: f32, pitch: f32, columns: i32) -> (usize, usize) {
    let columns = usize::try_from(columns).unwrap_or(1).max(1);
    let pitch = if pitch > 0.0 { pitch } else { 1.0 };
    let height = if height > 0.0 { height } else { pitch * 3.0 };
    let first_row = ((scroll - pitch).max(0.0) / pitch).floor() as usize;
    let last_row = ((scroll + height + pitch).max(0.0) / pitch).floor() as usize;
    (first_row * columns, (last_row + 1 - first_row) * columns)
}

/// The posters of the cards that are on screen. The rest of a catalog is left alone until it is
/// scrolled to: a page of Discover can be a hundred cards, and asking for all of them at once was
/// both what the image host choked on and the only reason a browser looks faster than this --
/// it fetches what is visible (`loading="lazy"`) and nothing more.
pub fn card_images<'a>(
    data: &'a Data,
    (first, count): (usize, usize),
) -> impl Iterator<Item = String> + 'a {
    data.cards
        .iter()
        .skip(first)
        .take(count)
        .filter_map(|card| card.poster.clone())
}


#[cfg(test)]
mod tests {
    use super::{card_images, visible_range, Data};
    use crate::screens::Card;

    #[test]
    fn the_range_is_what_is_on_screen_plus_a_row_each_side() {
        // Four columns, rows 300px apart, a 720px grid at the top: three rows are visible and one
        // row of slack is kept below.
        assert_eq!(visible_range(0.0, 720.0, 300.0, 4), (0, 16));
        // Scrolled two rows down: one row of slack above, and the range follows the scroll.
        assert_eq!(visible_range(600.0, 720.0, 300.0, 4), (4, 20));
    }

    #[test]
    fn an_unsized_grid_still_asks_for_a_screenful() {
        // The first report from a window that has not been laid out yet has no height at all;
        // treating that as "nothing is visible" would leave the screen blank.
        let (_, count) = visible_range(0.0, 0.0, 300.0, 4);
        assert!(count >= 12, "expected a screenful, got {count}");
    }

    #[test]
    fn only_the_cards_in_the_range_are_asked_for() {
        let data = Data {
            shape: "poster",
            cards: (0..10)
                .map(|index| Card {
                    id: format!("tt{index}"),
                    kind: "movie".to_owned(),
                    title: format!("Card {index}"),
                    poster: Some(format!("https://example.com/{index}.jpg")),
                    progress: 0.0,
                })
                .collect(),
            inputs: Vec::new(),
            previews: Vec::new(),
        };
        assert_eq!(
            card_images(&data, (2, 3)).collect::<Vec<_>>(),
            vec![
                "https://example.com/2.jpg",
                "https://example.com/3.jpg",
                "https://example.com/4.jpg"
            ]
        );
        // A range past the end of the catalog asks for nothing rather than panicking.
        assert_eq!(card_images(&data, (20, 8)).count(), 0);
    }
}
