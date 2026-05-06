use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub struct SettingsState {
    pub registered_schemas: Mutex<HashMap<String, Value>>,
    pub settings_lock: Mutex<()>,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            registered_schemas: Mutex::new(HashMap::new()),
            settings_lock: Mutex::new(()),
        }
    }
}

pub fn plugin_settings_path(plugins_dir: &Path, plugin_name: &str) -> Result<PathBuf, String> {
    crate::mods::validate_filename(plugin_name)?;
    Ok(plugins_dir.join(format!("{}.plugin.json", plugin_name)))
}

pub fn load_settings_file(path: &Path) -> Result<Value, String> {
    if !path.exists() {
        return Ok(Value::Null);
    }

    let content =
        std::fs::read_to_string(path).map_err(|e| format!("Failed to read settings: {}", e))?;
    serde_json::from_str(&content).map_err(|e| format!("Failed to parse settings: {}", e))
}

pub fn save_setting_file(path: &Path, key: &str, value: Value) -> Result<(), String> {
    let mut settings: serde_json::Map<String, Value> = if path.exists() {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("Failed to read settings: {}", e))?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        serde_json::Map::new()
    };

    settings.insert(key.to_string(), value);

    let json = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;
    std::fs::write(path, json).map_err(|e| format!("Failed to write settings: {}", e))
}

pub fn get_setting(plugins_dir: &Path, plugin_name: &str, key: &str) -> Result<Value, String> {
    let config_path = plugin_settings_path(plugins_dir, plugin_name)?;
    let settings = load_settings_file(&config_path)?;
    Ok(settings.get(key).cloned().unwrap_or(Value::Null))
}

pub fn save_setting(
    plugins_dir: &Path,
    plugin_name: &str,
    key: &str,
    value: Value,
) -> Result<(), String> {
    let config_path = plugin_settings_path(plugins_dir, plugin_name)?;
    save_setting_file(&config_path, key, value)
}

pub fn register_settings(
    schemas: &Mutex<HashMap<String, Value>>,
    plugin_name: String,
    schema: Value,
) -> Result<(), String> {
    schemas
        .lock()
        .map_err(|e| e.to_string())?
        .insert(plugin_name, schema);
    Ok(())
}

pub fn get_registered_settings(
    schemas: &Mutex<HashMap<String, Value>>,
    plugin_name: &str,
) -> Result<Value, String> {
    Ok(schemas
        .lock()
        .map_err(|e| e.to_string())?
        .get(plugin_name)
        .cloned()
        .unwrap_or(Value::Null))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    fn temp_file(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "stremio-lightning-settings-test-{}-{}",
            std::process::id(),
            name
        ));
        let _ = fs::remove_file(&path);
        path
    }

    #[test]
    fn settings_load_missing_file_as_null() {
        let path = temp_file("missing-settings.json");
        assert_eq!(load_settings_file(&path).unwrap(), Value::Null);
    }

    #[test]
    fn settings_save_and_load_round_trip() {
        let path = temp_file("settings-round-trip.json");
        save_setting_file(&path, "enabled", json!(true)).unwrap();
        save_setting_file(&path, "quality", json!("1080p")).unwrap();

        let settings = load_settings_file(&path).unwrap();
        assert_eq!(settings["enabled"], true);
        assert_eq!(settings["quality"], "1080p");

        let _ = fs::remove_file(path);
    }

    #[test]
    fn plugin_settings_path_rejects_traversal_names() {
        let dir = std::env::temp_dir();
        assert!(plugin_settings_path(&dir, "cinema").is_ok());
        assert!(plugin_settings_path(&dir, "../cinema").is_err());
        assert!(plugin_settings_path(&dir, "nested\\cinema").is_err());
    }
}
