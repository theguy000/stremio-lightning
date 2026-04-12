// src/lib/ipc.ts
import { invoke } from '@tauri-apps/api/core';
import type { InstalledMod, Registry, UpdateInfo } from './types';

// ── Mod management ──

export function getPlugins(): Promise<InstalledMod[]> {
  return invoke('get_plugins');
}

export function getThemes(): Promise<InstalledMod[]> {
  return invoke('get_themes');
}

export function downloadMod(url: string, modType: string): Promise<string> {
  return invoke('download_mod', { url, modType });
}

export function deleteMod(filename: string, modType: string): Promise<void> {
  return invoke('delete_mod', { filename, modType });
}

export function getModContent(filename: string, modType: string): Promise<string> {
  return invoke('get_mod_content', { filename, modType });
}

export function getRegistry(): Promise<Registry> {
  return invoke('get_registry');
}

export function checkModUpdates(filename: string, modType: string): Promise<UpdateInfo> {
  return invoke('check_mod_updates', { filename, modType });
}

// ── Plugin settings ──

export function getSetting(pluginName: string, key: string): Promise<unknown> {
  return invoke('get_setting', { pluginName, key });
}

export function saveSetting(pluginName: string, key: string, value: string): Promise<void> {
  return invoke('save_setting', { pluginName, key, value });
}

export function registerSettings(pluginName: string, schema: string): Promise<void> {
  return invoke('register_settings', { pluginName, schema });
}

export function getRegisteredSettings(pluginName: string): Promise<unknown> {
  return invoke('get_registered_settings', { pluginName });
}

// ── Discord RPC ──

export function startDiscordRpc(): Promise<void> {
  return invoke('start_discord_rpc');
}

export function stopDiscordRpc(): Promise<void> {
  return invoke('stop_discord_rpc');
}

// ── Auto-pause on unfocus ──

/** Enable or disable the auto-pause-on-unfocus feature on the Rust side.
 *  Persists the setting to `PlayerState::auto_pause_enabled` (AtomicBool)
 *  so the window event callback can read it synchronously without locking. */
export function setAutoPause(enabled: boolean): Promise<void> {
  return invoke('set_auto_pause', { enabled });
}

/** Query whether auto-pause-on-unfocus is currently enabled on the Rust side.
 *  Used on startup (when no localStorage value exists) to sync the frontend
 *  toggle with the Rust backend's default value. */
export function getAutoPause(): Promise<boolean> {
  return invoke('get_auto_pause');
}

/** Enable or disable the "PiP disables auto-pause" setting on the Rust side.
 *  When enabled, auto-pause-on-unfocus is suppressed while PiP is active. */
export function setPipDisablesAutoPause(enabled: boolean): Promise<void> {
  return invoke('set_pip_disables_auto_pause', { enabled });
}

/** Query whether the "PiP disables auto-pause" setting is enabled on the Rust side. */
export function getPipDisablesAutoPause(): Promise<boolean> {
  return invoke('get_pip_disables_auto_pause');
}

// ── Picture-in-Picture ──

/** Toggle Picture-in-Picture mode on the Rust side.
 *  Returns the new PiP state (true = PiP active, false = normal mode). */
export function togglePip(): Promise<boolean> {
  return invoke('toggle_pip');
}

/** Query whether Picture-in-Picture mode is currently active on the Rust side.
 *  Used on startup to sync the frontend toggle with the Rust backend's state. */
export function getPipMode(): Promise<boolean> {
  return invoke('get_pip_mode');
}

// ── Misc ──

export function openExternalUrl(url: string): Promise<void> {
  return invoke('open_external_url', { url });
}
