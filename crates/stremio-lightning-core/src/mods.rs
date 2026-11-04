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

#[must_use]
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

/// The file a download is stored as. `filename` is the name the caller wants it under, which
/// `install_mod` works out from the registry entry; `None` asks for the name the url ends with,
/// which is what a caller that has only a url can know.
///
/// # Errors
/// Returns an error when the url gives no usable file name.
pub async fn download_mod_as(
    app_data_dir: &Path,
    url: &str,
    mod_type: ModType,
    filename: Option<&str>,
) -> Result<String, String> {
    let filename = match filename {
        Some(filename) => filename.to_string(),
        None => filename_from_url(url)?,
    };
    validate_mod_filename(&filename, mod_type)?;

    let response = crate::http::client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to download: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "Download failed with status: {}",
            response.status()
        ));
    }

    let content = response
        .bytes()
        .await
        .map_err(|e| format!("Failed to read response body: {e}"))?;
    write_mod_content(app_data_dir, &filename, mod_type, &content)?;
    Ok(filename)
}

/// The body at `url`, read into memory and written nowhere. An update is fetched with this so a
/// body that is not a mod is known before it can replace a working file.
///
/// # Errors
/// Returns an error when the request fails or answers with a status other than success.
pub async fn fetch_text(url: &str) -> Result<String, String> {
    let response = crate::http::client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to download: {e}"))?;
    if !response.status().is_success() {
        return Err(format!(
            "Download failed with status: {}",
            response.status()
        ));
    }
    response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {e}"))
}

/// Downloads `url` into the folder for `mod_type` under the name the url ends with.
///
/// # Errors
/// Returns an error when the mod cannot be downloaded or written to disk.
pub async fn download_mod(
    app_data_dir: &Path,
    url: &str,
    mod_type: ModType,
) -> Result<String, String> {
    download_mod_as(app_data_dir, url, mod_type, None).await
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

/// One read of the registry and the instant it was read, kept for five minutes so the shell's
/// every look at the Marketplace page is not a request.
///
/// The cache belongs to the address it was read from, so a test that points the registry at its
/// own server is never answered with the copy another test's server handed over.
pub struct RegistryCache {
    url: String,
    registry: Registry,
    fetched_at: std::time::Instant,
}

static REGISTRY_CACHE: OnceLock<std::sync::Mutex<Option<RegistryCache>>> = OnceLock::new();
static REGISTRY_FETCH_MUTEX: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn cached_registry(
    cache_mutex: &std::sync::Mutex<Option<RegistryCache>>,
    url: &str,
) -> Option<Registry> {
    let guard = cache_mutex.lock().ok()?;
    let cache = guard.as_ref()?;
    if cache.url != url {
        return None;
    }
    if cache.fetched_at.elapsed() < std::time::Duration::from_secs(300) {
        Some(cache.registry.clone())
    } else {
        None
    }
}

/// # Errors
/// Returns an error when the registry cannot be fetched, parsed, or cached.
pub async fn fetch_registry() -> Result<Registry, String> {
    let cache_mutex = REGISTRY_CACHE.get_or_init(|| std::sync::Mutex::new(None));
    let fetch_mutex = REGISTRY_FETCH_MUTEX.get_or_init(|| tokio::sync::Mutex::new(()));
    let url = registry_url()?;

    // 1. Fast path: return cached registry if it exists, is for this address, and is less than
    // 5 minutes old
    if let Some(registry) = cached_registry(cache_mutex, &url) {
        return Ok(registry);
    }

    // 2. Slow path: serialize network fetch using async Mutex
    let _guard = fetch_mutex.lock().await;

    // 3. Double-check cache inside the lock
    if let Some(registry) = cached_registry(cache_mutex, &url) {
        return Ok(registry);
    }

    // 4. Fetch over network on cache miss
    fetch_registry_from(&url, cache_mutex).await
}

/// Where the registry is read from, unless a test names another address.
const REGISTRY_URL: &str = concat!(
    "https://raw.githubusercontent.com/theguy000/",
    "stremio-lightning-registry/refs/heads/main/registry.json",
);

/// The address the registry is read from: `REGISTRY_URL`, or the address a test set.
///
/// # Errors
/// Returns an error when a test set an address that is not an absolute http or https url.
pub fn registry_url() -> Result<String, String> {
    let name = std::env::var(REGISTRY_URL_VAR).ok();
    registry_url_from(name.as_deref(), REGISTRY_URL)
}

/// The environment variable a test sets to have the registry read from its own server. The shell
/// never sets it, and it is read on every fetch so one test cannot hand another its server.
pub const REGISTRY_URL_VAR: &str = "STREMIO_LIGHTNING_REGISTRY_URL";

/// `registry_url` as a plain function of the variable's value, so the rule can be checked without
/// setting an environment variable other tests share.
///
/// # Errors
/// Returns an error when the named address is not an absolute http or https url.
pub fn registry_url_from(named: Option<&str>, default: &str) -> Result<String, String> {
    let Some(named) = named.map(str::trim).filter(|named| !named.is_empty()) else {
        return Ok(default.to_string());
    };
    let lower = named.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        Ok(named.to_string())
    } else {
        Err(format!(
            "{REGISTRY_URL_VAR} must be an http or https address, not {named}"
        ))
    }
}

