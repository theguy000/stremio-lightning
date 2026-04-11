import App from './App.svelte';
import { mount } from 'svelte';

function init() {
  const target = document.createElement('div');
  target.id = 'stremio-lightning-overlay';
  document.body.appendChild(target);
  mount(App, { target });
}

if (document.body) {
  init();
} else {
  document.addEventListener('DOMContentLoaded', init);
}
