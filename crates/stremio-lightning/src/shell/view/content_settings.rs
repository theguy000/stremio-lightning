//! Projection from `screens::settings::content::ContentSettingsView` to Slint `SettingsUi`.

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use super::sync::slint_image;
use crate::plumbing::images::Images;
use crate::screens::settings::content::{ContentSettingsView, OrganizeItem};
use crate::{ContentOrganizeRow, MainWindow, SettingsUi, StreamPresetRow};

fn to_organize_rows(items: &[OrganizeItem], images: &Images) -> ModelRc<ContentOrganizeRow> {
    let rows: Vec<ContentOrganizeRow> = items
        .iter()
        .map(|item| {
            let icon = slint_image(images, &item.icon);
            ContentOrganizeRow {
                id: item.id.clone().into(),
                label: item.label.clone().into(),
                has_icon: icon.is_some(),
                icon: icon.unwrap_or_default(),
                active: item.active,
            }
        })
        .collect();
    ModelRc::new(VecModel::from(rows))
}

fn to_string_model(items: &[String]) -> ModelRc<SharedString> {
    let vec: Vec<SharedString> = items.iter().map(|s| s.clone().into()).collect();
    ModelRc::new(VecModel::from(vec))
}

pub(super) fn apply_content_settings(
    window: &MainWindow,
    view: &ContentSettingsView,
    images: &Images,
) {
    let ui = window.global::<SettingsUi>();
    let presets: Vec<StreamPresetRow> = view
        .presets
        .iter()
        .map(|p| StreamPresetRow {
            id: p.id.clone().into(),
            name: p.name.clone().into(),
            include_tags: to_string_model(&p.include_tags),
            exclude_tags: to_string_model(&p.exclude_tags),
            active: p.active,
        })
        .collect();

    ui.set_content_board_items(to_organize_rows(&view.board_items, images));
    ui.set_content_discover_items(to_organize_rows(&view.discover_items, images));
    ui.set_content_search_items(to_organize_rows(&view.search_items, images));
    ui.set_content_stream_items(to_organize_rows(&view.stream_items, images));
    ui.set_content_presets(ModelRc::new(VecModel::from(presets)));
}

/// Scrolls the fixed-height Organize content list when the wheel pointer is inside its bounds.
pub(crate) fn wheel_content_list(
    window: &MainWindow,
    at: slint::LogicalPosition,
    down: f32,
) -> bool {
    let s = window.global::<SettingsUi>();
    let max_inner = (s.get_content_list_content_h() - s.get_content_list_h()).max(0.0);
    if max_inner <= 0.0 || s.get_active_page() != "content" || !s.get_search_query().is_empty() {
        return false;
    }
    let x = s.get_content_list_x();
    let y = s.get_content_list_y();
    if at.x < x
        || at.x > x + s.get_content_list_w()
        || at.y < y
        || at.y > y + s.get_content_list_h()
    {
        return false;
    }
    s.set_content_list_scroll((s.get_content_list_scroll() - down).clamp(-max_inner, 0.0));
    true
}
