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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModType {
    Plugin,
    Theme,
}

impl ModType {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plugin => "plugin",
            Self::Theme => "theme",
        }
    }

    #[must_use]
    pub fn directory_name(self) -> &'static str {
        match self {
            Self::Plugin => "plugins",
            Self::Theme => "themes",
        }
    }

    #[must_use]
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
            _ => Err(format!("Unknown mod type: {value}")),
        }
    }
}

#[must_use]
pub fn mods_dir(app_data_dir: &Path, mod_type: ModType) -> PathBuf {
    app_data_dir
        .join("stremio-lightning")
        .join(mod_type.directory_name())
}

/// # Errors
/// Returns an error when the mod directories cannot be created.
pub fn ensure_dirs(app_data_dir: &Path) -> Result<(), String> {
    for mod_type in [ModType::Plugin, ModType::Theme] {
        std::fs::create_dir_all(mods_dir(app_data_dir, mod_type))
            .map_err(|e| format!("Failed to create {} dir: {}", mod_type.directory_name(), e))?;
    }
    Ok(())
}

static BLOCK_RE: OnceLock<Regex> = OnceLock::new();
static TAG_RE: OnceLock<Regex> = OnceLock::new();

/// # Panics
/// Panics if the compiled-in metadata regex is invalid, which cannot happen for the fixed pattern.
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

/// # Errors
/// Returns an error when the mod directory cannot be listed or a mod file cannot be read.
pub fn list_mods(app_data_dir: &Path, mod_type: ModType) -> Result<Vec<InstalledMod>, String> {
    let dir = mods_dir(app_data_dir, mod_type);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let entries = std::fs::read_dir(&dir).map_err(|e| format!("Failed to read directory: {e}"))?;
    let mut mods = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {e}"))?;
        let filename = entry.file_name().to_string_lossy().to_string();

        if filename.ends_with(mod_type.file_extension()) {
            let content = std::fs::read_to_string(entry.path())
                .map_err(|e| format!("Failed to read file {filename}: {e}"))?;
            mods.push(InstalledMod {
                filename,
                mod_type: mod_type.as_str().to_string(),
                metadata: parse_metadata(&content),
            });
        }
    }

    Ok(mods)
}

/// # Errors
/// Returns an error when the filename is invalid or the mod file cannot be read.
pub fn read_mod_content(
    app_data_dir: &Path,
    filename: &str,
    mod_type: ModType,
) -> Result<String, String> {
    validate_mod_filename(filename, mod_type)?;
    std::fs::read_to_string(mods_dir(app_data_dir, mod_type).join(filename))
        .map_err(|e| format!("Failed to read file: {e}"))
}

/// # Errors
/// Returns an error when the filename is invalid or the mod file cannot be written.
pub fn write_mod_content(
    app_data_dir: &Path,
    filename: &str,
    mod_type: ModType,
    content: &[u8],
) -> Result<(), String> {
    validate_mod_filename(filename, mod_type)?;
    let dir = mods_dir(app_data_dir, mod_type);
    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create mod dir: {e}"))?;
    std::fs::write(dir.join(filename), content).map_err(|e| format!("Failed to write file: {e}"))
}

/// # Errors
/// Returns an error when the filename is invalid or the mod cannot be removed.
pub fn delete_mod(app_data_dir: &Path, filename: &str, mod_type: ModType) -> Result<(), String> {
    validate_mod_filename(filename, mod_type)?;
    let dir = mods_dir(app_data_dir, mod_type);
    std::fs::remove_file(dir.join(filename)).map_err(|e| format!("Failed to delete file: {e}"))?;

    if mod_type == ModType::Plugin {
        let config_name = filename.replace(".plugin.js", ".plugin.json");
        let config_path = dir.join(config_name);
        if config_path.exists() {
            let _ = std::fs::remove_file(config_path);
        }
    }

    Ok(())
}

pub use crate::validation::validate_filename;

/// # Errors
/// Returns an error when the filename is empty, contains path separators, or has the wrong extension.
pub fn validate_mod_filename(filename: &str, mod_type: ModType) -> Result<(), String> {
    validate_filename(filename)?;
    if !filename.ends_with(mod_type.file_extension()) {
        return Err(format!("Invalid {} filename extension", mod_type.as_str()));
    }
    Ok(())
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
}
