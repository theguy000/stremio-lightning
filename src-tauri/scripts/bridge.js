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

    // Server event subscriptions
    onServerStarted: function(callback) {
      return listen('server-started', function() { callback(); });
    },
    onServerStopped: function(callback) {
      return listen('server-stopped', function(e) { callback(e.payload); });
    },
  };

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
  try {
    var originalUA = navigator.userAgent;
    Object.defineProperty(Navigator.prototype, 'userAgent', {
      get: function() { return originalUA + ' StremioShell/4.4'; },
      configurable: true
    });
  } catch(e) {
    console.warn('[StremioLightning] Could not override userAgent:', e);
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

  document.addEventListener('keydown', function(e) {
    // F11: Toggle fullscreen
    if (e.key === 'F11') {
      e.preventDefault();
      appWindow.isFullscreen().then(function(fs) {
        appWindow.setFullscreen(!fs);
      });
      return;
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
