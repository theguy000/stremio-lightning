#[must_use]
pub fn windows_host_adapter() -> String {
    host_adapter()
}

#[must_use]
pub fn host_adapter() -> String {
    r#"(function () {
  "use strict";

  if (window.StremioLightningHost) return;

  var nativeWebview = window.chrome && window.chrome.webview;
  var nativePostMessage = nativeWebview && typeof nativeWebview.postMessage === "function"
    ? nativeWebview.postMessage.bind(nativeWebview)
    : null;
  var nextRequestId = 1;
  var nextListenerId = 1;
  var pending = {};
  var listeners = {};
  function logError() {
    var logger = window.StremioLightningLogger;
    if (logger) {
      var args = ["bridge.host-adapter.windows"].concat(Array.prototype.slice.call(arguments));
      logger.error.apply(logger, args);
    } else {
      console.error.apply(console, arguments);
    }
  }

  function post(kind, payload) {
    if (!nativePostMessage) {
      return Promise.reject(new Error("WebView2 host bridge is not available"));
    }
    return new Promise(function (resolve, reject) {
      var id = nextRequestId++;
      pending[id] = { resolve: resolve, reject: reject };
      nativePostMessage({
        id: id,
        kind: kind,
        payload: payload || null
      });
    });
  }

  function resolveResponse(message) {
    var callbacks = pending[message.id];
    if (!callbacks) return;
    delete pending[message.id];
    if (message.ok) {
      callbacks.resolve(message.value);
    } else {
      var val = message.value;
      var errorMessage = val && val.message ? val.message : String(val);
      callbacks.reject(new Error(errorMessage));
    }
  }

  function dispatchEventMessage(message) {
    Object.keys(listeners).forEach(function (id) {
      var listener = listeners[id];
      if (!listener || listener.event !== message.event) return;
      try {
        listener.callback({ event: message.event, payload: message.payload });
      } catch (error) {
        logError("[StremioLightning] Windows listener failed:", error);
      }
    });
  }

  window.chrome.webview.addEventListener("message", function (event) {
    var message = typeof event.data === "string" ? JSON.parse(event.data) : event.data;
    if (!message || !message.kind) return;
    if (message.kind === "response") resolveResponse(message);
    else if (message.kind === "event") dispatchEventMessage(message);
  });

  window.StremioLightningHost = {
    invoke: function (command, payload) {
      return post("invoke", { command: command, payload: payload });
    },
    listen: function (event, callback) {
      var id = nextListenerId++;
      listeners[id] = { event: event, callback: callback };
      return post("listen", { id: id, event: event }).then(function () {
        return function () {
          delete listeners[id];
          return post("unlisten", { id: id });
        };
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
