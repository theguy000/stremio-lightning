use serde::{Deserialize, Serialize};

const UPDATE_MANIFEST_URL: &str = "https://theguy000.github.io/stremio-lightning/latest.json";
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
#[serde(rename_all = "camelCase")]
struct UpdateManifest {
    version: String,
    release_url: String,
}

/// # Errors
/// Returns an error when the release feed cannot be fetched or parsed.
pub async fn check_app_update(current_version: &str) -> Result<AppUpdateInfo, String> {
    let current_version_norm = normalize_version(current_version);
    if current_version_norm == "0.0.0"
        || (cfg!(debug_assertions)
            && std::env::var_os("STREMIO_LIGHTNING_FORCE_UPDATE_CHECK").is_none())
    {
        return Ok(AppUpdateInfo {
            has_update: false,
            current_version: current_version_norm,
            new_version: None,
            release_url: None,
        });
    }

    let response = crate::http::client()
        .get(UPDATE_MANIFEST_URL)
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

    let manifest = response
        .json::<UpdateManifest>()
        .await
        .map_err(|e| format!("Failed to parse app update response: {e}"))?;
    let latest_version = normalize_version(&manifest.version);
    let current_version = normalize_version(current_version);

    let has_update = is_newer_version(&latest_version, &current_version);
    Ok(AppUpdateInfo {
        has_update,
        current_version,
        new_version: has_update.then_some(latest_version),
        release_url: has_update.then_some(manifest.release_url),
    })
}

fn normalize_version(version: &str) -> String {
    version.trim().trim_start_matches('v').to_string()
}

fn is_newer_version(candidate: &str, installed: &str) -> bool {
    use std::cmp::Ordering;

    fn parse(version: &str) -> (Vec<u64>, Option<&str>) {
        let (core, pre) = version
            .split_once('-')
            .map_or((version, None), |(core, pre)| (core, Some(pre)));
        (
            core.split('.').map(|p| p.parse().unwrap_or(0)).collect(),
            pre,
        )
    }
    // Semver precedence: numeric identifiers sort below alphanumeric ones.
    fn pre_key(pre: &str) -> impl Iterator<Item = (u8, u64, &str)> + '_ {
        pre.split('.')
            .map(|id| id.parse().map_or((1, 0, id), |n| (0, n, "")))
    }

    let ((mut candidate_core, candidate_pre), (mut installed_core, installed_pre)) =
        (parse(candidate), parse(installed));
    let len = candidate_core.len().max(installed_core.len());
    candidate_core.resize(len, 0);
    installed_core.resize(len, 0);

    candidate_core
        .cmp(&installed_core)
        .then_with(|| match (candidate_pre, installed_pre) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(c), Some(i)) => pre_key(c).cmp(pre_key(i)),
        })
        == Ordering::Greater
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn serializes_banner_shape_as_camel_case() {
        let info = AppUpdateInfo {
            has_update: true,
            current_version: "0.1.4".to_string(),
            new_version: Some("0.2.0".to_string()),
            release_url: Some(
                "https://github.com/theguy000/stremio-lightning/releases/tag/v0.2.0".to_string(),
            ),
        };

        assert_eq!(
            serde_json::to_value(info).unwrap(),
            json!({
                "hasUpdate": true,
                "currentVersion": "0.1.4",
                "newVersion": "0.2.0",
                "releaseUrl": "https://github.com/theguy000/stremio-lightning/releases/tag/v0.2.0"
            })
        );
    }

    #[test]
    fn parses_update_manifest_shape() {
        let manifest: UpdateManifest = serde_json::from_value(json!({
            "version": "0.2.0",
            "releaseUrl": "https://github.com/theguy000/stremio-lightning/releases/tag/v0.2.0"
        }))
        .unwrap();

        assert_eq!(manifest.version, "0.2.0");
        assert_eq!(
            manifest.release_url,
            "https://github.com/theguy000/stremio-lightning/releases/tag/v0.2.0"
        );
    }

    #[test]
    fn compares_semver_like_versions() {
        assert!(is_newer_version("0.2.0", "0.1.9"));
        assert!(is_newer_version("1.0.0", "1.0.0-beta.1"));
        assert!(!is_newer_version("1.0.0-beta.1", "1.0.0"));
        assert!(!is_newer_version("1.0.0", "1.0.0"));
        // Prerelease names order alphabetically, numbers after them.
        assert!(is_newer_version("1.0.0-rc.1", "1.0.0-beta.2"));
        assert!(is_newer_version("1.0.0-beta.2", "1.0.0-beta.1"));
        assert!(is_newer_version("1.0.0-beta.10", "1.0.0-beta.2"));
        assert!(!is_newer_version("1.0.0-beta.2", "1.0.0-rc.1"));
        assert!(!is_newer_version("1.0.0-rc.1", "1.0.0-rc.1"));
        assert!(is_newer_version("1.1", "1.0.9"));
    }

    #[tokio::test]
    async fn dev_builds_and_zero_version_suppress_update_checks() {
        let info = check_app_update("0.0.0").await.unwrap();
        assert!(!info.has_update);
        assert_eq!(info.current_version, "0.0.0");
        assert!(info.new_version.is_none());

        // In test mode (debug build without STREMIO_LIGHTNING_FORCE_UPDATE_CHECK),
        // update checks should also be automatically suppressed.
        let dev_info = check_app_update("0.1.4").await.unwrap();
        assert!(!dev_info.has_update);
        assert_eq!(dev_info.current_version, "0.1.4");
        assert!(dev_info.new_version.is_none());
    }
}
