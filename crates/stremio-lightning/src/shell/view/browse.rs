//! Board, Discover, Library, Addons, and Calendar view projections via `ModelMirror`.

use slint::{ComponentHandle, Model, ModelRc, VecModel};

use super::sync::{
    image_rev, poster_data, preview_data, select_input_data, slint_image, with_mirrors, ModelMirror,
};
use crate::plumbing::images::Images;
use crate::screens::{addons, board, calendar, discover, library, SelectMenu};
use crate::{
    AddonData, AddonPendingData, AddonsUi, BoardUi, CalendarData, CalendarDay, CalendarSlot,
    CatalogRow, DiscoverUi, LibraryCard, LibrarySort, LibraryUi, MainWindow, SelectOption,
};

pub(super) fn apply_board(window: &MainWindow, board: &crate::app::BoardData, images: &Images) {
    sync_board_rows(window, &board.rows, images);
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

pub(super) fn apply_discover(window: &MainWindow, discover: &discover::Data, images: &Images) {
    let ui = window.global::<DiscoverUi>();
    ui.set_shape(discover.shape.into());
    let selected = usize::try_from(ui.get_selected_index()).unwrap_or(0);
    let menus: Vec<SelectMenu> = discover.inputs.iter().map(|i| i.menu.clone()).collect();
    with_mirrors(|m| {
        if let Some(replaced) = m.discover_cards.sync(
            &ui.get_cards(),
            &discover.cards,
            |_, card| (card.clone(), image_rev(images, &card.poster)),
            |_, card| poster_data(card, images),
        ) {
            ui.set_cards(replaced);
        }
        if let Some(replaced) = m.discover_inputs.sync(
            &ui.get_inputs(),
            &menus,
            |_, menu| menu.clone(),
            |_, menu| select_input_data(menu),
        ) {
            ui.set_inputs(replaced);
        }
        if let Some(replaced) = m.discover_previews.sync(
            &ui.get_previews(),
            &discover.previews,
            |idx, p| {
                let (l, b) = if idx == selected {
                    (image_rev(images, &p.logo), image_rev(images, &p.background))
                } else {
                    (None, None)
                };
                (p.clone(), l, b)
            },
            |idx, p| preview_data(p, images, idx == selected),
        ) {
            ui.set_previews(replaced);
        }
    });
}

pub(super) fn apply_library(window: &MainWindow, library: &library::Data, images: &Images) {
    let ui = window.global::<LibraryUi>();
    ui.set_type_title(library.types.menu.title.clone().into());
    ui.set_type_index(i32::try_from(library.types.menu.selected).unwrap_or_default());
    let sorts: Vec<(String, bool)> = library
        .sorts
        .iter()
        .map(|s| (s.label.clone(), s.selected))
        .collect();
    with_mirrors(|m| {
        if let Some(replaced) = m.library_types.sync(
            &ui.get_type_options(),
            &library.types.menu.options,
            |_, opt| opt.clone(),
            |_, (label, selected)| SelectOption {
                label: label.clone().into(),
                selected: *selected,
            },
        ) {
            ui.set_type_options(replaced);
        }
        if let Some(replaced) = m.library_sorts.sync(
            &ui.get_sorts(),
            &sorts,
            |_, s| s.clone(),
            |_, (label, selected)| LibrarySort {
                label: label.clone().into(),
                selected: *selected,
            },
        ) {
            ui.set_sorts(replaced);
        }
        if let Some(replaced) = m.library_cards.sync(
            &ui.get_cards(),
            &library.cards,
            |_, card| (card.clone(), image_rev(images, &card.poster)),
            |_, card| {
                let poster = slint_image(images, &card.poster);
                LibraryCard {
                    item_id: card.id.clone().into(),
                    kind: card.kind.clone().into(),
                    title: card.title.clone().into(),
                    has_poster: poster.is_some(),
                    poster: poster.unwrap_or_default(),
                    shape: card.shape.into(),
                    progress: card.progress as f32,
                    watched: card.watched,
                    playable: card.playable,
                    new_videos: i32::try_from(card.new_videos).unwrap_or(i32::MAX),
                }
            },
        ) {
            ui.set_cards(replaced);
        }
    });
    ui.set_message(library.message.clone().into());
    ui.set_has_more(library.has_more);
    ui.set_grid_key(library.grid_key.clone().into());
}

pub(super) fn apply_addons(window: &MainWindow, addons_data: &addons::Data, images: &Images) {
    let ui = window.global::<AddonsUi>();
    let visible: Vec<addons::AddonCard> = addons::visible(&addons_data.cards, &ui.get_query())
        .into_iter()
        .cloned()
        .collect();
    let menus: Vec<SelectMenu> = addons_data.inputs.iter().map(|i| i.menu.clone()).collect();
    with_mirrors(|m| {
        if let Some(replaced) = m.addons_cards.sync(
            &ui.get_addons(),
            &visible,
            |_, card| (card.clone(), image_rev(images, &card.logo)),
            |_, card| {
                let logo = slint_image(images, &card.logo);
                AddonData {
                    transport_url: card.transport_url.clone().into(),
                    name: card.name.clone().into(),
                    version: card.version.clone().into(),
                    types: card.types.clone().into(),
                    description: card.description.clone().into(),
                    has_logo: logo.is_some(),
                    logo: logo.unwrap_or_default(),
                    installed: card.installed,
                    configuration_required: card.configuration_required,
                    configurable: card.configurable,
                }
            },
        ) {
            ui.set_addons(replaced);
        }
        if let Some(replaced) = m.addons_inputs.sync(
            &ui.get_inputs(),
            &menus,
            |_, menu| menu.clone(),
            |_, menu| select_input_data(menu),
        ) {
            ui.set_inputs(replaced);
        }
    });
    ui.set_pending(addon_pending(&addons_data.pending, images));
    ui.set_message(addons_data.message.clone().into());
}

fn addon_pending(pending: &Option<addons::Pending>, images: &Images) -> AddonPendingData {
    let Some(pending) = pending else {
        return AddonPendingData::default();
    };
    let logo = slint_image(images, &pending.logo);
    AddonPendingData {
        transport_url: pending.transport_url.clone().into(),
        name: pending.name.clone().into(),
        version: pending.version.clone().into(),
        types: pending.types.clone().into(),
        description: pending.description.clone().into(),
        has_logo: logo.is_some(),
        logo: logo.unwrap_or_default(),
        ready: pending.ready,
        configuration_required: pending.configuration_required,
        installed: pending.installed,
        error: pending.error.clone().into(),
    }
}

pub(super) fn calendar_view(data: &calendar::Data, images: &Images) -> CalendarData {
    let day = |day: &calendar::Day| CalendarDay {
        day: i32::try_from(day.day).unwrap_or(i32::MAX),
        today: day.today,
        heading: day.heading.clone().into(),
        slots: ModelRc::new(VecModel::from(
            day.slots
                .iter()
                .map(|slot| {
                    let poster = slint_image(images, &slot.poster);
                    let thumbnail = slint_image(images, &slot.thumbnail);
                    CalendarSlot {
                        video_id: slot.video_id.clone().into(),
                        meta_id: slot.meta_id.clone().into(),
                        kind: slot.kind.clone().into(),
                        title: slot.title.clone().into(),
                        name: slot.name.clone().into(),
                        label: slot.label.clone().into(),
                        has_poster: poster.is_some(),
                        poster: poster.unwrap_or_default(),
                        has_thumbnail: thumbnail.is_some(),
                        thumbnail: thumbnail.unwrap_or_default(),
                    }
                })
                .collect::<Vec<_>>(),
        )),
    };
    CalendarData {
        signed_in: data.signed_in,
        loading: data.loading,
        year: data.year,
        month: data.month.clone().into(),
        prev: data.prev.clone().into(),
        next: data.next.clone().into(),
        first_weekday: i32::try_from(data.first_weekday).unwrap_or_default(),
        days: ModelRc::new(VecModel::from(
            data.days.iter().map(day).collect::<Vec<_>>(),
        )),
        list: ModelRc::new(VecModel::from(
            data.list.iter().map(day).collect::<Vec<_>>(),
        )),
    }
}
