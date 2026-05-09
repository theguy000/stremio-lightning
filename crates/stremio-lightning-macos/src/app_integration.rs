use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub const BUNDLE_IDENTIFIER: &str = "com.stremio-lightning.macos";
pub const BUNDLE_EXECUTABLE: &str = "stremio-lightning-macos";
pub const BUNDLE_NAME: &str = crate::APP_NAME;
pub const MINIMUM_MACOS_VERSION: &str = "12.0";
pub const INFO_PLIST: &str = include_str!("../resources/Info.plist");

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleMetadata {
    pub identifier: &'static str,
    pub executable: &'static str,
    pub name: &'static str,
    pub minimum_system_version: &'static str,
    pub icon_file: &'static str,
    pub supports_automatic_graphics_switching: bool,
    pub url_schemes: Vec<&'static str>,
    pub document_extensions: Vec<&'static str>,
}

impl Default for BundleMetadata {
    fn default() -> Self {
        Self {
            identifier: BUNDLE_IDENTIFIER,
            executable: BUNDLE_EXECUTABLE,
            name: BUNDLE_NAME,
            minimum_system_version: MINIMUM_MACOS_VERSION,
            icon_file: "AppIcon",
            supports_automatic_graphics_switching: true,
            url_schemes: vec!["stremio", "magnet"],
            document_extensions: vec!["torrent"],
        }
    }
}

