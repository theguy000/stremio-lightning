//! Mods Screen (Plugins, Themes, Settings, About) view projection.

use slint::{ModelRc, SharedString, VecModel};

use super::mods_logs::log_row;
use super::sync::menu_options;
use crate::screens::mods::{self, ModCard};
use crate::{
    ModCardData, ModsAboutLine, ModsNavItem, ModsViewData, SettingsFieldData, SettingsPluginData,
};

/// The Mods section: its page menu, the line about the last action, and the plugins and themes
/// on their pages. What is armed for removal is the window's own state, not this snapshot's, so
/// it is left alone here and the cards carry whatever arm the wiring last set.
pub(crate) fn mods_view(data: &crate::screens::mods::Data) -> ModsViewData {
    let card = |kind: mods::catalogue::Kind, card: &ModCard| {
        let armed_here = data.armed.as_deref() == Some(card.filename.as_str());
        let state = match kind {
            mods::catalogue::Kind::Plugin => "enabled",
            mods::catalogue::Kind::Theme => "on",
        };
        ModCardData {
            id: card.id.clone().into(),
            filename: card.filename.clone().into(),
            name: mods::catalogue::title(card).into(),
            byline: mods::catalogue::byline(card).into(),
            description: card.description.clone().into(),
            unreadable: mods::catalogue::unreadable(card).into(),
            warnings: ModelRc::new(VecModel::from(
                if card.builtin {
                    Vec::new()
                } else {
                    card.warnings.clone()
                }
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
            )),
            enabled: card.enabled,
            armed: armed_here,
            state: state.into(),
            removable: card.removable,
            has_settings: card.has_settings,
        }
    };
    ModsViewData {
        page: data.page.clone().into(),
        menu: ModelRc::new(VecModel::from(
            data.menu
                .iter()
                .map(|item| ModsNavItem {
                    id: item.id.clone().into(),
                    label: item.label.clone().into(),
                    selected: item.selected,
                })
                .collect::<Vec<_>>(),
        )),
        notice: data.notice.text.clone().into(),
        notice_failed: data.notice.failed,
        plugins: ModelRc::new(VecModel::from(
            data.plugins
                .iter()
                .map(|card_data| card(mods::catalogue::Kind::Plugin, card_data))
                .collect::<Vec<_>>(),
        )),
        themes: ModelRc::new(VecModel::from(
            data.themes
                .iter()
                .map(|card_data| card(mods::catalogue::Kind::Theme, card_data))
                .collect::<Vec<_>>(),
        )),
        themes_note: data.themes_note.clone().into(),
        settings_plugins: ModelRc::new(VecModel::from(
            data.settings_plugins
                .iter()
                .map(settings_plugin)
                .collect::<Vec<_>>(),
        )),
        settings_plugin: data.plugin.clone().into(),
        settings_title: data
            .settings_plugins
            .iter()
            .find(|entry| entry.filename == data.plugin)
            .map(|entry| entry.name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| data.plugin.clone())
            .into(),
        settings_fields: ModelRc::new(VecModel::from(
            data.edits
                .controls
                .iter()
                .map(settings_field)
                .collect::<Vec<_>>(),
        )),
        settings_trouble: ModelRc::new(VecModel::from(
            trouble(data)
                .into_iter()
                .map(SharedString::from)
                .collect::<Vec<_>>(),
        )),
        settings_empty: empty_settings(data).into(),
        settings_dirty: data.edits.dirty(),
        log_source_options: menu_options(&data.logs.source_options, data.logs.source_index),
        log_level_options: menu_options(&data.logs.level_options, data.logs.level_index),
        log_source_index: data.logs.source_index,
        log_level_index: data.logs.level_index,
        log_search: data.logs.filter.search.clone().into(),
        log_rows: ModelRc::new(VecModel::from(
            data.logs.rows.iter().map(log_row).collect::<Vec<_>>(),
        )),
        log_empty: data.logs.empty.clone().into(),
        log_detail_heading: data.logs.detail_heading.clone().into(),
        log_open_id: i32::try_from(data.logs.open_id).unwrap_or(0),
        log_active_index: data.logs.active_index,
        log_selected_count: i32::try_from(data.logs.selected_count).unwrap_or(0),
        log_count: data.logs.count.clone().into(),
        log_filtered: data.logs.filtered,
        log_prepended: 0,
        log_prepend_seq: 0,
        log_detail: ModelRc::new(VecModel::from(
            data.logs
                .detail
                .iter()
                .map(|line| SharedString::from(line.clone()))
                .collect::<Vec<_>>(),
        )),
        log_clear_armed: data.logs.clear_armed,
        about_build: about_lines(&data.about.build),
        about_places: about_lines(&data.about.places),
        about_counts: about_lines(&data.about.counts),
        about_enabled: about_lines(&data.about.enabled),
        about_license: data.about.license.clone().into(),
        about_export: data.about.export.clone().into(),
    }
}


/// One section of the About page, as the rows it draws.
fn about_lines(lines: &[mods::about::Line]) -> ModelRc<ModsAboutLine> {
    ModelRc::new(VecModel::from(
        lines
            .iter()
            .map(|line| ModsAboutLine {
                label: line.label.clone().into(),
                value: line.value.clone().into(),
            })
            .collect::<Vec<_>>(),
    ))
}

/// One row of the Plugin settings page's left list.
fn settings_plugin(plugin: &mods::settings::Plugin) -> SettingsPluginData {
    SettingsPluginData {
        filename: plugin.filename.clone().into(),
        name: plugin.name.clone().into(),
        controls: match plugin.fields {
            0 => "no controls".to_owned(),
            1 => "1 control".to_owned(),
            count => format!("{count} controls"),
        }
        .into(),
        enabled: plugin.enabled,
    }
}

/// What a control on the Plugin settings page holds: the value the user is editing, made into
/// what its kind of control draws. The value the file holds is the screen's business.
fn settings_field(control: &mods::settings::Control) -> SettingsFieldData {
    let mut field = SettingsFieldData {
        key: control.field.key.into(),
        label: control.field.label.into(),
        kind: "text".into(),
        hint: control.hint().into(),
        on: false,
        text: "".into(),
        options: ModelRc::new(VecModel::from(Vec::<SharedString>::new())),
        number: 0.0,
        min: 0.0,
        max: 1.0,
        shown: "".into(),
    };
    match control.field.kind {
        mods::builtin::FieldKind::Toggle { .. } => {
            field.kind = "toggle".into();
            field.on = control.edited.as_bool().unwrap_or(false);
        }
        mods::builtin::FieldKind::Text { .. } => {
            field.kind = "text".into();
            field.text = control.edited.as_str().unwrap_or_default().into();
        }
    }
    field
}

fn trouble(data: &crate::screens::mods::Data) -> Vec<String> {
    let edits = &data.edits;
    if !edits.filename.is_empty() && !edits.error.is_empty() {
        vec![edits.error.clone()]
    } else {
        Vec::new()
    }
}

/// What the Plugin settings page says when it holds no control: nothing picked, or a plugin that
/// registered none.
fn empty_settings(data: &crate::screens::mods::Data) -> String {
    if data.plugin.is_empty() {
        mods::settings::NOTHING_PICKED.to_owned()
    } else if data.edits.controls.is_empty() {
        mods::settings::NO_SCHEMA.to_owned()
    } else {
        String::new()
    }
}

