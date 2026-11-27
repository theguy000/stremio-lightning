//! Projection from `screens::settings::content::ContentSettingsView` to Slint `SettingsUi`.

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::screens::settings::content::ContentSettingsView;
use crate::{ContentCatalogRow, MainWindow, SettingsUi, StreamAddonRow, StreamPresetRow};

pub(super) fn apply_content_settings(window: &MainWindow, view: &ContentSettingsView) {
    let ui = window.global::<SettingsUi>();
    let total = view.catalogs.len();
    let catalogs: Vec<ContentCatalogRow> = view
        .catalogs
        .iter()
        .enumerate()
        .map(|(i, c)| ContentCatalogRow {
            title: c.title.clone().into(),
            addon_name: c.addon_name.clone().into(),
            can_board: c.can_board,
            show_on_board: c.show_on_board,
            can_discover: c.can_discover,
            show_on_discover: c.show_on_discover,
            can_move_up: i > 0,
            can_move_down: i + 1 < total,
        })
        .collect();
    let addons: Vec<StreamAddonRow> = view
        .stream_addons
        .iter()
        .map(|a| StreamAddonRow {
            transport_url: a.transport_url.clone().into(),
            name: a.name.clone().into(),
            types_label: a.types_label.clone().into(),
            enabled: a.enabled,
        })
        .collect();
    let presets: Vec<StreamPresetRow> = view
        .presets
        .iter()
        .map(|p| StreamPresetRow {
            id: p.id.clone().into(),
            name: p.name.clone().into(),
            include_csv: p.include_csv.clone().into(),
            exclude_csv: p.exclude_csv.clone().into(),
            summary: p.summary.clone().into(),
            active: p.active,
        })
        .collect();

    ui.set_content_catalogs(ModelRc::new(VecModel::from(catalogs)));
    ui.set_content_has_custom_catalogs(view.has_custom_catalogs);
    ui.set_content_stream_addons(ModelRc::new(VecModel::from(addons)));
    ui.set_content_presets(ModelRc::new(VecModel::from(presets)));
    ui.set_content_active_preset_id(view.active_preset_id.clone().into());
}
