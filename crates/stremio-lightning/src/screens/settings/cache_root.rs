//! Caching drive (`cacheRoot`) projection and `ActionStreamingServer::UpdateCacheRoot` dispatch.

use stremio_core::models::common::Loadable;
use stremio_core::runtime::msg::{Action, ActionStreamingServer};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::streaming_server::SettingsOptionValue;
use url::Url;

use super::super::{AppModel, SelectMenu};
use crate::AppRuntime;

#[derive(Clone, Default)]
pub struct CacheRootView {
    pub supported: bool,
    pub current: String,
    pub menu: SelectMenu,
    pub values: Vec<String>,
    pub updating: bool,
    pub error: String,
}

fn is_local_server(url: &Url) -> bool {
    matches!(
        url.host_str().map(|h| h.to_ascii_lowercase()).as_deref(),
        Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
    )
}

pub(super) fn project(model: &AppModel) -> CacheRootView {
    let ss = &model.streaming_server;
    let Loadable::Ready(settings) = &ss.settings else {
        return CacheRootView::default();
    };
    if !is_local_server(&ss.selected.transport_url) {
        return CacheRootView::default();
    }
    let Some(opt) = ss.settings_options.iter().find(|o| o.id == "cacheRoot") else {
        return CacheRootView::default();
    };
    if opt.supports_directory != Some(true) {
        return CacheRootView::default();
    }
    let mut values = Vec::new();
    let mut labels = Vec::new();
    for sel in &opt.selections {
        if let SettingsOptionValue::String(val) = &sel.val {
            values.push(val.clone());
            labels.push(sel.name.clone());
        }
    }
    if values.is_empty() {
        return CacheRootView::default();
    }
    let selected_idx = values.iter().position(|v| v == &settings.cache_root);
    let title = selected_idx
        .and_then(|i| labels.get(i).cloned())
        .filter(|_| !settings.cache_root.is_empty())
        .unwrap_or_else(|| {
            if settings.cache_root.is_empty() {
                "Default".to_owned()
            } else {
                settings.cache_root.clone()
            }
        });
    labels.push("Select...".to_owned());
    let options = labels
        .into_iter()
        .enumerate()
        .map(|(i, l)| (l, selected_idx == Some(i)))
        .collect();
    let updating = matches!(ss.cache_root_update, Some(Loadable::Loading));
    let error = match &ss.cache_root_update {
        Some(Loadable::Err(_)) => "Something went wrong!".to_owned(),
        _ => String::new(),
    };

    CacheRootView {
        supported: true,
        current: settings.cache_root.clone(),
        menu: SelectMenu {
            title,
            selected: selected_idx.unwrap_or(0),
            options,
        },
        values,
        updating,
        error,
    }
}

pub(crate) fn update_cache_root(runtime: &AppRuntime, cache_root: String) {
    let Ok(model) = runtime.model() else {
        return;
    };
    let transport_url = model.streaming_server.selected.transport_url.clone();
    drop(model);
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::StreamingServer(ActionStreamingServer::UpdateCacheRoot {
            transport_url,
            cache_root,
        }),
    });
}
