// Stremio Lightning - Frontend Bridge & Keyboard Shortcuts
// Injected via Tauri initialization_script - runs on every page load
(function () {
  "use strict";

  // Guard: ensure Tauri IPC is available
  if (!window.__TAURI__) {
    console.error(
      "[StremioLightning] __TAURI__ not available - bridge not loaded",
    );
    return;
  }

  // Tauri APIs exposed via withGlobalTauri: true
  var invoke = window.__TAURI__.core.invoke;
  var listen = window.__TAURI__.event.listen;
  var getCurrentWindow = window.__TAURI__.window.getCurrentWindow;
  var getCurrentWebview = window.__TAURI__.webview.getCurrentWebview;

  var appWindow = getCurrentWindow();
  var webview = getCurrentWebview();

  // ============================================
  // IPC Shell Transport Compatibility
  // ============================================
  var shellTransportEnabled =
    window.__STREMIO_LIGHTNING_ENABLE_NATIVE_PLAYER__ === true;
  if (shellTransportEnabled) {
    console.info(
      "[StremioLightning] Native player mode enabled (libmpv transport)",
    );
  }
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

    if (!shellTransportEnabled || !payload) return;

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
    if (!shellTransportEnabled || discordMpvState.observed) return;

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
    return invoke("shell_transport_send", { message: serialized }).catch(
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
    invoke("shell_bridge_ready").catch(function (error) {
      console.error("[StremioLightning] shell bridge ready failed:", error);
    });
  }

  if (window.self === window.top) {
    listen("shell-transport-message", function (event) {
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

    if (shellTransportEnabled) {
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
  }

  // ============================================
  // Frontend Bridge: window.StremioEnhancedAPI
  // ============================================
  // Mirrors the API shape from the Electron version.
  // Plugins and the web UI can call these methods.
  window.StremioEnhancedAPI = {
    // Window management
    minimizeWindow: function () {
      return appWindow.minimize();
    },
    maximizeWindow: function () {
      return appWindow.toggleMaximize();
    },
    closeWindow: function () {
      return appWindow.close();
    },
    isMaximized: function () {
      return appWindow.isMaximized();
    },
    isFullscreen: function () {
      return appWindow.isFullscreen();
    },
    dragWindow: function () {
      return appWindow.startDragging();
    },

    // Event subscriptions (returns unlisten function)
    onMaximizedChange: function (callback) {
      return listen("window-maximized-changed", function (e) {
        callback(e.payload);
      });
    },
    onFullscreenChange: function (callback) {
      return listen("window-fullscreen-changed", function (e) {
        callback(e.payload);
      });
    },

    // Streaming server management
    startStreamingServer: function () {
      return invoke("start_streaming_server");
    },
    stopStreamingServer: function () {
      return invoke("stop_streaming_server");
    },
    restartStreamingServer: function () {
      return invoke("restart_streaming_server");
    },
    getStreamingServerStatus: function () {
      return invoke("get_streaming_server_status");
    },
    getNativePlayerStatus: function () {
      return invoke("get_native_player_status");
    },

    // Server event subscriptions
    onServerStarted: function (callback) {
      return listen("server-started", function () {
        callback();
      });
    },
    onServerStopped: function (callback) {
      return listen("server-stopped", function (e) {
        callback(e.payload);
      });
    },

    // ============================================
    // Mod Management
    // ============================================
    getPlugins: function () {
      return invoke("get_plugins");
    },
    getThemes: function () {
      return invoke("get_themes");
    },
    downloadMod: function (url, modType) {
      return invoke("download_mod", { url: url, modType: modType });
    },
    deleteMod: function (filename, modType) {
      return invoke("delete_mod", { filename: filename, modType: modType });
    },
    getModContent: function (filename, modType) {
      return invoke("get_mod_content", {
        filename: filename,
        modType: modType,
      });
    },
    getRegistry: function () {
      return invoke("get_registry");
    },
    checkModUpdates: function (filename, modType) {
      return invoke("check_mod_updates", {
        filename: filename,
        modType: modType,
      });
    },

    // Settings
    getSetting: function (pluginName, key) {
      return invoke("get_setting", { pluginName: pluginName, key: key });
    },
    saveSetting: function (pluginName, key, value) {
      return invoke("save_setting", {
        pluginName: pluginName,
        key: key,
        value: JSON.stringify(value),
      });
    },
    registerSettings: function (pluginName, schema) {
      return invoke("register_settings", {
        pluginName: pluginName,
        schema: JSON.stringify(schema),
      });
    },
    getRegisteredSettings: function (pluginName) {
      return invoke("get_registered_settings", { pluginName: pluginName });
    },

    // Logging (tagged by plugin name)
    info: function (tag, msg) {
      console.log("[" + tag + "]", msg);
    },
    warn: function (tag, msg) {
      console.warn("[" + tag + "]", msg);
    },
    error: function (tag, msg) {
      console.error("[" + tag + "]", msg);
    },

    // Settings saved callbacks (per-plugin)
    _settingsCallbacks: {},
    onSettingsSaved: function (pluginName, callback) {
      if (!window.StremioEnhancedAPI._settingsCallbacks[pluginName]) {
        window.StremioEnhancedAPI._settingsCallbacks[pluginName] = [];
      }
      window.StremioEnhancedAPI._settingsCallbacks[pluginName].push(callback);
    },
    _notifySettingsSaved: function (pluginName, settings) {
      var cbs = window.StremioEnhancedAPI._settingsCallbacks[pluginName] || [];
      cbs.forEach(function (cb) {
        try {
          cb(settings);
        } catch (e) {}
      });
    },

    // Theme application
    _themeInlineProps: [],

    _applyInlineThemeProperties: function (css) {
      var root = document.documentElement;
      var props = [];
      var clean = css.replace(/\/\*[\s\S]*?\*\//g, "");
      var regex = /(--[\w-]+)\s*:\s*([^;!}]+)/g;
      var match;
      while ((match = regex.exec(clean)) !== null) {
        var name = match[1].trim();
        var value = match[2].trim();
        if (value) {
          root.style.setProperty(name, value);
          props.push(name);
        }
      }
      this._themeInlineProps = props;
    },

    _clearInlineThemeProperties: function () {
      var root = document.documentElement;
      var props = this._themeInlineProps || [];
      for (var i = 0; i < props.length; i++) {
        root.style.removeProperty(props[i]);
      }
      this._themeInlineProps = [];
    },

    applyTheme: function (fileName) {
      window.StremioEnhancedAPI._clearInlineThemeProperties();

      if (fileName === "Default") {
        var el = document.getElementById("activeTheme");
        if (el) el.remove();
        localStorage.setItem("currentTheme", "Default");
        window.dispatchEvent(new CustomEvent("sl-theme-changed"));
        return Promise.resolve();
      }
      return invoke("get_mod_content", {
        filename: fileName,
        modType: "theme",
      }).then(function (css) {
        var el = document.getElementById("activeTheme");
        if (el) el.remove();
        var style = document.createElement("style");
        style.id = "activeTheme";
        style.textContent = css;
        document.head.appendChild(style);
        localStorage.setItem("currentTheme", fileName);
        window.StremioEnhancedAPI._applyInlineThemeProperties(css);
        window.dispatchEvent(new CustomEvent("sl-theme-changed"));
      });
    },
  };

  // ============================================
  // Auto-load Plugins & Theme
  // ============================================
  function loadEnabledPlugins() {
    var enabled = JSON.parse(localStorage.getItem("enabledPlugins") || "[]");
    enabled.forEach(function (pluginName) {
      if (document.getElementById(pluginName)) return;
      invoke("get_mod_content", { filename: pluginName, modType: "plugin" })
        .then(function (content) {
          var baseName = pluginName.replace(".plugin.js", "");
          var wrapped =
            "(function() {\n" +
            "var StremioEnhancedAPI = {\n" +
            "  logger: {\n" +
            '    info: function(m) { window.StremioEnhancedAPI.info("' +
            baseName +
            '", m); },\n' +
            '    warn: function(m) { window.StremioEnhancedAPI.warn("' +
            baseName +
            '", m); },\n' +
            '    error: function(m) { window.StremioEnhancedAPI.error("' +
            baseName +
            '", m); }\n' +
            "  },\n" +
            '  getSetting: function(k) { return window.StremioEnhancedAPI.getSetting("' +
            baseName +
            '", k); },\n' +
            '  saveSetting: function(k, v) { return window.StremioEnhancedAPI.saveSetting("' +
            baseName +
            '", k, v); },\n' +
            '  registerSettings: function(s) { return window.StremioEnhancedAPI.registerSettings("' +
            baseName +
            '", s); },\n' +
            '  onSettingsSaved: function(cb) { return window.StremioEnhancedAPI.onSettingsSaved("' +
            baseName +
            '", cb); }\n' +
            "};\n" +
            "try {\n" +
            content +
            '\n} catch(err) { console.error("[ModController] Plugin crashed: ' +
            pluginName +
            '", err); }\n' +
            "})();";
          var script = document.createElement("script");
          script.id = pluginName;
          script.textContent = wrapped;
          document.body.appendChild(script);
        })
        .catch(function (e) {
          console.error(
            "[StremioLightning] Failed to load plugin:",
            pluginName,
            e,
          );
        });
    });
  }

  function loadActiveTheme() {
    var theme = localStorage.getItem("currentTheme");
    if (theme && theme !== "Default") {
      window.StremioEnhancedAPI.applyTheme(theme).catch(function (e) {
        console.error("[StremioLightning] Failed to load theme:", theme, e);
      });
    }
  }

  // Load theme immediately (no delay on refresh) - document.head is
  // available in initialization_script context, so inject the <style> ASAP.
  loadActiveTheme();

  // Load plugins after page is ready (they may depend on DOM)
  if (document.readyState === "complete") {
    loadEnabledPlugins();
  } else {
    window.addEventListener("load", function () {
      loadEnabledPlugins();
    });
  }

  // ============================================
  // External URL Handling (OAuth, popups, etc.)
  // ============================================
  // Intercept window.open() calls and open them in the system browser
  // instead of creating popup windows inside the webview.
  // This mirrors Electron's setWindowOpenHandler -> shell.openExternal.
  window.open = function (url) {
    if (url) {
      invoke("open_external_url", { url: String(url) }).catch(function (e) {
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
  if (shellTransportEnabled) {
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
      invoke("toggle_devtools");
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

      if (shellTransportEnabled) {
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

                invoke("update_discord_activity", { activity: activity })
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

                invoke("update_discord_activity", { activity: activity }).catch(
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
        invoke("update_discord_activity", {
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
        invoke("update_discord_activity", {
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
      invoke("update_discord_activity", {
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

  // Auto-start Discord RPC if enabled in localStorage
  function initDiscordRpc() {
    var enabled = localStorage.getItem("discordrichpresence");
    console.info(
      "[DiscordRPC] initDiscordRpc called, localStorage value:",
      enabled,
    );
    if (enabled === "true") {
      console.info("[DiscordRPC] Calling start_discord_rpc...");
      invoke("start_discord_rpc")
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
  } else {
    window.addEventListener("load", initDiscordRpc, { once: true });
  }
})();
