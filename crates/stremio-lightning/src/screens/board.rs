//! The Board: Continue Watching, then one row per catalog the addons offer, each with its first
//! page of posters. Based on stremio-web src/routes/Board (Board.js, CatalogRow.js).

use stremio_core::models::common::Loadable;
use stremio_core::models::continue_watching_preview::ContinueWatchingPreview;
use stremio_core::types::library::LibraryItem;
use stremio_core::types::resource::PosterShape;

use super::{card_of, catalog_title, shape_of, Card};
use crate::AppModel;

/// stremio-web's `CATALOG_PREVIEW_SIZE`: how many cards a Board row keeps.
const CATALOG_PREVIEW_SIZE: usize = 10;

/// Web: `BOARD_CONTINUE_WATCHING`.
const CONTINUE_WATCHING_TITLE: &str = "Continue Watching";

#[derive(Clone)]
pub struct Row {
    pub title: String,
    pub shape: &'static str,
    pub cards: Vec<Card>,
}

/// What the Board shows, and the line of text above it.
pub fn data(model: &AppModel) -> (String, Vec<Row>) {
    let mut catalogs = Vec::new();
    let (mut ready, mut loading, mut failed) = (0, 0, 0);
    for catalog in &model.board.catalogs {
        let Some(page) = catalog.first() else {
            continue;
        };
        match &page.content {
            Some(Loadable::Ready(items)) => {
                ready += 1;
                catalogs.push(Row {
                    title: catalog_title(model, &page.request),
                    shape: shape_of(items),
                    cards: items
                        .iter()
                        .take(CATALOG_PREVIEW_SIZE)
                        .map(card_of)
                        .collect(),
                });
            }
            Some(Loadable::Err(_)) => failed += 1,
            _ => loading += 1,
        }
    }
    let status = format!(
        "{} addons, {} catalogs ready, {} loading, {} failed",
        model.ctx.profile.addons.len(),
        ready,
        loading,
        failed
    );
    // Web: Board.js draws the continued titles above the addon rows, and only while there are
    // any, so a board with nothing started looks exactly as it did before.
    let mut rows = continue_watching_row(&model.continue_watching_preview)
        .into_iter()
        .collect::<Vec<_>>();
    rows.append(&mut catalogs);
    (status, rows)
}

/// The row the web puts first: what the account left unfinished, newest first, as the core's own
/// preview holds it. A client that has never started a title -- and every signed-out one -- has
/// nothing here.
fn continue_watching_row(preview: &ContinueWatchingPreview) -> Option<Row> {
    let items = preview
        .items
        .iter()
        .take(CATALOG_PREVIEW_SIZE)
        .map(|item| &item.library_item)
        .collect::<Vec<_>>();
    items.first()?;
    Some(Row {
        title: CONTINUE_WATCHING_TITLE.to_owned(),
        shape: shape_of_library_items(items.iter().copied()),
        cards: items.iter().map(|item| card_of_library_item(item)).collect(),
    })
}

/// A library item's own poster shape, with one web-ism kept: stremio-core's preview serialiser
/// turns a Landscape poster into a Square one, because a landscape shape in a row of posters
/// reads as a mistake (`serialize_continue_watching_preview.rs`).
fn shape_of_library_items<'a>(
    items: impl IntoIterator<Item = &'a LibraryItem>,
) -> &'static str {
    match items.into_iter().next().map(|item| &item.poster_shape) {
        Some(PosterShape::Landscape | PosterShape::Square) => "square",
        _ => "poster",
    }
}

/// One card of the Continue Watching row. The core keeps the one watch position a title is
/// sitting on, and `progress()` is that position as a percentage of its length.
fn card_of_library_item(item: &LibraryItem) -> Card {
    Card {
        id: item.id.clone(),
        kind: item.r#type.clone(),
        title: item.name.clone(),
        poster: item.poster.as_ref().map(|url| url.to_string()),
        progress: item.progress(),
    }
}
