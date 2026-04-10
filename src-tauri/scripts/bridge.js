// Stremio Lightning - Frontend Bridge & Keyboard Shortcuts
// Injected via Tauri initialization_script — runs on every page load
(function() {
  'use strict';

  // Guard: ensure Tauri IPC is available
  if (!window.__TAURI__) {
    console.error('[StremioLightning] __TAURI__ not available — bridge not loaded');
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
  // Official Shell Transport Compatibility
  // ============================================
  var shellTransportEnabled = window.__STREMIO_LIGHTNING_ENABLE_NATIVE_PLAYER__ === true;
  if (shellTransportEnabled) {
    console.info('[StremioLightning] Native player mode enabled (libmpv transport)');
  }
  var shellMessageListeners = [];
  var nativeChromeWebview = null;

  try {
    nativeChromeWebview = window.chrome && window.chrome.webview ? window.chrome.webview : null;
  } catch (error) {
    console.warn('[StremioLightning] Could not access native chrome.webview:', error);
  }

  function dispatchShellTransportMessage(payload) {
    var event = { data: payload };

    try {
      if (window.qt && window.qt.webChannelTransport && typeof window.qt.webChannelTransport.onmessage === 'function') {
        window.qt.webChannelTransport.onmessage(event);
      }
    } catch (error) {
      console.error('[StremioLightning] qt.webChannelTransport handler failed:', error);
    }

    try {
      if (nativeChromeWebview && typeof nativeChromeWebview.dispatchEvent === 'function') {
        nativeChromeWebview.dispatchEvent(new MessageEvent('message', { data: payload }));
      }
    } catch (error) {
      console.error('[StremioLightning] native chrome.webview dispatch failed:', error);
    }

    shellMessageListeners.slice().forEach(function(listener) {
      try {
        listener(event);
      } catch (error) {
        console.error('[StremioLightning] chrome.webview message listener failed:', error);
      }
    });
  }

  function sendShellTransportMessage(payload) {
    var serialized = typeof payload === 'string' ? payload : JSON.stringify(payload);
    return invoke('shell_transport_send', { message: serialized }).catch(function(error) {
      console.error('[StremioLightning] shell transport send failed:', error, serialized);
    });
  }

  function notifyShellBridgeReady() {
    invoke('shell_bridge_ready').catch(function(error) {
      console.error('[StremioLightning] shell bridge ready failed:', error);
    });
  }

  if (window.self === window.top) {
    listen('shell-transport-message', function(event) {
      dispatchShellTransportMessage(event.payload);
    }).then(function() {
      if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', notifyShellBridgeReady, { once: true });
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
          addEventListener: function(name, listener) {
            if (name !== 'message') {
              throw new Error('Unsupported event: ' + name);
            }
            shellMessageListeners.push(listener);
          },
          removeEventListener: function(name, listener) {
            if (name !== 'message') {
              throw new Error('Unsupported event: ' + name);
            }
            shellMessageListeners = shellMessageListeners.filter(function(item) {
              return item !== listener;
            });
          }
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
    minimizeWindow: function() { return appWindow.minimize(); },
    maximizeWindow: function() { return appWindow.toggleMaximize(); },
    closeWindow: function() { return appWindow.close(); },
    isMaximized: function() { return appWindow.isMaximized(); },
    isFullscreen: function() { return appWindow.isFullscreen(); },
    dragWindow: function() { return appWindow.startDragging(); },

    // Event subscriptions (returns unlisten function)
    onMaximizedChange: function(callback) {
      return listen('window-maximized-changed', function(e) { callback(e.payload); });
    },
    onFullscreenChange: function(callback) {
      return listen('window-fullscreen-changed', function(e) { callback(e.payload); });
    },

    // Streaming server management
    startStreamingServer: function() { return invoke('start_streaming_server'); },
    stopStreamingServer: function() { return invoke('stop_streaming_server'); },
    restartStreamingServer: function() { return invoke('restart_streaming_server'); },
    getStreamingServerStatus: function() { return invoke('get_streaming_server_status'); },
    getNativePlayerStatus: function() { return invoke('get_native_player_status'); },

    // Server event subscriptions
    onServerStarted: function(callback) {
      return listen('server-started', function() { callback(); });
    },
    onServerStopped: function(callback) {
      return listen('server-stopped', function(e) { callback(e.payload); });
    },

    // ============================================
    // Mod Management
    // ============================================
    getPlugins: function() { return invoke('get_plugins'); },
    getThemes: function() { return invoke('get_themes'); },
    downloadMod: function(url, modType) { return invoke('download_mod', { url: url, modType: modType }); },
    deleteMod: function(filename, modType) { return invoke('delete_mod', { filename: filename, modType: modType }); },
    getModContent: function(filename, modType) { return invoke('get_mod_content', { filename: filename, modType: modType }); },
    getRegistry: function() { return invoke('get_registry'); },
    checkModUpdates: function(filename, modType) { return invoke('check_mod_updates', { filename: filename, modType: modType }); },

    // Settings
    getSetting: function(pluginName, key) { return invoke('get_setting', { pluginName: pluginName, key: key }); },
    saveSetting: function(pluginName, key, value) { return invoke('save_setting', { pluginName: pluginName, key: key, value: JSON.stringify(value) }); },
    registerSettings: function(pluginName, schema) { return invoke('register_settings', { pluginName: pluginName, schema: JSON.stringify(schema) }); },
    getRegisteredSettings: function(pluginName) { return invoke('get_registered_settings', { pluginName: pluginName }); },

    // Logging (tagged by plugin name)
    info: function(tag, msg) { console.log('[' + tag + ']', msg); },
    warn: function(tag, msg) { console.warn('[' + tag + ']', msg); },
    error: function(tag, msg) { console.error('[' + tag + ']', msg); },

    // Settings saved callbacks (per-plugin)
    _settingsCallbacks: {},
    onSettingsSaved: function(pluginName, callback) {
      if (!window.StremioEnhancedAPI._settingsCallbacks[pluginName]) {
        window.StremioEnhancedAPI._settingsCallbacks[pluginName] = [];
      }
      window.StremioEnhancedAPI._settingsCallbacks[pluginName].push(callback);
    },
    _notifySettingsSaved: function(pluginName, settings) {
      var cbs = window.StremioEnhancedAPI._settingsCallbacks[pluginName] || [];
      cbs.forEach(function(cb) { try { cb(settings); } catch(e) {} });
    },

    // Theme application
    _themeInlineProps: [],

    _applyInlineThemeProperties: function(css) {
      var root = document.documentElement;
      var props = [];
      var clean = css.replace(/\/\*[\s\S]*?\*\//g, '');
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

    _clearInlineThemeProperties: function() {
      var root = document.documentElement;
      var props = this._themeInlineProps || [];
      for (var i = 0; i < props.length; i++) {
        root.style.removeProperty(props[i]);
      }
      this._themeInlineProps = [];
    },

    applyTheme: function(fileName) {
      window.StremioEnhancedAPI._clearInlineThemeProperties();

      if (fileName === 'Default') {
        var el = document.getElementById('activeTheme');
        if (el) el.remove();
        localStorage.setItem('currentTheme', 'Default');
        window.dispatchEvent(new CustomEvent('sl-theme-changed'));
        return Promise.resolve();
      }
      return invoke('get_mod_content', { filename: fileName, modType: 'theme' }).then(function(css) {
        var el = document.getElementById('activeTheme');
        if (el) el.remove();
        var style = document.createElement('style');
        style.id = 'activeTheme';
        style.textContent = css;
        document.head.appendChild(style);
        localStorage.setItem('currentTheme', fileName);
        window.StremioEnhancedAPI._applyInlineThemeProperties(css);
        window.dispatchEvent(new CustomEvent('sl-theme-changed'));
      });
    },
  };

  // ============================================
  // Auto-load Plugins & Theme
  // ============================================
  function loadEnabledPlugins() {
    var enabled = JSON.parse(localStorage.getItem('enabledPlugins') || '[]');
    enabled.forEach(function(pluginName) {
      if (document.getElementById(pluginName)) return;
      invoke('get_mod_content', { filename: pluginName, modType: 'plugin' }).then(function(content) {
        var baseName = pluginName.replace('.plugin.js', '');
        var wrapped = '(function() {\n' +
          'var StremioEnhancedAPI = {\n' +
          '  logger: {\n' +
          '    info: function(m) { window.StremioEnhancedAPI.info("' + baseName + '", m); },\n' +
          '    warn: function(m) { window.StremioEnhancedAPI.warn("' + baseName + '", m); },\n' +
          '    error: function(m) { window.StremioEnhancedAPI.error("' + baseName + '", m); }\n' +
          '  },\n' +
          '  getSetting: function(k) { return window.StremioEnhancedAPI.getSetting("' + baseName + '", k); },\n' +
          '  saveSetting: function(k, v) { return window.StremioEnhancedAPI.saveSetting("' + baseName + '", k, v); },\n' +
          '  registerSettings: function(s) { return window.StremioEnhancedAPI.registerSettings("' + baseName + '", s); },\n' +
          '  onSettingsSaved: function(cb) { return window.StremioEnhancedAPI.onSettingsSaved("' + baseName + '", cb); }\n' +
          '};\n' +
          'try {\n' + content + '\n} catch(err) { console.error("[ModController] Plugin crashed: ' + pluginName + '", err); }\n' +
          '})();';
        var script = document.createElement('script');
        script.id = pluginName;
        script.textContent = wrapped;
        document.body.appendChild(script);
      }).catch(function(e) {
        console.error('[StremioLightning] Failed to load plugin:', pluginName, e);
      });
    });
  }

  function loadActiveTheme() {
    var theme = localStorage.getItem('currentTheme');
    if (theme && theme !== 'Default') {
      window.StremioEnhancedAPI.applyTheme(theme).catch(function(e) {
        console.error('[StremioLightning] Failed to load theme:', theme, e);
      });
    }
  }

  // Load theme immediately (no delay on refresh) — document.head is
  // available in initialization_script context, so inject the <style> ASAP.
  loadActiveTheme();

  // Load plugins after page is ready (they may depend on DOM)
  if (document.readyState === 'complete') {
    loadEnabledPlugins();
  } else {
    window.addEventListener('load', function() {
      loadEnabledPlugins();
    });
  }

  // ============================================
  // External URL Handling (OAuth, popups, etc.)
  // ============================================
  // Intercept window.open() calls and open them in the system browser
  // instead of creating popup windows inside the webview.
  // This mirrors Electron's setWindowOpenHandler → shell.openExternal.
  window.open = function(url) {
    if (url) {
      invoke('open_external_url', { url: String(url) }).catch(function(e) {
        console.error('[StremioLightning] Failed to open external URL:', url, e);
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
      Object.defineProperty(Navigator.prototype, 'userAgent', {
        get: function() { return originalUA + ' StremioShell/4.4'; },
        configurable: true
      });
    } catch(e) {
      console.warn('[StremioLightning] Could not override userAgent:', e);
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
    return window.location.hash.indexOf('/intro') !== -1;
  }

  function updateBackButton() {
    var btn = document.getElementById('sl-back-btn');
    if (isIntroPage()) {
      if (!btn) injectBackButton();
    } else if (btn) {
      btn.remove();
    }
  }

  function injectBackButton() {
    if (document.getElementById('sl-back-btn')) return;

    var btn = document.createElement('div');
    btn.id = 'sl-back-btn';
    btn.title = 'Go Back';
    btn.innerHTML = '<svg viewBox="0 0 512 512" style="width:20px;height:20px;">' +
      '<path d="M328.6 106.5l-143.5 136.9 143.5 136.9" ' +
      'style="stroke:currentColor;stroke-linecap:round;stroke-linejoin:round;stroke-width:48;fill:none;"></path></svg>';

    var style = document.getElementById('sl-back-btn-style');
    if (!style) {
      style = document.createElement('style');
      style.id = 'sl-back-btn-style';
      style.textContent =
        '#sl-back-btn {' +
          'position:fixed; top:12px; z-index:10000;' +
          'margin-left:max(0rem, calc(1rem - var(--safe-area-inset-left, 0px)));' +
          'cursor:pointer; color:white;' +
          'align-items:center; display:flex; flex:none;' +
          'justify-content:center;' +
          'height:3.5rem; width:3.5rem;' +
          'border-radius:0.75rem; opacity:0.6;' +
          'transition:opacity 0.15s, background 0.15s;' +
        '}' +
        '#sl-back-btn:hover {' +
          'opacity:1; background:rgba(255,255,255,0.08);' +
        '}';
      document.head.appendChild(style);
    }

    btn.addEventListener('click', function() {
      window.history.back();
    });

    document.body.appendChild(btn);
  }

  window.addEventListener('hashchange', updateBackButton);
  if (document.body) {
    updateBackButton();
  } else {
    document.addEventListener('DOMContentLoaded', updateBackButton);
  }

  // ============================================
  // Keyboard Shortcuts
  // ============================================
  var zoomLevel = 1.0;

  function toggleFullscreen() {
    appWindow.isFullscreen().then(function(fs) {
      appWindow.setFullscreen(!fs);
    });
  }

  // ============================================
  // Fullscreen Button Interception
  // ============================================
  // The Stremio web UI has fullscreen buttons with title "Enter fullscreen mode"
  // or "Exit fullscreen mode". We intercept clicks on these to use native fullscreen.
  document.addEventListener('click', function(e) {
    var el = e.target;
    // Walk up from the click target to find the button container
    for (var i = 0; i < 5 && el && el !== document; i++) {
      var title = el.getAttribute && el.getAttribute('title');
      if (title && (title.indexOf('fullscreen') !== -1 || title.indexOf('Fullscreen') !== -1)) {
        e.preventDefault();
        e.stopPropagation();
        toggleFullscreen();
        return;
      }
      el = el.parentElement;
    }
  }, true);

  document.addEventListener('keydown', function(e) {
    // F11: Toggle fullscreen
    if (e.key === 'F11') {
      e.preventDefault();
      toggleFullscreen();
      return;
    }

    // F key: Toggle fullscreen (not when typing in input fields)
    if (e.key === 'f' && !e.ctrlKey && !e.altKey && !e.metaKey && !e.shiftKey) {
      var tag = document.activeElement ? document.activeElement.tagName : '';
      var isInput = tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' ||
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
    if (e.shiftKey && (e.key === 'I' || e.key === 'i')) {
      e.preventDefault();
      invoke('toggle_devtools');
      return;
    }

    // Ctrl+R: Reload page
    if (!e.shiftKey && (e.key === 'r' || e.key === 'R')) {
      e.preventDefault();
      window.location.reload();
      return;
    }

    // Ctrl+= or Ctrl++: Zoom in
    if (e.key === '+' || e.key === '=') {
      e.preventDefault();
      zoomLevel = Math.min(zoomLevel + 0.1, 3.0);
      webview.setZoom(zoomLevel);
      return;
    }

    // Ctrl+-: Zoom out
    if (e.key === '-') {
      e.preventDefault();
      zoomLevel = Math.max(zoomLevel - 0.1, 0.5);
      webview.setZoom(zoomLevel);
      return;
    }
  });
})();
