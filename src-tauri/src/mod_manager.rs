use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ModMetadata {
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requirements: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InstalledMod {
    pub filename: String,
    pub mod_type: String,
    pub metadata: Option<ModMetadata>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RegistryEntry {
    pub name: String,
    pub author: String,
    #[serde(default)]
    pub description: Option<String>,
    pub version: String,
    pub repo: String,
    pub download: String,
    #[serde(default)]
    pub preview: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Registry {
    pub plugins: Vec<RegistryEntry>,
    pub themes: Vec<RegistryEntry>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UpdateInfo {
    pub has_update: bool,
    pub installed_version: Option<String>,
    pub new_version: Option<String>,
    pub registry_version: Option<String>,
    pub update_url: Option<String>,
}

pub struct ModManagerState {
    pub registered_schemas: Mutex<HashMap<String, serde_json::Value>>,
}

/// Returns the directory for the given mod type (plugin or theme).
pub fn get_mods_dir(app: &tauri::AppHandle, mod_type: &str) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;

    let sub = match mod_type {
        "plugin" => "plugins",
        "theme" => "themes",
        _ => return Err(format!("Unknown mod type: {}", mod_type)),
    };

    Ok(base.join("stremio-lightning").join(sub))
}

/// Creates both plugins/ and themes/ directories if they don't exist.
pub fn ensure_dirs(app: &tauri::AppHandle) -> Result<(), String> {
    let plugins_dir = get_mods_dir(app, "plugin")?;
    let themes_dir = get_mods_dir(app, "theme")?;

    std::fs::create_dir_all(&plugins_dir).map_err(|e| format!("Failed to create plugins dir: {}", e))?;
    std::fs::create_dir_all(&themes_dir).map_err(|e| format!("Failed to create themes dir: {}", e))?;

    Ok(())
}

/// Parses JSDoc-style metadata from file content.
pub fn parse_metadata(content: &str) -> Option<ModMetadata> {
    let block_re = Regex::new(r"(?s)/\*\*(.*?)\*/").ok()?;
    let block_match = block_re.find(content)?;
    let block = block_match.as_str();

    let tag_re = Regex::new(r"@(\w+)\s+([^\n\r]+)").ok()?;

    let mut tags: HashMap<String, String> = HashMap::new();
    for cap in tag_re.captures_iter(block) {
        let key = cap[1].to_string();
        let value = cap[2].trim().to_string();
        tags.insert(key, value);
    }

    let name = tags.get("name")?.clone();
    let description = tags.get("description")?.clone();
    let author = tags.get("author")?.clone();
    let version = tags.get("version")?.clone();

    let update_url = tags.get("updateUrl").cloned();
    let source = tags.get("source").cloned();
    let license = tags.get("license").cloned();
    let homepage = tags.get("homepage").cloned();

    let requirements = tags.get("requirements").map(|raw| {
        // Try JSON parse first
        if let Ok(parsed) = serde_json::from_str::<Vec<String>>(raw) {
            parsed
        } else {
            // Fall back to comma-separated
            raw.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
        }
    });

    Some(ModMetadata {
        name,
        description,
        author,
        version,
        update_url,
        source,
        license,
        homepage,
        requirements,
    })
}

/// Returns true if v1 is a newer version than v2.
pub fn is_newer_version(v1: &str, v2: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.strip_prefix('v')
            .unwrap_or(v)
            .split('.')
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect()
    };

    let parts1 = parse(v1);
    let parts2 = parse(v2);

    let max_len = parts1.len().max(parts2.len());
    for i in 0..max_len {
        let a = parts1.get(i).copied().unwrap_or(0);
        let b = parts2.get(i).copied().unwrap_or(0);
        if a > b {
            return true;
        }
        if a < b {
            return false;
        }
    }

    false
}

/// Lists all installed mods of the given type.
pub fn list_mods(app: &tauri::AppHandle, mod_type: &str) -> Result<Vec<InstalledMod>, String> {
    let dir = get_mods_dir(app, mod_type)?;

    if !dir.exists() {
        return Ok(Vec::new());
    }

    let ext = match mod_type {
        "plugin" => ".plugin.js",
        "theme" => ".theme.css",
        _ => return Err(format!("Unknown mod type: {}", mod_type)),
    };

    let entries = std::fs::read_dir(&dir).map_err(|e| format!("Failed to read directory: {}", e))?;

    let mut mods = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let filename = entry.file_name().to_string_lossy().to_string();

        if filename.ends_with(ext) {
            let content = std::fs::read_to_string(entry.path())
                .map_err(|e| format!("Failed to read file {}: {}", filename, e))?;
            let metadata = parse_metadata(&content);

            mods.push(InstalledMod {
                filename,
                mod_type: mod_type.to_string(),
                metadata,
            });
        }
    }

    Ok(mods)
}

/// Reads the content of a mod file.
pub fn read_mod_content(
    app: &tauri::AppHandle,
    filename: &str,
    mod_type: &str,
) -> Result<String, String> {
    validate_filename(filename)?;

    let dir = get_mods_dir(app, mod_type)?;
    let path = dir.join(filename);

    std::fs::read_to_string(&path).map_err(|e| format!("Failed to read file: {}", e))
}

/// Downloads a mod from a URL and saves it to the appropriate directory.
pub async fn download_mod(
    app: &tauri::AppHandle,
    url: &str,
    mod_type: &str,
) -> Result<String, String> {
    let dir = get_mods_dir(app, mod_type)?;

    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("Failed to download: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Download failed with status: {}", response.status()));
    }

    // Extract filename from URL path (last segment)
    let filename = url
        .split('/')
        .last()
        .ok_or_else(|| "Could not extract filename from URL".to_string())?
        .split('?')
        .next()
        .unwrap_or("mod_file")
        .to_string();

    let content = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    let file_path = dir.join(&filename);
    std::fs::write(&file_path, &content)
        .map_err(|e| format!("Failed to write file: {}", e))?;

    Ok(filename)
}

