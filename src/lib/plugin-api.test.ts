import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { HostCommand, HostEvent, StremioLightningHost } from './host/host-api';

let invoke: ReturnType<typeof vi.fn>;
let listen: ReturnType<typeof vi.fn>;
let hostWindow: StremioLightningHost['window'];

type PluginApi = Record<string, unknown> & {
  StremioLightningHost?: unknown;
};
type PluginApiTestCase = [method: string, command: HostCommand, args: unknown[]];
type PluginApiEventTestCase = [method: string, event: HostEvent, payload?: unknown];

const pluginApiInvokeCases: PluginApiTestCase[] = [
  ['startStreamingServer', 'start_streaming_server', []],
  ['stopStreamingServer', 'stop_streaming_server', []],
  ['restartStreamingServer', 'restart_streaming_server', []],
  ['getStreamingServerStatus', 'get_streaming_server_status', []],
  ['getNativePlayerStatus', 'get_native_player_status', []],
  ['getPlugins', 'get_plugins', []],
  ['getThemes', 'get_themes', []],
  ['downloadMod', 'download_mod', ['https://example.test/mod.js', 'plugin']],
  ['deleteMod', 'delete_mod', ['a.plugin.js', 'plugin']],
  ['checkModUpdates', 'check_mod_updates', ['a.plugin.js', 'plugin']],
  ['getModContent', 'get_mod_content', ['a.plugin.js', 'plugin']],
  ['getRegistry', 'get_registry', []],
  ['getRegisteredSettings', 'get_registered_settings', ['plugin-a']],
  ['getSetting', 'get_setting', ['plugin-a', 'enabled']],
  ['checkAppUpdate', 'check_app_update', []],
];

const payloadEventCases: PluginApiEventTestCase[] = [
  ['onMaximizedChange', 'window-maximized-changed', true],
  ['onFullscreenChange', 'window-fullscreen-changed', false],
];

const emptyEventCases: PluginApiEventTestCase[] = [
  ['onServerStarted', 'server-started'],
  ['onServerStopped', 'server-stopped'],
];

function callApiMethod(api: PluginApi, method: string, args: unknown[] = []): Promise<unknown> {
  return (api[method] as (...args: unknown[]) => Promise<unknown>)(...args);
}

function expectedPayload(command: HostCommand, args: unknown[]): unknown {
  const payloads: Partial<Record<HostCommand, unknown>> = {
    download_mod: { url: args[0], modType: args[1] },
    delete_mod: { filename: args[0], modType: args[1] },
    check_mod_updates: { filename: args[0], modType: args[1] },
    get_mod_content: { filename: args[0], modType: args[1] },
    get_registered_settings: { pluginName: args[0] },
    get_setting: { pluginName: args[0], key: args[1] },
  };

  return payloads[command];
}

beforeEach(() => {
  invoke = vi.fn().mockResolvedValue('ok');
  listen = vi.fn().mockResolvedValue(() => {});
  hostWindow = {
    minimize: vi.fn().mockResolvedValue(undefined),
    toggleMaximize: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
    isMaximized: vi.fn().mockResolvedValue(false),
    isFullscreen: vi.fn().mockResolvedValue(false),
    setFullscreen: vi.fn().mockResolvedValue(undefined),
    startDragging: vi.fn().mockResolvedValue(undefined),
  };
  window.StremioLightningHost = {
    invoke: invoke as StremioLightningHost['invoke'],
    listen: listen as StremioLightningHost['listen'],
    window: hostWindow,
    webview: {
      setZoom: vi.fn().mockResolvedValue(undefined),
    },
  };
});

afterEach(() => {
  delete window.StremioLightningHost;
  delete (window as unknown as { StremioEnhancedAPI?: unknown }).StremioEnhancedAPI;
  vi.resetModules();
});

async function initApi() {
  const { initPluginAPI } = await import('./plugin-api');
  initPluginAPI();
  return (window as unknown as { StremioEnhancedAPI: PluginApi }).StremioEnhancedAPI;
}