/// Reads and caches the registry at `url`. The cache is the caller's, so a test's own server can
/// be read again with no wait.
///
/// # Errors
/// Returns an error when the registry cannot be fetched or parsed.
pub async fn fetch_registry_from(
    url: &str,
    cache_mutex: &std::sync::Mutex<Option<RegistryCache>>,
) -> Result<Registry, String> {
    let response = crate::http::client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch registry: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!("Registry fetch failed with status: {status}"));
    }

    let registry = response
        .json::<Registry>()
        .await
        .map_err(|e| format!("Failed to parse registry: {e}"))?;

    if let Ok(mut guard) = cache_mutex.lock() {
        *guard = Some(RegistryCache {
            url: url.to_string(),
            registry: registry.clone(),
            fetched_at: std::time::Instant::now(),
        });
    }

    Ok(registry)
}

async fn check_mod_updates_internal(
    filename: &str,
    mod_type: ModType,
    metadata: Option<ModMetadata>,
    registry: Option<&Registry>,
) -> Result<UpdateInfo, String> {
    validate_mod_filename(filename, mod_type)?;
    let Some(metadata) = metadata else {
        return Ok(UpdateInfo::unavailable(None));
    };

    let installed_version = metadata.version.clone();
    let mut new_version = None;
    let mut has_update = false;
    let mut resolved_update_url = metadata.update_url.clone();

    if let Some(update_url) = &metadata.update_url {
        let remote_response = crate::http::client()
            .get(update_url)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch update: {e}"))?;

        if remote_response.status().is_success() {
            let remote_content = remote_response
                .text()
                .await
                .map_err(|e| format!("Failed to read remote content: {e}"))?;
            if let Some(remote_meta) = parse_metadata(&remote_content) {
                if is_newer_version(&remote_meta.version, &installed_version) {
                    has_update = true;
                    new_version = Some(remote_meta.version);
                }
            }
        }
    }

    let mut registry_version = None;
    if let Some(registry) = registry {
        if let Some(entry) = registry_match(registry, filename, mod_type, &metadata) {
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

/// # Errors
/// Returns an error when the remote update index cannot be fetched or parsed.
pub async fn check_mod_updates(
    app_data_dir: &Path,
    mod_type: ModType,
) -> Result<HashMap<String, UpdateInfo>, String> {
    let mods = list_mods(app_data_dir, mod_type)?;
    let registry = fetch_registry().await.ok();
    let mut tasks = Vec::new();

    for md in mods {
        let registry = registry.clone();
        let InstalledMod {
            filename, metadata, ..
        } = md;
        tasks.push(async move {
            let res =
                check_mod_updates_internal(&filename, mod_type, metadata, registry.as_ref()).await;
            (filename, res)
        });
    }

    let results = futures::future::join_all(tasks).await;
    let mut map = HashMap::new();
    for (filename, res) in results {
        map.insert(
            filename,
            res.unwrap_or_else(|_| UpdateInfo::unavailable(None)),
        );
    }

    Ok(map)
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

/// # Errors
/// Returns an error when the filename is empty, contains path separators, or has the wrong extension.
pub fn validate_mod_filename(filename: &str, mod_type: ModType) -> Result<(), String> {
    validate_filename(filename)?;
    if !filename.ends_with(mod_type.file_extension()) {
        return Err(format!("Invalid {} filename extension", mod_type.as_str()));
    }
    Ok(())
}

/// # Errors
/// Returns an error when the URL has no usable file name.
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
        .next_back()
        .filter(|filename| !filename.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "Could not extract filename from URL".to_string())
}

/// The file name a registry download is stored under: the name its URL already ends with when
/// that name carries the mod's extension, and otherwise the name the registry gives it, so a
/// download URL such as `.../latest` lands on disk as `name.plugin.js`.
///
/// # Errors
/// Returns an error when neither part gives a name the mod folder allows.
pub fn installed_filename(entry: &RegistryEntry, mod_type: ModType) -> Result<String, String> {
    match filename_from_url(&entry.download) {
        Ok(filename) if filename.ends_with(mod_type.file_extension()) => Ok(filename),
        _ => {
            // Every character the folder does not accept becomes a separator, and the separators
            // at either end go: "Cool theme!" is stored as "Cool-theme.theme.css", not
            // "Cool-theme-.theme.css". A name with nothing left cannot name a file.
            let name: String = entry
                .name
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                        c
                    } else {
                        '-'
                    }
                })
                .collect();
            let name = name.trim_matches('-');
            if name.is_empty() {
                return Err("The registry gives this mod no name to store it under".to_string());
            }
            let filename = format!("{name}{}", mod_type.file_extension());
            validate_mod_filename(&filename, mod_type)?;
            Ok(filename)
        }
    }
}

