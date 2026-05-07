// Stremio Lightning - Frontend Bridge & Keyboard Shortcuts
// Injected via Tauri initialization_script - runs on every page load
(function () {
  "use strict";

  var host = window.StremioLightningHost || null;

  if (!host && window.__TAURI__) {
    var tauriWindow = window.__TAURI__.window.getCurrentWindow();
    var tauriWebview = window.__TAURI__.webview.getCurrentWebview();

    host = {
      invoke: function (command, payload) {
        if (arguments.length > 1) {
          return window.__TAURI__.core.invoke(command, payload);
        }
        return window.__TAURI__.core.invoke(command);
      },
      listen: function (event, callback) {
        return window.__TAURI__.event.listen(event, callback);
      },
      window: {
        minimize: function () {
          return tauriWindow.minimize();
        },
        toggleMaximize: function () {
          return tauriWindow.toggleMaximize();
        },
        close: function () {
          return tauriWindow.close();
        },
        isMaximized: function () {
          return tauriWindow.isMaximized();
        },
        isFullscreen: function () {
          return tauriWindow.isFullscreen();
        },
        setFullscreen: function (fullscreen) {
          return tauriWindow.setFullscreen(fullscreen);
        },
        startDragging: function () {
          return tauriWindow.startDragging();
        },
      },
      webview: {
        setZoom: function (level) {
          return tauriWebview.setZoom(level);
        },
      },
    };
    window.StremioLightningHost = host;
  }

  if (!host) {
    console.error(
      "[StremioLightning] host adapter not available - bridge not loaded",
    );
    return;
  }

  var appWindow = host.window;
  var webview = host.webview;

  // ============================================
  // Chromecast API Availability Fallback
  // ============================================
  // Stremio always loads Google's Cast sender script and its Chromecast transport
  // waits for __onGCastApiAvailable. On WebKitGTK/Linux the sender API is not
  // available, so the callback may never fire and Stremio logs an initialization
  // error. Report Chromecast as unavailable up front; if the real sender script
  // later calls the callback successfully, that value can still be updated. This
  // uses a timer so Stremio's transport can finish assigning n/r listeners before
  // the callback fires.
  var castApiAvailabilityCallback = window.__onGCastApiAvailable;
  var castApiUnavailableTimer = null;
  Object.defineProperty(window, "__onGCastApiAvailable", {
    configurable: true,
    enumerable: true,
    get: function () {
      return castApiAvailabilityCallback;
    },
    set: function (callback) {
      castApiAvailabilityCallback = function (available) {
        if (castApiUnavailableTimer !== null) {
          clearTimeout(castApiUnavailableTimer);
          castApiUnavailableTimer = null;
        }
        return callback.apply(this, arguments);
      };

      castApiUnavailableTimer = setTimeout(function () {
        if (castApiAvailabilityCallback) {
          castApiAvailabilityCallback(false);
        }
      }, 0);
    },
  });
  // ============================================
  // IPC Shell Transport Compatibility
  // ============================================
  var nativePlayerEnabled =
    window.__STREMIO_LIGHTNING_ENABLE_NATIVE_PLAYER__ === true;
  if (nativePlayerEnabled) {
    console.info(
      "[StremioLightning] Native player mode enabled (libmpv transport)",
    );
  }
  // Keep the desktop shell transport enabled on every platform. Stremio uses
  // qt.webChannelTransport / chrome.webview to detect that it is running in a
  // desktop shell and to wire up streaming-server integration. Native MPV may
  // still be unavailable on Linux; MPV commands are ignored on the Rust side
  // when the native player backend is disabled.
  var shellTransportEnabled = true;
  var shellMessageListeners = [];
  var nativeChromeWebview = null;
  var discordMpvState = {
    observed: false,
    timePos: 0,
    duration: 0,
    pause: false,
    pausedForCache: false,
    lastUpdatedAt: 0,
  };

  function updateDiscordMpvStateFromTransport(payload) {
    var parsed;
    var args;
    var eventName;
    var eventPayload;

    if (!nativePlayerEnabled || !payload) return;

    try {
      parsed = typeof payload === "string" ? JSON.parse(payload) : payload;
    } catch (error) {
      return;
    }

    args = parsed && parsed.args;
    if (!Array.isArray(args) || args.length < 2) return;

    eventName = args[0];
    eventPayload = args[1] || {};

    if (eventName === "mpv-prop-change" && eventPayload.name) {
      if (eventPayload.name === "time-pos") {
        discordMpvState.timePos = toFiniteNumber(eventPayload.data);
      } else if (eventPayload.name === "duration") {
        discordMpvState.duration = toFiniteNumber(eventPayload.data);
      } else if (eventPayload.name === "pause") {
        discordMpvState.pause = !!eventPayload.data;
      } else if (eventPayload.name === "paused-for-cache") {
        discordMpvState.pausedForCache = !!eventPayload.data;
      }
      discordMpvState.lastUpdatedAt = Date.now();
    } else if (eventName === "mpv-event-ended") {
      discordMpvState.timePos = 0;
      discordMpvState.duration = 0;
      discordMpvState.pause = false;
      discordMpvState.pausedForCache = false;
      discordMpvState.lastUpdatedAt = Date.now();
    }
  }

  function observeDiscordMpvProperties() {
    if (!nativePlayerEnabled || discordMpvState.observed) return;

    discordMpvState.observed = true;
    ["time-pos", "duration", "pause", "paused-for-cache"].forEach(
      function (name, index) {
        sendShellTransportMessage({
          id: 9000 + index,
          type: 6,
          args: ["mpv-observe-prop", name],
        }).catch(function (error) {
          console.error(
            "[DiscordRPC] Failed to observe MPV property:",
            name,
            error,
          );
        });
      },
    );
  }

  try {
    nativeChromeWebview =
      window.chrome && window.chrome.webview ? window.chrome.webview : null;
  } catch (error) {
    console.warn(
      "[StremioLightning] Could not access native chrome.webview:",
      error,
    );
  }

  function dispatchShellTransportMessage(payload) {
    var event = { data: payload };

    updateDiscordMpvStateFromTransport(payload);

    // Handle Picture-in-Picture events from the Rust backend
    try {
      var parsed = typeof payload === "string" ? JSON.parse(payload) : payload;
      if (
        parsed &&
        parsed.args &&
        Array.isArray(parsed.args) &&
        parsed.args.length >= 1
      ) {
        var eventName = parsed.args[0];
        if (eventName === "showPictureInPicture") {
          document.dispatchEvent(new CustomEvent("sl-pip-enabled"));
        } else if (eventName === "hidePictureInPicture") {
          document.dispatchEvent(new CustomEvent("sl-pip-disabled"));
        }
      }
    } catch (error) {
      // Ignore parse errors for non-PiP payloads
    }

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

  if (window.self === window.top && shellTransportEnabled) {
    host.listen("shell-transport-message", function (event) {
      dispatchShellTransportMessage(event.payload);
    }).then(function () {
      if (document.readyState === "loading") {
        document.addEventListener("DOMContentLoaded", notifyShellBridgeReady, {
          once: true,
        });
      } else {
        notifyShellBridgeReady();
      }
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
          shellMessageListeners = shellMessageListeners.filter(
            function (item) {
              return item !== listener;
            },
          );
        },
      };
      nativeChromeWebview = window.chrome.webview;
    }
  }


  // ============================================
  // Frontend Bridge: window.StremioEnhancedAPI
  // ============================================
  // NOTE: The full API object, plugin auto-loading, and theme auto-loading
  // are now handled by the Svelte overlay (mod-ui-svelte.iife.js).
  // See: src/lib/plugin-api.ts, src/App.svelte
  //
  // We create a minimal stub here so the Discord RPC tracker (below)
  // can attach hooks before the Svelte bundle runs and overwrites this
  // with the complete API.
  window.StremioEnhancedAPI = window.StremioEnhancedAPI || {};

  // ============================================
  // External URL Handling (OAuth, popups, etc.)
  // ============================================
  // Intercept window.open() calls and open them in the system browser
  // instead of creating popup windows inside the webview.
  // This mirrors Electron's setWindowOpenHandler -> shell.openExternal.
  window.open = function (url) {
    if (url) {
      host.invoke("open_external_url", { url: String(url) }).catch(function (e) {
        console.error(
          "[StremioLightning] Failed to open external URL:",
          url,
          e,
        );
      });
    }
    return null;
  };

  // ============================================
  // Shell Detection (StremioShell user agent)
  // ============================================
  if (nativePlayerEnabled) {
    try {
      var originalUA = navigator.userAgent;
      Object.defineProperty(Navigator.prototype, "userAgent", {
        get: function () {
          return originalUA + " StremioShell/4.4";
        },
        configurable: true,
      });
    } catch (e) {
      console.warn("[StremioLightning] Could not override userAgent:", e);
    }
  }

  // ============================================
  // Back Button
  // ============================================
  // The web UI's native back button requires a Qt WebChannel transport
  // that only the official Qt-based shell provides. We inject our own.
  // ============================================
  // Back Button (login/intro page only)
  // ============================================
  function isIntroPage() {
    return window.location.hash.indexOf("/intro") !== -1;
  }

  function updateBackButton() {
    var btn = document.getElementById("sl-back-btn");
    if (isIntroPage()) {
      if (!btn) injectBackButton();
    } else if (btn) {
      btn.remove();
    }
  }

  function injectBackButton() {
    if (document.getElementById("sl-back-btn")) return;

    var btn = document.createElement("div");
    btn.id = "sl-back-btn";
    btn.title = "Go Back";
    btn.innerHTML =
      '<svg viewBox="0 0 512 512" style="width:20px;height:20px;">' +
      '<path d="M328.6 106.5l-143.5 136.9 143.5 136.9" ' +
      'style="stroke:currentColor;stroke-linecap:round;stroke-linejoin:round;stroke-width:48;fill:none;"></path></svg>';

    var style = document.getElementById("sl-back-btn-style");
    if (!style) {
      style = document.createElement("style");
      style.id = "sl-back-btn-style";
      style.textContent =
        "#sl-back-btn {" +
        "position:fixed; top:12px; z-index:10000;" +
        "margin-left:max(0rem, calc(1rem - var(--safe-area-inset-left, 0px)));" +
        "cursor:pointer; color:white;" +
        "align-items:center; display:flex; flex:none;" +
        "justify-content:center;" +
        "height:3.5rem; width:3.5rem;" +
        "border-radius:0.75rem; opacity:0.6;" +
        "transition:opacity 0.15s, background 0.15s;" +
        "}" +
        "#sl-back-btn:hover {" +
        "opacity:1; background:rgba(255,255,255,0.08);" +
        "}";
      document.head.appendChild(style);
    }

    btn.addEventListener("click", function () {
      window.history.back();
    });

    document.body.appendChild(btn);
  }

  window.addEventListener("hashchange", updateBackButton);
  if (document.body) {
    updateBackButton();
  } else {
    document.addEventListener("DOMContentLoaded", updateBackButton);
  }

  // ============================================
  // Picture-in-Picture: shared state
  // ============================================
  // Declared early so the keyboard shortcut handler can access them.
  var _pipFeatureOn = localStorage.getItem("sl-pip-feature") !== "false";

  function isPlayerRoute() {
    var hash = window.location.hash || "";
    return hash.indexOf("/player") !== -1;
  }

  // ============================================
  // Keyboard Shortcuts
  // ============================================
  var zoomLevel = 1.0;

  function toggleFullscreen() {
    appWindow.isFullscreen().then(function (fs) {
      appWindow.setFullscreen(!fs);
    });
  }

  // ============================================
  // Fullscreen Button Interception
  // ============================================
  // The Stremio web UI has fullscreen buttons with title "Enter fullscreen mode"
  // or "Exit fullscreen mode". We intercept clicks on these to use native fullscreen.
  document.addEventListener(
    "click",
    function (e) {
      var el = e.target;
      // Walk up from the click target to find the button container
      for (var i = 0; i < 5 && el && el !== document; i++) {
        var title = el.getAttribute && el.getAttribute("title");
        if (
          title &&
          (title.indexOf("fullscreen") !== -1 ||
            title.indexOf("Fullscreen") !== -1)
        ) {
          e.preventDefault();
          e.stopPropagation();
          toggleFullscreen();
          return;
        }
        el = el.parentElement;
      }
    },
    true,
  );

  document.addEventListener("keydown", function (e) {
    // F11: Toggle fullscreen
    if (e.key === "F11") {
      e.preventDefault();
      toggleFullscreen();
      return;
    }

    // F key: Toggle fullscreen (not when typing in input fields)
    if (e.key === "f" && !e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey) {
      var tag = document.activeElement ? document.activeElement.tagName : "";
      var isInput =
        tag === "INPUT" ||
        tag === "TEXTAREA" ||
        tag === "SELECT" ||
        (document.activeElement && document.activeElement.isContentEditable);
      if (!isInput) {
        e.preventDefault();
        toggleFullscreen();
        return;
      }
    }

    // Only process Ctrl+ shortcuts below
    if (!e.ctrlKey) return;

    // Ctrl+Shift+I: Toggle DevTools
    if (e.shiftKey && (e.key === "I" || e.key === "i")) {
      e.preventDefault();
      host.invoke("toggle_devtools");
      return;
    }

    // Ctrl+Shift+P: Toggle Picture-in-Picture (only on player route when feature is enabled)
    if (e.shiftKey && (e.key === "P" || e.key === "p")) {
      if (isPlayerRoute() && _pipFeatureOn) {
        e.preventDefault();
        host.invoke("toggle_pip").catch(function (err) {
          console.error("[StremioLightning] PiP toggle failed:", err);
        });
      }
      return;
    }

    // Ctrl+R: Reload page
    if (!e.shiftKey && (e.key === "r" || e.key === "R")) {
      e.preventDefault();
      window.location.reload();
      return;
    }

    // Ctrl+= or Ctrl++: Zoom in
    if (e.key === "+" || e.key === "=") {
      e.preventDefault();
      zoomLevel = Math.min(zoomLevel + 0.1, 3.0);
      webview.setZoom(zoomLevel);
      return;
    }

    // Ctrl+-: Zoom out
    if (e.key === "-") {
      e.preventDefault();
      zoomLevel = Math.max(zoomLevel - 0.1, 0.5);
      webview.setZoom(zoomLevel);
      return;
    }
  });

  // ============================================
  // Picture-in-Picture Button Injection
  // ============================================
  // Injects a PiP button into the Stremio web player's ControlBar.
  // The button only appears when the PiP feature is enabled (localStorage)
  // and the user is on the player route. Clicking it calls toggle_pip.
  // ============================================

  var _pipBtnInjected = false;

  function injectPipButton() {
    if (_pipBtnInjected) return;
    // Find the control-bar-buttons-container inside the player
    var containers = document.querySelectorAll(
      '[class*="control-bar-buttons-container"]',
    );
    if (!containers.length) return;

    var btnContainer = containers[containers.length - 1];

    // Create the PiP button matching the ControlBar button style
    var btn = document.createElement("button");
    btn.id = "sl-pip-btn";
    btn.title = "Picture in Picture";
    btn.setAttribute("tabindex", "-1");
    btn.innerHTML =
      '<svg viewBox="0 0 24 24" style="width:3rem;height:2rem;fill:rgba(255,255,255,0.85);">' +
      '<path d="M19 11h-8v6h8v-6zm4 8V4.98C23 3.88 22.1 3 21 3H3c-1.1 0-2 .88-2 1.98V19c0 1.1.9 2 2 2h18c1.1 0 2-.9 2-2zm-2 .02H3V4.97h18v14.05z"/>' +
      "</svg>";

    // Style to match existing control-bar-button class
    btn.style.cssText =
      "flex:none;width:4rem;height:4rem;display:flex;justify-content:center;align-items:center;" +
      "background:transparent;border:none;cursor:pointer;padding:0;outline:none;";

    btn.addEventListener("mouseenter", function () {
      btn.querySelector("svg").style.fill = "rgba(255,255,255,1)";
    });
    btn.addEventListener("mouseleave", function () {
      btn.querySelector("svg").style.fill = "rgba(255,255,255,0.85)";
    });

    btn.addEventListener("click", function () {
      host.invoke("toggle_pip").catch(function (err) {
        console.error("[StremioLightning] PiP toggle failed:", err);
      });
      btn.blur();
    });

    // Insert before the .spacing div (left side of the control bar, after volume)
    var spacing = btnContainer.querySelector('[class*="spacing"]');
    if (spacing) {
      btnContainer.insertBefore(btn, spacing);
    } else {
      btnContainer.appendChild(btn);
    }

    _pipBtnInjected = true;
  }

  function removePipButton() {
    var btn = document.getElementById("sl-pip-btn");
    if (btn) {
      btn.remove();
      _pipBtnInjected = false;
    }
  }

  function updatePipButton() {
    if (isPlayerRoute() && _pipFeatureOn) {
      // Try to inject; the control bar may not be in the DOM yet
      injectPipButton();
      // If not yet injected, observe for it
      if (!_pipBtnInjected) {
        var observer = new MutationObserver(function () {
          if (_pipBtnInjected) {
            observer.disconnect();
            return;
          }
          injectPipButton();
          if (_pipBtnInjected) {
            observer.disconnect();
          }
        });
        observer.observe(document.body, { childList: true, subtree: true });
        // Safety: stop observing after 30s
        setTimeout(function () {
          observer.disconnect();
        }, 30000);
      }
    } else {
      removePipButton();
    }
  }

  // Listen for route changes
  window.addEventListener("hashchange", updatePipButton);
  if (document.body) {
    updatePipButton();
  } else {
    document.addEventListener("DOMContentLoaded", updatePipButton);
  }

  // Listen for PiP feature toggle from the Svelte settings UI
  document.addEventListener("sl-pip-feature-changed", function (e) {
    _pipFeatureOn = e.detail !== false;
    updatePipButton();
  });

  // When PiP is activated/deactivated, update the button icon
  document.addEventListener("sl-pip-enabled", function () {
    var btn = document.getElementById("sl-pip-btn");
    if (btn) btn.title = "Exit Picture in Picture";
    _pipDragActive = true;
  });
  document.addEventListener("sl-pip-disabled", function () {
    var btn = document.getElementById("sl-pip-btn");
    if (btn) btn.title = "Picture in Picture";
    _pipDragActive = false;
  });

  // ============================================
  // PiP Drag-from-Anywhere
  // ============================================
  // When PiP mode is active (borderless window), allow the user to
  // grab and drag the window from anywhere by clicking on non-interactive
  // areas. Interactive elements (buttons, inputs, sliders, etc.) are
  // excluded so they still work normally.
  // ============================================

  var _pipDragActive = false;

  function isInteractiveNode(el) {
    var tag = el.tagName;
    if (
      tag === "BUTTON" ||
      tag === "INPUT" ||
      tag === "TEXTAREA" ||
      tag === "SELECT" ||
      tag === "A" ||
      tag === "LABEL"
    ) {
      return true;
    }
    if (el.isContentEditable) return true;
    var role = el.getAttribute && el.getAttribute("role");
    if (
      role === "button" ||
      role === "slider" ||
      role === "textbox" ||
      role === "menuitem" ||
      role === "tab" ||
      role === "option"
    ) {
      return true;
    }
    if (el.className && typeof el.className === "string") {
      var cls = el.className;
      if (
        cls.indexOf("control-bar") !== -1 ||
        cls.indexOf("button") !== -1 ||
        cls.indexOf("slider") !== -1 ||
        cls.indexOf("seek") !== -1 ||
        cls.indexOf("volume") !== -1
      ) {
        return true;
      }
    }
    return false;
  }

  // Walk from the click target up to the document to see if any ancestor is interactive
  function isInsideInteractive(el) {
    while (el && el !== document.body && el !== document.documentElement) {
      if (isInteractiveNode(el)) return true;
      el = el.parentElement;
    }
    return false;
  }

  document.addEventListener(
    "mousedown",
    function (e) {
      if (!_pipDragActive) return;
      // Only left-click
      if (e.button !== 0) return;
      // Don't drag if clicking on/inside an interactive element
      if (isInsideInteractive(e.target)) return;
      // Stop the event so Stremio's "hold for 2x speed" never sees this mousedown.
      e.stopImmediatePropagation();
      e.preventDefault();
      appWindow.startDragging();
    },
    true,
  );

  // ============================================
  // Discord Rich Presence API & Tracker
  // ============================================
  // Port of discordTracker.ts + PlaybackState.ts + Helpers._eval()
  // from the Electron version (stremio-enhanced).
  // ============================================

  // Inject a <script> that runs in the page context (where window.services.core lives)
  // and returns the result via a CustomEvent. Each call uses a unique event name.
  var _evalCounter = 0;
  function _eval(js) {
    return new Promise(function (resolve, reject) {
      try {
        var eventName = "sl-eval-" + ++_evalCounter + "-" + Date.now();
        var script = document.createElement("script");
        window.addEventListener(
          eventName,
          function handler(e) {
            script.remove();
            resolve(e.detail);
          },
          { once: true },
        );
        script.textContent =
          "(function() {" +
          "  try {" +
          "    var core = window.services && window.services.core;" +
          '    if (!core) { window.dispatchEvent(new CustomEvent("' +
          eventName +
          '", { detail: null })); return; }' +
          "    var result = " +
          js +
          ";" +
          '    if (result && typeof result.then === "function") {' +
          '      result.then(function(r) { window.dispatchEvent(new CustomEvent("' +
          eventName +
          '", { detail: r })); })' +
          '            .catch(function() { window.dispatchEvent(new CustomEvent("' +
          eventName +
          '", { detail: null })); });' +
          "    } else {" +
          '      window.dispatchEvent(new CustomEvent("' +
          eventName +
          '", { detail: result }));' +
          "    }" +
          "  } catch(err) {" +
          '    window.dispatchEvent(new CustomEvent("' +
          eventName +
          '", { detail: null }));' +
          "  }" +
          "})();";
        document.head.appendChild(script);
        // Safety timeout
        setTimeout(function () {
          if (script.parentElement) {
            script.remove();
            resolve(null);
          }
        }, 10000);
      } catch (err) {
        reject(err);
      }
    });
  }

  function waitForElm(selector, timeout) {
    timeout = timeout || 10000;
    return new Promise(function (resolve, reject) {
      var el = document.querySelector(selector);
      if (el) return resolve(el);

      var observer = new MutationObserver(function () {
        var found = document.querySelector(selector);
        if (found) {
          observer.disconnect();
          resolve(found);
        }
      });

      observer.observe(document.body, { childList: true, subtree: true });

      setTimeout(function () {
        observer.disconnect();
        reject(new Error("waitForElm timeout: " + selector));
      }, timeout);
    });
  }

  function toFiniteNumber(value) {
    if (typeof value === "number" && isFinite(value)) return value;
    if (typeof value === "string" && value.trim() !== "") {
      var parsed = Number(value);
      if (isFinite(parsed)) return parsed;
    }
    return 0;
  }

  function normalizePlaybackState(fullState) {
    var streamState =
      fullState && fullState.streamState ? fullState.streamState : {};
    var currentTime = toFiniteNumber(streamState.time);
    if (!(currentTime > 0))
      currentTime = toFiniteNumber(streamState["time-pos"]);
    if (!(currentTime > 0))
      currentTime = toFiniteNumber(fullState && fullState.time);
    if (!(currentTime > 0))
      currentTime = toFiniteNumber(fullState && fullState["time-pos"]);
    if (!(currentTime > 0))
      currentTime = toFiniteNumber(discordMpvState.timePos);

    var duration = toFiniteNumber(streamState.duration);
    if (!(duration > 0))
      duration = toFiniteNumber(fullState && fullState.duration);
    if (!(duration > 0))
      duration = toFiniteNumber(fullState && fullState["duration"]);
    if (!(duration > 0)) duration = toFiniteNumber(discordMpvState.duration);

    var pausedValue = streamState.paused;
    if (typeof pausedValue !== "boolean") pausedValue = streamState.pause;
    if (typeof pausedValue !== "boolean")
      pausedValue = streamState["paused-for-cache"];
    if (typeof pausedValue !== "boolean")
      pausedValue = fullState && fullState.paused;
    if (typeof pausedValue !== "boolean")
      pausedValue = fullState && fullState.pause;
    if (typeof pausedValue !== "boolean") pausedValue = discordMpvState.pause;
    if (!pausedValue && discordMpvState.pausedForCache) pausedValue = true;

    return {
      currentTime: Math.max(0, currentTime),
      duration: Math.max(0, duration),
      isPaused: !!pausedValue,
    };
  }

  function buildWatchingLabels(meta, seriesInfo) {
    var details = meta && meta.name ? meta.name : "Unknown title";
    var state = "Watching";

    if (meta && meta.type === "series" && seriesInfo) {
      var parts = [];
      var isKitsu = meta.id && meta.id.indexOf("kitsu:") === 0;

      if (!isKitsu && seriesInfo.season != null) {
        parts.push("Season " + seriesInfo.season);
      }
      if (seriesInfo.episode != null) {
        parts.push("Episode " + seriesInfo.episode);
      }

      if (parts.length) {
        state = parts.join(" - ");
      }
    }

    return {
      details: details,
      state: state,
    };
  }

  function buildWatchingActivity(
    meta,
    seriesInfo,
    currentTime,
    duration,
    isPaused,
  ) {
    var labels = buildWatchingLabels(meta, seriesInfo);
    var activity = {
      details: labels.details,
      state: isPaused ? "Paused" : labels.state,
      largeImageKey: meta && meta.poster ? meta.poster : "stremio",
      largeImageText: meta && meta.name ? meta.name : "Stremio Lightning",
      activityType: 3,
    };

    if (!isPaused && duration > 0) {
      var now = Math.floor(Date.now() / 1000);
      var safeCurrentTime = Math.max(0, Math.floor(currentTime));
      var safeDuration = Math.max(safeCurrentTime, Math.ceil(duration));
      activity.startTimestamp = now - safeCurrentTime;
      activity.endTimestamp = activity.startTimestamp + safeDuration;
    }

    return activity;
  }

  // Retry wrapper for getting core state
  var CORESTATE_MAX_RETRIES = 30;
  var CORESTATE_RETRY_INTERVAL = 1000;

  function getPlayerState() {
    var attempt = 0;
    return new Promise(function (resolve) {
      function tryOnce() {
        if (attempt >= CORESTATE_MAX_RETRIES) return resolve(null);
        attempt++;
        _eval('core.transport.getState("player")')
          .then(function (state) {
            if (state && state.metaItem && state.metaItem.content) {
              resolve({
                seriesInfoDetails: state.seriesInfo || null,
                metaDetails: state.metaItem.content,
                stream: state.stream || null,
              });
            } else {
              setTimeout(tryOnce, CORESTATE_RETRY_INTERVAL);
            }
          })
          .catch(function () {
            setTimeout(tryOnce, CORESTATE_RETRY_INTERVAL);
          });
      }
      tryOnce();
    });
  }

  function getMetaDetails() {
    var attempt = 0;
    return new Promise(function (resolve) {
      function tryOnce() {
        if (attempt >= CORESTATE_MAX_RETRIES) return resolve(null);
        attempt++;
        _eval('core.transport.getState("meta_details")')
          .then(function (state) {
            if (
              state &&
              state.metaItem &&
              state.metaItem.content &&
              state.metaItem.content.content
            ) {
              resolve(state.metaItem.content.content);
            } else {
              setTimeout(tryOnce, CORESTATE_RETRY_INTERVAL);
            }
          })
          .catch(function () {
            setTimeout(tryOnce, CORESTATE_RETRY_INTERVAL);
          });
      }
      tryOnce();
    });
  }

  // Discord tracker object
  var _discordTrackerActive = false;

  var discordTracker = {
    init: function () {
      if (_discordTrackerActive) return;
      _discordTrackerActive = true;
      observeDiscordMpvProperties();
      console.info(
        "[DiscordRPC] Tracker initialized, current hash:",
        location.hash,
      );
      window.addEventListener("hashchange", discordTracker.handleNavigation);
      discordTracker.handleNavigation();
    },

    stop: function () {
      console.info("[DiscordRPC] Tracker stopped");
      _discordTrackerActive = false;
      discordTracker._stopMpvPoll();
      window.removeEventListener("hashchange", discordTracker.handleNavigation);
    },

    handleNavigation: function () {
      if (!_discordTrackerActive) return;
      var hash = location.hash;
      console.info("[DiscordRPC] handleNavigation, hash:", hash);

      // If we have an active MPV poll and the hash becomes empty/transient,
      // skip navigation handling - the player page often triggers brief
      // empty-hash events during loading
      if (discordTracker._mpvPollInterval && (hash === "" || hash === "#/")) {
        console.info(
          "[DiscordRPC] Ignoring transient empty hash while MPV poll active",
        );
        return;
      }

      discordTracker._checkWatching();
      discordTracker._checkExploring();
      discordTracker._checkMainMenu();
    },

    // Polling interval handle for MPV-based Discord activity updates
    _mpvPollInterval: null,

    _stopMpvPoll: function () {
      if (discordTracker._mpvPollInterval) {
        clearInterval(discordTracker._mpvPollInterval);
        discordTracker._mpvPollInterval = null;
        console.info("[DiscordRPC] MPV poll stopped");
      }
    },

    _checkWatching: function () {
      if (location.href.indexOf("#/player") === -1) {
        discordTracker._stopMpvPoll();
        return;
      }

      if (nativePlayerEnabled) {
        // MPV native player path: poll core transport state since there's no <video> element
        console.info(
          "[DiscordRPC] On player page (MPV mode), starting poll-based tracker...",
        );
        observeDiscordMpvProperties();
        discordTracker._stopMpvPoll(); // clear any existing poll

        // Get initial meta once, then poll for time/pause updates
        getPlayerState().then(function (playerState) {
          if (!playerState) {
            console.warn("[DiscordRPC] Could not get player state");
            return;
          }
          console.info(
            "[DiscordRPC] Player state:",
            playerState.metaDetails.name,
            playerState.metaDetails.type,
          );

          function pollAndUpdate() {
            if (location.href.indexOf("#/player") === -1) {
              discordTracker._stopMpvPoll();
              return;
            }

            // Get time/duration/paused from the player state
            // core.transport.getState("player") returns a Promise, so we handle it properly
            _eval('core.transport.getState("player")')
              .then(function (fullState) {
                if (!fullState) {
                  console.warn("[DiscordRPC] pollAndUpdate: no player state");
                  return;
                }

                var playback = normalizePlaybackState(fullState);
                console.info(
                  "[DiscordRPC] player state keys:",
                  Object.keys(fullState),
                );
                console.info(
                  "[DiscordRPC] player streamState:",
                  JSON.stringify(fullState.streamState || {}).substring(0, 300),
                );
                console.info(
                  "[DiscordRPC] cached MPV state:",
                  JSON.stringify(discordMpvState),
                );
                console.info(
                  "[DiscordRPC] player state raw time/duration/paused:",
                  fullState.time,
                  fullState.duration,
                  fullState.paused,
                );
                console.info(
                  "[DiscordRPC] player state full dump:",
                  JSON.stringify(fullState).substring(0, 500),
                );

                // Get meta from the state directly
                var meta =
                  fullState.metaItem && fullState.metaItem.content
                    ? fullState.metaItem.content
                    : null;
                var seriesInfo = fullState.seriesInfo || null;

                if (!meta) {
                  console.warn(
                    "[DiscordRPC] pollAndUpdate: no meta in player state",
                  );
                  return;
                }

                console.info(
                  "[DiscordRPC] pollAndUpdate:",
                  meta.name,
                  "time=" + playback.currentTime.toFixed(1) + "s",
                  "duration=" + playback.duration.toFixed(1) + "s",
                  "paused=" + playback.isPaused,
                );

                var activity = buildWatchingActivity(
                  meta,
                  seriesInfo,
                  playback.currentTime,
                  playback.duration,
                  playback.isPaused,
                );

                host.invoke("update_discord_activity", { activity: activity })
                  .then(function () {
                    console.info(
                      "[DiscordRPC] Activity sent:",
                      activity.details,
                      activity.state,
                      activity.startTimestamp,
                      activity.endTimestamp,
                    );
                  })
                  .catch(function (e) {
                    console.error("[DiscordRPC] update failed:", e);
                  });
              })
              .catch(function (e) {
                console.error("[DiscordRPC] pollAndUpdate error:", e);
              });
          }

          // Update immediately, then poll every 5 seconds for pause/seek changes.
          pollAndUpdate();
          discordTracker._mpvPollInterval = setInterval(pollAndUpdate, 5000);
        });
      } else {
        // Web player path: use DOM <video> element events
        console.info(
          "[DiscordRPC] On player page, waiting for video element...",
        );

        waitForElm("video")
          .then(function () {
            var video = document.getElementsByTagName("video")[0];
            if (!video) {
              console.warn("[DiscordRPC] video element not found");
              return;
            }
            console.info(
              "[DiscordRPC] Video element found, getting player state...",
            );

            getPlayerState().then(function (playerState) {
              if (!playerState) {
                console.warn("[DiscordRPC] Could not get player state");
                return;
              }
              console.info(
                "[DiscordRPC] Player state:",
                playerState.metaDetails.name,
                playerState.metaDetails.type,
              );
              var meta = playerState.metaDetails;

              function syncVideoActivity() {
                var activity = buildWatchingActivity(
                  meta,
                  playerState.seriesInfoDetails,
                  video.currentTime,
                  video.duration || 0,
                  !!video.paused,
                );

                host.invoke("update_discord_activity", { activity: activity }).catch(
                  function (e) {
                    console.error("[DiscordRPC] update failed:", e);
                  },
                );
              }

              // Prevent duplicate listeners
              video.removeEventListener(
                "playing",
                video._slHandleVideoActivity,
              );
              video.removeEventListener("pause", video._slHandleVideoActivity);
              video.removeEventListener("seeked", video._slHandleVideoActivity);
              video.removeEventListener(
                "durationchange",
                video._slHandleVideoActivity,
              );
              video._slHandleVideoActivity = syncVideoActivity;
              video.addEventListener("playing", syncVideoActivity);
              video.addEventListener("pause", syncVideoActivity);
              video.addEventListener("seeked", syncVideoActivity);
              video.addEventListener("durationchange", syncVideoActivity);
              syncVideoActivity();
            });
          })
          .catch(function () {});
      }
    },

    _checkExploring: function () {
      if (location.href.indexOf("#/detail") === -1) return;
      console.info("[DiscordRPC] On detail page, getting meta details...");

      getMetaDetails().then(function (meta) {
        if (!meta) {
          console.warn("[DiscordRPC] Could not get meta details");
          return;
        }
        console.info("[DiscordRPC] Exploring:", meta.name);
        host.invoke("update_discord_activity", {
          activity: {
            details: meta.name,
            state: "Exploring",
            largeImageKey: meta.poster || "stremio",
            largeImageText: "Stremio Lightning",
            smallImageKey: "hamburger",
            smallImageText: "Main Menu",
            activityType: 3,
          },
        }).catch(function (e) {
          console.error("[DiscordRPC] update failed:", e);
        });
      });
    },

    _checkMainMenu: function () {
      var hashMap = {
        "": "Home",
        "#/": "Home",
        "#/board": "Home",
        "#/discover": "Discover",
        "#/library": "Library",
        "#/calendar": "Calendar",
        "#/addons": "Addons",
        "#/settings": "Settings",
        "#/search": "Search",
      };

      var activity = hashMap[location.hash];
      if (activity) {
        console.info(
          "[DiscordRPC] Main menu activity:",
          activity,
          "hash:",
          location.hash,
        );
        host.invoke("update_discord_activity", {
          activity: {
            details: activity,
            largeImageKey: "stremio",
            largeImageText: "Stremio Lightning",
            smallImageKey: "hamburger",
            smallImageText: "Main Menu",
            activityType: 3,
          },
        })
          .then(function () {
            console.info("[DiscordRPC] Activity sent successfully");
          })
          .catch(function (e) {
            console.error("[DiscordRPC] update failed:", e);
          });
      } else {
        console.info(
          "[DiscordRPC] _checkMainMenu: no match for hash:",
          location.hash,
        );
      }
    },
  };

  // Expose tracker init/stop on the API so mod-ui.js settings toggle can use them
  window.StremioEnhancedAPI._discordTrackerInit = function () {
    discordTracker.init();
  };
  window.StremioEnhancedAPI._discordTrackerStop = function () {
    discordTracker.stop();
  };

  // Listen for enable/disable events from mod-ui.js (fallback path)
  window.addEventListener("sl-discord-rpc-enable", function () {
    discordTracker.init();
  });
  window.addEventListener("sl-discord-rpc-disable", function () {
    discordTracker.stop();
  });

  // Listen for mods panel open/close to update Discord activity
  window.addEventListener("sl-mods-panel", function (e) {
    if (!_discordTrackerActive) return;
    if (e.detail) {
      // Panel opened
      console.info("[DiscordRPC] Mods panel opened");
      host.invoke("update_discord_activity", {
        activity: {
          details: "Mods",
          state: "Browsing mods",
          largeImageKey: "stremio",
          largeImageText: "Stremio Lightning",
          smallImageKey: "hamburger",
          smallImageText: "Main Menu",
          activityType: 3,
        },
      }).catch(function (e) {
        console.error("[DiscordRPC] update failed:", e);
      });
    } else {
      // Panel closed - refresh the current page activity
      discordTracker.handleNavigation();
    }
  });

  // ============================================
  // App Update Banner
  // ============================================

  function injectUpdateBannerStyles() {
    if (document.getElementById("sl-update-banner-styles")) return;
    var style = document.createElement("style");
    style.id = "sl-update-banner-styles";
    style.textContent =
      "@keyframes sl-banner-slide-down { from { transform:translateY(-100%); opacity:0; } to { transform:translateY(0); opacity:1; } }" +
      ".sl-update-banner { position:fixed; top:0; left:0; right:0; z-index:200000; display:flex; align-items:center; justify-content:center; padding:0; background:linear-gradient(180deg, rgba(12,11,17,0.95) 0%, rgba(12,11,17,0.88) 100%); border-bottom:1px solid rgba(255,255,255,0.06); backdrop-filter:blur(30px) saturate(140%); -webkit-backdrop-filter:blur(30px) saturate(140%); box-shadow:0 8px 32px rgba(0,0,0,0.4), 0 2px 8px rgba(0,0,0,0.2); animation:sl-banner-slide-down 0.4s cubic-bezier(0.16,1,0.3,1); font-family:inherit; box-sizing:border-box; }" +
      ".sl-update-banner-content { display:flex; align-items:center; gap:1rem; width:100%; padding:0.75rem 1.25rem; box-sizing:border-box; }" +
      ".sl-update-banner-icon { flex:none; display:flex; align-items:center; justify-content:center; width:2rem; height:2rem; border-radius:50%; background:rgba(123,91,245,0.12); color:var(--primary-accent-color, #7b5bf5); }" +
      ".sl-update-banner-icon svg { width:1rem; height:1rem; }" +
      ".sl-update-banner-text { flex:1; font-size:inherit; font-weight:400; color:var(--primary-foreground-color, rgba(255,255,255,0.8)); line-height:1.4; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }" +
      ".sl-update-banner-version { font-weight:600; color:var(--primary-accent-color, #7b5bf5); }" +
      ".sl-update-banner-current { opacity:0.5; }" +
      ".sl-update-banner-actions { flex:none; display:flex; align-items:center; gap:0.5rem; }" +
      ".sl-update-banner-download { flex:none; padding:0.45rem 1.1rem; border:none; border-radius:var(--border-radius, 0.5rem); background:var(--primary-accent-color, #7b5bf5); color:#fff; font-size:inherit; font-weight:600; cursor:pointer; transition:background 0.15s, transform 0.1s, box-shadow 0.15s; box-shadow:0 2px 8px rgba(123,91,245,0.3); }" +
      ".sl-update-banner-download:hover { background:color-mix(in srgb, var(--primary-accent-color, #7b5bf5) 85%, white); box-shadow:0 4px 16px rgba(123,91,245,0.4); }" +
      ".sl-update-banner-download:active { transform:scale(0.97); }" +
      ".sl-update-banner-close { flex:none; display:flex; align-items:center; justify-content:center; width:2rem; height:2rem; border:none; border-radius:var(--border-radius, 0.5rem); background:transparent; color:rgba(255,255,255,0.35); cursor:pointer; transition:background 0.15s, color 0.15s; padding:0; }" +
      ".sl-update-banner-close:hover { background:var(--overlay-color, rgba(255,255,255,0.08)); color:rgba(255,255,255,0.8); }" +
      ".sl-update-banner-close svg { width:0.9rem; height:0.9rem; }" +
      "@media only screen and (max-width: 600px) { .sl-update-banner-content { padding:0.6rem 0.75rem; gap:0.6rem; } .sl-update-banner-current { display:none; } }";
    document.head.appendChild(style);
  }

  function showUpdateBanner(info) {
    if (document.getElementById("sl-update-banner")) return;

    injectUpdateBannerStyles();

    var banner = document.createElement("div");
    banner.id = "sl-update-banner";
    banner.className = "sl-update-banner";

    var content = document.createElement("div");
    content.className = "sl-update-banner-content";

    // Icon (static SVG, no user data)
    var iconDiv = document.createElement("div");
    iconDiv.className = "sl-update-banner-icon";
    iconDiv.innerHTML =
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>';

    // Text — using textContent for user-controlled values (auto-escapes)
    var textSpan = document.createElement("span");
    textSpan.className = "sl-update-banner-text";
    textSpan.appendChild(
      document.createTextNode(
        "A new version of Stremio Lightning is available: ",
      ),
    );

    var versionSpan = document.createElement("span");
    versionSpan.className = "sl-update-banner-version";
    versionSpan.textContent = info.newVersion;
    textSpan.appendChild(versionSpan);

    textSpan.appendChild(document.createTextNode(" "));

    var currentSpan = document.createElement("span");
    currentSpan.className = "sl-update-banner-current";
    currentSpan.textContent = "(you have v" + info.currentVersion + ")";
    textSpan.appendChild(currentSpan);

    // Actions
    var actionsDiv = document.createElement("div");
    actionsDiv.className = "sl-update-banner-actions";

    var downloadBtn = document.createElement("button");
    downloadBtn.className = "sl-update-banner-download";
    downloadBtn.textContent = "Download Update";
    downloadBtn.addEventListener("click", function () {
      host.invoke("open_external_url", { url: info.releaseUrl }).catch(function (e) {
        console.error("[AppUpdater] Failed to open release URL:", e);
      });
    });

    var closeBtn = document.createElement("button");
    closeBtn.className = "sl-update-banner-close";
    closeBtn.title = "Dismiss";
    closeBtn.innerHTML =
      '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>';
    closeBtn.addEventListener("click", function () {
      banner.style.animation = "none";
      banner.style.transition =
        "transform 0.25s ease-in, opacity 0.25s ease-in";
      banner.style.transform = "translateY(-100%)";
      banner.style.opacity = "0";
      setTimeout(function () {
        banner.remove();
      }, 260);
      try {
        localStorage.setItem("sl-dismissed-update", info.newVersion);
      } catch (_) {}
    });

    actionsDiv.appendChild(downloadBtn);
    actionsDiv.appendChild(closeBtn);

    content.appendChild(iconDiv);
    content.appendChild(textSpan);
    content.appendChild(actionsDiv);
    banner.appendChild(content);

    document.body.insertBefore(banner, document.body.firstChild);
  }

  function initUpdateChecker() {
    setTimeout(function () {
      window.StremioEnhancedAPI.checkAppUpdate()
        .then(function (info) {
          if (!info.hasUpdate) {
            console.info(
              "[AppUpdater] No update available (current: v" +
                info.currentVersion +
                ")",
            );
            return;
          }
          // Skip if user already dismissed this version
          try {
            var dismissed = localStorage.getItem("sl-dismissed-update");
            if (dismissed === info.newVersion) {
              console.info(
                "[AppUpdater] Update " +
                  info.newVersion +
                  " was dismissed by user",
              );
              return;
            }
          } catch (_) {}

          console.info(
            "[AppUpdater] Update available: " +
              info.newVersion +
              " (current: v" +
              info.currentVersion +
              ")",
          );
          showUpdateBanner(info);
        })
        .catch(function (e) {
          console.error("[AppUpdater] Failed to check for updates:", e);
        });
    }, 5000);
  }

  // Auto-start Discord RPC if enabled in localStorage
  function initDiscordRpc() {
    var enabled = localStorage.getItem("discordrichpresence");
    console.info(
      "[DiscordRPC] initDiscordRpc called, localStorage value:",
      enabled,
    );
    if (enabled === "true") {
      console.info("[DiscordRPC] Calling start_discord_rpc...");
      host.invoke("start_discord_rpc")
        .then(function () {
          console.info(
            "[DiscordRPC] start_discord_rpc succeeded, initializing tracker",
          );
          discordTracker.init();
          console.info("[StremioLightning] Discord RPC started");
        })
        .catch(function (e) {
          console.error("[StremioLightning] Failed to start Discord RPC:", e);
        });
    }
  }

  if (document.readyState === "complete") {
    initDiscordRpc();
    initUpdateChecker();
  } else {
    window.addEventListener(
      "load",
      function () {
        initDiscordRpc();
        initUpdateChecker();
      },
      { once: true },
    );
  }
})();
