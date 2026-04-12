import { writable, get } from 'svelte/store';
import { startDiscordRpc, stopDiscordRpc, setAutoPause, getAutoPause, setPipDisablesAutoPause, getPipDisablesAutoPause, togglePip, getPipMode } from '../ipc';

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
const _blurIntRaw = parseInt(localStorage.getItem('sl-blur-intensity') || '100', 10);
export const blurIntensity = writable(isNaN(_blurIntRaw) ? 100 : _blurIntRaw);

function adjustAlpha(color: string, alpha: number): string {
  const match = color.match(/rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)/);
  if (match) return `rgba(${match[1]}, ${match[2]}, ${match[3]}, ${alpha})`;
  return `rgba(0, 0, 0, ${alpha})`;
}

export function applyBlurIntensity(percent: number, enabled: boolean): void {
  const root = document.documentElement;
  const blurVal = enabled ? `${16 * (percent / 100)}px` : '0px';
  const blurPanelVal = enabled ? `${30 * (percent / 100)}px` : '0px';

  root.style.setProperty('--sl-blur', blurVal);
  root.style.setProperty('--sl-blur-panel', blurPanelVal);

  const bg = root.style.getPropertyValue('--primary-background-color').trim()
    || getComputedStyle(root).getPropertyValue('--primary-background-color').trim()
    || 'rgba(12, 11, 17, 1)';
  const bg2 = root.style.getPropertyValue('--secondary-background-color').trim()
    || getComputedStyle(root).getPropertyValue('--secondary-background-color').trim()
    || 'rgba(26, 23, 62, 1)';

  if (!enabled) {
    root.style.setProperty('--sl-panel-bg', `linear-gradient(41deg, ${bg} 0%, ${bg2} 100%)`);
  } else {
    root.style.setProperty('--sl-panel-bg', `linear-gradient(180deg, ${adjustAlpha(bg, 0.28)} 0%, ${adjustAlpha(bg2, 0.16)} 16%, ${adjustAlpha(bg2, 0.12)} 100%)`);
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

// PiP disables auto-pause
export const pipDisablesAutoPause = writable(true);

/** Toggle the "PiP disables auto-pause" setting.
 *  When enabled, auto-pause-on-unfocus is suppressed while PiP is active. */
export async function togglePipDisablesAutoPause(enabled: boolean): Promise<void> {
  await setPipDisablesAutoPause(enabled);
  localStorage.setItem('sl-pip-disables-auto-pause', String(enabled));
  pipDisablesAutoPause.set(enabled);
}

export function loadSettingsFromStorage(): void {
  const blurEn = localStorage.getItem('sl-blur-enabled') !== 'false';
  const blurIntRaw = parseInt(localStorage.getItem('sl-blur-intensity') || '100', 10);
  const blurInt = isNaN(blurIntRaw) ? 100 : blurIntRaw;
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

  // ── PiP disables auto-pause ──
  const pipPauseStored = localStorage.getItem('sl-pip-disables-auto-pause');
  if (pipPauseStored !== null) {
    const enabled = pipPauseStored === 'true';
    pipDisablesAutoPause.set(enabled);
    setPipDisablesAutoPause(enabled).catch(() => {});
  } else {
    getPipDisablesAutoPause().then((enabled) => {
      pipDisablesAutoPause.set(enabled);
    }).catch(() => {});
  }

  // ── Picture-in-Picture ──
  // PiP feature preference: persisted to localStorage, controls whether
  // the PiP button appears in the player control bar.
  const pipStored = localStorage.getItem('sl-pip-feature');
  if (pipStored !== null) {
    pipFeatureEnabled.set(pipStored === 'true');
  } else {
    pipFeatureEnabled.set(true); // enabled by default
  }

  // Sync runtime PiP state with the Rust backend on startup
  getPipMode().then((active) => {
    pipModeActive.set(active);
  }).catch(() => {});
}

export const pipFeatureEnabled = writable(true);

/** Toggle the PiP feature preference (whether the PiP button is shown).
 *  This does NOT activate PiP — it only controls whether the feature is available. */
export function togglePipFeature(enabled: boolean): void {
  localStorage.setItem('sl-pip-feature', String(enabled));
  pipFeatureEnabled.set(enabled);
  // If disabling the feature while PiP is active, exit PiP
  if (!enabled && get(pipModeActive)) {
    togglePipActivation().catch(() => {});
  }
  // Notify bridge.js so the injected PiP button can show/hide
  document.dispatchEvent(new CustomEvent('sl-pip-feature-changed', { detail: enabled }));
}

export const pipModeActive = writable(false);

/** Activate or deactivate Picture-in-Picture mode.
 *  Calls the Rust backend to toggle the window state (borderless, always-on-top).
 *  Only works when the player is active. Updates the runtime store with the
 *  new state returned by Rust. */
export async function togglePipActivation(): Promise<void> {
  try {
    const newState = await togglePip();
    pipModeActive.set(newState);
  } catch (err) {
    console.warn('PiP toggle failed (player may not be active):', err);
    pipModeActive.set(false);
  }
}

// Listen for PiP events dispatched by bridge.js (from shell transport or keyboard shortcut)
// so the runtime store stays in sync regardless of how PiP was toggled.
if (typeof document !== 'undefined') {
  document.addEventListener('sl-pip-enabled', () => pipModeActive.set(true));
  document.addEventListener('sl-pip-disabled', () => pipModeActive.set(false));
}