/// Whether a mod file expects its next update to arrive from `url`: the url's file name is the
/// mod's file name, or the file's own metadata names the url as its `updateUrl`. The first test
/// is how a registry entry finds the file it installed; the second is all a file has otherwise.
#[must_use]
pub fn same_mod_file(filename: &str, metadata: Option<&ModMetadata>, url: &str) -> bool {
    if filename_from_url(url).ok().as_deref() == Some(filename) {
        return true;
    }
    metadata
        .and_then(|metadata| metadata.update_url.as_deref())
        .is_some_and(|update_url| update_url == url)
}

/// Downloads `url` into the folder for `mod_type` under `filename`, which `installed_filename`
/// works out, and returns that name. The bytes must be a file of that kind: a page that answered
/// where a mod should be is refused rather than written, and the sentence says what arrived.
///
/// # Errors
/// Returns the same sentences `download_mod` does, plus the one for a body that is not the kind
/// of file the folder holds.
pub async fn install_mod(
    app_data_dir: &Path,
    url: &str,
    mod_type: ModType,
    filename: &str,
) -> Result<String, String> {
    validate_mod_filename(filename, mod_type)?;
    let written = download_mod(app_data_dir, url, mod_type).await?;
    let content = read_mod_content(app_data_dir, &written, mod_type)?;
    // A url that named its own file is the one `download_mod` wrote, and that name is what the
    // folder holds now: nothing is renamed under a running plugin.
    let filename = written.clone();
    if !looks_like(&content, mod_type) {
        let _ = delete_mod(app_data_dir, &filename, mod_type);
        return Err(wrong_file_said(mod_type));
    }
    Ok(filename)
}

/// The sentence for a download that arrived but is not a mod of the kind that was asked for. The
/// page shows it as it is, so it names the file type and what it found instead.
#[must_use]
pub fn wrong_file_said(mod_type: ModType) -> String {
    match mod_type {
        ModType::Plugin => {
            "The download was not a plugin. A plugin file is JavaScript with the extension .plugin.js and a comment that names it.".to_string()
        }
        ModType::Theme => {
            "The download was not a theme. A theme file is CSS with the extension .theme.css and a comment that names it.".to_string()
        }
    }
}

