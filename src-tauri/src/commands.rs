use tauri::{Manager, WebviewWindow};
use tauri_plugin_opener::OpenerExt;

use crate::mod_manager::{self, ModManagerState};
use crate::player;
use crate::shell_transport;
use crate::streaming_server;

#[tauri::command]
pub fn toggle_devtools(window: WebviewWindow) {
    if window.is_devtools_open() {
        window.close_devtools();
    } else {
        window.open_devtools();
    }
}

#[tauri::command]
pub async fn open_external_url(app: tauri::AppHandle, url: String) -> Result<(), String> {
    app.opener()
        .open_url(&url, None::<&str>)
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
    let parsed: serde_json::Value =
        serde_json::from_str(&schema).map_err(|e| e.to_string())?;
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
