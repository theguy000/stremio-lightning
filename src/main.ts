import App from './App.svelte';
import { mount } from 'svelte';
import { hasHost } from './lib/host/host-api';
import { initPluginAPI } from './lib/plugin-api';
import './app.css';

console.log('[SL-Svelte] Module loaded, readyState:', document.readyState);

function init() {
  try {
    const hostAvailable = hasHost();
    console.log('[SL-Svelte] init() called, host:', hostAvailable);

    if (!hostAvailable) {
      console.warn('[SL-Svelte] host adapter not available, retrying in 500ms...');
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
