use crate::host::Host;
use crate::player::PlayerBackend;
use crate::streaming_server::ProcessSpawner;
use serde_json::Value;
use std::sync::Arc;

pub const MACOS_HOST_ADAPTER_NAME: &str = "macos-host-adapter";
pub const HOST_ADAPTER_NAME: &str = MACOS_HOST_ADAPTER_NAME;
pub const BRIDGE_UTILS_NAME: &str = "bridge/utils.js";
pub const BRIDGE_CAST_FALLBACK_NAME: &str = "bridge/cast-fallback.js";
pub const BRIDGE_SHELL_TRANSPORT_NAME: &str = "bridge/shell-transport.js";
pub const BRIDGE_EXTERNAL_LINKS_NAME: &str = "bridge/external-links.js";
pub const BRIDGE_SHELL_DETECTION_NAME: &str = "bridge/shell-detection.js";
pub const BRIDGE_BACK_BUTTON_NAME: &str = "bridge/back-button.js";
pub const BRIDGE_SHORTCUTS_NAME: &str = "bridge/shortcuts.js";
pub const BRIDGE_PIP_NAME: &str = "bridge/pip.js";
pub const BRIDGE_DISCORD_RPC_NAME: &str = "bridge/discord-rpc.js";
pub const BRIDGE_UPDATE_BANNER_NAME: &str = "bridge/update-banner.js";
pub const BRIDGE_NAME: &str = "bridge.js";
pub const MOD_UI_NAME: &str = "mod-ui-svelte.iife.js";

#[derive(Debug, Clone)]
pub struct InjectionScript {
    pub name: &'static str,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct InjectionBundle {
    scripts: Vec<InjectionScript>,
}

impl InjectionBundle {
    pub fn load() -> Result<Self, String> {
        let mut scripts = vec![InjectionScript {
            name: HOST_ADAPTER_NAME,
            source: host_adapter(),
        }];
        scripts.extend(bridge_module_scripts());
        scripts.extend([
            InjectionScript {
                name: BRIDGE_NAME,
                source: include_str!("../../../web/bridge/bridge.js").to_string(),
            },
            InjectionScript {
                name: MOD_UI_NAME,
                source: include_str!("../../../src/dist/mod-ui-svelte.iife.js").to_string(),
            },
        ]);

        Ok(Self { scripts })
    }

    pub fn scripts(&self) -> &[InjectionScript] {
        &self.scripts
    }

    pub fn script_names(&self) -> Vec<&'static str> {
        self.scripts.iter().map(|script| script.name).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebviewLoadState {
    pub url: String,
    pub devtools: bool,
    pub document_start_scripts: Vec<&'static str>,
    pub loaded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebUiSmokeReport {
    pub host_available: bool,
    pub enhanced_api_available: bool,
    pub mod_ui_injected: bool,
    pub document_start_scripts: Vec<&'static str>,
}

pub struct MacosWebviewRuntime<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    url: String,
    devtools: bool,
    injection: InjectionBundle,
    host: Arc<Host<B, P>>,
    loaded: bool,
}

pub type WebviewRuntime<B, P> = MacosWebviewRuntime<B, P>;

impl<B, P> MacosWebviewRuntime<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    pub fn new(
        url: impl Into<String>,
        devtools: bool,
        injection: InjectionBundle,
        host: Arc<Host<B, P>>,
    ) -> Self {
        Self {
            url: url.into(),
            devtools,
            injection,
            host,
            loaded: false,
        }
    }

    pub fn load(&mut self) -> Result<WebviewLoadState, String> {
        validate_load_url(&self.url)?;
        self.loaded = true;
        Ok(self.load_state())
    }

    pub fn bootstrap_headless(mut self) -> Result<WebviewLoadState, String> {
        self.load()
    }

    pub fn load_state(&self) -> WebviewLoadState {
        WebviewLoadState {
            url: self.url.clone(),
            devtools: self.devtools,
            document_start_scripts: self.injection.script_names(),
            loaded: self.loaded,
        }
    }

    pub fn invoke_host_init(&self) -> Result<serde_json::Value, String> {
        self.host.invoke("init", None)
    }

    pub fn dispatch_ipc(&self, kind: &str, payload: Option<Value>) -> Result<Value, String> {
        self.host.dispatch_ipc(kind, payload)
    }

    pub fn shutdown(&self) -> Result<(), String> {
        self.host.shutdown()
    }

    pub fn script_source(&self, name: &str) -> Option<String> {
        self.injection
            .scripts()
            .iter()
            .find(|script| script.name == name)
            .map(|script| script.source.clone())
    }

    pub fn drain_event_dispatch_scripts(&self) -> Result<Vec<String>, String> {
        self.host
            .drain_emitted_events()?
            .into_iter()
            .map(|event| {
                let event_name = serde_json::to_string(&event.event)
                    .map_err(|e| format!("Failed to serialize macOS host event name: {e}"))?;
                let payload = serde_json::to_string(&event.payload)
                    .map_err(|e| format!("Failed to serialize macOS host event payload: {e}"))?;
                Ok(format!(
                    "window.__STREMIO_LIGHTNING_MACOS_DISPATCH__({event_name}, {payload});"
                ))
            })
            .collect()
    }

