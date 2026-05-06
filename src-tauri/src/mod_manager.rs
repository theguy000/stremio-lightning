pub use stremio_lightning_core::mods::{is_newer_version, InstalledMod, Registry, UpdateInfo};
pub use stremio_lightning_core::settings::SettingsState as ModManagerState;

use std::path::PathBuf;

use stremio_lightning_core::mods::{self, ModType};
use tauri::Manager;

fn app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

pub fn get_mods_dir(app: &tauri::AppHandle, mod_type: &str) -> Result<PathBuf, String> {
    let app_data_dir = app_data_dir(app)?;
    Ok(mods::mods_dir(&app_data_dir, mod_type.parse::<ModType>()?))
}

pub fn ensure_dirs(app: &tauri::AppHandle) -> Result<(), String> {
    mods::ensure_dirs(&app_data_dir(app)?)
}

pub fn list_mods(app: &tauri::AppHandle, mod_type: &str) -> Result<Vec<InstalledMod>, String> {
    mods::list_mods(&app_data_dir(app)?, mod_type.parse::<ModType>()?)
}

pub fn read_mod_content(
    app: &tauri::AppHandle,
    filename: &str,
    mod_type: &str,
) -> Result<String, String> {
    mods::read_mod_content(&app_data_dir(app)?, filename, mod_type.parse::<ModType>()?)
}

pub async fn download_mod(
    app: &tauri::AppHandle,
    url: &str,
    mod_type: &str,
) -> Result<String, String> {
    mods::download_mod(&app_data_dir(app)?, url, mod_type.parse::<ModType>()?).await
}

pub fn delete_mod(app: &tauri::AppHandle, filename: &str, mod_type: &str) -> Result<(), String> {
    mods::delete_mod(&app_data_dir(app)?, filename, mod_type.parse::<ModType>()?)
}

pub async fn fetch_registry() -> Result<Registry, String> {
    mods::fetch_registry().await
}

pub async fn check_mod_updates(
    app: &tauri::AppHandle,
    filename: &str,
    mod_type: &str,
) -> Result<UpdateInfo, String> {
    mods::check_mod_updates(&app_data_dir(app)?, filename, mod_type.parse::<ModType>()?).await
}

pub fn get_setting(
    app: &tauri::AppHandle,
    plugin_name: &str,
    key: &str,
) -> Result<serde_json::Value, String> {
    let plugins_dir = get_mods_dir(app, "plugin")?;
    stremio_lightning_core::settings::get_setting(&plugins_dir, plugin_name, key)
}

pub fn save_setting(
    app: &tauri::AppHandle,
    plugin_name: &str,
    key: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    let state = app.state::<ModManagerState>();
    let _guard = state.settings_lock.lock().map_err(|e| e.to_string())?;
    let plugins_dir = get_mods_dir(app, "plugin")?;
    stremio_lightning_core::settings::save_setting(&plugins_dir, plugin_name, key, value)
}
