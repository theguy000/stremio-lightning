//! Mods Logs row reconciliation and in-place Plugin settings field preservation.

use slint::{Model, ModelRc, SharedString, VecModel};

use super::mods::mods_view;
use crate::screens::mods;
use crate::{MainWindow, ModsLogRow, ModsNavItem, ModsViewData, SettingsFieldData};

/// filtered row ids are unchanged and reporting live prepends so a scrolled reader stays anchored.
pub(crate) fn apply_logs_data(window: &MainWindow, logs: &mods::logs::Data) {
    let old = window.get_mods();
    let mut data = old.clone();
    data.log_source_index = logs.source_index;
    data.log_level_index = logs.level_index;
    data.log_search = logs.filter.search.clone().into();
    data.log_empty = logs.empty.clone().into();
    data.log_count = logs.count.clone().into();
    data.log_filtered = logs.filtered;
    data.log_detail_heading = logs.detail_heading.clone().into();
    data.log_detail = ModelRc::new(VecModel::from(
        logs.detail
            .iter()
            .map(|line| SharedString::from(line.clone()))
            .collect::<Vec<_>>(),
    ));
    data.log_open_id = i32::try_from(logs.open_id).unwrap_or(0);
    data.log_active_index = logs.active_index;
    data.log_selected_count = i32::try_from(logs.selected_count).unwrap_or(0);
    data.log_clear_armed = logs.clear_armed;
    reconcile_log_rows(&old, &logs.rows, &mut data);
    window.set_mods(data);
}

/// Keeps the existing `log_rows` model in place when the visible row ids are unchanged, and
/// detects live records prepended at the top under an unchanged filter so the UI can preserve
/// the reader's scroll offset.
fn reconcile_log_rows(old: &ModsViewData, rows: &[mods::logs::Row], next: &mut ModsViewData) {
    next.log_prepend_seq = old.log_prepend_seq;
    next.log_prepended = 0;
    let same_filter = old.log_source_index == next.log_source_index
        && old.log_level_index == next.log_level_index
        && old.log_search == next.log_search;
    let same_rows = old.log_rows.row_count() == rows.len()
        && rows.iter().enumerate().all(|(at, row)| {
            old.log_rows
                .row_data(at)
                .is_some_and(|existing| existing.id == row.id as i32)
        });
    if same_rows {
        if let Some(existing) = old.log_rows.as_any().downcast_ref::<VecModel<ModsLogRow>>() {
            for (at, row) in rows.iter().enumerate() {
                if let Some(mut cur) = existing.row_data(at) {
                    if cur.selected != row.selected {
                        cur.selected = row.selected;
                        existing.set_row_data(at, cur);
                    }
                }
            }
            next.log_rows = old.log_rows.clone();
            return;
        }
    } else if same_filter {
        if let Some(old_top) = old.log_rows.row_data(0).map(|row| row.id) {
            if let Some(prepended) = rows.iter().position(|row| row.id as i32 == old_top) {
                if prepended > 0 {
                    next.log_prepended = i32::try_from(prepended).unwrap_or(0);
                    next.log_prepend_seq = old.log_prepend_seq.wrapping_add(1);
                }
            }
        }
    }
    next.log_rows = ModelRc::new(VecModel::from(rows.iter().map(log_row).collect::<Vec<_>>()));
}

/// One record of the Logs page. A record id past what the window's integer holds keeps its low
/// bits, which still tell one row from its neighbours.
pub(super) fn log_row(row: &mods::logs::Row) -> ModsLogRow {
    ModsLogRow {
        id: row.id as i32,
        source: row.source.clone().into(),
        level: row.level.clone().into(),
        level_id: row.level_id.clone().into(),
        time: row.time.clone().into(),
        text: row.text.clone().into(),
        selected: row.selected,
    }
}


/// content itself. The menu is written only when its rows differ from the ones the window holds,
/// because a new model tears down every row -- including the one a press may be standing on. Which
/// row is marked is not in the model: the menu marks the page `data.page` names.
pub(crate) fn show_mods(window: &MainWindow, data: &crate::screens::mods::Data) {
    let old = window.get_mods();
    let mut view = mods_view(data);
    keep_settings_fields(window, &mut view);
    reconcile_log_rows(&old, &data.logs.rows, &mut view);
    let rows: Vec<(String, String)> = data
        .menu
        .iter()
        .map(|item| (item.id.clone(), item.label.clone()))
        .collect();
    if rows != window_nav_rows(window) {
        window.set_mods_nav(nav_rows(data));
    }
    window.set_mods_page(data.page.clone().into());
    window.set_mods(view);
}

/// Keeps the settings field rows the window already holds when the same plugin's page is drawn
/// again. A fresh model tears every row down, and the text box a person is typing into loses its
/// focus with them; the data of each row is updated in place instead, so the box survives the
/// repaint and the letters after the first are not typed into an editor that no longer exists.
fn keep_settings_fields(window: &MainWindow, view: &mut ModsViewData) {
    let old = window.get_mods();
    if old.settings_plugin != view.settings_plugin {
        return;
    }
    let model = old.settings_fields.clone();
    let Some(fields) = model.as_any().downcast_ref::<VecModel<SettingsFieldData>>() else {
        return;
    };
    let keys = |fields: &ModelRc<SettingsFieldData>| -> Vec<String> {
        (0..fields.row_count())
            .filter_map(|at| fields.row_data(at))
            .map(|field| field.key.to_string())
            .collect()
    };
    if keys(&view.settings_fields) != keys(&old.settings_fields) {
        return;
    }
    for at in 0..view.settings_fields.row_count() {
        if let Some(field) = view.settings_fields.row_data(at) {
            fields.set_row_data(at, field);
        }
    }
    view.settings_fields = model;
}

/// The menu's rows the window holds now, as pairs of id and label.
fn window_nav_rows(window: &MainWindow) -> Vec<(String, String)> {
    let nav = window.get_mods_nav();
    (0..nav.row_count())
        .filter_map(|row| nav.row_data(row))
        .map(|item| (item.id.to_string(), item.label.to_string()))
        .collect()
}

/// The menu's rows as the window holds them, compared by id and label: whether a row is marked
/// follows the page, so it is not part of what makes the model a new one.
type NavRows = slint::ModelRc<ModsNavItem>;

fn nav_rows(data: &crate::screens::mods::Data) -> NavRows {
    let rows: Vec<ModsNavItem> = data
        .menu
        .iter()
        .map(|item| ModsNavItem {
            id: item.id.clone().into(),
            label: item.label.clone().into(),
            selected: false,
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}
