//! The Board: optional Spotlight hero carousel, Continue Watching, then addon catalog rows.
//! Based on stremio-web src/routes/Board (Board.js, CatalogRow.js, Spotlight).

pub mod spotlight;
mod spotlight_settings;

use stremio_core::models::common::Loadable;
use stremio_core::models::continue_watching_preview::ContinueWatchingPreview;

pub use spotlight::{SpotlightCatalogOption, SpotlightItem, SpotlightSettingsView};
pub use spotlight_settings::group_catalogs;

use super::{card_of, card_of_library_item, catalog_title, library_poster_shape, shape_of, Card};
use crate::plumbing::local_settings::LocalExtras;
use crate::AppModel;

const CATALOG_PREVIEW_SIZE: usize = 10;
const CONTINUE_WATCHING_TITLE: &str = "Continue Watching";

#[derive(Clone)]
pub struct Row {
    pub title: String,
    pub shape: &'static str,
    pub cards: Vec<Card>,
    pub menu: bool,
}

/// Projects Board with default local settings (Spotlight disabled).
pub fn data(model: &AppModel) -> (String, Vec<Row>) {
    let (status, rows, _) = data_with_extras(model, &LocalExtras::default());
    (status, rows)
}

/// Projects Board status, catalog rows (hiding Spotlight source rows if configured), and Spotlight slides.
pub fn data_with_extras(
    model: &AppModel,
    extras: &LocalExtras,
) -> (String, Vec<Row>, Vec<SpotlightItem>) {
    let (slides, hidden_picks) = spotlight::project_board(model, &extras.spotlight);
    let mut ranked_catalogs: Vec<(usize, Row)> = Vec::new();
    let (mut ready, mut loading, mut failed) = (0, 0, 0);
    for catalog in &model.board.catalogs {
        let Some(page) = catalog.first() else {
            continue;
        };
        let pick = spotlight::pick_from_request(&page.request);
        let hidden =
            hidden_picks.contains(&pick) || extras.content.hidden_board_catalogs.contains(&pick);
        match &page.content {
            Some(Loadable::Ready(items)) => {
                ready += 1;
                if !hidden {
                    let rank = crate::screens::settings::content::catalog_rank(
                        &extras.content.catalog_order,
                        &pick,
                    );
                    ranked_catalogs.push((
                        rank,
                        Row {
                            title: catalog_title(model, &page.request),
                            shape: shape_of(items),
                            cards: items
                                .iter()
                                .take(CATALOG_PREVIEW_SIZE)
                                .map(card_of)
                                .collect(),
                            menu: false,
                        },
                    ));
                }
            }
            Some(Loadable::Err(_)) => failed += 1,
            _ => loading += 1,
        }
    }
    ranked_catalogs.sort_by_key(|(rank, _)| *rank);
    let mut catalogs: Vec<Row> = ranked_catalogs.into_iter().map(|(_, row)| row).collect();
    let status = format!(
        "{} addons, {} catalogs ready, {} loading, {} failed",
        model.ctx.profile.addons.len(),
        ready,
        loading,
        failed
    );
    let mut rows = continue_watching_row(model, &model.continue_watching_preview)
        .into_iter()
        .collect::<Vec<_>>();
    rows.append(&mut catalogs);
    (status, rows, slides)
}

fn continue_watching_row(model: &AppModel, preview: &ContinueWatchingPreview) -> Option<Row> {
    let items = preview
        .items
        .iter()
        .take(CATALOG_PREVIEW_SIZE)
        .map(|item| &item.library_item)
        .collect::<Vec<_>>();
    items.first()?;
    Some(Row {
        title: CONTINUE_WATCHING_TITLE.to_owned(),
        shape: items
            .first()
            .map_or("poster", |item| library_poster_shape(&item.poster_shape)),
        cards: items
            .iter()
            .map(|item| card_of_library_item(model, item, false))
            .collect(),
        menu: true,
    })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub scroll: f32,
    pub height: f32,
    pub content_width: f32,
    pub viewport_width: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            scroll: 0.0,
            height: 0.0,
            content_width: 0.0,
            viewport_width: 1280.0,
        }
    }
}

