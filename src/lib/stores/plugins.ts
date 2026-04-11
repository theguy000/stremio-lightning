import { writable } from 'svelte/store';
import { getPlugins, getModContent, getRegisteredSettings, checkModUpdates } from '../ipc';
import type { InstalledMod, UpdateInfo } from '../types';

export const plugins = writable<InstalledMod[]>([]);
export const enabledPlugins = writable<string[]>([]);

export async function refreshPlugins() {
  const list = await getPlugins();
  plugins.set(list);
}

export function loadEnabledFromStorage(): string[] {
  try {
    const stored = localStorage.getItem('enabledPlugins');
    const parsed = stored ? JSON.parse(stored) : [];
    enabledPlugins.set(parsed);
    return parsed;
  } catch {
    enabledPlugins.set([]);
    return [];
  }
}

export function saveEnabledToStorage(enabled: string[]) {
  localStorage.setItem('enabledPlugins', JSON.stringify(enabled));
  enabledPlugins.set(enabled);
}

export async function loadPlugin(pluginName: string): Promise<void> {
  const content = await getModContent(pluginName, 'plugin');
  const baseName = pluginName.replace('.plugin.js', '');

  // Wrap in IIFE with scoped API — mirrors current bridge.js plugin loading
  const wrapped = `(function() {
    var _api = window.StremioEnhancedAPI;
    var StremioEnhancedAPI = {
      logger: {
        info: function(m) { _api.info('${baseName}', m); },
        warn: function(m) { _api.warn('${baseName}', m); },
        error: function(m) { _api.error('${baseName}', m); }
      },
      info: function() { _api.info.apply(_api, arguments); },
      warn: function() { _api.warn.apply(_api, arguments); },
      error: function() { _api.error.apply(_api, arguments); },
      getSetting: function(key) { return _api.getSetting('${baseName}', key); },
      saveSetting: function(key, val) { return _api.saveSetting('${baseName}', key, val); },
      registerSettings: function(schema) { return _api.registerSettings('${baseName}', schema); },
      onSettingsSaved: function(cb) { _api._registerSettingsCallback('${baseName}', cb); },
      minimizeWindow: function() { return _api.minimizeWindow(); },
      maximizeWindow: function() { return _api.maximizeWindow(); },
      closeWindow: function() { return _api.closeWindow(); },
      isMaximized: function() { return _api.isMaximized(); },
      isFullscreen: function() { return _api.isFullscreen(); },
      dragWindow: function() { return _api.dragWindow(); },
      startStreamingServer: function() { return _api.startStreamingServer(); },
      stopStreamingServer: function() { return _api.stopStreamingServer(); },
      restartStreamingServer: function() { return _api.restartStreamingServer(); },
      getPlugins: function() { return _api.getPlugins(); },
      getThemes: function() { return _api.getThemes(); },
      downloadMod: function(u,t) { return _api.downloadMod(u,t); },
      deleteMod: function(f,t) { return _api.deleteMod(f,t); },
      checkModUpdates: function(f,t) { return _api.checkModUpdates(f,t); },
      getModContent: function(f,t) { return _api.getModContent(f,t); },
      getRegistry: function() { return _api.getRegistry(); },
      applyTheme: function(f) { return _api.applyTheme(f); },
      onMaximizedChange: function(cb) { return _api.onMaximizedChange(cb); },
      onFullscreenChange: function(cb) { return _api.onFullscreenChange(cb); },
      onServerStarted: function(cb) { return _api.onServerStarted(cb); },
      onServerStopped: function(cb) { return _api.onServerStopped(cb); }
    };
    try {
      ${content}
    } catch(e) {
      console.error('[${baseName}] Plugin error:', e);
    }
  })();`;

  // Inject as script element
  const existing = document.getElementById(pluginName);
  if (existing) existing.remove();

  const script = document.createElement('script');
  script.id = pluginName;
  script.textContent = wrapped;
  document.body.appendChild(script);

  // Update enabled list
  const stored = loadEnabledFromStorage();
  if (!stored.includes(pluginName)) {
    saveEnabledToStorage([...stored, pluginName]);
  }
}

export function unloadPlugin(pluginName: string): void {
  const el = document.getElementById(pluginName);
  if (el) el.remove();

  const stored = loadEnabledFromStorage();
  saveEnabledToStorage(stored.filter((p) => p !== pluginName));
}

export async function checkPluginUpdate(filename: string): Promise<UpdateInfo> {
  const modType = filename.endsWith('.theme.css') ? 'theme' : 'plugin';
  return checkModUpdates(filename, modType);
}

export async function getPluginSchema(pluginName: string): Promise<unknown> {
  const baseName = pluginName.replace('.plugin.js', '');
  return getRegisteredSettings(baseName);
}
