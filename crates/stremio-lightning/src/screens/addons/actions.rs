//! Core action dispatchers and pure URL helpers for the Addons screen (zero browser I/O).

use stremio_core::models::addon_details::Selected as AddonDetailsSelected;
use stremio_core::models::catalog_with_filters::Selected as CatalogSelected;
use stremio_core::models::common::Loadable;
use stremio_core::models::installed_addons_with_filters::Selected as InstalledSelected;
use stremio_core::runtime::msg::{Action, ActionCtx, ActionLoad};
use stremio_core::runtime::RuntimeAction;
use url::Url;

use super::{AddonCard, Choice};
use crate::app::{AppModelField, AppRuntime};

/// Outcome of clicking Install on a marketplace card: either dispatched to core or needs the
/// addon's configuration page opened by `shell/`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InstallOutcome {
    Done,
    OpenConfigure(String),
}

/// Web: picking an option in either Addons menu loads that installed or remote list.
pub(crate) fn select(runtime: &AppRuntime, choice: &Choice) {
    let (field, action) = match choice {
        Choice::Installed(request) => {
            runtime.dispatch(RuntimeAction {
                field: Some(AppModelField::RemoteAddons),
                action: Action::Unload,
            });
            (
                Some(AppModelField::InstalledAddons),
                Action::Load(ActionLoad::InstalledAddonsWithFilters(InstalledSelected {
                    request: request.clone(),
                })),
            )
        }
        Choice::Remote(request) => (
            Some(AppModelField::RemoteAddons),
            Action::Load(ActionLoad::CatalogWithFilters(Some(CatalogSelected {
                request: request.clone(),
            }))),
        ),
    };
    runtime.dispatch(RuntimeAction { field, action });
}

/// Web: installs a marketplace card directly, or returns its configuration URL for `shell/`.
pub(crate) fn install_card(runtime: &AppRuntime, card: &AddonCard) -> InstallOutcome {
    if card.installed {
        return InstallOutcome::Done;
    }
    if card.configuration_required {
        return InstallOutcome::OpenConfigure(configure_url(&card.transport_url));
    }
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Ctx(ActionCtx::InstallAddon(card.descriptor.clone())),
    });
    InstallOutcome::Done
}

/// Parses a pasted addon URL (`stremio://` -> `https://`) and loads `AddonDetails`.
pub(crate) fn open_details(runtime: &AppRuntime, text: &str) -> Result<(), String> {
    let url = parse_addon_url(text)?;
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Load(ActionLoad::AddonDetails(AddonDetailsSelected {
            transport_url: url,
        })),
    });
    Ok(())
}

fn parse_addon_url(text: &str) -> Result<Url, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Paste the addon's manifest URL first.".to_owned());
    }
    let normalized = text
        .strip_prefix("stremio://")
        .map_or_else(|| text.to_owned(), |rest| format!("https://{rest}"));
    let url =
        Url::parse(&normalized).map_err(|error| format!("Failed to parse addon url: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("An addon URL starts with https:// or stremio://.".to_owned());
    }
    Ok(url)
}

/// Dispatches `InstallAddon` for the manifest currently loaded in `addon_details`.
pub(crate) fn install(runtime: &AppRuntime) {
    let Ok(model) = runtime.model() else {
        return;
    };
    let addon = model
        .addon_details
        .remote_addon
        .as_ref()
        .and_then(|addon| match &addon.content {
            Loadable::Ready(addon) => Some(addon.clone()),
            _ => None,
        });
    drop(model);
    if let Some(addon) = addon {
        runtime.dispatch(RuntimeAction {
            field: None,
            action: Action::Ctx(ActionCtx::InstallAddon(addon)),
        });
    }
}

/// Web: an addon's configuration page URL (`manifest.json` swapped for `configure`).
pub(crate) fn configure_url(transport_url: &str) -> String {
    transport_url.replace("manifest.json", "configure")
}

/// Dispatches `UninstallAddon` for the installed descriptor matching `transport_url`.
pub(crate) fn uninstall(runtime: &AppRuntime, transport_url: &str) {
    let Ok(model) = runtime.model() else {
        return;
    };
    let addon = model
        .ctx
        .profile
        .addons
        .iter()
        .find(|addon| addon.transport_url.as_str() == transport_url)
        .cloned();
    drop(model);
    if let Some(addon) = addon {
        runtime.dispatch(RuntimeAction {
            field: None,
            action: Action::Ctx(ActionCtx::UninstallAddon(addon)),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_addon_url_is_read_the_way_the_web_reads_it() {
        assert_eq!(
            parse_addon_url("https://v3-cinemeta.strem.io/manifest.json")
                .unwrap()
                .as_str(),
            "https://v3-cinemeta.strem.io/manifest.json"
        );
        assert_eq!(
            parse_addon_url("  stremio://v3-cinemeta.strem.io/manifest.json  ")
                .unwrap()
                .as_str(),
            "https://v3-cinemeta.strem.io/manifest.json"
        );
        assert!(parse_addon_url("").is_err());
        assert!(parse_addon_url("file:///etc/passwd").is_err());
        assert!(parse_addon_url("not a url").is_err());
        assert_eq!(
            configure_url("https://example.com/manifest.json"),
            "https://example.com/configure"
        );
    }
}