    pub fn web_ui_smoke_report(&self) -> WebUiSmokeReport {
        let host_adapter = self.script_source(HOST_ADAPTER_NAME).unwrap_or_default();
        WebUiSmokeReport {
            host_available: host_adapter.contains("window.StremioLightningHost"),
            enhanced_api_available: host_adapter.contains("window.StremioEnhancedAPI"),
            mod_ui_injected: self
                .script_source(MOD_UI_NAME)
                .map(|source| !source.trim().is_empty())
                .unwrap_or(false),
            document_start_scripts: self.injection.script_names(),
        }
    }
}

pub fn macos_host_adapter() -> String {
    host_adapter()
}

fn bridge_module_scripts() -> Vec<InjectionScript> {
    vec![
        InjectionScript {
            name: BRIDGE_UTILS_NAME,
            source: include_str!("../../../web/bridge/src/utils.js").to_string(),
        },
        InjectionScript {
            name: BRIDGE_CAST_FALLBACK_NAME,
            source: include_str!("../../../web/bridge/src/cast-fallback.js").to_string(),
        },
        InjectionScript {
            name: BRIDGE_SHELL_TRANSPORT_NAME,
            source: include_str!("../../../web/bridge/src/shell-transport.js").to_string(),
        },
        InjectionScript {
            name: BRIDGE_EXTERNAL_LINKS_NAME,
            source: include_str!("../../../web/bridge/src/external-links.js").to_string(),
        },
        InjectionScript {
            name: BRIDGE_SHELL_DETECTION_NAME,
            source: include_str!("../../../web/bridge/src/shell-detection.js").to_string(),
        },
        InjectionScript {
            name: BRIDGE_BACK_BUTTON_NAME,
            source: include_str!("../../../web/bridge/src/back-button.js").to_string(),
        },
        InjectionScript {
            name: BRIDGE_SHORTCUTS_NAME,
            source: include_str!("../../../web/bridge/src/shortcuts.js").to_string(),
        },
        InjectionScript {
            name: BRIDGE_PIP_NAME,
            source: include_str!("../../../web/bridge/src/pip.js").to_string(),
        },
        InjectionScript {
            name: BRIDGE_DISCORD_RPC_NAME,
            source: include_str!("../../../web/bridge/src/discord-rpc.js").to_string(),
        },
        InjectionScript {
            name: BRIDGE_UPDATE_BANNER_NAME,
            source: include_str!("../../../web/bridge/src/update-banner.js").to_string(),
        },
    ]
}

fn host_adapter() -> String {
    r#"(function () {
  "use strict";
  if (window.StremioLightningHost) return;

  var nextRequestId = 1;
  var nextListenerId = 1;
  var pending = new Map();
  var listeners = new Map();

  window.__STREMIO_LIGHTNING_MACOS_RESOLVE__ = function (id, result, error) {
    var entry = pending.get(id);
    if (!entry) return;
    pending.delete(id);
    if (error) {
      entry.reject(new Error(String(error)));
    } else {
      entry.resolve(result);
    }
  };

  window.__STREMIO_LIGHTNING_MACOS_DISPATCH__ = function (event, payload) {
    listeners.forEach(function (entry) {
      if (entry.event === event) entry.callback({ event: event, payload: payload });
    });
  };

  function post(kind, payload) {
    if (!window.webkit || !window.webkit.messageHandlers || !window.webkit.messageHandlers.ipc) {
      return Promise.reject(new Error("macOS IPC handler is not available"));
    }

    var id = nextRequestId++;
    return new Promise(function (resolve, reject) {
      pending.set(id, { resolve: resolve, reject: reject });
      try {
        window.webkit.messageHandlers.ipc.postMessage({ id: id, kind: kind, payload: payload });
      } catch (error) {
        pending.delete(id);
        reject(error);
      }
    });
  }

  window.StremioLightningHost = {
    platform: "macos",
    invoke: function (command, payload) {
      return post("invoke", {
        command: command,
        payload: payload
      });
    },
    listen: function (event, callback) {
      var id = nextListenerId++;
      listeners.set(id, { event: event, callback: callback });
      post("listen", { id: id, event: event }).catch(function () {});
      return Promise.resolve(function () {
        listeners.delete(id);
        return post("unlisten", { id: id }).catch(function () {});
      });
    },
    window: {
      minimize: function () { return post("window.minimize"); },
      toggleMaximize: function () { return post("window.toggleMaximize"); },
      close: function () { return post("window.close"); },
      isMaximized: function () { return post("window.isMaximized"); },
      isFullscreen: function () { return post("window.isFullscreen"); },
      setFullscreen: function (fullscreen) {
        return post("window.setFullscreen", { fullscreen: fullscreen });
      },
      startDragging: function () { return post("window.startDragging"); }
    },
    webview: {
      setZoom: function (level) { return post("webview.setZoom", { level: level }); }
    }
  };
  window.StremioEnhancedAPI = window.StremioEnhancedAPI || {};
})();"#
        .to_string()
}

