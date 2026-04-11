// src/lib/plugin-api.ts
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { applyTheme } from './stores/themes';

type SettingsCallback = (values: Record<string, unknown>) => void;

const _settingsCallbacks: Record<string, SettingsCallback> = {};

export function initPluginAPI(): void {
  const appWindow = getCurrentWindow();

  const api = {
    // ── Window management ──
    minimizeWindow: () => appWindow.minimize(),
    maximizeWindow: () => appWindow.toggleMaximize(),
    closeWindow: () => appWindow.close(),
    isMaximized: () => appWindow.isMaximized(),
    isFullscreen: () => appWindow.isFullscreen(),
    dragWindow: () => appWindow.startDragging(),

    // ── Streaming server ──
    startStreamingServer: (): Promise<void> => invoke('start_streaming_server'),
    stopStreamingServer: (): Promise<void> => invoke('stop_streaming_server'),
    restartStreamingServer: (): Promise<void> => invoke('restart_streaming_server'),
    getStreamingServerStatus: (): Promise<boolean> => invoke('get_streaming_server_status'),

    // ── Native player ──
    getNativePlayerStatus: (): Promise<unknown> => invoke('get_native_player_status'),

    // ── Mod management ──
    getPlugins: (): Promise<unknown> => invoke('get_plugins'),
    getThemes: (): Promise<unknown> => invoke('get_themes'),
    downloadMod: (url: string, modType: string): Promise<unknown> =>
      invoke('download_mod', { url, modType }),
    deleteMod: (filename: string, modType: string): Promise<void> =>
      invoke('delete_mod', { filename, modType }),
    checkModUpdates: (filename: string, modType: string): Promise<unknown> =>
      invoke('check_mod_updates', { filename, modType }),
    getModContent: (filename: string, modType: string): Promise<string> =>
      invoke('get_mod_content', { filename, modType }),
    getRegistry: (): Promise<unknown> => invoke('get_registry'),
    getRegisteredSettings: (pluginName: string): Promise<unknown> =>
      invoke('get_registered_settings', { pluginName }),

    // ── Settings ──
    getSetting: (pluginName: string, key: string): Promise<unknown> =>
      invoke('get_setting', { pluginName, key }),
    saveSetting: (pluginName: string, key: string, value: unknown): Promise<void> =>
      invoke('save_setting', { pluginName, key, value: JSON.stringify(value) }),
    registerSettings: (pluginName: string, schema: unknown): Promise<void> =>
      invoke('register_settings', { pluginName, schema: JSON.stringify(schema) }),

    // ── Theme ──
    applyTheme: (filename: string) => applyTheme(filename),

    // ── App updates ──
    checkAppUpdate: (): Promise<unknown> => invoke('check_app_update'),

    // ── Logging ──
    info: (...args: unknown[]) => console.info('[StremioEnhanced]', ...args),
    warn: (...args: unknown[]) => console.warn('[StremioEnhanced]', ...args),
    error: (...args: unknown[]) => console.error('[StremioEnhanced]', ...args),

    // ── Event subscriptions ──
    onMaximizedChange: (cb: (maximized: boolean) => void): Promise<UnlistenFn> =>
      listen<boolean>('window-maximized-changed', (e) => cb(e.payload)),
    onFullscreenChange: (cb: (fullscreen: boolean) => void): Promise<UnlistenFn> =>
      listen<boolean>('window-fullscreen-changed', (e) => cb(e.payload)),
    onServerStarted: (cb: () => void): Promise<UnlistenFn> =>
      listen('server-started', () => cb()),
    onServerStopped: (cb: () => void): Promise<UnlistenFn> =>
      listen('server-stopped', () => cb()),

    // ── Plugin settings callbacks ──
    _settingsCallbacks: _settingsCallbacks,
    onSettingsSaved: (pluginName: string, cb: SettingsCallback) => {
      _settingsCallbacks[pluginName] = cb;
    },
    _registerSettingsCallback: (pluginName: string, cb: SettingsCallback) => {
      _settingsCallbacks[pluginName] = cb;
    },
    _notifySettingsSaved: (pluginName: string, values: Record<string, unknown>) => {
      _settingsCallbacks[pluginName]?.(values);
    },

    // ── Discord tracker hooks (set by bridge.js Discord RPC tracker) ──
    _discordTrackerInit: null as (() => void) | null,
    _discordTrackerStop: null as (() => void) | null,

    // ── Theme inline props tracking (shared with themes.ts via window.__slThemeInlineProps) ──
    _themeInlineProps: (window as any).__slThemeInlineProps || [] as string[],
    _applyInlineThemeProperties: function (css: string) {
      const root = document.documentElement;
      const clean = css.replace(/\/\*[\s\S]*?\*\//g, '');
      const regex = /(--[\w-]+)\s*:\s*([^;!}]+)/g;
      let match;
      while ((match = regex.exec(clean)) !== null) {
        const name = match[1].trim();
        const value = match[2].trim();
        if (value) {
          root.style.setProperty(name, value);
          api._themeInlineProps.push(name);
        }
      }
    },
    _clearInlineThemeProperties: function () {
      const root = document.documentElement;
      const props = api._themeInlineProps || [];
      for (let i = 0; i < props.length; i++) {
        root.style.removeProperty(props[i]);
      }
      api._themeInlineProps.length = 0;
    },
  };

  // Preserve any hooks bridge.js already attached to the stub
  // (e.g., _discordTrackerInit, _discordTrackerStop from the Discord RPC tracker)
  const existingStub = (window as any).StremioEnhancedAPI;
  if (existingStub && typeof existingStub === 'object') {
    for (const key of Object.keys(existingStub)) {
      if (!(key in api)) {
        (api as any)[key] = existingStub[key];
      }
    }
  }

  (window as any).StremioEnhancedAPI = api;
}
