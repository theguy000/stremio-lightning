//! Board screen view projection: Spotlight hero slides and catalog rows via `ModelMirror`.

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use super::sync::{image_rev, poster_data, slint_image, with_mirrors, ModelMirror};
use crate::plumbing::images::Images;
use crate::screens::board;
use crate::{BoardUi, CatalogRow, MainWindow, SpotlightSlide};

pub(super) fn apply_board(window: &MainWindow, board: &crate::app::BoardData, images: &Images) {
    sync_board_rows(window, &board.rows, images);
    let slides: Vec<SpotlightSlide> = board
        .spotlight
        .iter()
        .map(|s| {
            let bg = slint_image(images, &s.background);
            let logo = slint_image(images, &s.logo);
            SpotlightSlide {
                id: s.id.clone().into(),
                kind: s.kind.clone().into(),
                title: s.title.clone().into(),
                has_background: bg.is_some(),
                background: bg.unwrap_or_default(),
                has_logo: logo.is_some(),
                logo: logo.unwrap_or_default(),
                year: s.year.clone().into(),
                runtime: s.runtime.clone().into(),
                imdb_rating: s.imdb_rating.clone().into(),
                genres: s.genres.clone().into(),
                description: s.description.clone().into(),
                in_library: s.in_library,
            }
        })
        .collect();
    window
        .global::<BoardUi>()
        .set_spotlight(ModelRc::new(VecModel::from(slides)));
}

fn sync_board_rows(window: &MainWindow, rows: &[board::Row], images: &Images) {
    let ui = window.global::<BoardUi>();
    let existing = ui.get_rows();
    let updated_in_place = with_mirrors(|mirrors| {
        let Some(model) = existing.as_any().downcast_ref::<VecModel<CatalogRow>>() else {
            return false;
        };
        if model.row_count() != rows.len()
            || mirrors.board_rows.len() != rows.len()
            || !rows
                .iter()
                .zip(mirrors.board_rows.iter())
                .all(|(r, (t, s, m, _))| r.title == *t && r.shape == *s && r.menu == *m)
        {
            return false;
        }
        for (index, (row, (_, _, _, mirror))) in
            rows.iter().zip(mirrors.board_rows.iter_mut()).enumerate()
        {
            if let Some(mut existing_row) = model.row_data(index) {
                if let Some(replaced) = mirror.sync(
                    &existing_row.cards,
                    &row.cards,
                    |_, card| (card.clone(), image_rev(images, &card.poster)),
                    |_, card| poster_data(card, images),
                ) {
                    existing_row.cards = replaced;
                    model.set_row_data(index, existing_row);
                }
            }
        }
        true
    });
    if updated_in_place {
        return;
    }
    let catalog_rows = with_mirrors(|mirrors| {
        mirrors.board_rows.clear();
        rows.iter()
            .map(|row| {
                let mut mirror = ModelMirror::default();
                let cards = mirror
                    .sync(
                        &ModelRc::default(),
                        &row.cards,
                        |_, card| (card.clone(), image_rev(images, &card.poster)),
                        |_, card| poster_data(card, images),
                    )
                    .unwrap_or_default();
                mirrors
                    .board_rows
                    .push((row.title.clone(), row.shape, row.menu, mirror));
                CatalogRow {
                    title: row.title.clone().into(),
                    shape: row.shape.into(),
                    menu: row.menu,
                    cards,
                }
            })
            .collect::<Vec<_>>()
    });
    ui.set_rows(ModelRc::new(VecModel::from(catalog_rows)));
}
