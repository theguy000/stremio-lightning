//! Board Spotlight carousel selection and Settings preview/picker projection.
//!
//! Mirrors Stremio Web + upstream `stremio-core` (`src/models/common/board.rs`):
//! up to 3 catalogs (`MAX_CATALOGS`), up to 6 titles with backgrounds (`SPOTLIGHT_ITEMS_COUNT`),
//! round-robin interleaved by catalog with deduplication by `meta_id`.

use std::collections::HashSet;

use stremio_core::models::common::Loadable;
use stremio_core::types::addon::ResourceRequest;
use stremio_core::types::resource::{MetaItemPreview, Stream, StreamSource};

use crate::app::AppModel;
use crate::plumbing::images::{logo_key, page_key};
use crate::plumbing::local_settings::{SpotlightCatalogPick, SpotlightConfig};

pub const SPOTLIGHT_ITEMS_COUNT: usize = 6;
pub const MAX_CATALOGS: usize = 3;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpotlightItem {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub background: Option<String>,
    pub logo: Option<String>,
    pub year: String,
    pub runtime: String,
    pub imdb_rating: String,
    pub genres: String,
    pub description: String,
    pub in_library: bool,
    pub trailer_url: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpotlightCatalogOption {
    pub transport_url: String,
    pub kind: String,
    pub id: String,
    pub name: String,
    pub addon_name: String,
    pub selected_rank: usize,
    pub is_default: bool,
    pub ready: bool,
    pub preview_bg: Option<String>,
    pub preview_logo: Option<String>,
    pub preview_title: String,
    pub item_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpotlightSettingsView {
    pub enabled: bool,
    pub show_catalog_rows: bool,
    pub is_default: bool,
    pub picks_count: usize,
    pub picks_summary: String,
    pub picks_titles: String,
    pub picks_addon: String,
    pub preview_bg: Option<String>,
    pub preview_logo: Option<String>,
    pub preview_title: String,
    pub preview_count: String,
    pub preview_slides_count: usize,
    pub preview_sub: String,
    pub catalogs: Vec<SpotlightCatalogOption>,
}

pub(crate) fn pick_from_request(req: &ResourceRequest) -> SpotlightCatalogPick {
    SpotlightCatalogPick {
        transport_url: req.base.to_string(),
        r#type: req.path.r#type.clone(),
        id: req.path.id.clone(),
    }
}

pub(super) fn matches_pick(req: &ResourceRequest, pick: &SpotlightCatalogPick) -> bool {
    req.base.as_str() == pick.transport_url
        && req.path.r#type == pick.r#type
        && req.path.id == pick.id
}

pub(super) fn default_picks(requests: &[&ResourceRequest]) -> Vec<SpotlightCatalogPick> {
    let mut picks = Vec::new();
    for kind in ["movie", "series"] {
        if let Some(req) = requests.iter().find(|r| r.path.r#type == kind) {
            picks.push(pick_from_request(req));
        }
    }
    if picks.is_empty() {
        if let Some(first) = requests.first() {
            picks.push(pick_from_request(first));
        }
    }
    picks
}

pub(super) fn effective_picks(
    requests: &[&ResourceRequest],
    cfg: &SpotlightConfig,
) -> (Vec<SpotlightCatalogPick>, bool) {
    let valid: Vec<SpotlightCatalogPick> = cfg
        .catalogs
        .iter()
        .filter(|p| requests.iter().any(|r| matches_pick(r, p)))
        .take(MAX_CATALOGS)
        .cloned()
        .collect();
    if valid.is_empty() {
        (default_picks(requests), true)
    } else {
        (valid, false)
    }
}

pub(super) fn addon_name_for(model: &AppModel, req: &ResourceRequest) -> String {
    model
        .ctx
        .profile
        .addons
        .iter()
        .find(|a| a.transport_url == req.base)
        .map(|a| a.manifest.name.clone())
        .unwrap_or_else(|| req.base.host_str().unwrap_or("Addon").to_owned())
}

fn valid_youtube_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

pub(crate) fn trailer_url_of(streams: &[Stream]) -> Option<String> {
    streams.iter().find_map(|s| match &s.source {
        StreamSource::YouTube { yt_id } if valid_youtube_id(yt_id) => {
            Some(format!("https://www.youtube.com/watch?v={yt_id}"))
        }
        StreamSource::External {
            external_url: Some(url),
            ..
        } if matches!(url.scheme(), "http" | "https") => Some(url.to_string()),
        StreamSource::Url { url } if matches!(url.scheme(), "http" | "https") => {
            Some(url.to_string())
        }
        _ => None,
    })
}

fn item_from_preview(model: &AppModel, m: &MetaItemPreview) -> SpotlightItem {
    let year = m
        .release_info
        .clone()
        .or_else(|| m.released.map(|d| d.format("%Y").to_string()))
        .unwrap_or_default();
    let imdb_rating = m
        .links
        .iter()
        .find(|l| l.category.eq_ignore_ascii_case("imdb"))
        .map(|l| l.name.clone())
        .unwrap_or_default();
    let genres = m
        .links
        .iter()
        .filter(|l| l.category.eq_ignore_ascii_case("genres"))
        .take(3)
        .map(|l| l.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let in_library = model
        .ctx
        .library
        .items
        .get(&m.id)
        .is_some_and(|item| !item.removed);
    SpotlightItem {
        id: m.id.clone(),
        kind: m.r#type.clone(),
        title: m.name.clone(),
        background: m.background.as_ref().map(page_key),
        logo: m.logo.as_ref().map(logo_key),
        year,
        runtime: m.runtime.clone().unwrap_or_default(),
        imdb_rating,
        genres,
        description: m.description.clone().unwrap_or_default(),
        in_library,
        trailer_url: trailer_url_of(&m.trailer_streams),
    }
}

/// Round-robin interleaves items with backgrounds from selected catalogs (max 6 unique IDs),
/// returning `(spotlight_items, loading, hidden_catalog_picks)`.
pub(crate) fn project_board(
    model: &AppModel,
    cfg: &SpotlightConfig,
) -> (Vec<SpotlightItem>, bool, Vec<SpotlightCatalogPick>) {
    if !cfg.enabled {
        return (Vec::new(), false, Vec::new());
    }
    let pages: Vec<_> = model
        .board
        .catalogs
        .iter()
        .filter_map(|c| c.first())
        .collect();
    let requests: Vec<&ResourceRequest> = pages.iter().map(|p| &p.request).collect();
    let (picks, _) = effective_picks(&requests, cfg);
    if picks.is_empty() {
        return (Vec::new(), false, Vec::new());
    }

    let mut buckets: Vec<(SpotlightCatalogPick, Vec<&MetaItemPreview>, bool)> = picks
        .iter()
        .filter_map(|pick| {
            let page = pages.iter().find(|p| matches_pick(&p.request, pick))?;
            match &page.content {
                Some(Loadable::Ready(items)) => {
                    let with_bg: Vec<_> = items.iter().filter(|m| m.background.is_some()).collect();
                    Some((pick.clone(), with_bg, false))
                }
                Some(Loadable::Err(_)) => None,
                _ => Some((pick.clone(), Vec::new(), true)),
            }
        })
        .collect();

    let mut seen: HashSet<&str> = HashSet::new();
    let mut out = Vec::new();
    let mut contributed = vec![false; buckets.len()];
    let mut cursors = vec![0usize; buckets.len()];
    while out.len() < SPOTLIGHT_ITEMS_COUNT {
        let mut progressed = false;
        for (bi, (_, items, _)) in buckets.iter().enumerate() {
            while cursors[bi] < items.len() {
                let candidate = items[cursors[bi]];
                cursors[bi] += 1;
                if seen.insert(candidate.id.as_str()) {
                    out.push(item_from_preview(model, candidate));
                    contributed[bi] = true;
                    progressed = true;
                    break;
                }
            }
            if out.len() >= SPOTLIGHT_ITEMS_COUNT {
                break;
            }
        }
        if !progressed {
            break;
        }
    }

    let loading = out.is_empty() && buckets.iter().any(|(_, _, is_loading)| *is_loading);
    let hidden = if !cfg.show_catalog_rows {
        buckets
            .drain(..)
            .enumerate()
            .filter(|(i, (_, _, is_loading))| *is_loading || contributed[*i])
            .map(|(_, (pick, _, _))| pick)
            .collect()
    } else {
        Vec::new()
    };
    (out, loading, hidden)
}

pub use super::spotlight_settings::settings_view;
