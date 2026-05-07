import type { InstalledMod, Registry, UpdateInfo } from '../types';

export type DiscordActivity = Record<string, unknown>;

export type AppUpdateInfo = {
  hasUpdate: boolean;
  currentVersion: string;
  newVersion?: string;
  releaseUrl?: string;
};

type HostCommandMap = {
  toggle_devtools: { payload: undefined; result: void };
  open_external_url: { payload: { url: string }; result: void };
  shell_transport_send: { payload: { message: string }; result: void };
  shell_bridge_ready: { payload: undefined; result: void };
  get_native_player_status: { payload: undefined; result: unknown };
  start_streaming_server: { payload: undefined; result: void };
  stop_streaming_server: { payload: undefined; result: void };
  restart_streaming_server: { payload: undefined; result: void };
  get_streaming_server_status: { payload: undefined; result: boolean };
  get_plugins: { payload: undefined; result: InstalledMod[] };
  get_themes: { payload: undefined; result: InstalledMod[] };
  download_mod: { payload: { url: string; modType: string }; result: string };
  delete_mod: { payload: { filename: string; modType: string }; result: void };
  get_mod_content: { payload: { filename: string; modType: string }; result: string };
  get_registry: { payload: undefined; result: Registry };
  check_mod_updates: { payload: { filename: string; modType: string }; result: UpdateInfo };
  get_setting: { payload: { pluginName: string; key: string }; result: unknown };
  save_setting: { payload: { pluginName: string; key: string; value: string }; result: void };
  register_settings: { payload: { pluginName: string; schema: string }; result: void };
  get_registered_settings: { payload: { pluginName: string }; result: unknown };
  start_discord_rpc: { payload: undefined; result: void };
  stop_discord_rpc: { payload: undefined; result: void };
  update_discord_activity: { payload: { activity: DiscordActivity }; result: void };
  check_app_update: { payload: undefined; result: AppUpdateInfo };
  set_auto_pause: { payload: { enabled: boolean }; result: void };
  get_auto_pause: { payload: undefined; result: boolean };
  set_pip_disables_auto_pause: { payload: { enabled: boolean }; result: void };
  get_pip_disables_auto_pause: { payload: undefined; result: boolean };
  toggle_pip: { payload: undefined; result: boolean };
  get_pip_mode: { payload: undefined; result: boolean };
};

export type HostCommand = keyof HostCommandMap;
export type HostCommandPayload<C extends HostCommand> = HostCommandMap[C]['payload'];
export type HostCommandResult<C extends HostCommand> = HostCommandMap[C]['result'];

export type HostEventMap = {
  'window-maximized-changed': boolean;
  'window-fullscreen-changed': boolean;
  'server-started': undefined;
  'server-stopped': undefined;
  'shell-transport-message': string;
};

export type HostEvent = keyof HostEventMap;
export type HostEventPayload<E extends HostEvent> = HostEventMap[E];
export type HostUnlistenFn = () => void;
export type HostEventCallback<E extends HostEvent> = (event: {
  event: E;
  payload: HostEventPayload<E>;
}) => void;

type Invoke = {
  <C extends HostCommand>(
    command: C,
    ...args: HostCommandPayload<C> extends undefined ? [] : [payload: HostCommandPayload<C>]
  ): Promise<HostCommandResult<C>>;
};

type Listen = {
  <E extends HostEvent>(
    event: E,
    callback: HostEventCallback<E>,
  ): Promise<HostUnlistenFn>;
};

export interface StremioLightningHost {
  invoke: Invoke;
  listen: Listen;
  window: {
    minimize(): Promise<void>;
    toggleMaximize(): Promise<void>;
    close(): Promise<void>;
    isMaximized(): Promise<boolean>;
    isFullscreen(): Promise<boolean>;
    setFullscreen(fullscreen: boolean): Promise<void>;
    startDragging(): Promise<void>;
  };
  webview: {
    setZoom(level: number): Promise<void>;
  };
}

declare global {
  interface Window {
    StremioLightningHost?: StremioLightningHost;
  }
}

let missingHostLogged = false;

function logMissingHost(): void {
  if (missingHostLogged) return;
  missingHostLogged = true;
  console.error('[StremioLightning] host adapter not available');
}

export function hasHost(): boolean {
  const available = typeof window !== 'undefined' && !!window.StremioLightningHost;
  if (!available) logMissingHost();
  return available;
}

export function getHost(): StremioLightningHost {
  if (typeof window !== 'undefined' && window.StremioLightningHost) {
    return window.StremioLightningHost;
  }

  logMissingHost();
  throw new Error('Stremio Lightning host adapter is not available');
}
