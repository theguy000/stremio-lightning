use serde::{Deserialize, Serialize};

const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/theguy000/stremio-lightning/releases/latest";
const USER_AGENT: &str = "stremio-lightning-updater";

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateInfo {
    pub has_update: bool,
    pub current_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_url: Option<String>,
}

#[derive(Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
}

pub async fn check_app_update(current_version: &str) -> Result<AppUpdateInfo, String> {
    let response = reqwest::Client::new()
        .get(LATEST_RELEASE_URL)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .send()
        .await
        .map_err(|e| format!("Failed to check app update: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "App update check failed with status: {}",
            response.status()
        ));
    }

    let release = response
        .json::<GitHubRelease>()
        .await
        .map_err(|e| format!("Failed to parse app update response: {e}"))?;
    let latest_version = normalize_version(&release.tag_name);
    let current_version = normalize_version(current_version);

    let has_update = is_newer_version(&latest_version, &current_version);
    Ok(AppUpdateInfo {
        has_update,
        current_version,
        new_version: has_update.then_some(latest_version),
        release_url: has_update.then_some(release.html_url),
    })
}

fn normalize_version(version: &str) -> String {
    version.trim().trim_start_matches('v').to_string()
}

fn is_newer_version(candidate: &str, installed: &str) -> bool {
    let parse = |version: &str| -> Vec<(u64, bool)> {
        version
            .strip_prefix('v')
            .unwrap_or(version)
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

    let candidate_parts = parse(candidate);
    let installed_parts = parse(installed);
    for index in 0..candidate_parts.len().max(installed_parts.len()) {
        let (candidate_num, candidate_pre) =
            candidate_parts.get(index).copied().unwrap_or((0, false));
        let (installed_num, installed_pre) =
            installed_parts.get(index).copied().unwrap_or((0, false));
        if candidate_num > installed_num
            || (candidate_num == installed_num && !candidate_pre && installed_pre)
        {
            return true;
        }
        if candidate_num < installed_num
            || (candidate_num == installed_num && candidate_pre && !installed_pre)
        {
            return false;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serializes_banner_shape_as_camel_case() {
        let info = AppUpdateInfo {
            has_update: true,
            current_version: "0.1.0".to_string(),
            new_version: Some("0.2.0".to_string()),
            release_url: Some(
                "https://github.com/theguy000/stremio-lightning/releases/tag/v0.2.0".to_string(),
            ),
        };

        assert_eq!(
            serde_json::to_value(info).unwrap(),
            json!({
                "hasUpdate": true,
                "currentVersion": "0.1.0",
                "newVersion": "0.2.0",
                "releaseUrl": "https://github.com/theguy000/stremio-lightning/releases/tag/v0.2.0"
            })
        );
    }

    #[test]
    fn compares_semver_like_versions() {
        assert!(is_newer_version("0.2.0", "0.1.9"));
        assert!(is_newer_version("1.0.0", "1.0.0-beta.1"));
        assert!(!is_newer_version("1.0.0-beta.1", "1.0.0"));
        assert!(!is_newer_version("1.0.0", "1.0.0"));
    }
}
