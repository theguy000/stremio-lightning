//! Disk persistence for local shell/streaming settings (`<data_dir>/settings.json`) and
//! random UUIDv4 generation for local client identifiers.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

/// Persisted catalog pick for Board Spotlight (mirrors `stremio-core` `ResourceRequest` identity).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotlightCatalogPick {
    pub transport_url: String,
    pub r#type: String,
    pub id: String,
}

/// Board Spotlight settings kept in `<data_dir>/settings.json` (mirrors upstream `BoardSpotlight`).
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpotlightConfig {
    pub enabled: bool,
    pub catalogs: Vec<SpotlightCatalogPick>,
    pub show_catalog_rows: bool,
}

impl Default for SpotlightConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            catalogs: Vec::new(),
            show_catalog_rows: true,
        }
    }
}

/// Settings kept beside the stremio-core profile (`<data_dir>/settings.json`).
/// Unknown keys are ignored on load so a file from another build never resets active choices.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct LocalExtras {
    pub overrides: HashMap<String, Value>,
    #[serde(default)]
    pub wrap_streams: bool,
    #[serde(default = "yes")]
    pub skip_intros: bool,
    #[serde(default)]
    pub enabled_themes: Vec<String>,
    #[serde(default)]
    pub builtin_settings: HashMap<String, HashMap<String, Value>>,
    #[serde(default)]
    pub mpv_config_path: Option<String>,
    #[serde(default)]
    pub spotlight: SpotlightConfig,
}

fn yes() -> bool {
    true
}

impl Default for LocalExtras {
    fn default() -> Self {
        Self {
            overrides: HashMap::new(),
            wrap_streams: false,
            skip_intros: true,
            enabled_themes: Vec::new(),
            builtin_settings: HashMap::new(),
            mpv_config_path: None,
            spotlight: SpotlightConfig::default(),
        }
    }
}

impl LocalExtras {
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join("settings.json");
        let previous = data_dir.join("native_settings.json");
        if !path.exists() && previous.exists() {
            let _ = std::fs::rename(&previous, &path);
        }
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, data_dir: &Path) {
        let path = data_dir.join("settings.json");
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::create_dir_all(data_dir);
            let _ = std::fs::write(path, json);
        }
    }
}

/// Generates a random RFC-4122 v4 UUID using OS entropy, or `None` if entropy is unavailable.
pub(crate) fn new_uuid() -> Option<String> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).ok()?;
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Some(format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_streams_survives_restart_and_legacy_filename_migrates() {
        let dir =
            std::env::temp_dir().join(format!("stremio-local-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let mut extras = LocalExtras::default();
        assert!(!extras.wrap_streams);
        extras.wrap_streams = true;
        extras.save(&dir);
        assert!(LocalExtras::load(&dir).wrap_streams);

        let legacy = dir.join("legacy");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("native_settings.json"), r#"{"overrides":{}}"#).unwrap();
        assert!(!LocalExtras::load(&legacy).wrap_streams);
        assert!(legacy.join("settings.json").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
