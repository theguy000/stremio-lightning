use crate::host::WindowsHost;
use std::sync::Arc;

pub const WINDOWS_HOST_ADAPTER_NAME: &str = "windows-host-adapter";
pub const NATIVE_FLAGS_NAME: &str = "native-flags";
pub const BRIDGE_NAME: &str = "bridge.js";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectionScript {
    pub name: &'static str,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InjectionBundle {
    scripts: Vec<InjectionScript>,
}

impl InjectionBundle {
    pub fn load() -> Self {
        Self {
            scripts: vec![
                InjectionScript {
                    name: WINDOWS_HOST_ADAPTER_NAME,
                    source: windows_host_adapter(),
                },
                InjectionScript {
                    name: NATIVE_FLAGS_NAME,
                    source: "window.__STREMIO_LIGHTNING_ENABLE_NATIVE_PLAYER__ = true;".to_string(),
                },
                InjectionScript {
                    name: BRIDGE_NAME,
                    source: include_str!("../../../web/bridge/bridge.js").to_string(),
                },
            ],
        }
    }

    pub fn scripts(&self) -> &[InjectionScript] {
        &self.scripts
    }
}

pub struct WindowsWebView2Shell {
    url: String,
    injection: InjectionBundle,
    #[allow(dead_code)]
    host: Arc<WindowsHost>,
}

impl WindowsWebView2Shell {
    pub fn new(url: impl Into<String>) -> Result<Self, String> {
        let url = url.into();
        if !(url.starts_with("https://") || url.starts_with("http://127.0.0.1:")) {
            return Err(format!("Unsupported WebView2 load URL: {url}"));
        }

        Ok(Self {
            url,
            injection: InjectionBundle::load(),
            host: Arc::new(WindowsHost::default()),
        })
    }

    pub fn document_start_script_names(&self) -> Vec<&'static str> {
        self.injection
            .scripts()
            .iter()
            .map(|script| script.name)
            .collect()
    }

    pub fn run(&self) -> Result<(), String> {
        platform::run_webview2_shell(&self.url, &self.injection)
    }
}

#[cfg(windows)]
mod platform {
    use super::InjectionBundle;

    pub fn run_webview2_shell(_url: &str, _injection: &InjectionBundle) -> Result<(), String> {
        Err("WebView2 window creation is not wired yet".to_string())
    }
}

#[cfg(not(windows))]
mod platform {
    use super::InjectionBundle;

    pub fn run_webview2_shell(_url: &str, _injection: &InjectionBundle) -> Result<(), String> {
        Err("WebView2 shell can only run on Windows".to_string())
    }
}

fn windows_host_adapter() -> String {
    r#"(function () {
  "use strict";

  if (window.StremioLightningHost) return;

  var nextListenerId = 1;
  var listeners = {};

  function post(kind, payload) {
    var message = JSON.stringify({ kind: kind, payload: payload || null });
    if (!window.chrome || !window.chrome.webview) {
      return Promise.reject(new Error("WebView2 host bridge is not available"));
    }
    window.chrome.webview.postMessage(message);
    return Promise.resolve(null);
  }

  window.StremioLightningHost = {
    invoke: function (command, payload) {
      return post("invoke", { command: command, payload: payload });
    },
    listen: function (event, callback) {
      var id = nextListenerId++;
      listeners[id] = { event: event, callback: callback };
      return Promise.resolve(function () { delete listeners[id]; });
    },
    window: {
      minimize: function () { return post("window.minimize"); },
      toggleMaximize: function () { return post("window.toggleMaximize"); },
      close: function () { return post("window.close"); },
      isMaximized: function () { return Promise.resolve(false); },
      isFullscreen: function () { return Promise.resolve(false); },
      setFullscreen: function (fullscreen) { return post("window.setFullscreen", { fullscreen: fullscreen }); },
      startDragging: function () { return post("window.startDragging"); }
    },
    webview: {
      setZoom: function (level) { return post("webview.setZoom", { level: level }); }
    }
  };
})();"#
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn injects_windows_adapter_before_shared_bridge() {
        let shell = WindowsWebView2Shell::new("https://web.stremio.com/").unwrap();

        assert_eq!(
            shell.document_start_script_names(),
            vec![WINDOWS_HOST_ADAPTER_NAME, NATIVE_FLAGS_NAME, BRIDGE_NAME]
        );
    }

    #[test]
    fn moved_shared_bridge_is_loaded_from_web_folder() {
        let bundle = InjectionBundle::load();
        let bridge = bundle
            .scripts()
            .iter()
            .find(|script| script.name == BRIDGE_NAME)
            .unwrap();

        assert!(bridge
            .source
            .contains("Stremio Lightning - Frontend Bridge"));
    }
}
