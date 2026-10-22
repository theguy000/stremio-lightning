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
