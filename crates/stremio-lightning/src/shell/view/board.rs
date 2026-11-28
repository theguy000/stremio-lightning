//! Board screen view projection: Spotlight hero slides and catalog rows via `ModelMirror`.

use std::cell::RefCell;
use std::collections::HashMap;

use slint::{
    ComponentHandle, Image, Model, ModelRc, Rgba8Pixel, SharedPixelBuffer, SharedString, VecModel,
};

use super::sync::{image_rev, poster_data, slint_image, with_mirrors, ModelMirror};
use crate::plumbing::images::{DecodedImage, Images};
use crate::screens::board::{self, SpotlightItem};
use crate::{BoardUi, CatalogRow, MainWindow, SpotlightSlide};

thread_local! {
    static TRIMMED_LOGOS: RefCell<HashMap<String, (u64, Image)>> = RefCell::new(HashMap::new());
}

/// Crops transparent margins (alpha <= 16, matching stremio-web `Logo.tsx::measureInk`) so every
/// Spotlight title logo sits flush-left above the metadata row.
fn trim_alpha(decoded: &DecodedImage) -> Image {
    let (w, h) = (decoded.width() as usize, decoded.height() as usize);
    let raw = decoded.as_raw();
    let (mut left, mut right, mut top, mut bottom) = (w, 0usize, h, 0usize);
    for y in 0..h {
        let row = y * w;
        for x in 0..w {
            if raw[(row + x) * 4 + 3] > 16 {
                left = left.min(x);
                right = right.max(x);
                top = top.min(y);
                bottom = bottom.max(y);
            }
        }
    }
    if right < left || bottom < top || (left == 0 && top == 0 && right + 1 == w && bottom + 1 == h)
    {
        return Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
            raw,
            decoded.width(),
            decoded.height(),
        ));
    }
    let (cw, ch) = (right - left + 1, bottom - top + 1);
    let mut out = Vec::with_capacity(cw * ch * 4);
    for y in top..=bottom {
        let start = (y * w + left) * 4;
        out.extend_from_slice(&raw[start..start + cw * 4]);
    }
    Image::from_rgba8(SharedPixelBuffer::<Rgba8Pixel>::clone_from_slice(
        &out, cw as u32, ch as u32,
    ))
}

fn trimmed_logo_image(images: &Images, key: &Option<String>) -> Option<Image> {
    let key = key.as_ref()?;
    let decoded = images.get(&Some(key.clone()))?;
    let rev = decoded.rev();
    TRIMMED_LOGOS.with(|cell| {
        let mut cache = cell.borrow_mut();
        if let Some((cached_rev, img)) = cache.get(key) {
            if *cached_rev == rev {
                return Some(img.clone());
            }
        }
        if cache.len() >= 32 {
            cache.clear();
        }
        let img = trim_alpha(&decoded);
        cache.insert(key.clone(), (rev, img.clone()));
        Some(img)
    })
}

pub(super) fn apply_board(window: &MainWindow, board: &crate::app::BoardData, images: &Images) {
    sync_board_rows(window, &board.rows, images);
    let ui = window.global::<BoardUi>();
    ui.set_spotlight_loading(board.spotlight_loading);
    if board.spotlight.is_empty() && !board.spotlight_loading {
        ui.set_scrolled_past_spotlight(false);
    }
    sync_spotlight(&ui, &board.spotlight, images);
}

fn to_slide(s: &SpotlightItem, images: &Images) -> SpotlightSlide {
    let bg = slint_image(images, &s.background);
    let logo = trimmed_logo_image(images, &s.logo);
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
        has_trailer: s.trailer_url.is_some(),
        trailer_url: s.trailer_url.clone().unwrap_or_default().into(),
    }
}

fn slide_key(items: &[SpotlightItem]) -> SharedString {
    items
        .iter()
        .map(|s| format!("{}:{}", s.kind, s.id))
        .collect::<Vec<_>>()
        .join("|")
        .into()
}

/// Updates `BoardUi.spotlight` in-place when slide IDs are unchanged so arriving artwork
/// images never rebuild the Slint repeater or reset active zoom/progress animations.
fn sync_spotlight(ui: &BoardUi<'_>, items: &[SpotlightItem], images: &Images) {
    let next_key = slide_key(items);
    let same_ids = ui.get_spotlight_key() == next_key;
    if same_ids {
        let existing = ui.get_spotlight();
        if let Some(model) = existing.as_any().downcast_ref::<VecModel<SpotlightSlide>>() {
            if model.row_count() == items.len() {
                let labels_rc = ui.get_spotlight_labels();
                let labels_vec = labels_rc.as_any().downcast_ref::<VecModel<SharedString>>();
                for (i, item) in items.iter().enumerate() {
                    let next = to_slide(item, images);
                    if model.row_data(i).as_ref() != Some(&next) {
                        model.set_row_data(i, next);
                    }
                    if let Some(lv) = labels_vec {
                        let lbl: SharedString = item.title.clone().into();
                        if lv.row_data(i).as_ref() != Some(&lbl) {
                            lv.set_row_data(i, lbl);
                        }
                    }
                }
                return;
            }
        }
    }
    let slides: Vec<SpotlightSlide> = items.iter().map(|s| to_slide(s, images)).collect();
    let labels: Vec<SharedString> = items.iter().map(|s| s.title.clone().into()).collect();
    ui.set_spotlight_key(next_key);
    ui.set_spotlight_labels(ModelRc::new(VecModel::from(labels)));
    ui.set_spotlight(ModelRc::new(VecModel::from(slides)));
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
