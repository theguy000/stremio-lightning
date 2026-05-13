use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::OnceLock;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct InstalledMod {
    pub filename: String,
    pub mod_type: String,
    pub metadata: Option<ModMetadata>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
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

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Registry {
    pub plugins: Vec<RegistryEntry>,
    pub themes: Vec<RegistryEntry>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UpdateInfo {
    pub has_update: bool,
    pub installed_version: Option<String>,
    pub new_version: Option<String>,
    pub registry_version: Option<String>,
    pub update_url: Option<String>,
}

impl UpdateInfo {
    fn unavailable(installed_version: Option<String>) -> Self {
        Self {
            has_update: false,
            installed_version,
            new_version: None,
            registry_version: None,
            update_url: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModType {
    Plugin,
    Theme,
}

impl ModType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plugin => "plugin",
            Self::Theme => "theme",
        }
    }

    pub fn directory_name(self) -> &'static str {
        match self {
            Self::Plugin => "plugins",
            Self::Theme => "themes",
        }
    }

    pub fn file_extension(self) -> &'static str {
        match self {
            Self::Plugin => ".plugin.js",
            Self::Theme => ".theme.css",
        }
    }
}

impl FromStr for ModType {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "plugin" => Ok(Self::Plugin),
            "theme" => Ok(Self::Theme),
            _ => Err(format!("Unknown mod type: {}", value)),
        }
    }
}

pub fn mods_dir(app_data_dir: &Path, mod_type: ModType) -> PathBuf {
    app_data_dir
        .join("stremio-lightning")
        .join(mod_type.directory_name())
}

pub fn ensure_dirs(app_data_dir: &Path) -> Result<(), String> {
    for mod_type in [ModType::Plugin, ModType::Theme] {
        std::fs::create_dir_all(mods_dir(app_data_dir, mod_type))
            .map_err(|e| format!("Failed to create {} dir: {}", mod_type.directory_name(), e))?;
    }
    Ok(())
}

static BLOCK_RE: OnceLock<Regex> = OnceLock::new();
static TAG_RE: OnceLock<Regex> = OnceLock::new();

