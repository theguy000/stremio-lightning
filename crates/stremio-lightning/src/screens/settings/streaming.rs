//! Real `stremio-core` StreamingServer settings projection and mutation actions.

use stremio_core::models::common::Loadable;
use stremio_core::models::streaming_server::StreamingServer;
use stremio_core::runtime::msg::{Action, ActionCtx, ActionStreamingServer};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::streaming_server::Settings as ServerSettings;
use url::Url;

use super::super::{AppModel, SelectMenu};
use crate::AppRuntime;

/// Matches stremio-web `useStreamingOptions.ts` `CACHE_SIZES` (bytes, `None` = Infinite).
const CACHE_SIZES: &[Option<f64>] = &[
    Some(0.0),
    Some(2_147_483_648.0),
    Some(5_368_709_120.0),
    Some(10_737_418_240.0),
    Some(53_687_091_200.0),
    Some(107_374_182_400.0),
    Some(214_748_364_800.0),
    None,
];

/// `(label, hard_limit, soft_limit, handshake_timeout, max_connections, min_peers, request_timeout)`
const TORRENT_PRESETS: &[(&str, f64, f64, u64, u64, u64, u64)] = &[
    ("Default", 3_670_016.0, 2_621_440.0, 20_000, 55, 5, 4_000),
    ("Soft", 1_677_721.6, 1_677_721.6, 20_000, 35, 5, 4_000),
    ("Fast", 39_321_600.0, 4_194_304.0, 20_000, 200, 10, 4_000),
    (
        "Ultra Fast",
        78_643_200.0,
        8_388_608.0,
        25_000,
        400,
        10,
        6_000,
    ),
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerUrlEntry {
    pub url: String,
    pub selected: bool,
    pub removable: bool,
}

#[derive(Clone, Default)]
pub struct StreamingView {
    /// `"online"`, `"loading"`, or `"offline"`.
    pub status: String,
    pub server_version: String,
    pub remote_url: String,
    pub server_urls: Vec<ServerUrlEntry>,
    pub has_https_endpoint: bool,
    pub https_endpoint: SelectMenu,
    pub has_transcoding_profile: bool,
    pub cache_size: Option<SelectMenu>,
    pub torrent_profile: Option<SelectMenu>,
    pub transcoding_profile: Option<SelectMenu>,
    pub cache_root: super::cache_root::CacheRootView,
}

fn cache_label(size: Option<f64>) -> String {
    match size {
        None => "Infinite".to_owned(),
        Some(v) if v <= 0.0 => "No caching".to_owned(),
        Some(v) => format!("{:.0} GiB", (v / 1024.0 / 1024.0 / 1024.0).ceil()),
    }
}

fn matches_preset(s: &ServerSettings, p: &(&str, f64, f64, u64, u64, u64, u64)) -> bool {
    (s.bt_download_speed_hard_limit - p.1).abs() < 1.0
        && (s.bt_download_speed_soft_limit - p.2).abs() < 1.0
        && s.bt_handshake_timeout == p.3
        && s.bt_max_connections == p.4
        && s.bt_min_peers_for_stable == p.5
        && s.bt_request_timeout == p.6
}

fn menu_from_labels(labels: Vec<String>, selected: usize) -> SelectMenu {
    SelectMenu {
        title: labels.get(selected).cloned().unwrap_or_default(),
        selected,
        options: labels
            .into_iter()
            .enumerate()
            .map(|(i, l)| (l, i == selected))
            .collect(),
    }
}

fn optional_menu(current: Option<&str>, choices: &[String]) -> SelectMenu {
    let mut opts = vec!["Disabled".to_owned()];
    opts.extend(choices.iter().cloned());
    let sel = current
        .and_then(|cur| opts.iter().position(|o| o == cur))
        .unwrap_or(0);
    menu_from_labels(opts, sel)
}

pub(super) fn project(model: &AppModel) -> StreamingView {
    let ss = &model.streaming_server;
    let selected_url = model.ctx.profile.settings.streaming_server_url.to_string();
    let default_url = stremio_core::constants::STREAMING_SERVER_URL.as_str();

    let mut urls: Vec<String> = model
        .ctx
        .streaming_server_urls
        .items
        .keys()
        .map(ToString::to_string)
        .collect();
    if !urls.iter().any(|u| u == &selected_url) {
        urls.insert(0, selected_url.clone());
    }
    let server_urls = urls
        .into_iter()
        .map(|url| {
            let selected = url == selected_url;
            ServerUrlEntry {
                removable: !selected && url != default_url,
                url,
                selected,
            }
        })
        .collect();

    let remote_url = ss
        .remote_url
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_default();
    let (status, settings) = match &ss.settings {
        Loadable::Ready(s) => ("online".to_owned(), Some(s)),
        Loadable::Loading => ("loading".to_owned(), None),
        Loadable::Err(_) => ("offline".to_owned(), None),
    };
    let Some(s) = settings else {
        return StreamingView {
            status,
            remote_url,
            server_urls,
            ..Default::default()
        };
    };

    let cache_sel = CACHE_SIZES
        .iter()
        .position(|c| match (c, s.cache_size) {
            (None, None) => true,
            (Some(a), Some(b)) => (a - b).abs() < 1.0,
            _ => false,
        })
        .unwrap_or(1);
    let cache_size = Some(menu_from_labels(
        CACHE_SIZES.iter().copied().map(cache_label).collect(),
        cache_sel,
    ));

    let mut torrent_labels: Vec<String> = TORRENT_PRESETS.iter().map(|p| p.0.to_owned()).collect();
    let torrent_sel = TORRENT_PRESETS
        .iter()
        .position(|p| matches_preset(s, p))
        .unwrap_or_else(|| {
            torrent_labels.push("Custom".to_owned());
            torrent_labels.len() - 1
        });
    let torrent_profile = Some(menu_from_labels(torrent_labels, torrent_sel));

    let (has_https_endpoint, https_endpoint) = match &ss.network_info {
        Loadable::Ready(net) if model.ctx.profile.auth.is_some() => (
            true,
            optional_menu(s.remote_https.as_deref(), &net.available_interfaces),
        ),
        _ => (false, SelectMenu::default()),
    };
    let (has_transcoding_profile, transcoding_profile) = match &ss.device_info {
        Loadable::Ready(dev) => (
            true,
            Some(optional_menu(
                s.transcode_profile.as_deref(),
                &dev.available_hardware_accelerations,
            )),
        ),
        _ => (false, None),
    };

    StreamingView {
        status,
        server_version: s.server_version.clone(),
        remote_url,
        server_urls,
        has_https_endpoint,
        https_endpoint,
        has_transcoding_profile,
        cache_size,
        torrent_profile,
        transcoding_profile,
        cache_root: super::cache_root::project(model),
    }
}

/// Applies a streaming server dropdown choice to `StreamingServer::settings` when online.
pub(crate) fn update_server_option(
    ss: &StreamingServer,
    key: &str,
    index: usize,
) -> Option<ServerSettings> {
    let Loadable::Ready(current) = &ss.settings else {
        return None;
    };
    let mut next = current.clone();
    match key {
        "caching" => next.cache_size = *CACHE_SIZES.get(index)?,
        "torrentProfile" => {
            let p = TORRENT_PRESETS.get(index)?;
            next.bt_download_speed_hard_limit = p.1;
            next.bt_download_speed_soft_limit = p.2;
            next.bt_handshake_timeout = p.3;
            next.bt_max_connections = p.4;
            next.bt_min_peers_for_stable = p.5;
            next.bt_request_timeout = p.6;
        }
        "transcodingProfile" => {
            let Loadable::Ready(dev) = &ss.device_info else {
                return None;
            };
            next.transcode_profile = (index > 0)
                .then(|| dev.available_hardware_accelerations.get(index - 1).cloned())
                .flatten();
        }
        "remoteHttps" => {
            let Loadable::Ready(net) = &ss.network_info else {
                return None;
            };
            next.remote_https = (index > 0)
                .then(|| net.available_interfaces.get(index - 1).cloned())
                .flatten();
        }
        _ => return None,
    }
    Some(next)
}

pub(crate) fn dispatch_server_settings(runtime: &AppRuntime, settings: ServerSettings) {
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::StreamingServer(ActionStreamingServer::UpdateSettings(settings)),
    });
}

pub(crate) fn add_server_url(runtime: &AppRuntime, raw: &str) -> Result<(), String> {
    let url = Url::parse(raw.trim()).map_err(|_| "Please provide a valid URL".to_owned())?;
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Ctx(ActionCtx::AddServerUrl(url)),
    });
    Ok(())
}

pub(crate) fn delete_server_url(runtime: &AppRuntime, raw: &str) {
    if let Ok(url) = Url::parse(raw) {
        runtime.dispatch(RuntimeAction {
            field: None,
            action: Action::Ctx(ActionCtx::DeleteServerUrl(url)),
        });
    }
}

pub(crate) fn select_server_url(runtime: &AppRuntime, raw: &str) -> Result<(), String> {
    let url = Url::parse(raw.trim()).map_err(|_| "Invalid server URL".to_owned())?;
    let Ok(model) = runtime.model() else {
        return Err("App state unavailable".to_owned());
    };
    let mut settings = model.ctx.profile.settings.clone();
    drop(model);
    settings.streaming_server_url = url;
    super::dispatch_settings(runtime, settings);
    Ok(())
}
