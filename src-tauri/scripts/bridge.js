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
  };

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