pub fn parse_metadata(content: &str) -> Option<ModMetadata> {
    let block_re = BLOCK_RE
        .get_or_init(|| Regex::new(r"(?s)/\*\*(.*?)\*/").expect("block regex should compile"));
    let block_match = block_re.find(content)?;
    let block = block_match.as_str();

    let tag_re = TAG_RE
        .get_or_init(|| Regex::new(r"@(\w+)\s+([^\n\r]+)").expect("tag regex should compile"));

    let mut tags: HashMap<String, String> = HashMap::new();
    for cap in tag_re.captures_iter(block) {
        tags.insert(cap[1].to_string(), cap[2].trim().to_string());
    }

    let requirements = tags.get("requirements").map(|raw| {
        serde_json::from_str::<Vec<String>>(raw).unwrap_or_else(|_| {
            raw.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
    });

    Some(ModMetadata {
        name: tags.get("name")?.clone(),
        description: tags.get("description")?.clone(),
        author: tags.get("author")?.clone(),
        version: tags.get("version")?.clone(),
        update_url: tags.get("updateUrl").cloned(),
        source: tags.get("source").cloned(),
        license: tags.get("license").cloned(),
        homepage: tags.get("homepage").cloned(),
        requirements,
    })
}

pub fn is_newer_version(v1: &str, v2: &str) -> bool {
    let parse = |v: &str| -> Vec<(u64, bool)> {
        v.strip_prefix('v')
            .unwrap_or(v)
            .split('.')
            .map(|part| {
                let (num_part, is_prerelease) = if let Some(idx) = part.find('-') {
                    (&part[..idx], true)
                } else {
                    (part, false)
                };
                (num_part.parse::<u64>().unwrap_or(0), is_prerelease)
            })
            .collect()
    };

    let parts1 = parse(v1);
    let parts2 = parse(v2);

    for i in 0..parts1.len().max(parts2.len()) {
        let (a_num, a_pre) = parts1.get(i).copied().unwrap_or((0, false));
        let (b_num, b_pre) = parts2.get(i).copied().unwrap_or((0, false));
        if a_num > b_num || (a_num == b_num && !a_pre && b_pre) {
            return true;
        }
        if a_num < b_num || (a_num == b_num && a_pre && !b_pre) {
            return false;
        }
    }

    false
}

pub fn list_mods(app_data_dir: &Path, mod_type: ModType) -> Result<Vec<InstalledMod>, String> {
    let dir = mods_dir(app_data_dir, mod_type);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let entries =
        std::fs::read_dir(&dir).map_err(|e| format!("Failed to read directory: {}", e))?;
    let mut mods = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
        let filename = entry.file_name().to_string_lossy().to_string();

        if filename.ends_with(mod_type.file_extension()) {
            let content = std::fs::read_to_string(entry.path())
                .map_err(|e| format!("Failed to read file {}: {}", filename, e))?;
            mods.push(InstalledMod {
                filename,
                mod_type: mod_type.as_str().to_string(),
                metadata: parse_metadata(&content),
            });
        }
    }

    Ok(mods)
}

pub fn read_mod_content(
    app_data_dir: &Path,
    filename: &str,
    mod_type: ModType,
) -> Result<String, String> {
    validate_mod_filename(filename, mod_type)?;
    std::fs::read_to_string(mods_dir(app_data_dir, mod_type).join(filename))
        .map_err(|e| format!("Failed to read file: {}", e))
}

pub fn write_mod_content(
    app_data_dir: &Path,
    filename: &str,
    mod_type: ModType,
    content: &[u8],
) -> Result<(), String> {
    validate_mod_filename(filename, mod_type)?;
    let dir = mods_dir(app_data_dir, mod_type);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create mod dir: {}", e))?;
    std::fs::write(dir.join(filename), content).map_err(|e| format!("Failed to write file: {}", e))
}

pub async fn download_mod(
    app_data_dir: &Path,
    url: &str,
    mod_type: ModType,
) -> Result<String, String> {
    let filename = filename_from_url(url)?;
    validate_mod_filename(&filename, mod_type)?;

    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("Failed to download: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Download failed with status: {}",
            response.status()
        ));
    }

    let content = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;
    write_mod_content(app_data_dir, &filename, mod_type, &content)?;
    Ok(filename)
}

pub fn delete_mod(app_data_dir: &Path, filename: &str, mod_type: ModType) -> Result<(), String> {
    validate_mod_filename(filename, mod_type)?;
    let dir = mods_dir(app_data_dir, mod_type);
    std::fs::remove_file(dir.join(filename))
        .map_err(|e| format!("Failed to delete file: {}", e))?;

    if mod_type == ModType::Plugin {
        let config_name = filename.replace(".plugin.js", ".plugin.json");
        let config_path = dir.join(config_name);
        if config_path.exists() {
            let _ = std::fs::remove_file(config_path);
        }
    }

    Ok(())
}

pub async fn fetch_registry() -> Result<Registry, String> {
    let url = "https://raw.githubusercontent.com/REVENGE977/stremio-enhanced-registry/refs/heads/main/registry.json";
    let response = reqwest::get(url)
        .await
        .map_err(|e| format!("Failed to fetch registry: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Registry fetch failed with status: {}",
            response.status()
        ));
    }

    response
        .json::<Registry>()
        .await
        .map_err(|e| format!("Failed to parse registry: {}", e))
}