fn rem_size(viewport_width: f32) -> f32 {
    if viewport_width > 2800.0 {
        18.0
    } else if viewport_width <= 1600.0 {
        14.0
    } else if viewport_width <= 2200.0 {
        15.0
    } else {
        16.0
    }
}

fn poster_items(viewport_width: f32) -> usize {
    if viewport_width <= 800.0 {
        4
    } else if viewport_width <= 1000.0 {
        5
    } else if viewport_width <= 1300.0 {
        6
    } else if viewport_width <= 1600.0 {
        7
    } else if viewport_width <= 1900.0 {
        8
    } else if viewport_width <= 2200.0 {
        9
    } else {
        10
    }
}

/// Posters for visible Board rows plus pinned Spotlight hero artwork.
pub fn visible_images_with_spotlight(
    rows: &[Row],
    spotlight: &[SpotlightItem],
    viewport: Viewport,
) -> Vec<String> {
    let mut out = Vec::new();
    for slide in spotlight {
        if let Some(bg) = &slide.background {
            out.push(bg.clone());
        }
        if let Some(logo) = &slide.logo {
            out.push(logo.clone());
        }
    }
    if rows.is_empty() {
        return out;
    }
    let vw = if viewport.viewport_width > 0.0 {
        viewport.viewport_width
    } else {
        1280.0
    };
    let rem = rem_size(vw);
    let posters = poster_items(vw);
    let visible_count = |shape: &str| {
        if shape == "landscape" && posters < 10 {
            posters - 1
        } else {
            posters
        }
    };
    if viewport.height <= 0.0 || viewport.content_width <= 0.0 {
        out.extend(rows.iter().take(4).flat_map(|row| {
            row.cards
                .iter()
                .take(visible_count(row.shape))
                .filter_map(|card| card.poster.clone())
        }));
        return out;
    }
    let row_width = (viewport.content_width - 2.0 * rem).max(1.0);
    let spacing = 2.0 * rem;
    let hero_offset = if spotlight.is_empty() {
        0.0
    } else {
        (vw * 0.38).clamp(26.0 * rem, 38.0 * rem) + spacing
    };
    let mut y = rem + hero_offset;
    for row in rows {
        let count = visible_count(row.shape);
        let ratio = match row.shape {
            "square" => 1.0,
            "landscape" => 0.5625,
            _ => 1.464,
        };
        let item_width = row_width / count as f32;
        let item_height = 2.0 * rem + (item_width - 2.0 * rem).max(0.0) * ratio + 4.0 * rem;
        let row_height = 2.75 * rem + item_height;
        if y + row_height >= viewport.scroll - row_height
            && y <= viewport.scroll + viewport.height + row_height
        {
            out.extend(
                row.cards
                    .iter()
                    .take(count)
                    .filter_map(|card| card.poster.clone()),
            );
        }
        y += row_height + spacing;
    }
    out
}

pub fn visible_images(rows: &[Row], viewport: Viewport) -> Vec<String> {
    visible_images_with_spotlight(rows, &[], viewport)
}

#[cfg(test)]
mod spotlight_tests;

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: usize) -> Row {
        Row {
            title: format!("Row {id}"),
            shape: "poster",
            cards: (0..10)
                .map(|card| Card {
                    id: format!("r{id}c{card}"),
                    kind: "movie".to_owned(),
                    title: format!("Card {card}"),
                    poster: Some(format!("https://example.com/r{id}c{card}.jpg")),
                    progress: 0.0,
                    watched: false,
                    playable: false,
                })
                .collect(),
            menu: false,
        }
    }

    #[test]
    fn visible_images_caps_horizontally_and_drops_far_rows() {
        let rows: Vec<Row> = (0..12).map(row).collect();
        let at_top = visible_images(
            &rows,
            Viewport {
                scroll: 0.0,
                height: 700.0,
                content_width: 1200.0,
                viewport_width: 1280.0,
            },
        );
        assert!(at_top.contains(&"https://example.com/r0c5.jpg".to_owned()));
        assert!(!at_top.contains(&"https://example.com/r0c6.jpg".to_owned()));
        assert!(!at_top.contains(&"https://example.com/r10c0.jpg".to_owned()));
    }
}
