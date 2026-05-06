use std::collections::HashMap;

use tauri::Manager;
use tauri_plugin_opener::OpenerExt;

use crate::app_updater;
use crate::discord_rpc::{self, ActivityPayload, DiscordRpcState};
use crate::mod_manager::{self, ModManagerState};
use crate::player;
use crate::shell_transport;
use crate::streaming_server;

#[tauri::command]
pub fn toggle_devtools(app: tauri::AppHandle) {
    if let Some(webview) = app.get_webview(player::MAIN_APP_LABEL) {
        if webview.is_devtools_open() {
            webview.close_devtools();
        } else {
            webview.open_devtools();
        }
    }
}

fn is_allowed_external_url(url: &str) -> bool {
    let normalized = url.trim().to_ascii_lowercase();
    ["http://", "https://", "stremio://"]
        .iter()
        .any(|prefix| normalized.starts_with(prefix))
}

#[tauri::command]
pub async fn open_external_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
    if !is_allowed_external_url(&url) {
        return Err("Rejected non-whitelisted external URL".into());
    }

    app.opener()
        .open_url(url.trim(), None::<&str>)
        .map_err(|e| format!("Failed to open URL: {}", e))
}

#[tauri::command]
pub async fn shell_transport_send(app: tauri::AppHandle, message: String) -> Result<(), String> {
    shell_transport::handle_message(&app, &message)
}

#[tauri::command]
pub async fn shell_bridge_ready(app: tauri::AppHandle) -> Result<(), String> {
    shell_transport::notify_bridge_ready(&app)
}

#[tauri::command]
pub async fn get_native_player_status(
    app: tauri::AppHandle,
) -> Result<player::NativePlayerStatus, String> {
    Ok(player::status(&app))
}

#[tauri::command]
pub async fn start_streaming_server(app: tauri::AppHandle) -> Result<(), String> {
    streaming_server::start_server(&app)
}

#[tauri::command]
pub async fn stop_streaming_server(app: tauri::AppHandle) -> Result<(), String> {
    streaming_server::stop_server(&app)
}

#[tauri::command]
pub async fn restart_streaming_server(app: tauri::AppHandle) -> Result<(), String> {
    let _ = streaming_server::stop_server(&app);
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    streaming_server::start_server(&app)
}

#[tauri::command]
pub async fn get_streaming_server_status(app: tauri::AppHandle) -> bool {
    streaming_server::is_server_running(&app)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStreamingServerResponse {
    status: u16,
    status_text: String,
    headers: Vec<(String, String)>,
    body: String,
}

fn is_hop_by_hop_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "connection"
            | "keep-alive"
            | "proxy-authenticate"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "host"
            | "content-length"
    )
}

fn validate_streaming_server_proxy_path(path: &str) -> Result<(), String> {
    if !path.starts_with('/') || path.starts_with("//") || path.contains("://") {
        return Err("Rejected invalid streaming server proxy path".into());
    }

    if path.contains('\\') || path.contains('\0') {
        return Err("Rejected invalid streaming server proxy path".into());
    }

    Ok(())
}

#[tauri::command]
pub async fn proxy_streaming_server_request(
    method: String,
    path: String,
    headers: Option<HashMap<String, String>>,
    body: Option<String>,
) -> Result<ProxyStreamingServerResponse, String> {
    validate_streaming_server_proxy_path(&path)?;

    let method = method.trim().to_ascii_uppercase();
    let method = match method.as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PUT" => reqwest::Method::PUT,
        "PATCH" => reqwest::Method::PATCH,
        "DELETE" => reqwest::Method::DELETE,
        "OPTIONS" => reqwest::Method::OPTIONS,
        "HEAD" => reqwest::Method::HEAD,
        _ => return Err("Rejected unsupported streaming server proxy method".into()),
    };

    let url = format!("http://127.0.0.1:11470{}", path);
    let client = reqwest::Client::new();
    let mut request = client.request(method.clone(), &url);

    if let Some(headers) = headers {
        for (name, value) in headers {
            if is_hop_by_hop_header(&name) {
                continue;
            }
            request = request.header(name, value);
        }
    }

    if method != reqwest::Method::GET && method != reqwest::Method::HEAD {
        if let Some(body) = body {
            request = request.body(body);
        }
    }

    let response = request
        .send()
        .await
        .map_err(|e| format!("Streaming server proxy request failed: {}", e))?;

    let status = response.status();
    let response_headers = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            if is_hop_by_hop_header(name.as_str()) {
                return None;
            }
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_string(), value.to_string()))
        })
        .collect::<Vec<_>>();

    let response_body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read streaming server proxy response: {}", e))?;

    eprintln!(
        "[StreamingServerProxy] {} {} -> {}",
        method.as_str(),
        path,
        status.as_u16()
    );

    Ok(ProxyStreamingServerResponse {
        status: status.as_u16(),
        status_text: status.canonical_reason().unwrap_or("").to_string(),
        headers: response_headers,
        body: response_body,
    })
}


// ── Mod management commands ──

#[tauri::command]
pub async fn get_plugins(app: tauri::AppHandle) -> Result<Vec<mod_manager::InstalledMod>, String> {
    mod_manager::list_mods(&app, "plugin")
}

#[tauri::command]
pub async fn get_themes(app: tauri::AppHandle) -> Result<Vec<mod_manager::InstalledMod>, String> {
    mod_manager::list_mods(&app, "theme")
}

#[tauri::command]
pub async fn download_mod(
    app: tauri::AppHandle,
    url: String,
    mod_type: String,
) -> Result<String, String> {
    mod_manager::download_mod(&app, &url, &mod_type).await
}

