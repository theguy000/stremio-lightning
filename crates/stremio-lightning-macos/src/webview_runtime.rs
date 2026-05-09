use crate::host::Host;
use crate::player::PlayerBackend;
use crate::streaming_server::ProcessSpawner;
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
  var pending = new Map();

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

  window.__STREMIO_LIGHTNING_MACOS_DISPATCH__ = function (kind, payload) {
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
  };

  window.StremioLightningHost = {
    platform: "macos",
    invoke: function (command, payload) {
      return window.__STREMIO_LIGHTNING_MACOS_DISPATCH__("invoke", {
        command: command,
        payload: payload
      });
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
    use crate::player::MpvPlayerBackend;
    use crate::streaming_server::{RealProcessSpawner, StreamingServer};

    #[test]
    fn loads_expected_injection_order() {
        let bundle = InjectionBundle::load().expect("injection bundle");
        let names = bundle.script_names();
        assert_eq!(names.first(), Some(&MACOS_HOST_ADAPTER_NAME));
        assert_eq!(names.last(), Some(&MOD_UI_NAME));
        assert!(names.contains(&BRIDGE_NAME));
        assert!(bundle.scripts()[0]
            .source
            .contains("__STREMIO_LIGHTNING_MACOS_RESOLVE__"));
    }

    #[test]
    fn headless_load_marks_runtime_loaded() {
        let host = Arc::new(Host::new(
            MpvPlayerBackend::default(),
            StreamingServer::new(RealProcessSpawner::default()),
        ));
        let runtime = MacosWebviewRuntime::new(
            "file:///tmp/smoke.html",
            true,
            InjectionBundle::load().expect("injection bundle"),
            host,
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
}
