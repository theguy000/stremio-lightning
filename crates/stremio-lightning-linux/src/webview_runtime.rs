use crate::host::LinuxHost;
use crate::player::PlayerBackend;
use crate::streaming_server::ProcessSpawner;
use serde_json::Value;
use std::sync::Arc;

pub const LINUX_HOST_ADAPTER_NAME: &str = "linux-host-adapter";
pub const NATIVE_FLAGS_NAME: &str = "native-flags";
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
        Ok(Self {
            scripts: vec![
                InjectionScript {
                    name: LINUX_HOST_ADAPTER_NAME,
                    source: linux_host_adapter(),
                },
                InjectionScript {
                    name: NATIVE_FLAGS_NAME,
                    source: native_flags(),
                },
                InjectionScript {
                    name: BRIDGE_NAME,
                    source: include_str!("../../../src-tauri/scripts/bridge.js").to_string(),
                },
                InjectionScript {
                    name: MOD_UI_NAME,
                    source: include_str!("../../../src/dist/mod-ui-svelte.iife.js").to_string(),
                },
            ],
        })
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

pub struct LinuxWebviewRuntime<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    url: String,
    devtools: bool,
    injection: InjectionBundle,
    host: Arc<LinuxHost<B, P>>,
    loaded: bool,
}

impl<B, P> LinuxWebviewRuntime<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    pub fn new(
        url: impl Into<String>,
        devtools: bool,
        injection: InjectionBundle,
        host: Arc<LinuxHost<B, P>>,
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

    pub fn load_state(&self) -> WebviewLoadState {
        WebviewLoadState {
            url: self.url.clone(),
            devtools: self.devtools,
            document_start_scripts: self.injection.script_names(),
            loaded: self.loaded,
        }
    }

    pub fn dispatch_ipc(&self, kind: &str, payload: Option<Value>) -> Result<Value, String> {
        self.host.dispatch_linux_ipc(kind, payload)
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
                    .map_err(|e| format!("Failed to serialize Linux host event name: {e}"))?;
                let payload = serde_json::to_string(&event.payload)
                    .map_err(|e| format!("Failed to serialize Linux host event payload: {e}"))?;
                Ok(format!(
                    "window.__STREMIO_LIGHTNING_LINUX_DISPATCH__({event_name}, {payload});"
                ))
            })
            .collect()
    }

    pub fn emit_native_player_property_changed(
        &self,
        name: impl Into<String>,
        data: Value,
    ) -> Result<(), String> {
        self.host.emit_native_player_property_changed(name, data)
    }

    pub fn emit_native_player_ended(&self, reason: impl Into<String>) -> Result<(), String> {
        self.host.emit_native_player_ended(reason)
    }
}

pub fn linux_host_adapter() -> String {
    r#"(function () {
  "use strict";
  if (window.StremioLightningHost) return;

  var nextListenerId = 1;
  var listeners = new Map();

  function post(kind, payload) {
    if (!window.__STREMIO_LIGHTNING_LINUX_IPC__) {
      return Promise.reject(new Error("Linux host IPC is not available"));
    }
    return window.__STREMIO_LIGHTNING_LINUX_IPC__(kind, payload);
  }

  window.__STREMIO_LIGHTNING_LINUX_DISPATCH__ = function (event, payload) {
    listeners.forEach(function (entry) {
      if (entry.event === event) entry.callback({ event: event, payload: payload });
    });
  };

  window.StremioLightningHost = {
    invoke: function (command, payload) {
      return post("invoke", { command: command, payload: payload });
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
})();"#
        .to_string()
}

pub fn native_flags() -> String {
    "window.__STREMIO_LIGHTNING_ENABLE_NATIVE_PLAYER__ = true;".to_string()
}

fn validate_load_url(url: &str) -> Result<(), String> {
    let lower = url.to_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("file://")
    {
        Ok(())
    } else {
        Err("Linux webview URL must use http, https, or file".to_string())
    }
}

#[derive(Debug, Default)]
pub struct CefOsrSurface {
    pub width: u32,
    pub height: u32,
    pub texture_dirty: bool,
}

