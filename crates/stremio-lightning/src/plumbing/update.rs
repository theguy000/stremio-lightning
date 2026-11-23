//! Checks the published GitHub Pages release manifest for a newer version.

use std::cmp::Ordering;
use std::time::Duration;

use serde::Deserialize;

const UPDATE_MANIFEST_URL: &str = "https://theguy000.github.io/stremio-lightning/latest.json";
const USER_AGENT: &str = "stremio-lightning-updater";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UpdateInfo {
    pub(crate) current_version: String,
    pub(crate) new_version: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateManifest {
    version: String,
}

/// Checks for a newer release. Suppressed in debug builds unless
/// `STREMIO_LIGHTNING_FORCE_UPDATE_CHECK` is set.
pub(crate) async fn check_app_update(current_version: &str) -> Result<UpdateInfo, String> {
    let current = normalize_version(current_version);
    if current == "0.0.0"
        || (cfg!(debug_assertions)
            && std::env::var_os("STREMIO_LIGHTNING_FORCE_UPDATE_CHECK").is_none())
    {
        return Ok(UpdateInfo {
            current_version: current,
            new_version: None,
        });
    }
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("Failed to build update client: {e}"))?;
    check_at(&client, UPDATE_MANIFEST_URL, &current).await
}

async fn check_at(
    client: &reqwest::Client,
    manifest_url: &str,
    current_version: &str,
) -> Result<UpdateInfo, String> {
    let response = client
        .get(manifest_url)
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
    let latest = normalize_version(&manifest.version);
    let current = normalize_version(current_version);
    let new_version = is_newer_version(&latest, &current).then_some(latest);
    Ok(UpdateInfo {
        current_version: current,
        new_version,
    })
}

fn normalize_version(version: &str) -> String {
    version.trim().trim_start_matches('v').to_owned()
}

fn is_newer_version(candidate: &str, installed: &str) -> bool {
    fn parse(version: &str) -> (Vec<u64>, Option<&str>) {
        let (core, pre) = version
            .split_once('-')
            .map_or((version, None), |(core, pre)| (core, Some(pre)));
        (
            core.split('.').map(|p| p.parse().unwrap_or(0)).collect(),
            pre,
        )
    }
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

    #[test]
    fn compares_semver_like_versions() {
        assert!(is_newer_version("0.2.0", "0.1.9"));
        assert!(is_newer_version("1.0.0", "1.0.0-beta.1"));
        assert!(!is_newer_version("1.0.0-beta.1", "1.0.0"));
        assert!(!is_newer_version("1.0.0", "1.0.0"));
        assert!(is_newer_version("1.0.0-rc.1", "1.0.0-beta.2"));
        assert!(is_newer_version("1.0.0-beta.10", "1.0.0-beta.2"));
        assert!(is_newer_version("1.1", "1.0.9"));
    }

    #[tokio::test]
    async fn dev_builds_and_zero_version_suppress_update_checks() {
        let zero = check_app_update("0.0.0").await.unwrap();
        assert_eq!(zero.current_version, "0.0.0");
        assert!(zero.new_version.is_none());

        let dev = check_app_update("0.1.4").await.unwrap();
        assert_eq!(dev.current_version, "0.1.4");
        assert!(dev.new_version.is_none());
    }
}
