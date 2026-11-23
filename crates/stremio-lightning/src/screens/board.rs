//! The Board: Continue Watching, then one row per catalog the addons offer, each with its first
//! page of posters. Based on stremio-web src/routes/Board (Board.js, CatalogRow.js).

use stremio_core::models::common::Loadable;
use stremio_core::models::continue_watching_preview::ContinueWatchingPreview;
use stremio_core::types::library::LibraryItem;
use stremio_core::types::resource::PosterShape;

use super::{can_play, card_of, catalog_title, shape_of, Card};
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
    /// Whether the row's cards carry the action list. The web offers it in the Continue Watching
    /// row, whose cards are library items; an addon catalog's cards do not.
    pub menu: bool,
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
                    menu: false,
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
    let mut rows = continue_watching_row(model, &model.continue_watching_preview)
        .into_iter()
        .collect::<Vec<_>>();
    rows.append(&mut catalogs);
    (status, rows)
}

/// The row the web puts first: what the account left unfinished, newest first, as the core's own
/// preview holds it. A client that has never started a title -- and every signed-out one -- has
/// nothing here.
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
        shape: shape_of_library_items(items.iter().copied()),
        cards: items
            .iter()
            .map(|item| card_of_library_item(model, item))
            .collect(),
        menu: true,
    })
}

/// A library item's own poster shape, with one web-ism kept: stremio-core's preview serialiser
/// turns a Landscape poster into a Square one, because a landscape shape in a row of posters
/// reads as a mistake (`serialize_continue_watching_preview.rs`).
fn shape_of_library_items<'a>(items: impl IntoIterator<Item = &'a LibraryItem>) -> &'static str {
    match items.into_iter().next().map(|item| &item.poster_shape) {
        Some(PosterShape::Landscape | PosterShape::Square) => "square",
        _ => "poster",
    }
}

/// One card of the Continue Watching row. The core keeps the one watch position a title is
/// sitting on, and `progress()` is that position as a percentage of its length.
fn card_of_library_item(model: &AppModel, item: &LibraryItem) -> Card {
    Card {
        id: item.id.clone(),
        kind: item.r#type.clone(),
        title: item.name.clone(),
        poster: item.poster.as_ref().map(|url| url.to_string()),
        progress: item.progress(),
        // No `watched` flag: the web's continue-watching serialiser sends `_id`, `name`, `type`,
        // `poster`, `posterShape`, `progress`, `deepLinks` and `state.videoId` and nothing else, so
        // `LibItem` there is handed `watched: undefined` -- no check mark on a Continue Watching
        // card, and no "Mark as watched" row in its list. The core's own `watched()` is
        // `times_watched > 0`, which stays true while a title is being re-watched, so drawing it
        // here would check mark a title that is half way through an episode. Only the Library
        // passes it (`serialize_library.rs`), and only the Library draws it.
        watched: false,
        playable: can_play(model, item),
    }
}

/// Viewport snapshot reported by `BoardContent` whenever it scrolls or resizes.
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

/// Posters for the Board rows within the viewport (+ one row slack above and below), taking only
/// the cards visible horizontally (`visible-items` in `ui/board.slint`).
pub fn visible_images(rows: &[Row], viewport: Viewport) -> Vec<String> {
    if rows.is_empty() {
        return Vec::new();
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
        // Before the first layout report, keep the top screenful warm without pinning every row.
        return rows
            .iter()
            .take(4)
            .flat_map(|row| {
                row.cards
                    .iter()
                    .take(visible_count(row.shape))
                    .filter_map(|card| card.poster.clone())
            })
            .collect();
    }
    let row_width = (viewport.content_width - 2.0 * rem).max(1.0);
    let spacing = 2.0 * rem;
    let mut y = rem;
    let mut out = Vec::new();
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
        // 1280px shows 6 poster items per row; cards 6..10 are never visible.
        assert!(at_top.contains(&"https://example.com/r0c5.jpg".to_owned()));
        assert!(!at_top.contains(&"https://example.com/r0c6.jpg".to_owned()));
        assert!(!at_top.contains(&"https://example.com/r10c0.jpg".to_owned()));

        let scrolled = visible_images(
            &rows,
            Viewport {
                scroll: 2200.0,
                height: 700.0,
                content_width: 1200.0,
                viewport_width: 1280.0,
            },
        );
        assert!(!scrolled.contains(&"https://example.com/r0c0.jpg".to_owned()));
        assert!(scrolled.contains(&"https://example.com/r6c0.jpg".to_owned()));
    }
}
