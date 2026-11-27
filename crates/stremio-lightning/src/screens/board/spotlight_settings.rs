//! Settings -> Interface -> Board Spotlight preview and Catalog Picker projection.

use stremio_core::models::common::Loadable;
use stremio_core::types::addon::ResourceRequest;

use super::spotlight::{
    addon_name_for, default_picks, effective_picks, matches_pick, pick_from_request, project_board,
    SpotlightCatalogOption, SpotlightSettingsView,
};
use crate::app::AppModel;
use crate::plumbing::images::page_key;
use crate::plumbing::local_settings::{LocalExtras, SpotlightConfig};
use crate::screens::catalog_title;

/// Projects Settings -> Interface -> Board Spotlight group + Catalog Picker modal state.
pub fn settings_view(model: &AppModel, extras: &LocalExtras) -> SpotlightSettingsView {
    let cfg = &extras.spotlight;
    let pages: Vec<_> = model
        .board
        .catalogs
        .iter()
        .filter_map(|c| c.first())
        .collect();
    let requests: Vec<&ResourceRequest> = pages.iter().map(|p| &p.request).collect();
    let defaults = default_picks(&requests);
    let (picks, is_default) = effective_picks(&requests, cfg);

    let catalogs: Vec<SpotlightCatalogOption> = pages
        .iter()
        .map(|page| {
            let pick = pick_from_request(&page.request);
            let selected_rank = picks
                .iter()
                .position(|p| p == &pick)
                .map_or(0, |idx| idx + 1);
            let (preview_bg, item_count) = match &page.content {
                Some(Loadable::Ready(items)) => {
                    let with_bg: Vec<_> = items.iter().filter(|m| m.background.is_some()).collect();
                    let bg = with_bg
                        .first()
                        .and_then(|m| m.background.as_ref())
                        .map(page_key);
                    (bg, with_bg.len())
                }
                _ => (None, 0),
            };
            SpotlightCatalogOption {
                transport_url: pick.transport_url,
                kind: pick.r#type,
                id: pick.id,
                name: catalog_title(model, &page.request),
                addon_name: addon_name_for(model, &page.request),
                selected_rank,
                is_default: defaults.iter().any(|d| matches_pick(&page.request, d)),
                preview_bg,
                item_count,
            }
        })
        .collect();

    let picked_opts: Vec<&SpotlightCatalogOption> = picks
        .iter()
        .filter_map(|p| {
            catalogs
                .iter()
                .find(|c| c.transport_url == p.transport_url && c.kind == p.r#type && c.id == p.id)
        })
        .collect();
    let picks_summary = match picked_opts.as_slice() {
        [] => "Default".to_owned(),
        [only] => only.name.clone(),
        many => format!("{} catalogs", many.len()),
    };
    let preview_cfg = SpotlightConfig {
        enabled: true,
        catalogs: picks.clone(),
        show_catalog_rows: true,
    };
    let (slides, _) = project_board(model, &preview_cfg);
    let first_slide = slides.first();
    let preview_sub = match picked_opts.as_slice() {
        [] => "Spotlight appears once a catalog is available.".to_owned(),
        [only] => format!("{} · {}", only.name, only.addon_name),
        many => many
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>()
            .join(" · "),
    };

    SpotlightSettingsView {
        enabled: cfg.enabled,
        show_catalog_rows: cfg.show_catalog_rows,
        is_default,
        picks_count: picks.len(),
        picks_summary,
        preview_bg: first_slide.and_then(|s| s.background.clone()),
        preview_logo: first_slide.and_then(|s| s.logo.clone()),
        preview_title: first_slide
            .map(|s| s.title.clone())
            .unwrap_or_else(|| "Nothing to show yet".to_owned()),
        preview_count: match slides.len() {
            0 => "No artwork".to_owned(),
            1 => "1 title".to_owned(),
            n => format!("{n} titles"),
        },
        preview_sub,
        catalogs,
    }
}
