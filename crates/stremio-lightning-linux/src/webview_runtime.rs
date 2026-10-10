use crate::host::Host;
use crate::player::PlayerBackend;
use crate::streaming_server::ProcessSpawner;
use serde_json::Value;
use std::ops::Deref;
use std::sync::Arc;
pub use stremio_lightning_core::bridge_assets::MOD_UI_NAME;
use stremio_lightning_core::pip::PipWindowController;
use stremio_lightning_core::player_api::PlayerEnded;
pub use stremio_lightning_core::webview_runtime::WebviewLoadState;
use stremio_lightning_core::webview_runtime::{
    event_dispatch_scripts, InjectionBundle as CoreInjectionBundle, WebviewSession,
};

pub const LINUX_HOST_ADAPTER_NAME: &str = "linux-host-adapter";
pub const HOST_ADAPTER_NAME: &str = LINUX_HOST_ADAPTER_NAME;
const DISPATCH_GLOBAL: &str = "__STREMIO_LIGHTNING_LINUX_DISPATCH__";

/// The core injection bundle, built from this shell's host adapter.
#[derive(Debug, Clone)]
pub struct InjectionBundle(CoreInjectionBundle);

impl InjectionBundle {
    pub fn load() -> Result<Self, String> {
        CoreInjectionBundle::load(HOST_ADAPTER_NAME, host_adapter()).map(Self)
    }
}

impl Deref for InjectionBundle {
    type Target = CoreInjectionBundle;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

pub struct LinuxWebviewRuntime<B, P>
where
    B: PlayerBackend,
    P: ProcessSpawner,
{
    session: WebviewSession,
    host: Arc<Host<B, P>>,
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
        host: Arc<Host<B, P>>,
    ) -> Self {
        Self {
            session: WebviewSession::new(url, devtools, injection.0),
            host,
        }
    }

    pub fn load(&mut self) -> Result<WebviewLoadState, String> {
        self.session.load()
    }

    pub fn load_state(&self) -> WebviewLoadState {
        self.session.load_state()
    }

    pub fn dispatch_ipc(&self, kind: &str, payload: Option<Value>) -> Result<Value, String> {
        self.host.dispatch_ipc(kind, payload)
    }

    pub fn emit_stremio_deep_link(&self, url: &str) -> Result<(), String> {
        self.host.emit_transport_event(
            stremio_lightning_core::host_api::stremio_deep_link_transport_args(url),
        )
    }

    pub fn emit_launch_intent(
        &self,
        intent: &stremio_lightning_core::launch_intent::LaunchIntent,
    ) -> Result<(), String> {
        self.host.emit_launch_intent(intent)
    }

    pub fn emit_media_key(&self, action: &str) -> Result<(), String> {
        self.host.emit_media_key(action)
    }

    pub fn shutdown(&self) -> Result<(), String> {
        self.host.shutdown()
    }

    pub fn script_source(&self, name: &str) -> Option<String> {
        self.session.script_source(name).map(str::to_string)
    }

    pub fn drain_event_dispatch_scripts(&self) -> Result<Vec<String>, String> {
        event_dispatch_scripts(DISPATCH_GLOBAL, self.host.drain_emitted_events()?)
    }

    pub fn emit_native_player_property_changed(
        &self,
        name: impl Into<String>,
        data: Value,
    ) -> Result<(), String> {
        self.host.emit_native_player_property_changed(name, data)
    }

    pub fn emit_window_maximized_changed(&self, maximized: bool) -> Result<(), String> {
        self.host.emit_window_maximized_changed(maximized)
    }

    pub fn emit_window_visible_changed(&self, visible: bool) -> Result<(), String> {
        self.host.emit_window_visible_changed(visible)
    }

    pub fn emit_native_player_ended(&self, ended: PlayerEnded) -> Result<(), String> {
        self.host.emit_native_player_ended(ended)
    }

    pub fn toggle_picture_in_picture(
        &self,
        controller: &mut impl PipWindowController,
    ) -> Result<bool, String> {
        self.host.toggle_picture_in_picture(controller)
    }

    pub fn exit_picture_in_picture(
        &self,
        controller: &mut impl PipWindowController,
    ) -> Result<bool, String> {
        self.host.exit_picture_in_picture(controller)
    }

    pub fn exit_picture_in_picture_for_player_end(
        &self,
        controller: &mut impl PipWindowController,
    ) -> Result<bool, String> {
        self.exit_picture_in_picture(controller)
    }
}

pub fn linux_host_adapter() -> String {
    host_adapter()
}

pub fn host_adapter() -> String {
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

pub type WebviewRuntime<B, P> = LinuxWebviewRuntime<B, P>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::SHELL_TRANSPORT_EVENT;
    use crate::player::MpvPlayerBackend;
    use crate::streaming_server::{RealProcessSpawner, StreamingServer};
    use serde_json::json;
    use std::path::PathBuf;
    use stremio_lightning_core::bridge_assets::bridge_scripts;

    #[test]
    fn injection_order_puts_linux_adapter_before_bridge() {
        let bundle = InjectionBundle::load().unwrap();
        let mut expected = vec![LINUX_HOST_ADAPTER_NAME];
        expected.extend(bridge_scripts().iter().map(|script| script.name));
        expected.push(MOD_UI_NAME);

        assert_eq!(bundle.script_names(), expected);
        assert!(bundle.scripts()[0]
            .source
            .contains("window.StremioLightningHost"));
    }

    #[test]
    fn webview_runtime_loads_with_document_start_injection() {
        let host = Arc::new(Host::with_app_data_dir(
            MpvPlayerBackend::default(),
            StreamingServer::with_project_root(RealProcessSpawner, PathBuf::from("/repo")),
            std::env::temp_dir(),
        ));
        let mut runtime = WebviewRuntime::new(
            "file:///tmp/stremio-lightning-smoke.html",
            true,
            InjectionBundle::load().unwrap(),
            host,
        );

        let state = runtime.load().unwrap();
        assert!(state.loaded);
        assert_eq!(state.url, "file:///tmp/stremio-lightning-smoke.html");
        let mut expected = vec![LINUX_HOST_ADAPTER_NAME];
        expected.extend(bridge_scripts().iter().map(|script| script.name));
        expected.push(MOD_UI_NAME);
        assert_eq!(state.document_start_scripts, expected);
        assert!(state.devtools);
    }

    #[test]
    fn webview_runtime_dispatches_js_ipc_and_drains_events() {
        let host = Arc::new(Host::with_app_data_dir(
            MpvPlayerBackend::default(),
            StreamingServer::with_project_root(RealProcessSpawner, PathBuf::from("/repo")),
            std::env::temp_dir(),
        ));
        let runtime = WebviewRuntime::new(
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
