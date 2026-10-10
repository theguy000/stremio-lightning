//! Webview pieces every shell shares: the document-start injection bundle, the
//! load state a shell reports, URL validation and the script that hands host
//! events to the page. Each shell keeps its own `Host` and its own JS adapter.

use crate::bridge_assets::{bridge_scripts, load_mod_ui_source, InjectionScript, MOD_UI_NAME};
use crate::host_api::HostEventRecord;
use serde_json::Value;

/// The scripts a shell injects at document start: its host adapter first, then
/// the shared bridge scripts, then the mod UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectionBundle {
    scripts: Vec<InjectionScript>,
}

impl InjectionBundle {
    /// # Errors
    /// Returns an error when the bundled mod UI cannot be read.
    pub fn load(adapter_name: &'static str, adapter_source: String) -> Result<Self, String> {
        Ok(Self::new(
            adapter_name,
            adapter_source,
            load_mod_ui_source()?,
        ))
    }

    #[must_use]
    pub fn new(adapter_name: &'static str, adapter_source: String, mod_ui_source: String) -> Self {
        let mut scripts = vec![InjectionScript {
            name: adapter_name,
            source: adapter_source,
        }];
        scripts.extend(bridge_scripts());
        scripts.push(InjectionScript {
            name: MOD_UI_NAME,
            source: mod_ui_source,
        });
        Self { scripts }
    }

    #[must_use]
    pub fn scripts(&self) -> &[InjectionScript] {
        &self.scripts
    }

    #[must_use]
    pub fn script_names(&self) -> Vec<&'static str> {
        self.scripts.iter().map(|script| script.name).collect()
    }

    #[must_use]
    pub fn script_source(&self, name: &str) -> Option<&str> {
        self.scripts
            .iter()
            .find(|script| script.name == name)
            .map(|script| script.source.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebviewLoadState {
    pub url: String,
    pub devtools: bool,
    pub document_start_scripts: Vec<&'static str>,
    pub loaded: bool,
}

/// What a shell's webview is asked to show, and whether it has been loaded.
#[derive(Debug, Clone)]
pub struct WebviewSession {
    url: String,
    devtools: bool,
    injection: InjectionBundle,
    loaded: bool,
}

impl WebviewSession {
    #[must_use]
    pub fn new(url: impl Into<String>, devtools: bool, injection: InjectionBundle) -> Self {
        Self {
            url: url.into(),
            devtools,
            injection,
            loaded: false,
        }
    }

    /// # Errors
    /// Returns an error when the URL scheme is not allowed.
    pub fn load(&mut self) -> Result<WebviewLoadState, String> {
        validate_load_url(&self.url)?;
        self.loaded = true;
        Ok(self.load_state())
    }

    #[must_use]
    pub fn load_state(&self) -> WebviewLoadState {
        WebviewLoadState {
            url: self.url.clone(),
            devtools: self.devtools,
            document_start_scripts: self.injection.script_names(),
            loaded: self.loaded,
        }
    }

    #[must_use]
    pub fn script_source(&self, name: &str) -> Option<&str> {
        self.injection.script_source(name)
    }
}

/// # Errors
/// Returns an error unless the URL uses `http`, `https` or `file` (any case).
pub fn validate_load_url(url: &str) -> Result<(), String> {
    let lower = url.to_lowercase();
    if ["https://", "http://", "file://"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
    {
        Ok(())
    } else {
        Err(format!("Webview URL must use http, https, or file: {url}"))
    }
}

/// Turns drained host events into scripts that call the shell's dispatch
/// function (`window.<dispatch_global>(event, payload)`) in the page.
///
/// # Errors
/// Returns an error when an event name or payload cannot be serialized.
pub fn event_dispatch_scripts(
    dispatch_global: &str,
    events: Vec<HostEventRecord>,
) -> Result<Vec<String>, String> {
    events
        .into_iter()
        .map(|event| {
            let name = serde_json::to_string(&event.event)
                .map_err(|e| format!("Failed to serialize host event name: {e}"))?;
            let payload = serde_json::to_string::<Value>(&event.payload)
                .map_err(|e| format!("Failed to serialize host event payload: {e}"))?;
            Ok(format!("window.{dispatch_global}({name}, {payload});"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn bundle() -> InjectionBundle {
        InjectionBundle::new(
            "test-adapter",
            "adapter();".to_string(),
            "ui();".to_string(),
        )
    }

    #[test]
    fn bundle_puts_adapter_first_then_bridge_then_mod_ui() {
        let bundle = bundle();
        let mut expected = vec!["test-adapter"];
        expected.extend(bridge_scripts().iter().map(|script| script.name));
        expected.push(MOD_UI_NAME);

        assert_eq!(bundle.script_names(), expected);
        assert_eq!(bundle.script_source("test-adapter"), Some("adapter();"));
        assert_eq!(bundle.script_source(MOD_UI_NAME), Some("ui();"));
        assert_eq!(bundle.script_source("missing"), None);
    }

    #[test]
    fn session_reports_load_state_after_loading() {
        let mut session = WebviewSession::new("file:///tmp/smoke.html", true, bundle());
        assert!(!session.load_state().loaded);

        let state = session.load().unwrap();

        assert!(state.loaded);
        assert!(state.devtools);
        assert_eq!(state.url, "file:///tmp/smoke.html");
        assert_eq!(state.document_start_scripts, bundle().script_names());
    }

    #[test]
    fn session_refuses_to_load_an_unsupported_url() {
        let mut session = WebviewSession::new("stremio://detail/movie", false, bundle());

        assert_eq!(
            session.load().unwrap_err(),
            "Webview URL must use http, https, or file: stremio://detail/movie"
        );
        assert!(!session.load_state().loaded);
    }

    #[test]
    fn load_url_scheme_check_ignores_case() {
        for url in ["https://a", "HTTP://a", "File:///tmp/a"] {
            assert!(validate_load_url(url).is_ok(), "{url}");
        }
        for url in ["ftp://a", "javascript:alert(1)", ""] {
            assert!(validate_load_url(url).is_err(), "{url}");
        }
    }

    #[test]
    fn events_become_dispatch_calls_on_the_shell_global() {
        let events = vec![HostEventRecord {
            event: "shell-transport".to_string(),
            payload: json!({"a": "q\"x"}),
        }];

        assert_eq!(
            event_dispatch_scripts("__DISPATCH__", events).unwrap(),
            vec![r#"window.__DISPATCH__("shell-transport", {"a":"q\"x"});"#.to_string()]
        );
        assert!(event_dispatch_scripts("__DISPATCH__", Vec::new())
            .unwrap()
            .is_empty());
    }
}
