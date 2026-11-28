//! Discover: the filter bar and the grid of cards the chosen catalog returned, plus the
//! preview panel for the selected card. Based on stremio-web src/routes/Discover
//! (Discover.js, useSelectableInputs.js) and MetaPreview.

use stremio_core::models::common::Loadable;
use stremio_core::types::addon::ResourceRequest;

pub use super::discover_filter::{
    active_type_first_visible, fallback_request_if_hidden, first_visible_for_type,
};
use super::settings::content::{catalog_key, custom_catalog_title, discover_rank};
use super::{capitalise, card_of, preview_of, shape_of, Card, Preview, SelectInput};
use crate::plumbing::local_settings::LocalExtras;
use crate::AppModel;

/// What the Discover screen shows: the labels of its first three inputs and every card loaded
/// so far.
#[derive(Clone, Default)]
pub struct Data {
    pub shape: &'static str,
    pub cards: Vec<Card>,
    pub inputs: Vec<SelectInput<ResourceRequest>>,
    pub previews: Vec<Preview>,
}

pub fn data(model: &AppModel) -> Data {
    data_with_extras(model, &LocalExtras::default())
}

pub fn data_with_extras(model: &AppModel, extras: &LocalExtras) -> Data {
    let discover = &model.discover;
    let content = &extras.content;
    let selected_hidden = discover.selected.as_ref().is_some_and(|s| {
        content
            .hidden_discover_catalogs
            .contains(&catalog_key(&s.request))
    }) || discover
        .selectable
        .catalogs
        .iter()
        .find(|c| c.selected)
        .is_some_and(|c| {
            content
                .hidden_discover_catalogs
                .contains(&catalog_key(&c.request))
        });
    let mut items: Vec<&stremio_core::types::resource::MetaItemPreview> = Vec::new();
    if !selected_hidden {
        for page in &discover.catalog {
            if let Some(Loadable::Ready(page_items)) = &page.content {
                items.extend(page_items);
            }
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
        let has_override =
            !content.hidden_discover_catalogs.is_empty() || !content.discover_order.is_empty();
        let type_opts: Vec<_> = selectable
            .types
            .iter()
            .filter_map(|item| {
                let req = if has_override {
                    let first = first_visible_for_type(model, content, &item.r#type);
                    if !content.hidden_discover_catalogs.is_empty() && first.is_none() {
                        return None;
                    }
                    first.unwrap_or_else(|| item.request.clone())
                } else {
                    item.request.clone()
                };
                Some((capitalise(&item.r#type), item.selected, req))
            })
            .collect();
        if !type_opts.is_empty() {
            inputs.push(SelectInput::new(title, type_opts.into_iter()));
        }
    }
    if !selectable.catalogs.is_empty() {
        let mut visible: Vec<_> = selectable
            .catalogs
            .iter()
            .filter(|item| {
                !content
                    .hidden_discover_catalogs
                    .contains(&catalog_key(&item.request))
            })
            .collect();
        visible.sort_by_key(|item| discover_rank(content, &catalog_key(&item.request)));
        if !visible.is_empty() {
            let label_of =
                |item: &stremio_core::models::catalog_with_filters::SelectableCatalog| {
                    custom_catalog_title(content, "discover", &catalog_key(&item.request))
                        .map(ToOwned::to_owned)
                        .unwrap_or_else(|| item.catalog.clone())
                };
            let title = visible
                .iter()
                .find(|item| item.selected)
                .map_or_else(|| "Select catalog".to_owned(), |item| label_of(item));
            inputs.push(SelectInput::new(
                title,
                visible
                    .into_iter()
                    .map(|item| (label_of(item), item.selected, item.request.clone())),
            ));
        }
    }
    if !selected_hidden {
        for extra in &selectable.extra {
            let title = extra
                .options
                .iter()
                .find(|option| option.selected)
                .and_then(|option| option.value.clone())
                .unwrap_or_else(|| capitalise(&extra.name));
            inputs.push(SelectInput::new(
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
    }
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

pub fn visible_range(scroll: f32, height: f32, pitch: f32, columns: i32) -> (usize, usize) {
    range(scroll, height, pitch, columns, 0.0)
}

pub fn settled_range(scroll: f32, height: f32, pitch: f32, columns: i32) -> (usize, usize) {
    range(scroll, height, pitch, columns, 1.0)
}

fn range(scroll: f32, height: f32, pitch: f32, columns: i32, slack: f32) -> (usize, usize) {
    let columns = usize::try_from(columns).unwrap_or(1).max(1);
    let pitch = if pitch > 0.0 { pitch } else { 1.0 };
    let height = if height > 0.0 { height } else { pitch * 3.0 };
    let first_row = ((scroll - slack * pitch) / pitch).floor().max(0.0) as usize;
    let last_row = ((scroll + height + slack * pitch).max(0.0) / pitch).floor() as usize;
    (first_row * columns, (last_row + 1 - first_row) * columns)
}

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
    use super::{card_images, settled_range, visible_range, Data};
    use crate::screens::Card;

    #[test]
    fn what_is_on_screen_is_asked_for_at_once_and_the_slack_once_it_settles() {
        assert_eq!(visible_range(0.0, 720.0, 300.0, 4), (0, 12));
        assert_eq!(visible_range(600.0, 720.0, 300.0, 4), (8, 12));
        assert_eq!(settled_range(0.0, 720.0, 300.0, 4), (0, 16));
        assert_eq!(settled_range(600.0, 720.0, 300.0, 4), (4, 20));
    }

    #[test]
    fn an_unsized_grid_still_asks_for_a_screenful() {
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
                    watched: false,
                    playable: false,
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
        assert_eq!(card_images(&data, (20, 8)).count(), 0);
    }
}
