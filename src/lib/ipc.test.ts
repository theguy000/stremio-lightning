import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { HostCommand, StremioLightningHost } from './host/host-api';

let invoke: ReturnType<typeof vi.fn>;

type IpcMethod = keyof typeof import('./ipc');
type IpcTestCase = [method: IpcMethod, command: HostCommand, args: unknown[]];

const ipcTestCases: IpcTestCase[] = [
  ['getPlugins', 'get_plugins', []],
  ['getThemes', 'get_themes', []],
  ['downloadMod', 'download_mod', ['https://example.test/mod.js', 'plugin']],
  ['deleteMod', 'delete_mod', ['a.plugin.js', 'plugin']],
  ['getModContent', 'get_mod_content', ['a.plugin.js', 'plugin']],
  ['getRegistry', 'get_registry', []],
  ['checkModUpdates', 'check_mod_updates', ['a.plugin.js', 'plugin']],
  ['getSetting', 'get_setting', ['plugin-a', 'enabled']],
  ['saveSetting', 'save_setting', ['plugin-a', 'enabled', 'true']],
  ['registerSettings', 'register_settings', ['plugin-a', '{"settings":[]}']],
  ['getRegisteredSettings', 'get_registered_settings', ['plugin-a']],
  ['startDiscordRpc', 'start_discord_rpc', []],
  ['stopDiscordRpc', 'stop_discord_rpc', []],
  ['setAutoPause', 'set_auto_pause', [true]],
  ['getAutoPause', 'get_auto_pause', []],
  ['setPipDisablesAutoPause', 'set_pip_disables_auto_pause', [false]],
  ['getPipDisablesAutoPause', 'get_pip_disables_auto_pause', []],
  ['togglePip', 'toggle_pip', []],
  ['getPipMode', 'get_pip_mode', []],
  ['openExternalUrl', 'open_external_url', ['https://example.test']],
];

function expectedPayload(command: HostCommand, args: unknown[]): unknown {
  const payloads: Partial<Record<HostCommand, unknown>> = {
    download_mod: { url: args[0], modType: args[1] },
    delete_mod: { filename: args[0], modType: args[1] },
    get_mod_content: { filename: args[0], modType: args[1] },
    check_mod_updates: { filename: args[0], modType: args[1] },
    get_setting: { pluginName: args[0], key: args[1] },
    save_setting: { pluginName: args[0], key: args[1], value: args[2] },
    register_settings: { pluginName: args[0], schema: args[1] },
    get_registered_settings: { pluginName: args[0] },
    set_auto_pause: { enabled: args[0] },
    set_pip_disables_auto_pause: { enabled: args[0] },
    open_external_url: { url: args[0] },
  };

  return payloads[command];
}

beforeEach(() => {
  invoke = vi.fn().mockResolvedValue('ok');
  window.StremioLightningHost = {
    invoke: invoke as StremioLightningHost['invoke'],
    listen: vi.fn().mockResolvedValue(() => {}) as StremioLightningHost['listen'],
    window: {
      minimize: vi.fn(),
      toggleMaximize: vi.fn(),
      close: vi.fn(),
      isMaximized: vi.fn(),
      isFullscreen: vi.fn(),
      setFullscreen: vi.fn(),
      startDragging: vi.fn(),
    },
    webview: {
      setZoom: vi.fn(),
    },
  };
});

afterEach(() => {
  delete window.StremioLightningHost;
});

describe('ipc host wrapper', () => {
  it.each(ipcTestCases)('%s invokes %s with the expected payload', async (method, command, args) => {
    const ipc = await import('./ipc');
    await (ipc[method as keyof typeof ipc] as (...args: unknown[]) => Promise<unknown>)(...args);

    const payload = expectedPayload(command, args);
    if (payload !== undefined) {
      expect(invoke).toHaveBeenCalledWith(command, payload);
    } else {
      expect(invoke).toHaveBeenCalledWith(command);
    }
  });
});
