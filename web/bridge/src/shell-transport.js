function createMpvState() {
  return {
    observed: false,
    timePos: 0,
    duration: 0,
    pause: false,
    pausedForCache: false,
    seeking: false,
  };
}

function parseTransportPayload(payload) {
  if (!payload) return null;
  if (typeof payload !== "string") return payload;
  try {
    return JSON.parse(payload);
  } catch (_) {
    return null;
  }
}

function initShellTransport(ctx) {
  var log = window.StremioLightningLogger.bind("bridge.shell-transport");
  var host = ctx.host;
  var shellMessageListeners = [];
  var nativeChromeWebview = null;
  var mpvState = createMpvState();

  try {
    nativeChromeWebview =
      window.chrome && window.chrome.webview ? window.chrome.webview : null;
  } catch (error) {
    log.warn(
      "[StremioLightning] Could not access native chrome.webview:",
      error,
    );
  }

  function updateMpvStateFromTransport(payload) {
    var parsed = parseTransportPayload(payload);
    var args = parsed && parsed.args;
    if (!Array.isArray(args) || args.length < 2) return;

    var eventName = args[0];
    var eventPayload = args[1] || {};

    if (eventName === "mpv-prop-change" && eventPayload.name) {
      if (eventPayload.name === "time-pos") {
        mpvState.timePos = toFiniteNumber(eventPayload.data);
      } else if (eventPayload.name === "duration") {
        mpvState.duration = toFiniteNumber(eventPayload.data);
      } else if (eventPayload.name === "pause") {
        mpvState.pause = !!eventPayload.data;
      } else if (eventPayload.name === "paused-for-cache") {
        mpvState.pausedForCache = !!eventPayload.data;
      } else if (eventPayload.name === "seeking") {
        mpvState.seeking = !!eventPayload.data;
      }
    } else if (eventName === "mpv-event-ended") {
      mpvState.timePos = 0;
      mpvState.duration = 0;
      mpvState.pause = false;
      mpvState.pausedForCache = false;
      mpvState.seeking = false;
    }
  }

  function dispatchPipEvents(payload) {
    var parsed = parseTransportPayload(payload);
    var args = parsed && parsed.args;
    if (!Array.isArray(args) || args.length < 1) return;

    if (args[0] === "showPictureInPicture") {
      document.dispatchEvent(new CustomEvent("sl-pip-enabled"));
    } else if (args[0] === "hidePictureInPicture") {
      document.dispatchEvent(new CustomEvent("sl-pip-disabled"));
    }
  }

  function dispatchShellTransportMessage(payload) {
    var event = { data: payload };

    updateMpvStateFromTransport(payload);
    dispatchPipEvents(payload);

    try {
      if (
        window.qt &&
        window.qt.webChannelTransport &&
        typeof window.qt.webChannelTransport.onmessage === "function"
      ) {
        window.qt.webChannelTransport.onmessage(event);
      }
    } catch (error) {
      log.error(
        "[StremioLightning] qt.webChannelTransport handler failed:",
        error,
      );
    }

    try {
      if (
        nativeChromeWebview &&
        typeof nativeChromeWebview.dispatchEvent === "function"
      ) {
        nativeChromeWebview.dispatchEvent(
          new MessageEvent("message", { data: payload }),
        );
      }
    } catch (error) {
      log.error(
        "[StremioLightning] native chrome.webview dispatch failed:",
        error,
      );
    }

    shellMessageListeners.slice().forEach(function (listener) {
      try {
        listener(event);
      } catch (error) {
        log.error(
          "[StremioLightning] chrome.webview message listener failed:",
          error,
        );
      }
    });
  }

  function sendShellTransportMessage(payload) {
    var parsed = parseTransportPayload(payload);
    var args = parsed && parsed.args;
    var command = Array.isArray(args) ? args[1] : null;
    if (
      args &&
      args[0] === "mpv-command" &&
      Array.isArray(command) &&
      command[0] === "loadfile"
    ) {
      log.info(
        "[StremioLightning] Forwarding MPV loadfile command (arguments redacted)",
      );
    }
    var serialized =
      typeof payload === "string" ? payload : JSON.stringify(payload);
    return host.invoke("shell_transport_send", { message: serialized }).catch(
      function (error) {
        log.error(
          "[StremioLightning] shell transport send failed:",
          error,
          "message length:",
          serialized.length,
        );
      },
    );
  }

  function notifyShellBridgeReady() {
    host.invoke("shell_bridge_ready").catch(function (error) {
      log.error("[StremioLightning] shell bridge ready failed:", error);
    });
  }

  function observeMpvProperties() {
    if (mpvState.observed) return;
    mpvState.observed = true;

    [
      "time-pos",
      "duration",
      "pause",
      "paused-for-cache",
      "seeking",
      "eof-reached",
      "cache-buffering-state",
      "demuxer-cache-time",
    ].forEach(
      function (name, index) {
        sendShellTransportMessage({
          id: 9000 + index,
          type: 6,
          args: ["mpv-observe-prop", name],
        });
      },
    );
  }

  if (window.self === window.top) {
    host.listen("shell-transport-message", function (event) {
      dispatchShellTransportMessage(event.payload);
    }).then(function () {
      onDomReady(notifyShellBridgeReady);
    });

    window.qt = window.qt || {};
    window.qt.webChannelTransport = window.qt.webChannelTransport || {};
    window.qt.webChannelTransport.send = sendShellTransportMessage;

    var chromeWebviewShim = {
      postMessage: sendShellTransportMessage,
      addEventListener: function (name, listener) {
        if (name !== "message") {
          throw new Error("Unsupported event: " + name);
        }
        shellMessageListeners.push(listener);
      },
      removeEventListener: function (name, listener) {
        if (name !== "message") {
          throw new Error("Unsupported event: " + name);
        }
        shellMessageListeners = shellMessageListeners.filter(function (item) {
          return item !== listener;
        });
      },
    };
    try {
      window.chrome = window.chrome || {};
      window.chrome.webview = chromeWebviewShim;
      if (window.chrome.webview !== chromeWebviewShim) {
        Object.defineProperty(window.chrome, "webview", {
          configurable: true,
          value: chromeWebviewShim,
        });
      }
    } catch (error) {
      log.error(
        "[StremioLightning] Could not install chrome.webview transport shim:",
        error,
      );
    }
  }

  return {
    mpvState: mpvState,
    observeMpvProperties: observeMpvProperties,
  };
}