pub async fn check_mod_updates(
    app_data_dir: &Path,
    filename: &str,
    mod_type: ModType,
) -> Result<UpdateInfo, String> {
    validate_mod_filename(filename, mod_type)?;
    let content = std::fs::read_to_string(mods_dir(app_data_dir, mod_type).join(filename))
        .map_err(|e| format!("Failed to read file: {}", e))?;

    let Some(metadata) = parse_metadata(&content) else {
        return Ok(UpdateInfo::unavailable(None));
    };

    let installed_version = metadata.version.clone();
    let mut new_version = None;
    let mut has_update = false;
    let mut resolved_update_url = metadata.update_url.clone();

    if let Some(update_url) = metadata.update_url.clone() {
        let remote_response = reqwest::get(&update_url)
            .await
            .map_err(|e| format!("Failed to fetch update: {}", e))?;

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
    }

    let mut registry_version = None;
    if let Ok(registry) = fetch_registry().await {
        if let Some(entry) = registry_match(&registry, filename, mod_type, &metadata) {
            registry_version = Some(entry.version.clone());
            if !has_update && is_newer_version(&entry.version, &installed_version) {
                has_update = true;
                new_version = Some(entry.version.clone());
                resolved_update_url = Some(entry.download.clone());
            }
        }
    }

    Ok(UpdateInfo {
        has_update,
        installed_version: Some(installed_version),
        new_version,
        registry_version,
        update_url: has_update.then_some(resolved_update_url).flatten(),
    })
}

fn registry_entries(registry: &Registry, mod_type: ModType) -> &[RegistryEntry] {
    match mod_type {
        ModType::Plugin => &registry.plugins,
        ModType::Theme => &registry.themes,
    }
}

fn registry_match<'a>(
    registry: &'a Registry,
    filename: &str,
    mod_type: ModType,
    metadata: &ModMetadata,
) -> Option<&'a RegistryEntry> {
    let entries = registry_entries(registry, mod_type);
    entries
        .iter()
        .find(|entry| filename_from_url(&entry.download).ok().as_deref() == Some(filename))
        .or_else(|| {
            entries
                .iter()
                .find(|entry| entry.name == metadata.name && entry.author == metadata.author)
        })
}

pub use crate::validation::validate_filename;

pub fn validate_mod_filename(filename: &str, mod_type: ModType) -> Result<(), String> {
    validate_filename(filename)?;
    if !filename.ends_with(mod_type.file_extension()) {
        return Err(format!("Invalid {} filename extension", mod_type.as_str()));
    }
    Ok(())
}

