use serde::Serialize;

use crate::mod_manager;

const GITHUB_RELEASES_URL: &str =
    "https://api.github.com/repos/theguy000/stremio-lightning/releases/latest";

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateInfo {
    pub has_update: bool,
    pub current_version: String,
    pub new_version: String,
    pub release_url: String,
    pub body: String,
}

impl Default for AppUpdateInfo {
    fn default() -> Self {
        Self {
            has_update: false,
            current_version: env!("CARGO_PKG_VERSION").to_string(),
            new_version: String::new(),
            release_url: String::new(),
            body: String::new(),
        }
    }
}

/// Checks the GitHub Releases API for a newer version of Stremio Lightning.
/// Returns update info on success, or a default "no update" struct on any error
/// (graceful degradation — never surfaces network errors to the user).
pub async fn check_app_update() -> Result<AppUpdateInfo, String> {
    let current_version = env!("CARGO_PKG_VERSION");

    let client = reqwest::Client::new();
    let response = client
        .get(GITHUB_RELEASES_URL)
        .header("User-Agent", "stremio-lightning")
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await;

    let response = match response {
        Ok(r) => r,
        Err(e) => {
            eprintln!("[AppUpdater] HTTP request failed: {}", e);
            return Ok(AppUpdateInfo::default());
        }
    };

    if !response.status().is_success() {
        eprintln!(
            "[AppUpdater] GitHub API returned status: {}",
            response.status()
        );
        return Ok(AppUpdateInfo::default());
    }

    let json: serde_json::Value = match response.json().await {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[AppUpdater] Failed to parse JSON: {}", e);
            return Ok(AppUpdateInfo::default());
        }
    };

    let tag_name = json["tag_name"].as_str().unwrap_or("").to_string();
    let html_url = json["html_url"].as_str().unwrap_or("").to_string();
    let body_raw = json["body"].as_str().unwrap_or("").to_string();

    if tag_name.is_empty() {
        eprintln!("[AppUpdater] No tag_name in release response");
        return Ok(AppUpdateInfo::default());
    }

    // Truncate release notes to ~500 chars (char-boundary-aware to avoid panics on multi-byte UTF-8)
    let body = if body_raw.len() > 500 {
        let mut end = 500;
        while !body_raw.is_char_boundary(end) {
            end -= 1;
        }
        let mut truncated = body_raw[..end].to_string();
        truncated.push_str("...");
        truncated
    } else {
        body_raw
    };

    let has_update = mod_manager::is_newer_version(&tag_name, current_version);

    Ok(AppUpdateInfo {
        has_update,
        current_version: current_version.to_string(),
        new_version: tag_name,
        release_url: html_url,
        body,
    })
}
