//! The Board: one row per catalog the addons offer, each with its first page of posters.
//! Based on stremio-web src/routes/Board (Board.js, CatalogRow.js).

use stremio_core::models::common::Loadable;

use super::{card_of, catalog_title, shape_of, Card};
use crate::AppModel;

/// stremio-web's `CATALOG_PREVIEW_SIZE`: how many cards a Board row keeps.
const CATALOG_PREVIEW_SIZE: usize = 10;

#[derive(Clone)]
pub(crate) struct Row {
    pub(crate) title: String,
    pub(crate) shape: &'static str,
    pub(crate) cards: Vec<Card>,
}

/// What the Board shows, and the line of text above it.
pub(crate) fn data(model: &AppModel) -> (String, Vec<Row>) {
    let mut rows = Vec::new();
    let (mut loading, mut failed) = (0, 0);
    for catalog in &model.board.catalogs {
        let Some(page) = catalog.first() else {
            continue;
        };
        match &page.content {
            Some(Loadable::Ready(items)) => rows.push(Row {
                title: catalog_title(model, &page.request),
                shape: shape_of(items),
                cards: items
                    .iter()
                    .take(CATALOG_PREVIEW_SIZE)
                    .map(card_of)
                    .collect(),
            }),
            Some(Loadable::Err(_)) => failed += 1,
            _ => loading += 1,
        }
    }
    let status = format!(
        "{} addons, {} catalogs ready, {} loading, {} failed",
        model.ctx.profile.addons.len(),
        rows.len(),
        loading,
        failed
    );
    (status, rows)
}