/// Whether a downloaded body is a mod of the kind asked for, rather than an error page: it carries
/// the metadata comment every card reads its name from, and nothing an HTML page would start with.
#[must_use]
pub fn looks_like(content: &str, mod_type: ModType) -> bool {
    if parse_metadata(content).is_none() {
        return false;
    }
    let start = content.trim_start().to_ascii_lowercase();
    match mod_type {
        ModType::Plugin => !start.starts_with("<!doctype") && !start.starts_with("<html"),
        ModType::Theme => true,
    }
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

    fn entry(name: &str, download: &str) -> RegistryEntry {
        RegistryEntry {
            name: name.to_string(),
            author: "Alice".to_string(),
            description: None,
            version: "1.0.0".to_string(),
            repo: "https://example.com/repo".to_string(),
            download: download.to_string(),
            preview: None,
        }
    }

    #[test]
    fn a_download_url_that_does_not_end_in_the_extension_is_stored_under_the_registrys_name() {
        assert_eq!(
            installed_filename(
                &entry("Cool Plugin", "https://example.com/a/cool.plugin.js"),
                ModType::Plugin
            )
            .unwrap(),
            "cool.plugin.js"
        );
        assert_eq!(
            installed_filename(
                &entry("Cool Plugin", "https://example.com/latest/download"),
                ModType::Plugin
            )
            .unwrap(),
            "Cool-Plugin.plugin.js"
        );
        assert_eq!(
            installed_filename(
                &entry("Cool theme!", "https://example.com/theme.min.css"),
                ModType::Theme
            )
            .unwrap(),
            "Cool-theme.theme.css"
        );
        assert!(
            installed_filename(&entry("", "https://example.com/latest"), ModType::Plugin).is_err(),
            "a name with nothing in it cannot be stored"
        );
    }

    #[test]
    fn a_mod_file_is_recognised_by_its_name_or_by_its_own_update_url() {
        let named = ModMetadata {
            name: "Renamed".to_string(),
            description: "Demo".to_string(),
            author: "Alice".to_string(),
            version: "1.0.0".to_string(),
            update_url: Some("https://example.com/renamed.plugin.js".to_string()),
            source: None,
            license: None,
            homepage: None,
            requirements: None,
        };
        assert!(same_mod_file(
            "renamed.plugin.js",
            Some(&named),
            "https://mirror.example/renamed.plugin.js"
        ));
        assert!(same_mod_file(
            "other.plugin.js",
            Some(&named),
            "https://example.com/renamed.plugin.js"
        ));
        assert!(!same_mod_file(
            "other.plugin.js",
            None,
            "https://example.com/renamed.plugin.js"
        ));
    }

    #[test]
    fn a_download_that_is_not_a_mod_is_not_written() {
        let plugin =
            "/**\n * @name Fine\n * @description Demo\n * @author Alice\n * @version 1.0.0\n */\n";
        assert!(looks_like(plugin, ModType::Plugin));
        assert!(
            !looks_like(
                "<!DOCTYPE html><html><body>404</body></html>",
                ModType::Plugin
            ),
            "an error page is not a plugin"
        );
        assert!(
            !looks_like("console.log(1)", ModType::Plugin),
            "a file with no metadata has no name to give"
        );
        assert!(wrong_file_said(ModType::Plugin).contains(".plugin.js"));
    }

    #[test]
    fn the_registry_address_is_cores_unless_a_test_names_one() {
        assert_eq!(registry_url_from(None, REGISTRY_URL).unwrap(), REGISTRY_URL);
        assert_eq!(
            registry_url_from(Some("  "), REGISTRY_URL).unwrap(),
            REGISTRY_URL
        );
        assert_eq!(
            registry_url_from(Some("http://127.0.0.1:9/registry.json"), REGISTRY_URL).unwrap(),
            "http://127.0.0.1:9/registry.json"
        );
        assert!(registry_url_from(Some("file:///etc/passwd"), REGISTRY_URL).is_err());
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

    #[tokio::test]
    async fn check_mod_updates_checks_all_installed_mods() {
        let root = temp_dir("bulk-check-test");
        write_mod_content(
            &root,
            "plugin1.plugin.js",
            ModType::Plugin,
            br"/**
 * @name Plugin One
 * @description Demo
 * @author Alice
 * @version 1.0.0
 */",
        )
        .unwrap();

        write_mod_content(
            &root,
            "plugin2.plugin.js",
            ModType::Plugin,
            br"/**
 * @name Plugin Two
 * @description Demo
 * @author Bob
 * @version 1.1.0
 */",
        )
        .unwrap();

        let results = check_mod_updates(&root, ModType::Plugin).await.unwrap();

        assert_eq!(results.len(), 2);
        assert!(results.contains_key("plugin1.plugin.js"));
        assert!(results.contains_key("plugin2.plugin.js"));

        let p1_res = results.get("plugin1.plugin.js").unwrap();
        assert_eq!(p1_res.installed_version, Some("1.0.0".to_string()));

        let _ = fs::remove_dir_all(root);
    }
}
