import { writable } from 'svelte/store';
import { startDiscordRpc, stopDiscordRpc, setAutoPause, getAutoPause } from '../ipc';

// Discord RPC
export const discordRpcEnabled = writable(localStorage.getItem('discordrichpresence') === 'true');

export async function toggleDiscordRpc(enabled: boolean): Promise<void> {
  if (enabled) {
    await startDiscordRpc();
    localStorage.setItem('discordrichpresence', 'true');
    document.dispatchEvent(new CustomEvent('sl-discord-rpc-enable'));
    // Notify bridge.js Discord tracker if it exists
    if (typeof (window as any).StremioEnhancedAPI?._discordTrackerInit === 'function') {
      (window as any).StremioEnhancedAPI._discordTrackerInit();
    }
  } else {
    await stopDiscordRpc();
    localStorage.setItem('discordrichpresence', 'false');
    document.dispatchEvent(new CustomEvent('sl-discord-rpc-disable'));
    if (typeof (window as any).StremioEnhancedAPI?._discordTrackerStop === 'function') {
      (window as any).StremioEnhancedAPI._discordTrackerStop();
    }
  }
  discordRpcEnabled.set(enabled);
}

// Blur settings
export const blurEnabled = writable(localStorage.getItem('sl-blur-enabled') !== 'false');
export const blurIntensity = writable(parseInt(localStorage.getItem('sl-blur-intensity') || '100', 10));

export function applyBlurIntensity(percent: number, enabled: boolean): void {
  const root = document.documentElement;
  const blurVal = enabled ? `${16 * (percent / 100)}px` : '0px';
  const blurPanelVal = enabled ? `${30 * (percent / 100)}px` : '0px';

  root.style.setProperty('--sl-blur', blurVal);
  root.style.setProperty('--sl-blur-panel', blurPanelVal);

  if (!enabled) {
    const bg = getComputedStyle(root).getPropertyValue('--primary-background-color').trim() || 'rgba(12, 11, 17, 1)';
    const bg2 = getComputedStyle(root).getPropertyValue('--secondary-background-color').trim() || 'rgba(26, 23, 62, 1)';
    root.style.setProperty('--sl-panel-bg', `linear-gradient(41deg, ${bg} 0%, ${bg2} 100%)`);
  } else {
    root.style.removeProperty('--sl-panel-bg');
  }

  // Force repaint: Chromium doesn't always re-composite backdrop-filter when
  // only CSS custom properties change. Directly set the property on elements.
  const panel = document.getElementById('sl-mod-panel');
  if (panel) {
    panel.style.backdropFilter = `blur(${blurPanelVal}) saturate(135%)`;
    panel.style.setProperty('-webkit-backdrop-filter', `blur(${blurPanelVal}) saturate(135%)`);
  }
  const btn = document.getElementById('sl-mods-btn');
  if (btn) {
    btn.style.backdropFilter = `blur(${blurVal})`;
    btn.style.setProperty('-webkit-backdrop-filter', `blur(${blurVal})`);
  }
  document.querySelectorAll('.sl-card, .sl-search').forEach((el) => {
    (el as HTMLElement).style.backdropFilter = `blur(${blurVal}) saturate(120%)`;
    (el as HTMLElement).style.setProperty('-webkit-backdrop-filter', `blur(${blurVal}) saturate(120%)`);
  });

  localStorage.setItem('sl-blur-intensity', String(percent));
  localStorage.setItem('sl-blur-enabled', String(enabled));
}

// Auto-pause on unfocus
// Svelte writable store backing the "Auto Pause on Unfocus" settings toggle.
// Initialized to `true` (feature enabled by default); the actual value is
// reconciled with localStorage / the Rust backend in `loadSettingsFromStorage`.
export const autoPauseEnabled = writable(true);

/** Toggle the auto-pause-on-unfocus feature.
 *  1. Persists the new state to the Rust backend (AtomicBool) so the window
 *     event callback can read it synchronously.
 *  2. Saves to localStorage so the preference survives app restarts.
 *  3. Updates the Svelte store so the settings UI reflects the change. */
export async function toggleAutoPause(enabled: boolean): Promise<void> {
  await setAutoPause(enabled);
  localStorage.setItem('sl-auto-pause', String(enabled));
  autoPauseEnabled.set(enabled);
}

export function loadSettingsFromStorage(): void {
  const blurEn = localStorage.getItem('sl-blur-enabled') !== 'false';
  const blurInt = parseInt(localStorage.getItem('sl-blur-intensity') || '100', 10);
  blurEnabled.set(blurEn);
  blurIntensity.set(blurInt);
  applyBlurIntensity(blurInt, blurEn);

  // ── Auto-pause on unfocus ──
  // Two-source reconciliation: localStorage is the primary source (survives
  // restarts), but on first launch (no localStorage key) we fall back to the
  // Rust backend's default (which is `true`). In both cases we sync the
  // Rust-side AtomicBool so the window event callback is up to date.
  const stored = localStorage.getItem('sl-auto-pause');
  if (stored !== null) {
    // We have a persisted preference — apply it to both the store and the Rust backend
    const enabled = stored === 'true';
    autoPauseEnabled.set(enabled);
    setAutoPause(enabled).catch(() => {});
  } else {
    // First launch — ask the Rust backend for its default and sync the store
    getAutoPause().then((enabled) => {
      autoPauseEnabled.set(enabled);
    }).catch(() => {});
  }
}

