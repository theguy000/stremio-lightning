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
export const autoPauseEnabled = writable(true);

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

  // Auto-pause: load persisted preference, fallback to Rust backend default
  const stored = localStorage.getItem('sl-auto-pause');
  if (stored !== null) {
    const enabled = stored === 'true';
    autoPauseEnabled.set(enabled);
    setAutoPause(enabled).catch(() => {});
  } else {
    getAutoPause().then((enabled) => {
      autoPauseEnabled.set(enabled);
    }).catch(() => {});
  }
}

