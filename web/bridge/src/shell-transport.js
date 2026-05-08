function createMpvState() {
  return {
    observed: false,
    timePos: 0,
    duration: 0,
    pause: false,
    pausedForCache: false,
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
  var host = ctx.host;
  var shellMessageListeners = [];
  var nativeChromeWebview = null;
  var mpvState = createMpvState();

  try {
    nativeChromeWebview =
      window.chrome && window.chrome.webview ? window.chrome.webview : null;
  } catch (error) {
    console.warn(
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
      }
    } else if (eventName === "mpv-event-ended") {
      mpvState.timePos = 0;
      mpvState.duration = 0;
      mpvState.pause = false;
      mpvState.pausedForCache = false;
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
      console.error(
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
      console.error(
        "[StremioLightning] native chrome.webview dispatch failed:",
        error,
      );
    }

    shellMessageListeners.slice().forEach(function (listener) {
      try {
        listener(event);
      } catch (error) {
        console.error(
          "[StremioLightning] chrome.webview message listener failed:",
          error,
        );
      }
    });
  }

  function sendShellTransportMessage(payload) {
    var serialized =
      typeof payload === "string" ? payload : JSON.stringify(payload);
    return host.invoke("shell_transport_send", { message: serialized }).catch(
      function (error) {
        console.error(
          "[StremioLightning] shell transport send failed:",
          error,
          serialized,
        );
      },
    );
  }

  function notifyShellBridgeReady() {
    host.invoke("shell_bridge_ready").catch(function (error) {
      console.error("[StremioLightning] shell bridge ready failed:", error);
    });
  }

  function observeMpvProperties() {
    if (mpvState.observed) return;
    mpvState.observed = true;

    ["time-pos", "duration", "pause", "paused-for-cache"].forEach(
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

    if (!nativeChromeWebview) {
      window.chrome = window.chrome || {};
      window.chrome.webview = {
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
      nativeChromeWebview = window.chrome.webview;
    }
  }

  return {
    mpvState: mpvState,
    observeMpvProperties: observeMpvProperties,
  };
}