#[tauri::command]
pub async fn delete_mod(
    app: tauri::AppHandle,
    filename: String,
    mod_type: String,
) -> Result<(), String> {
    mod_manager::delete_mod(&app, &filename, &mod_type)
}

#[tauri::command]
pub async fn get_mod_content(
    app: tauri::AppHandle,
    filename: String,
    mod_type: String,
) -> Result<String, String> {
    mod_manager::read_mod_content(&app, &filename, &mod_type)
}

#[tauri::command]
pub async fn get_registry() -> Result<mod_manager::Registry, String> {
    mod_manager::fetch_registry().await
}

#[tauri::command]
pub async fn check_mod_updates(
    app: tauri::AppHandle,
    filename: String,
    mod_type: String,
) -> Result<mod_manager::UpdateInfo, String> {
    mod_manager::check_mod_updates(&app, &filename, &mod_type).await
}

#[tauri::command]
pub async fn get_setting(
    app: tauri::AppHandle,
    plugin_name: String,
    key: String,
) -> Result<serde_json::Value, String> {
    mod_manager::get_setting(&app, &plugin_name, &key)
}

#[tauri::command]
pub async fn save_setting(
    app: tauri::AppHandle,
    plugin_name: String,
    key: String,
    value: String,
) -> Result<(), String> {
    let parsed: serde_json::Value =
        serde_json::from_str(&value).unwrap_or(serde_json::Value::String(value.clone()));
    mod_manager::save_setting(&app, &plugin_name, &key, parsed)
}

#[tauri::command]
pub async fn register_settings(
    app: tauri::AppHandle,
    plugin_name: String,
    schema: String,
) -> Result<(), String> {
    let parsed: serde_json::Value = serde_json::from_str(&schema).map_err(|e| e.to_string())?;
    let state = app.state::<ModManagerState>();
    let mut schemas = state
        .registered_schemas
        .lock()
        .map_err(|e: std::sync::PoisonError<_>| e.to_string())?;
    schemas.insert(plugin_name, parsed);
    Ok(())
}

#[tauri::command]
pub async fn get_registered_settings(
    app: tauri::AppHandle,
    plugin_name: String,
) -> Result<serde_json::Value, String> {
    let state = app.state::<ModManagerState>();
    let schemas = state
        .registered_schemas
        .lock()
        .map_err(|e: std::sync::PoisonError<_>| e.to_string())?;
    Ok(schemas
        .get(&plugin_name)
        .cloned()
        .unwrap_or(serde_json::Value::Null))
}

// ── Discord RPC commands ──

#[tauri::command]
pub async fn start_discord_rpc(app: tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<DiscordRpcState>();
    discord_rpc::start(&app, &state)
}

#[tauri::command]
pub async fn stop_discord_rpc(app: tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<DiscordRpcState>();
    discord_rpc::stop(&state)
}

#[tauri::command]
pub async fn update_discord_activity(
    app: tauri::AppHandle,
    activity: ActivityPayload,
) -> Result<(), String> {
    let state = app.state::<DiscordRpcState>();
    discord_rpc::update_activity(&app, &state, activity)
}

// ── App update check ──

#[tauri::command]
pub async fn check_app_update() -> Result<app_updater::AppUpdateInfo, String> {
    app_updater::check_app_update().await
}

// ── Auto-pause on unfocus ──

/// Tauri command: enable or disable the auto-pause-on-unfocus feature.
/// Persists the setting to the `PlayerState` atomic so the window event callback
/// can check it without any async or locking overhead.
/// When disabling, also clears `auto_paused_on_unfocus` to prevent a stale flag
/// from causing an unwanted resume the next time the window gains focus.
#[tauri::command]
pub async fn set_auto_pause(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let state = app.state::<player::PlayerState>();
    state
        .auto_pause_enabled
        .store(enabled, std::sync::atomic::Ordering::Relaxed);
    // If disabling, clear any existing auto-pause flag so we don't resume on next focus
    if !enabled {
        state
            .auto_paused_on_unfocus
            .store(false, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

/// Tauri command: query whether the auto-pause-on-unfocus feature is currently enabled.
/// Used by the frontend on startup to sync the settings UI toggle with the Rust-side default.
#[tauri::command]
pub async fn get_auto_pause(app: tauri::AppHandle) -> bool {
    let state = app.state::<player::PlayerState>();
    state
        .auto_pause_enabled
        .load(std::sync::atomic::Ordering::Relaxed)
}

/// Tauri command: enable or disable the "PiP disables auto-pause" setting.
/// When enabled (default), auto-pause-on-unfocus is suppressed while PiP is active.
#[tauri::command]
pub async fn set_pip_disables_auto_pause(
    app: tauri::AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let state = app.state::<player::PlayerState>();
    state
        .pip_disables_auto_pause
        .store(enabled, std::sync::atomic::Ordering::Relaxed);
    Ok(())
}

/// Tauri command: query whether the "PiP disables auto-pause" setting is enabled.
#[tauri::command]
pub async fn get_pip_disables_auto_pause(app: tauri::AppHandle) -> bool {
    let state = app.state::<player::PlayerState>();
    state
        .pip_disables_auto_pause
        .load(std::sync::atomic::Ordering::Relaxed)
}

// ── Picture-in-Picture ──

/// Tauri command: toggle Picture-in-Picture mode.
/// Returns the new PiP state (`true` = PiP active, `false` = normal mode).
#[tauri::command]
pub async fn toggle_pip(app: tauri::AppHandle) -> Result<bool, String> {
    player::toggle_pip_mode(&app)
}

/// Tauri command: query whether Picture-in-Picture mode is currently active.
/// Used by the frontend on startup to sync the settings UI toggle with the Rust-side state.
#[tauri::command]
pub async fn get_pip_mode(app: tauri::AppHandle) -> bool {
    player::get_pip_mode(&app)
}
