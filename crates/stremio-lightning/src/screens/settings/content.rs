//! Pure Content Settings view projection matching Stremio's Organize content + Filter streams UI.

use stremio_core::constants::STREAM_RESOURCE_NAME;
use stremio_core::types::addon::{ManifestResource, ResourcePath, ResourceRequest};

use crate::app::AppModel;
use crate::plumbing::local_settings::{CatalogKey, ContentSettings, LocalExtras};
use crate::screens::catalog_title;

/// One row in Stremio's `Organize content` sortable list (Board, Discover, or Streams tab).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OrganizeItem {
    pub id: String,
    pub label: String,
    pub default_label: String,
    pub icon: Option<String>,
    pub active: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StreamPresetView {
    pub id: String,
    pub name: String,
    pub include_tags: Vec<String>,
    pub exclude_tags: Vec<String>,
    pub active: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContentSettingsView {
    pub board_items: Vec<OrganizeItem>,
    pub discover_items: Vec<OrganizeItem>,
    pub search_items: Vec<OrganizeItem>,
    pub stream_items: Vec<OrganizeItem>,
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

/// Stable string key used in `ContentSettings.custom_titles` and drag/toggle callbacks.
pub fn catalog_item_id(tab: &str, key: &CatalogKey) -> String {
    format!("{tab}|{}|{}|{}", key.transport_url, key.r#type, key.id)
}

pub fn parse_catalog_item_id(id: &str) -> Option<(&str, CatalogKey)> {
    let mut parts = id.splitn(4, '|');
    let tab = parts.next()?;
    let transport_url = parts.next()?.to_owned();
    let r#type = parts.next()?.to_owned();
    let cat_id = parts.next()?.to_owned();
    Some((
        tab,
        CatalogKey {
            transport_url,
            r#type,
            id: cat_id,
        },
    ))
}

/// Sort key for a catalog: explicit index in `order`, else `usize::MAX` (manifest order preserved).
pub fn catalog_rank(order: &[CatalogKey], key: &CatalogKey) -> usize {
    order.iter().position(|k| k == key).unwrap_or(usize::MAX)
}

/// Sort key for Discover catalogs (independent from Board's `catalog_order`).
pub fn discover_rank(content: &ContentSettings, key: &CatalogKey) -> usize {
    catalog_rank(&content.discover_order, key)
}

/// Sort key for stream addons: explicit index in `stream_order`, else `usize::MAX`.
pub fn stream_addon_rank(order: &[String], transport_url: &str) -> usize {
    order
        .iter()
        .position(|u| u == transport_url)
        .unwrap_or(usize::MAX)
}

/// Custom user title override for a Board or Discover catalog, if renamed in `Organize content`.
pub fn custom_catalog_title<'a>(
    content: &'a ContentSettings,
    tab: &str,
    key: &CatalogKey,
) -> Option<&'a str> {
    content
        .custom_titles
        .get(&catalog_item_id(tab, key))
        .map(String::as_str)
}

fn default_catalog_label(model: &AppModel, addon_name: &str, req: &ResourceRequest) -> String {
    let cat = catalog_title(model, req);
    if cat.is_empty() || cat.eq_ignore_ascii_case(addon_name) {
        addon_name.to_owned()
    } else {
        format!("{addon_name} - {cat}")
    }
}

fn catalog_row(
    content: &ContentSettings,
    tab: &str,
    key: &CatalogKey,
    default_label: &str,
    icon: &Option<String>,
    hidden: &[CatalogKey],
) -> OrganizeItem {
    let id = catalog_item_id(tab, key);
    let label = content
        .custom_titles
        .get(&id)
        .cloned()
        .unwrap_or_else(|| default_label.to_owned());
    OrganizeItem {
        id,
        label,
        default_label: default_label.to_owned(),
        icon: icon.clone(),
        active: !hidden.contains(key),
    }
}

/// Projects installed Board, Discover, Search catalogs, Stream addons, and Stream Filter Presets.
pub fn project(model: &AppModel, extras: &LocalExtras) -> ContentSettingsView {
    let content = &extras.content;
    let mut board_rows: Vec<(usize, OrganizeItem)> = Vec::new();
    let mut discover_rows: Vec<(usize, OrganizeItem)> = Vec::new();
    let mut search_rows: Vec<(usize, OrganizeItem)> = Vec::new();
    let mut stream_rows: Vec<(usize, OrganizeItem)> = Vec::new();

    for addon in &model.ctx.profile.addons {
        let icon = addon.manifest.logo.as_ref().map(ToString::to_string);
        for cat in &addon.manifest.catalogs {
            let req = ResourceRequest {
                base: addon.transport_url.clone(),
                path: ResourcePath::without_extra("catalog", &cat.r#type, &cat.id),
            };
            let key = catalog_key(&req);
            let def = default_catalog_label(model, &addon.manifest.name, &req);
            if cat.is_extra_supported(&[]) {
                board_rows.push((
                    catalog_rank(&content.catalog_order, &key),
                    catalog_row(
                        content,
                        "board",
                        &key,
                        &def,
                        &icon,
                        &content.hidden_board_catalogs,
                    ),
                ));
            }
            if cat.default_required_extra().is_some() {
                discover_rows.push((
                    discover_rank(content, &key),
                    catalog_row(
                        content,
                        "discover",
                        &key,
                        &def,
                        &icon,
                        &content.hidden_discover_catalogs,
                    ),
                ));
            }
            if cat.extra.iter().any(|e| e.name == "search") {
                search_rows.push((
                    catalog_rank(&content.search_order, &key),
                    catalog_row(
                        content,
                        "search",
                        &key,
                        &def,
                        &icon,
                        &content.hidden_search_catalogs,
                    ),
                ));
            }
        }
        let has_stream = addon.manifest.resources.iter().any(|r| match r {
            ManifestResource::Short(name) | ManifestResource::Full { name, .. } => {
                name == STREAM_RESOURCE_NAME
            }
        });
        if has_stream {
            let url = addon.transport_url.to_string();
            let id = format!("streams|{url}");
            let def = addon.manifest.name.clone();
            let label = content
                .custom_titles
                .get(&id)
                .cloned()
                .unwrap_or_else(|| def.clone());
            stream_rows.push((
                stream_addon_rank(&content.stream_order, &url),
                OrganizeItem {
                    id,
                    label,
                    default_label: def,
                    icon,
                    active: !content.disabled_stream_addons.contains(&url),
                },
            ));
        }
    }

    board_rows.sort_by_key(|(rank, _)| *rank);
    discover_rows.sort_by_key(|(rank, _)| *rank);
    search_rows.sort_by_key(|(rank, _)| *rank);
    stream_rows.sort_by_key(|(rank, _)| *rank);

    let active_id = content.active_stream_preset.as_deref().unwrap_or("");
    let presets = content
        .stream_presets
        .iter()
        .map(|p| StreamPresetView {
            id: p.id.clone(),
            name: p.name.clone(),
            include_tags: p.include_tags.clone(),
            exclude_tags: p.exclude_tags.clone(),
            active: p.id == active_id,
        })
        .collect();

    ContentSettingsView {
        board_items: board_rows.into_iter().map(|(_, item)| item).collect(),
        discover_items: discover_rows.into_iter().map(|(_, item)| item).collect(),
        search_items: search_rows.into_iter().map(|(_, item)| item).collect(),
        stream_items: stream_rows.into_iter().map(|(_, item)| item).collect(),
        presets,
        active_preset_id: active_id.to_owned(),
    }
}
