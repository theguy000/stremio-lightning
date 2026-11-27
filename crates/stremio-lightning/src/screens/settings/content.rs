//! Pure Content Settings view projection and catalog ordering helpers.

use stremio_core::constants::STREAM_RESOURCE_NAME;
use stremio_core::types::addon::{ManifestResource, ResourcePath, ResourceRequest};

use crate::app::AppModel;
use crate::plumbing::local_settings::{CatalogKey, ContentSettings, LocalExtras};
use crate::screens::{capitalise, catalog_title};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CatalogContentRow {
    pub key: CatalogKey,
    pub title: String,
    pub addon_name: String,
    pub can_board: bool,
    pub show_on_board: bool,
    pub can_discover: bool,
    pub show_on_discover: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StreamAddonEntry {
    pub transport_url: String,
    pub name: String,
    pub types_label: String,
    pub enabled: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StreamPresetView {
    pub id: String,
    pub name: String,
    pub include_csv: String,
    pub exclude_csv: String,
    pub summary: String,
    pub active: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContentSettingsView {
    pub catalogs: Vec<CatalogContentRow>,
    pub has_custom_catalogs: bool,
    pub stream_addons: Vec<StreamAddonEntry>,
    pub presets: Vec<StreamPresetView>,
    pub active_preset_id: String,
}

pub fn catalog_key(req: &ResourceRequest) -> CatalogKey {
    CatalogKey {
        transport_url: req.base.to_string(),
        r#type: req.path.r#type.clone(),
        id: req.path.id.clone(),
    }
}

/// Sort key for a catalog: explicit index in `order`, else `usize::MAX` (manifest order preserved).
pub fn catalog_rank(order: &[CatalogKey], key: &CatalogKey) -> usize {
    order.iter().position(|k| k == key).unwrap_or(usize::MAX)
}

fn preset_summary(include: &[String], exclude: &[String]) -> String {
    match (include.is_empty(), exclude.is_empty()) {
        (true, true) => "No tag rules".to_owned(),
        (false, true) => format!("Include: {}", include.join(", ")),
        (true, false) => format!("Exclude: {}", exclude.join(", ")),
        (false, false) => format!(
            "Include: {} · Exclude: {}",
            include.join(", "),
            exclude.join(", ")
        ),
    }
}

/// Projects installed catalogs, stream-capable addons, and stream filter presets for Settings.
pub fn project(model: &AppModel, extras: &LocalExtras) -> ContentSettingsView {
    let content = &extras.content;
    let mut catalogs = Vec::new();
    for addon in &model.ctx.profile.addons {
        for cat in &addon.manifest.catalogs {
            let can_board = cat.is_extra_supported(&[]);
            let can_discover = cat.default_required_extra().is_some();
            if !can_board && !can_discover {
                continue;
            }
            let req = ResourceRequest {
                base: addon.transport_url.clone(),
                path: ResourcePath::without_extra("catalog", &cat.r#type, &cat.id),
            };
            let key = catalog_key(&req);
            catalogs.push(CatalogContentRow {
                title: catalog_title(model, &req),
                addon_name: addon.manifest.name.clone(),
                can_board,
                show_on_board: !content.hidden_board_catalogs.contains(&key),
                can_discover,
                show_on_discover: !content.hidden_discover_catalogs.contains(&key),
                key,
            });
        }
    }
    catalogs.sort_by_key(|row| catalog_rank(&content.catalog_order, &row.key));

    let stream_addons = model
        .ctx
        .profile
        .addons
        .iter()
        .filter(|addon| {
            addon.manifest.resources.iter().any(|r| match r {
                ManifestResource::Short(name) | ManifestResource::Full { name, .. } => {
                    name == STREAM_RESOURCE_NAME
                }
            })
        })
        .map(|addon| {
            let url = addon.transport_url.to_string();
            let types_label = addon
                .manifest
                .types
                .iter()
                .map(|t| capitalise(t))
                .collect::<Vec<_>>()
                .join(", ");
            StreamAddonEntry {
                enabled: !content.disabled_stream_addons.contains(&url),
                transport_url: url,
                name: addon.manifest.name.clone(),
                types_label,
            }
        })
        .collect();

    let active_id = content.active_stream_preset.as_deref().unwrap_or("");
    let presets = content
        .stream_presets
        .iter()
        .map(|p| StreamPresetView {
            id: p.id.clone(),
            name: p.name.clone(),
            include_csv: p.include_tags.join(", "),
            exclude_csv: p.exclude_tags.join(", "),
            summary: preset_summary(&p.include_tags, &p.exclude_tags),
            active: p.id == active_id,
        })
        .collect();

    let has_custom_catalogs = has_custom_catalog_settings(content);
    ContentSettingsView {
        catalogs,
        has_custom_catalogs,
        stream_addons,
        presets,
        active_preset_id: active_id.to_owned(),
    }
}

fn has_custom_catalog_settings(content: &ContentSettings) -> bool {
    !content.catalog_order.is_empty()
        || !content.hidden_board_catalogs.is_empty()
        || !content.hidden_discover_catalogs.is_empty()
}
