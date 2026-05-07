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
}