fn validate_load_url(url: &str) -> Result<(), String> {
    if url.starts_with("http://") || url.starts_with("https://") || url.starts_with("file://") {
        Ok(())
    } else {
        Err(format!("Unsupported macOS webview URL: {url}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::SHELL_TRANSPORT_EVENT;
    use crate::player::FakePlayerBackend;
    use crate::streaming_server::{FakeProcessSpawner, StreamingServer};
    use serde_json::json;

    #[test]
    fn injection_order_puts_macos_adapter_before_bridge() {
        let bundle = InjectionBundle::load().expect("injection bundle");
        assert_eq!(
            bundle.script_names(),
            vec![
                MACOS_HOST_ADAPTER_NAME,
                BRIDGE_UTILS_NAME,
                BRIDGE_CAST_FALLBACK_NAME,
                BRIDGE_SHELL_TRANSPORT_NAME,
                BRIDGE_EXTERNAL_LINKS_NAME,
                BRIDGE_SHELL_DETECTION_NAME,
                BRIDGE_BACK_BUTTON_NAME,
                BRIDGE_SHORTCUTS_NAME,
                BRIDGE_PIP_NAME,
                BRIDGE_DISCORD_RPC_NAME,
                BRIDGE_UPDATE_BANNER_NAME,
                BRIDGE_NAME,
                MOD_UI_NAME
            ]
        );
        assert!(bundle.scripts()[0]
            .source
            .contains("__STREMIO_LIGHTNING_MACOS_RESOLVE__"));
        assert!(bundle.scripts()[0]
            .source
            .contains("window.StremioLightningHost"));
        assert!(bundle.scripts()[0].source.contains("listen: function"));
    }

    fn test_host() -> Arc<Host<FakePlayerBackend, FakeProcessSpawner>> {
        Arc::new(Host::new(
            FakePlayerBackend::initialized(),
            StreamingServer::new(FakeProcessSpawner::default()),
        ))
    }

    #[test]
    fn headless_load_marks_runtime_loaded() {
        let runtime = MacosWebviewRuntime::new(
            "file:///tmp/smoke.html",
            true,
            InjectionBundle::load().expect("injection bundle"),
            test_host(),
        );
        let state = runtime.bootstrap_headless().expect("headless load");
        assert!(state.loaded);
        assert!(state.devtools);
        assert_eq!(state.url, "file:///tmp/smoke.html");
    }

    #[test]
    fn rejects_unsupported_url_scheme() {
        assert_eq!(
            validate_load_url("stremio://detail/movie").unwrap_err(),
            "Unsupported macOS webview URL: stremio://detail/movie"
        );
    }

    #[test]
    fn dispatches_js_ipc_and_drains_events() {
        let host = test_host();
        let runtime = MacosWebviewRuntime::new(
            "https://web.stremio.com/",
            false,
            InjectionBundle::load().expect("injection bundle"),
            host.clone(),
        );

        let init = runtime
            .dispatch_ipc("invoke", Some(json!({"command": "init"})))
            .unwrap();
        assert_eq!(init["platform"], "macos");

        runtime
            .dispatch_ipc(
                "listen",
                Some(json!({"id": 10, "event": SHELL_TRANSPORT_EVENT})),
            )
            .unwrap();
        host.emit_native_player_property_changed("pause", json!(true))
            .unwrap();

        let scripts = runtime.drain_event_dispatch_scripts().unwrap();
        assert_eq!(scripts.len(), 1);
        assert!(scripts[0].contains("__STREMIO_LIGHTNING_MACOS_DISPATCH__"));
        assert!(scripts[0].contains("mpv-prop-change"));
        assert!(scripts[0].contains("pause"));

        runtime
            .dispatch_ipc("unlisten", Some(json!({"id": 10})))
            .unwrap();
        host.emit_native_player_property_changed("pause", json!(false))
            .unwrap();
        assert!(runtime.drain_event_dispatch_scripts().unwrap().is_empty());
    }

    #[test]
    fn exposes_host_adapter_source_for_native_injection() {
        let runtime = MacosWebviewRuntime::new(
            "file:///tmp/smoke.html",
            false,
            InjectionBundle::load().expect("injection bundle"),
            test_host(),
        );
        let source = runtime
            .script_source(MACOS_HOST_ADAPTER_NAME)
            .expect("host adapter source");
        assert!(source.contains("__STREMIO_LIGHTNING_MACOS_DISPATCH__"));
        assert!(source.contains("window.webkit.messageHandlers.ipc.postMessage"));
    }

    #[test]
    fn web_ui_smoke_report_confirms_host_api_and_mod_ui_injection() {
        let runtime = MacosWebviewRuntime::new(
            "https://web.stremio.com/",
            false,
            InjectionBundle::load().expect("injection bundle"),
            test_host(),
        );
        let report = runtime.web_ui_smoke_report();
        assert!(report.host_available);
        assert!(report.enhanced_api_available);
        assert!(report.mod_ui_injected);
        assert!(report
            .document_start_scripts
            .contains(&MACOS_HOST_ADAPTER_NAME));
        assert!(report.document_start_scripts.contains(&MOD_UI_NAME));
    }
}
