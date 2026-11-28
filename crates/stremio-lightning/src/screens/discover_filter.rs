//! Discover catalog fallback and preferred-first selection under `ContentSettings`.

use stremio_core::types::addon::{ResourcePath, ResourceRequest};

use super::settings::content::{catalog_key, discover_rank};
use crate::app::AppModel;
use crate::plumbing::local_settings::ContentSettings;

pub fn first_visible_for_type(
    model: &AppModel,
    content: &ContentSettings,
    r#type: &str,
) -> Option<ResourceRequest> {
    let mut cands: Vec<(usize, ResourceRequest)> = Vec::new();
    for addon in &model.ctx.profile.addons {
        for cat in &addon.manifest.catalogs {
            if cat.r#type != r#type {
                continue;
            }
            let Some(extra) = cat.default_required_extra() else {
                continue;
            };
            let req = ResourceRequest {
                base: addon.transport_url.clone(),
                path: ResourcePath {
                    resource: "catalog".to_owned(),
                    r#type: cat.r#type.clone(),
                    id: cat.id.clone(),
                    extra,
                },
            };
            let key = catalog_key(&req);
            if !content.hidden_discover_catalogs.contains(&key) {
                cands.push((discover_rank(content, &key), req));
            }
        }
    }
    cands.sort_by_key(|(rank, _)| *rank);
    cands.into_iter().next().map(|(_, req)| req)
}

/// Returns the #1 visible Discover catalog for the currently loaded Discover type when it differs
/// from the currently selected catalog (so reordering Discover updates the active view immediately).
pub fn active_type_first_visible(
    model: &AppModel,
    content: &ContentSettings,
) -> Option<ResourceRequest> {
    let selected = model.discover.selected.as_ref()?;
    let top = first_visible_for_type(model, content, &selected.request.path.r#type)?;
    (catalog_key(&top) != catalog_key(&selected.request)).then_some(top)
}

/// Returns the first visible Discover catalog request (same type first, then any visible type)
/// if the currently selected Discover catalog is hidden.
pub fn fallback_request_if_hidden(
    model: &AppModel,
    content: &ContentSettings,
) -> Option<ResourceRequest> {
    let catalogs = &model.discover.selectable.catalogs;
    let selected_hidden = model.discover.selected.as_ref().is_some_and(|s| {
        content
            .hidden_discover_catalogs
            .contains(&catalog_key(&s.request))
    }) || catalogs.iter().find(|c| c.selected).is_some_and(|c| {
        content
            .hidden_discover_catalogs
            .contains(&catalog_key(&c.request))
    });
    if !selected_hidden {
        return None;
    }
    let mut visible: Vec<_> = catalogs
        .iter()
        .filter(|c| {
            !content
                .hidden_discover_catalogs
                .contains(&catalog_key(&c.request))
        })
        .collect();
    visible.sort_by_key(|c| discover_rank(content, &catalog_key(&c.request)));
    if let Some(first) = visible.first() {
        return Some(first.request.clone());
    }
    for t in &model.discover.selectable.types {
        if let Some(req) = first_visible_for_type(model, content, &t.r#type) {
            return Some(req);
        }
    }
    None
}