describe('plugin API contract', () => {
  it('keeps the plugin-facing method names stable', async () => {
    const api = await initApi();

    expect(Object.keys(api).sort()).toMatchInlineSnapshot(`
      [
        "_applyInlineThemeProperties",
        "_clearInlineThemeProperties",
        "_discordTrackerInit",
        "_discordTrackerStop",
        "_notifySettingsSaved",
        "_registerSettingsCallback",
        "_settingsCallbacks",
        "_themeInlineProps",
        "applyTheme",
        "checkAppUpdate",
        "checkModUpdates",
        "closeWindow",
        "deleteMod",
        "downloadMod",
        "dragWindow",
        "error",
        "getModContent",
        "getNativePlayerStatus",
        "getPlugins",
        "getRegisteredSettings",
        "getRegistry",
        "getSetting",
        "getStreamingServerStatus",
        "getThemes",
        "info",
        "isFullscreen",
        "isMaximized",
        "maximizeWindow",
        "minimizeWindow",
        "onFullscreenChange",
        "onMaximizedChange",
        "onServerStarted",
        "onServerStopped",
        "onSettingsSaved",
        "registerSettings",
        "restartStreamingServer",
        "saveSetting",
        "startStreamingServer",
        "stopStreamingServer",
        "warn",
      ]
    `);
    expect(api.StremioLightningHost).toBeUndefined();
  });

  it.each(pluginApiInvokeCases)('%s invokes %s', async (method, command, args) => {
    const api = await initApi();
    await callApiMethod(api, method, args);

    const payload = expectedPayload(command, args);
    if (payload !== undefined) {
      expect(invoke).toHaveBeenCalledWith(command, payload);
    } else {
      expect(invoke).toHaveBeenCalledWith(command);
    }
  });

  it('stringifies settings values and schemas before invoking native commands', async () => {
    const api = await initApi();

    await callApiMethod(api, 'saveSetting', ['plugin-a', 'enabled', true]);
    await callApiMethod(api, 'registerSettings', ['plugin-a', [{ key: 'enabled', type: 'toggle' }]]);

    expect(invoke).toHaveBeenCalledWith('save_setting', {
      pluginName: 'plugin-a',
      key: 'enabled',
      value: 'true',
    });
    expect(invoke).toHaveBeenCalledWith('register_settings', {
      pluginName: 'plugin-a',
      schema: '[{"key":"enabled","type":"toggle"}]',
    });
  });

  it('routes window helpers through the host window facade', async () => {
    const api = await initApi();

    await callApiMethod(api, 'minimizeWindow');
    await callApiMethod(api, 'maximizeWindow');
    await callApiMethod(api, 'closeWindow');
    await callApiMethod(api, 'isMaximized');
    await callApiMethod(api, 'isFullscreen');
    await callApiMethod(api, 'dragWindow');

    expect(hostWindow.minimize).toHaveBeenCalled();
    expect(hostWindow.toggleMaximize).toHaveBeenCalled();
    expect(hostWindow.close).toHaveBeenCalled();
    expect(hostWindow.isMaximized).toHaveBeenCalled();
    expect(hostWindow.isFullscreen).toHaveBeenCalled();
    expect(hostWindow.startDragging).toHaveBeenCalled();
  });

  it.each(payloadEventCases)(
    '%s subscribes to %s and unwraps payloads',
    async (method, event, payload) => {
      const api = await initApi();
      const callback = vi.fn();

      await callApiMethod(api, method, [callback]);
      const [, listener] = listen.mock.calls[listen.mock.calls.length - 1] as [
        HostEvent,
        (event: { payload: unknown }) => void,
      ];
      listener({ payload });

      expect(listen).toHaveBeenCalledWith(event, expect.any(Function));
      expect(callback).toHaveBeenCalledWith(payload);
    },
  );

  it.each(emptyEventCases)('%s subscribes to %s', async (method, event) => {
    const api = await initApi();
    const callback = vi.fn();

    await callApiMethod(api, method, [callback]);
    const [, listener] = listen.mock.calls[listen.mock.calls.length - 1] as [
      HostEvent,
      (event: { payload: unknown }) => void,
    ];
    listener({ payload: undefined });

    expect(listen).toHaveBeenCalledWith(event, expect.any(Function));
    expect(callback).toHaveBeenCalledWith();
  });
});
