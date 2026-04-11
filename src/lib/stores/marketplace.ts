import { writable } from 'svelte/store';
import { getRegistry, getPlugins, getThemes, downloadMod, deleteMod } from '../ipc';
import type { Registry, RegistryEntry, InstalledMod } from '../types';

export const registry = writable<Registry | null>(null);
export const installedPlugins = writable<InstalledMod[]>([]);
export const installedThemes = writable<InstalledMod[]>([]);
export const marketplaceLoading = writable(true);
export const searchQuery = writable('');

export async function refreshMarketplace(): Promise<void> {
  marketplaceLoading.set(true);
  try {
    const [reg, plugins, themes] = await Promise.all([
      getRegistry(),
      getPlugins(),
      getThemes(),
    ]);
    registry.set(reg);
    installedPlugins.set(plugins);
    installedThemes.set(themes);
  } catch (e) {
    console.error('Failed to load marketplace:', e);
  } finally {
    marketplaceLoading.set(false);
  }
}

export async function installMod(entry: RegistryEntry, type: 'plugin' | 'theme'): Promise<void> {
  await downloadMod(entry.download, type);
  await refreshMarketplace();
}

export async function uninstallMod(filename: string, type: 'plugin' | 'theme'): Promise<void> {
  await deleteMod(filename, type);

  // If it was a plugin, remove from enabled list
  if (type === 'plugin') {
    const el = document.getElementById(filename);
    if (el) el.remove();
    try {
      const stored = JSON.parse(localStorage.getItem('enabledPlugins') || '[]');
      localStorage.setItem('enabledPlugins', JSON.stringify(stored.filter((p: string) => p !== filename)));
    } catch { /* ignore */ }
  }

  // If it was the current theme, revert to default
  if (type === 'theme' && localStorage.getItem('currentTheme') === filename) {
    localStorage.removeItem('currentTheme');
    const prev = document.getElementById('activeTheme');
    if (prev) prev.remove();
  }

  await refreshMarketplace();
}

export function isInstalled(downloadUrl: string, installed: InstalledMod[]): InstalledMod | undefined {
  const urlFilename = (downloadUrl.split('/').pop() || '').split('?')[0];
  return installed.find((m) => m.filename === urlFilename);
}
