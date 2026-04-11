import App from './App.svelte';
import { mount } from 'svelte';
import { initPluginAPI } from './lib/plugin-api';
import './app.css';

console.log('[SL-Svelte] Module loaded, readyState:', document.readyState);

function init() {
  try {
    console.log('[SL-Svelte] init() called, __TAURI__:', !!(window as any).__TAURI__);

    if (!(window as any).__TAURI__) {
      console.warn('[SL-Svelte] __TAURI__ not available, retrying in 500ms...');
      setTimeout(init, 500);
      return;
    }

    if (!document.body) {
      console.warn('[SL-Svelte] document.body not available, deferring...');
      document.addEventListener('DOMContentLoaded', init, { once: true });
      return;
    }

    if (document.getElementById('stremio-lightning-overlay')) {
      console.log('[SL-Svelte] Already initialized, skipping');
      return;
    }

    initPluginAPI();
    console.log('[SL-Svelte] Plugin API initialized');

    const target = document.createElement('div');
    target.id = 'stremio-lightning-overlay';
    target.style.cssText = 'display: contents;';
    document.body.appendChild(target);
    mount(App, { target });
    console.log('[SL-Svelte] App mounted');
  } catch (e) {
    console.error('[SL-Svelte] init() error:', e);
  }
}

// Try immediately
if (document.body) {
  init();
}

// Also listen for DOMContentLoaded
document.addEventListener('DOMContentLoaded', () => {
  if (!document.getElementById('stremio-lightning-overlay')) {
    init();
  }
}, { once: true });

// Safety fallback
window.addEventListener('load', () => {
  if (!document.getElementById('stremio-lightning-overlay')) {
    init();
  }
}, { once: true });
