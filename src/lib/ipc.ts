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

// ── Misc ──

export function openExternalUrl(url: string): Promise<void> {
  return invoke('open_external_url', { url });
}
