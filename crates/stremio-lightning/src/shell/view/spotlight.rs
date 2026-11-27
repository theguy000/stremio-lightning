//! Projection for the Spotlight Catalog Picker modal (tray chips + addon-grouped 3-col grid).

use slint::{ComponentHandle, ModelRc, VecModel};

use super::sync::slint_image;
use crate::plumbing::images::Images;
use crate::screens::board::group_catalogs;
use crate::screens::board::spotlight::{SpotlightCatalogOption, SpotlightSettingsView};
use crate::{
    MainWindow, SettingsUi, SpotlightCardRow3, SpotlightCatalogCardData, SpotlightCatalogGroup,
    SpotlightPickChip,
};

fn count_label(item_count: usize) -> String {
    match item_count {
        0 => "No artwork".to_owned(),
        1 => "1 title".to_owned(),
        n => format!("{n} titles"),
    }
}

fn card_data(
    idx: usize,
    cat: &SpotlightCatalogOption,
    images: &Images,
) -> SpotlightCatalogCardData {
    let bg = slint_image(images, &cat.preview_bg);
    let logo = slint_image(images, &cat.preview_logo);
    SpotlightCatalogCardData {
        catalog_index: idx as i32,
        name: cat.name.clone().into(),
        count_label: count_label(cat.item_count).into(),
        ready: cat.ready,
        selected_rank: cat.selected_rank as i32,
        is_default: cat.is_default,
        has_bg: bg.is_some(),
        bg: bg.unwrap_or_default(),
        has_logo: logo.is_some(),
        logo: logo.unwrap_or_default(),
        fallback_title: cat.preview_title.clone().into(),
    }
}

pub(crate) fn apply_spotlight_picker(
    window: &MainWindow,
    sp: &SpotlightSettingsView,
    images: &Images,
) {
    let ui = window.global::<SettingsUi>();

    let mut ranked: Vec<(usize, &SpotlightCatalogOption)> = sp
        .catalogs
        .iter()
        .enumerate()
        .filter(|(_, c)| c.selected_rank > 0)
        .collect();
    ranked.sort_by_key(|(_, c)| c.selected_rank);
    let picks: Vec<SpotlightPickChip> = ranked
        .into_iter()
        .map(|(idx, c)| SpotlightPickChip {
            rank: c.selected_rank as i32,
            title: c.name.clone().into(),
            catalog_index: idx as i32,
        })
        .collect();
    ui.set_spotlight_picks(ModelRc::new(VecModel::from(picks)));

    let query = ui.get_spotlight_search_query().to_string();
    let groups: Vec<SpotlightCatalogGroup> = group_catalogs(&sp.catalogs, &query)
        .into_iter()
        .map(|grp| {
            let count = match grp.catalogs.len() {
                1 => "1 catalog".to_owned(),
                n => format!("{n} catalogs"),
            };
            let rows: Vec<SpotlightCardRow3> = grp
                .catalogs
                .chunks(3)
                .map(|chunk| {
                    let c0 = card_data(chunk[0].0, chunk[0].1, images);
                    let (has_1, c1) = match chunk.get(1) {
                        Some((i, c)) => (true, card_data(*i, c, images)),
                        None => (false, SpotlightCatalogCardData::default()),
                    };
                    let (has_2, c2) = match chunk.get(2) {
                        Some((i, c)) => (true, card_data(*i, c, images)),
                        None => (false, SpotlightCatalogCardData::default()),
                    };
                    SpotlightCardRow3 {
                        c0,
                        has_1,
                        c1,
                        has_2,
                        c2,
                    }
                })
                .collect();
            SpotlightCatalogGroup {
                addon_name: grp.addon_name.into(),
                count_label: count.into(),
                rows: ModelRc::new(VecModel::from(rows)),
            }
        })
        .collect();
    ui.set_spotlight_groups(ModelRc::new(VecModel::from(groups)));
}