impl CefOsrSurface {
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
        self.texture_dirty = true;
    }

    pub fn mark_painted(&mut self) {
        self.texture_dirty = true;
    }

    pub fn take_dirty(&mut self) -> bool {
        let dirty = self.texture_dirty;
        self.texture_dirty = false;
        dirty
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::SHELL_TRANSPORT_EVENT;
    use crate::player::FakePlayerBackend;
    use crate::streaming_server::{FakeProcessSpawner, StreamingServer};
    use serde_json::json;
    use std::path::PathBuf;

    #[test]
    fn injection_order_puts_linux_adapter_before_bridge() {
        let bundle = InjectionBundle::load().unwrap();
        assert_eq!(
            bundle.script_names(),
            vec![
                LINUX_HOST_ADAPTER_NAME,
                NATIVE_FLAGS_NAME,
                BRIDGE_NAME,
                MOD_UI_NAME
            ]
        );
        assert!(bundle.scripts()[0]
            .source
            .contains("window.StremioLightningHost"));
    }

    #[test]
    fn native_flags_enable_linux_native_player_only() {
        let flags = native_flags();
        assert!(flags.contains("__STREMIO_LIGHTNING_ENABLE_NATIVE_PLAYER__ = true"));
        assert!(!flags.contains("__STREMIO_LIGHTNING_ENABLE_WEBKITGTK_WORKAROUNDS__"));
    }

    #[test]
    fn webview_runtime_loads_with_document_start_injection() {
        let host = Arc::new(LinuxHost::with_app_data_dir(
            FakePlayerBackend::initialized(),
            StreamingServer::with_project_root(
                FakeProcessSpawner::default(),
                PathBuf::from("/repo"),
            ),
            std::env::temp_dir(),
        ));
        let mut runtime = LinuxWebviewRuntime::new(
            "file:///tmp/stremio-lightning-smoke.html",
            true,
            InjectionBundle::load().unwrap(),
            host,
        );

        let state = runtime.load().unwrap();
        assert!(state.loaded);
        assert_eq!(state.url, "file:///tmp/stremio-lightning-smoke.html");
        assert_eq!(
            state.document_start_scripts,
            vec![
                LINUX_HOST_ADAPTER_NAME,
                NATIVE_FLAGS_NAME,
                BRIDGE_NAME,
                MOD_UI_NAME
            ]
        );
        assert!(state.devtools);
    }

    #[test]
    fn webview_runtime_dispatches_js_ipc_and_drains_events() {
        let host = Arc::new(LinuxHost::with_app_data_dir(
            FakePlayerBackend::initialized(),
            StreamingServer::with_project_root(
                FakeProcessSpawner::default(),
                PathBuf::from("/repo"),
            ),
            std::env::temp_dir(),
        ));
        let runtime = LinuxWebviewRuntime::new(
            "https://web.stremio.com/",
            false,
            InjectionBundle::load().unwrap(),
            host.clone(),
        );

        runtime
            .dispatch_ipc(
                "listen",
                Some(json!({"id": 10, "event": SHELL_TRANSPORT_EVENT})),
            )
            .unwrap();
        runtime
            .dispatch_ipc("invoke", Some(json!({"command": "shell_bridge_ready"})))
            .unwrap();
        host.emit_native_player_property_changed("pause", json!(true))
            .unwrap();
        runtime
            .dispatch_ipc(
                "invoke",
                Some(json!({
                    "command": "shell_transport_send",
                    "payload": {"message": r#"{"id":1,"type":6,"args":["app-ready"]}"#}
                })),
            )
            .unwrap();

        let scripts = runtime.drain_event_dispatch_scripts().unwrap();
        assert_eq!(scripts.len(), 1);
        assert!(scripts[0].contains("__STREMIO_LIGHTNING_LINUX_DISPATCH__"));
        assert!(scripts[0].contains("mpv-prop-change"));

        runtime
            .dispatch_ipc("unlisten", Some(json!({"id": 10})))
            .unwrap();
        host.emit_native_player_property_changed("pause", json!(false))
            .unwrap();
        assert!(runtime.drain_event_dispatch_scripts().unwrap().is_empty());
    }
}