/// Deletes a mod file and its associated config file (for plugins).
pub fn delete_mod(
    app: &tauri::AppHandle,
    filename: &str,
    mod_type: &str,
) -> Result<(), String> {
    validate_filename(filename)?;

    let dir = get_mods_dir(app, mod_type)?;
    let path = dir.join(filename);

    std::fs::remove_file(&path).map_err(|e| format!("Failed to delete file: {}", e))?;

    // If plugin, also delete the associated settings file
    if mod_type == "plugin" {
        let config_name = filename.replace(".plugin.js", ".plugin.json");
        let config_path = dir.join(&config_name);
        if config_path.exists() {
            let _ = std::fs::remove_file(&config_path);
        }
    }

    Ok(())
}

/// Fetches the mod registry from GitHub.
pub async fn fetch_registry() -> Result<Registry, String> {
    let url = "https://raw.githubusercontent.com/REVENGE977/stremio-enhanced-registry/refs/heads/main/registry.json";

    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("Failed to fetch registry: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Registry fetch failed with status: {}", response.status()));
    }

    response
        .json::<Registry>()
        .await
        .map_err(|e| format!("Failed to parse registry: {}", e))
}

/// Checks for updates for a given mod.
pub async fn check_mod_updates(
    app: &tauri::AppHandle,
    filename: &str,
    mod_type: &str,
) -> Result<UpdateInfo, String> {
    let dir = get_mods_dir(app, mod_type)?;
    let path = dir.join(filename);

    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    let metadata = match parse_metadata(&content) {
        Some(m) => m,
        None => {
            return Ok(UpdateInfo {
                has_update: false,
                installed_version: None,
                new_version: None,
                registry_version: None,
                update_url: None,
            });
        }
    };

    let installed_version = metadata.version.clone();

    let update_url = match &metadata.update_url {
        Some(url) => url.clone(),
        None => {
            return Ok(UpdateInfo {
                has_update: false,
                installed_version: Some(installed_version),
                new_version: None,
                registry_version: None,
                update_url: None,
            });
        }
    };

    // Fetch remote content from update URL
    let remote_response = reqwest::get(&update_url)
        .await
        .map_err(|e| format!("Failed to fetch update: {}", e))?;

    let mut new_version: Option<String> = None;
    let mut has_update = false;
    let mut resolved_update_url = update_url.clone();

    if remote_response.status().is_success() {
        let remote_content = remote_response
            .text()
            .await
            .map_err(|e| format!("Failed to read remote content: {}", e))?;

        if let Some(remote_meta) = parse_metadata(&remote_content) {
            if is_newer_version(&remote_meta.version, &installed_version) {
                has_update = true;
                new_version = Some(remote_meta.version);
            }
        }
    }

    // For plugins, also check registry version
    let mut registry_version: Option<String> = None;
    if mod_type == "plugin" {
        if let Ok(registry) = fetch_registry().await {
            for entry in &registry.plugins {
                if entry.name == metadata.name {
                    registry_version = Some(entry.version.clone());
                    if !has_update && is_newer_version(&entry.version, &installed_version) {
                        has_update = true;
                        new_version = Some(entry.version.clone());
                        resolved_update_url = entry.download.clone();
                    }
                    break;
                }
            }
        }
    }

    Ok(UpdateInfo {
        has_update,
        installed_version: Some(installed_version),
        new_version,
        registry_version,
        update_url: if has_update { Some(resolved_update_url) } else { None },
    })
}

/// Gets a single setting value for a plugin.
pub fn get_setting(
    app: &tauri::AppHandle,
    plugin_name: &str,
    key: &str,
) -> Result<serde_json::Value, String> {
    let dir = get_mods_dir(app, "plugin")?;
    let config_path = dir.join(format!("{}.plugin.json", plugin_name));

    if !config_path.exists() {
        return Ok(serde_json::Value::Null);
    }

    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read settings: {}", e))?;

    let settings: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse settings: {}", e))?;

    Ok(settings.get(key).cloned().unwrap_or(serde_json::Value::Null))
}

/// Saves a single setting value for a plugin.
pub fn save_setting(
    app: &tauri::AppHandle,
    plugin_name: &str,
    key: &str,
    value: serde_json::Value,
) -> Result<(), String> {
    let dir = get_mods_dir(app, "plugin")?;
    let config_path = dir.join(format!("{}.plugin.json", plugin_name));

    let mut settings: serde_json::Map<String, serde_json::Value> = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read settings: {}", e))?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        serde_json::Map::new()
    };

    settings.insert(key.to_string(), value);

    let json = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;

    std::fs::write(&config_path, json)
        .map_err(|e| format!("Failed to write settings: {}", e))?;

    Ok(())
}

/// Gets all settings for a plugin.
pub fn get_all_settings(
    app: &tauri::AppHandle,
    plugin_name: &str,
) -> Result<serde_json::Value, String> {
    let dir = get_mods_dir(app, "plugin")?;
    let config_path = dir.join(format!("{}.plugin.json", plugin_name));

    if !config_path.exists() {
        return Ok(serde_json::json!({}));
    }

    let content = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("Failed to read settings: {}", e))?;

    serde_json::from_str(&content).map_err(|e| format!("Failed to parse settings: {}", e))
}

/// Validates that a filename doesn't contain path traversal characters.
fn validate_filename(filename: &str) -> Result<(), String> {
    if filename.contains('/')
        || filename.contains('\\')
        || filename.contains("..")
    {
        return Err("Invalid filename: path separators or traversal not allowed".to_string());
    }
    Ok(())
}
