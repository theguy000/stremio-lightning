import App from './App.svelte';
import { mount } from 'svelte';
import { hasHost } from './lib/host/host-api';
import { createLogger } from './lib/logging';
import { initPluginAPI } from './lib/plugin-api';
import './app.css';

const logger = createLogger('ui.app');
let hostRetryLogged = false;

function init() {
  try {
    const hostAvailable = hasHost();

    if (!hostAvailable) {
      if (!hostRetryLogged) {
        hostRetryLogged = true;
        logger.warn('Host adapter not available, retrying in 500ms');
      }
      setTimeout(init, 500);
      return;
    }

    if (!document.body) {
      logger.warn('Document body not available, deferring initialization');
      document.addEventListener('DOMContentLoaded', init, { once: true });
      return;
    }

    if (document.getElementById('stremio-lightning-overlay')) {
      return;
    }

    initPluginAPI();

    const target = document.createElement('div');
    target.id = 'stremio-lightning-overlay';
    target.style.cssText = 'display: contents;';
    document.body.appendChild(target);
    mount(App, { target });
    logger.info('Mods UI initialized');
  } catch (e) {
    logger.error('Mods UI initialization failed:', e);
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
