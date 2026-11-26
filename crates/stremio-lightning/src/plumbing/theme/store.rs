//! Disk storage for user `.theme.css` files under `<data_dir>/themes/`.
//!
//! Reads each theme file once (returning both metadata and CSS body) and validates filenames
//! against path traversal before touching disk.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::Regex;

const THEME_EXTENSION: &str = ".theme.css";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeMetadata {
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledTheme {
    pub filename: String,
    pub metadata: Option<ThemeMetadata>,
    pub css: String,
}

/// The folder holding local `.theme.css` files (`<data_dir>/themes`).
#[must_use]
pub fn themes_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("themes")
}

/// Parses the leading `/** @name ... @description ... @author ... @version ... */` comment.
pub fn parse_metadata(content: &str) -> Option<ThemeMetadata> {
    static BLOCK_RE: OnceLock<Regex> = OnceLock::new();
    static TAG_RE: OnceLock<Regex> = OnceLock::new();

    let block = BLOCK_RE
        .get_or_init(|| Regex::new(r"(?s)/\*\*(.*?)\*/").expect("valid block regex"))
        .find(content)?
        .as_str();
    let tag_re =
        TAG_RE.get_or_init(|| Regex::new(r"@(\w+)\s+([^\n\r]+)").expect("valid tag regex"));

    let mut tags: HashMap<String, String> = HashMap::new();
    for cap in tag_re.captures_iter(block) {
        tags.insert(cap[1].to_owned(), cap[2].trim().to_owned());
    }
    Some(ThemeMetadata {
        name: tags.get("name")?.clone(),
        description: tags.get("description")?.clone(),
        author: tags.get("author")?.clone(),
        version: tags.get("version")?.clone(),
    })
}

/// Lists every `.theme.css` file under `<data_dir>/themes`, reading each file once.
pub fn list(data_dir: &Path) -> Result<Vec<InstalledTheme>, String> {
    let dir = themes_dir(data_dir);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let entries = std::fs::read_dir(&dir).map_err(|e| format!("Failed to read directory: {e}"))?;
    let mut themes = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read entry: {e}"))?;
        let filename = entry.file_name().to_string_lossy().into_owned();
        if filename.ends_with(THEME_EXTENSION) {
            let css = std::fs::read_to_string(entry.path())
                .map_err(|e| format!("Failed to read file {filename}: {e}"))?;
            let metadata = parse_metadata(&css);
            themes.push(InstalledTheme {
                filename,
                metadata,
                css,
            });
        }
    }
    themes.sort_by(|a, b| a.filename.cmp(&b.filename));
    Ok(themes)
}

/// Reads one `.theme.css` file under `<data_dir>/themes`.
pub fn read(data_dir: &Path, filename: &str) -> Result<String, String> {
    validate_theme_filename(filename)?;
    std::fs::read_to_string(themes_dir(data_dir).join(filename))
        .map_err(|e| format!("Failed to read file: {e}"))
}

/// Deletes one `.theme.css` file under `<data_dir>/themes`.
pub fn delete(data_dir: &Path, filename: &str) -> Result<(), String> {
    validate_theme_filename(filename)?;
    std::fs::remove_file(themes_dir(data_dir).join(filename))
        .map_err(|e| format!("Failed to delete file: {e}"))
}

fn validate_theme_filename(filename: &str) -> Result<(), String> {
    if filename.is_empty()
        || filename.contains('/')
        || filename.contains('\\')
        || filename.contains("..")
        || filename.contains('\0')
    {
        return Err("Invalid theme filename".to_owned());
    }
    if !filename.ends_with(THEME_EXTENSION) {
        return Err("Invalid theme filename extension".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "stremio-lightning-themes-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn rejects_traversal_and_wrong_extensions() {
        for bad in [
            "",
            "../evil.theme.css",
            "..\\evil.theme.css",
            "nested/evil.theme.css",
            "evil\0.theme.css",
            "script.plugin.js",
        ] {
            assert!(validate_theme_filename(bad).is_err(), "{bad} should fail");
        }
        assert!(validate_theme_filename("dark.theme.css").is_ok());
    }

    #[test]
    fn lists_reads_and_deletes_under_data_dir_themes() {
        let data_dir = temp_dir("crud");
        let dir = themes_dir(&data_dir);
        std::fs::create_dir_all(&dir).unwrap();
        let body = "/**\n * @name Neon\n * @description Bright\n * @author Ada\n * @version 1.0.0\n */\n.nav-tab-button { color: #fff; }\n";
        std::fs::write(dir.join("neon.theme.css"), body).unwrap();

        let listed = list(&data_dir).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].metadata.as_ref().unwrap().name, "Neon");
        assert_eq!(listed[0].css, body);
        assert_eq!(read(&data_dir, "neon.theme.css").unwrap(), body);

        delete(&data_dir, "neon.theme.css").unwrap();
        assert!(list(&data_dir).unwrap().is_empty());
        let _ = std::fs::remove_dir_all(data_dir);
    }
}