pub fn filename_from_url(url: &str) -> Result<String, String> {
    let path = url.split('?').next().unwrap_or(url);
    if path.contains("/../") || path.ends_with("/..") || path.contains('\\') || path.contains('\0')
    {
        return Err("Invalid URL path: traversal not allowed".to_string());
    }
    let normalized = path.to_ascii_lowercase();
    if normalized.contains("%2e") || normalized.contains("%2f") || normalized.contains("%5c") {
        return Err("Invalid URL path: encoded traversal not allowed".to_string());
    }

    path.split('/')
        .last()
        .filter(|filename| !filename.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "Could not extract filename from URL".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    fn temp_dir(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "stremio-lightning-core-test-{}-{}",
            std::process::id(),
            name
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn validates_mod_types_and_extensions() {
        assert_eq!("plugin".parse::<ModType>().unwrap(), ModType::Plugin);
        assert_eq!("theme".parse::<ModType>().unwrap(), ModType::Theme);
        assert!("script".parse::<ModType>().is_err());
        assert_eq!(ModType::Plugin.file_extension(), ".plugin.js");
        assert_eq!(ModType::Theme.file_extension(), ".theme.css");
        assert!(validate_mod_filename("example.plugin.js", ModType::Plugin).is_ok());
        assert!(validate_mod_filename("example.theme.css", ModType::Theme).is_ok());
        assert!(validate_mod_filename("example.theme.css", ModType::Plugin).is_err());
        assert!(validate_mod_filename("example.plugin.js", ModType::Theme).is_err());
    }

    #[test]
    fn rejects_path_traversal_filenames() {
        for filename in [
            "../evil.plugin.js",
            "..\\evil.plugin.js",
            "nested/evil.plugin.js",
            "nested\\evil.plugin.js",
            "evil\0.plugin.js",
            "",
        ] {
            assert!(validate_filename(filename).is_err());
            assert!(validate_mod_filename(filename, ModType::Plugin).is_err());
        }
    }

    #[test]
    fn extracts_download_filename_before_validation() {
        assert_eq!(
            filename_from_url("https://example.com/mods/example.plugin.js?download=1").unwrap(),
            "example.plugin.js"
        );
        assert!(filename_from_url("https://example.com/mods/").is_err());
        assert!(filename_from_url("https://example.com/mods/../evil.plugin.js").is_err());
        assert!(filename_from_url("https://example.com/mods/%2e%2e%2fevil.plugin.js").is_err());
    }

    #[test]
    fn helpers_only_write_under_app_data_root() {
        let root = temp_dir("app-data-root");
        write_mod_content(
            &root,
            "example.plugin.js",
            ModType::Plugin,
            b"console.log(1)",
        )
        .unwrap();

        let expected = root
            .join("stremio-lightning")
            .join("plugins")
            .join("example.plugin.js");
        assert_eq!(fs::read_to_string(expected).unwrap(), "console.log(1)");
        assert!(!root.parent().unwrap().join("example.plugin.js").exists());

        for filename in [
            "../evil.plugin.js",
            "nested/evil.plugin.js",
            "evil\0.plugin.js",
        ] {
            assert!(write_mod_content(&root, filename, ModType::Plugin, b"bad").is_err());
        }

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parses_metadata_requirements() {
        let metadata = parse_metadata(
            r#"/**
 * @name Example
 * @description Demo
 * @author Tester
 * @version 1.0.0
 * @requirements ["a","b"]
 */"#,
        )
        .unwrap();

        assert_eq!(metadata.name, "Example");
        assert_eq!(
            metadata.requirements,
            Some(vec!["a".to_string(), "b".to_string()])
        );
        assert_eq!(json!(metadata)["update_url"], serde_json::Value::Null);
    }

    #[test]
    fn registry_match_prefers_download_filename() {
        let metadata = ModMetadata {
            name: "Shared Name".to_string(),
            description: "Demo".to_string(),
            author: "Alice".to_string(),
            version: "1.0.0".to_string(),
            update_url: None,
            source: None,
            license: None,
            homepage: None,
            requirements: None,
        };
        let registry = Registry {
            plugins: vec![
                RegistryEntry {
                    name: "Shared Name".to_string(),
                    author: "Alice".to_string(),
                    description: None,
                    version: "9.0.0".to_string(),
                    repo: "https://example.com/other".to_string(),
                    download: "https://example.com/other.plugin.js".to_string(),
                    preview: None,
                },
                RegistryEntry {
                    name: "Renamed".to_string(),
                    author: "Bob".to_string(),
                    description: None,
                    version: "2.0.0".to_string(),
                    repo: "https://example.com/current".to_string(),
                    download: "https://example.com/current.plugin.js".to_string(),
                    preview: None,
                },
            ],
            themes: vec![],
        };

        let entry = registry_match(&registry, "current.plugin.js", ModType::Plugin, &metadata)
            .expect("registry entry should match by download filename");

        assert_eq!(entry.version, "2.0.0");
    }

    #[test]
    fn registry_match_supports_themes() {
        let metadata = ModMetadata {
            name: "Theme".to_string(),
            description: "Demo".to_string(),
            author: "Alice".to_string(),
            version: "1.0.0".to_string(),
            update_url: None,
            source: None,
            license: None,
            homepage: None,
            requirements: None,
        };
        let registry = Registry {
            plugins: vec![],
            themes: vec![RegistryEntry {
                name: "Theme".to_string(),
                author: "Alice".to_string(),
                description: None,
                version: "2.0.0".to_string(),
                repo: "https://example.com/theme".to_string(),
                download: "https://example.com/theme.theme.css".to_string(),
                preview: None,
            }],
        };

        let entry = registry_match(&registry, "theme.theme.css", ModType::Theme, &metadata)
            .expect("theme registry entry should match");

        assert!(is_newer_version(&entry.version, &metadata.version));
        assert_eq!(entry.download, "https://example.com/theme.theme.css");
    }
}