impl BundleMetadata {
    pub fn validate(&self) -> Result<(), String> {
        if self.identifier.trim().is_empty() || !self.identifier.contains('.') {
            return Err("macOS bundle identifier must be reverse-DNS style".to_string());
        }
        if self.executable != BUNDLE_EXECUTABLE {
            return Err("macOS bundle executable must match the Cargo binary".to_string());
        }
        if self.name != crate::APP_NAME {
            return Err("macOS bundle name must match the app name".to_string());
        }
        if self.minimum_system_version != MINIMUM_MACOS_VERSION {
            return Err("macOS bundle minimum version must be 12.0".to_string());
        }
        if !self.url_schemes.contains(&"stremio") || !self.url_schemes.contains(&"magnet") {
            return Err("macOS bundle must register stremio and magnet URL schemes".to_string());
        }
        if !self.document_extensions.contains(&"torrent") {
            return Err("macOS bundle must register torrent documents".to_string());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchIntent {
    Focus,
    StremioDeepLink(String),
    Magnet(String),
    TorrentFile(String),
    FilePath(String),
}

impl LaunchIntent {
    pub fn open_media_value(&self) -> Option<String> {
        match self {
            Self::Focus => None,
            Self::StremioDeepLink(value) | Self::Magnet(value) => Some(value.clone()),
            Self::TorrentFile(value) | Self::FilePath(value) => {
                Some(normalize_file_argument(value))
            }
        }
    }

    pub fn transport_args(&self) -> Option<Value> {
        self.open_media_value()
            .map(|value| json!(["open-media", value]))
    }
}

pub fn launch_intent_from_args<I, S>(args: I) -> Result<LaunchIntent, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    for argument in args {
        let argument = argument.as_ref();
        if argument.starts_with('-') {
            continue;
        }
        if let Some(intent) = classify_launch_argument(argument)? {
            return Ok(intent);
        }
    }
    Ok(LaunchIntent::Focus)
}

pub fn classify_launch_argument(argument: &str) -> Result<Option<LaunchIntent>, String> {
    let lower = argument.to_ascii_lowercase();
    if lower.starts_with("stremio://") {
        Ok(Some(LaunchIntent::StremioDeepLink(argument.to_string())))
    } else if lower.starts_with("magnet:") {
        Ok(Some(LaunchIntent::Magnet(argument.to_string())))
    } else if lower.starts_with("file://") {
        let path = file_url_to_path(argument)?;
        Ok(Some(file_path_intent(path)))
    } else if lower.contains("://") {
        Err(format!("Unsupported macOS launch URL scheme: {argument}"))
    } else if lower.ends_with(".torrent") {
        Ok(Some(LaunchIntent::TorrentFile(argument.to_string())))
    } else if Path::new(argument).is_absolute() || argument.contains('/') || argument.contains('\\')
    {
        Ok(Some(LaunchIntent::FilePath(argument.to_string())))
    } else {
        Ok(None)
    }
}

pub fn lifecycle_event_payload(event: AppLifecycleEvent) -> (&'static str, Value) {
    match event {
        AppLifecycleEvent::BecameActive => ("app-became-active", json!({ "active": true })),
        AppLifecycleEvent::ResignedActive => ("app-resigned-active", json!({ "active": false })),
        AppLifecycleEvent::WindowFocused(focused) => {
            ("window-focus-changed", json!({ "focused": focused }))
        }
        AppLifecycleEvent::WindowVisible(visible) => {
            ("window-visible-changed", json!({ "visible": visible }))
        }
        AppLifecycleEvent::Shutdown => ("app-shutdown", Value::Null),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppLifecycleEvent {
    BecameActive,
    ResignedActive,
    WindowFocused(bool),
    WindowVisible(bool),
    Shutdown,
}

fn file_path_intent(path: String) -> LaunchIntent {
    if path.to_ascii_lowercase().ends_with(".torrent") {
        LaunchIntent::TorrentFile(path)
    } else {
        LaunchIntent::FilePath(path)
    }
}

fn file_url_to_path(url: &str) -> Result<String, String> {
    let path = url
        .strip_prefix("file://")
        .ok_or_else(|| "Invalid macOS file URL".to_string())?;
    if path.is_empty() {
        return Err("macOS file URL must include a path".to_string());
    }
    Ok(path.to_string())
}

fn normalize_file_argument(value: &str) -> String {
    if value.starts_with("file://") {
        value.to_string()
    } else {
        let path = PathBuf::from(value);
        if path.is_absolute() || path.exists() {
            format!("file://{}", value.replace('\\', "/"))
        } else {
            value.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_metadata_matches_phase_7_contract() {
        let metadata = BundleMetadata::default();
        metadata.validate().unwrap();
        assert!(INFO_PLIST.contains(BUNDLE_IDENTIFIER));
        assert!(INFO_PLIST.contains(BUNDLE_EXECUTABLE));
        assert!(INFO_PLIST.contains(MINIMUM_MACOS_VERSION));
        assert!(INFO_PLIST.contains("stremio"));
        assert!(INFO_PLIST.contains("magnet"));
        assert!(INFO_PLIST.contains("torrent"));
    }

    #[test]
    fn parses_url_and_file_launch_intents() {
        assert_eq!(
            classify_launch_argument("stremio://detail/movie/tt123")
                .unwrap()
                .unwrap(),
            LaunchIntent::StremioDeepLink("stremio://detail/movie/tt123".to_string())
        );
        assert_eq!(
            classify_launch_argument("magnet:?xt=urn:btih:test")
                .unwrap()
                .unwrap(),
            LaunchIntent::Magnet("magnet:?xt=urn:btih:test".to_string())
        );
        assert_eq!(
            classify_launch_argument("/tmp/movie.torrent")
                .unwrap()
                .unwrap(),
            LaunchIntent::TorrentFile("/tmp/movie.torrent".to_string())
        );
        assert_eq!(
            classify_launch_argument("/tmp/movie.mp4").unwrap().unwrap(),
            LaunchIntent::FilePath("/tmp/movie.mp4".to_string())
        );
    }

    #[test]
    fn rejects_unsupported_launch_schemes() {
        assert_eq!(
            classify_launch_argument("ftp://example.com/file.torrent").unwrap_err(),
            "Unsupported macOS launch URL scheme: ftp://example.com/file.torrent"
        );
    }

    #[test]
    fn launch_intent_serializes_open_media_transport_args() {
        assert_eq!(
            LaunchIntent::Magnet("magnet:?xt=urn:btih:test".to_string()).transport_args(),
            Some(json!(["open-media", "magnet:?xt=urn:btih:test"]))
        );
        assert_eq!(LaunchIntent::Focus.transport_args(), None);
    }

    #[test]
    fn lifecycle_events_serialize_to_host_payloads() {
        assert_eq!(
            lifecycle_event_payload(AppLifecycleEvent::BecameActive),
            ("app-became-active", json!({ "active": true }))
        );
        assert_eq!(
            lifecycle_event_payload(AppLifecycleEvent::WindowVisible(false)),
            ("window-visible-changed", json!({ "visible": false }))
        );
    }
}
