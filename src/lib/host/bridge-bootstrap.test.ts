import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { StremioLightningHost } from './host-api';

const testDir = dirname(fileURLToPath(import.meta.url));
const bridgeModuleNames = [
  'utils.js',
  'cast-fallback.js',
  'shell-transport.js',
  'external-links.js',
  'shell-detection.js',
  'back-button.js',
  'shortcuts.js',
  'pip.js',
  'discord-rpc.js',
  'update-banner.js',
];
const bridgeModuleSources = bridgeModuleNames.map((name) =>
  readFileSync(resolve(testDir, `../../../web/bridge/src/${name}`), 'utf8'),
);
const bridgeSource = readFileSync(
  resolve(testDir, '../../../web/bridge/bridge.js'),
  'utf8',
);

function runBridge(): void {
  for (const source of bridgeModuleSources) {
    window.eval(source);
  }
  window.eval(bridgeSource);
}

let appWindow: StremioLightningHost['window'];
let webview: StremioLightningHost['webview'];

beforeEach(() => {
  document.body.innerHTML = '';
  localStorage.clear();

  appWindow = {
    minimize: vi.fn().mockResolvedValue(undefined),
    toggleMaximize: vi.fn().mockResolvedValue(undefined),
    close: vi.fn().mockResolvedValue(undefined),
    isMaximized: vi.fn().mockResolvedValue(true),
    isFullscreen: vi.fn().mockResolvedValue(false),
    setFullscreen: vi.fn().mockResolvedValue(undefined),
    startDragging: vi.fn().mockResolvedValue(undefined),
  };
  webview = {
    setZoom: vi.fn().mockResolvedValue(undefined),
  };
});

afterEach(() => {
  delete window.StremioLightningHost;
  delete (window as typeof window & { StremioEnhancedAPI?: unknown }).StremioEnhancedAPI;
  delete (window as typeof window & { qt?: unknown }).qt;
  delete (window as typeof window & { chrome?: unknown }).chrome;
});

describe('bridge host bootstrap', () => {
  it('uses the host provided by the native shell', async () => {
    const nativeShellHost = {
      invoke: vi.fn().mockResolvedValue(undefined),
      listen: vi.fn().mockResolvedValue(() => {}),
      window: appWindow,
      webview,
    };
    window.StremioLightningHost = nativeShellHost as unknown as StremioLightningHost;

    runBridge();

    expect(window.StremioLightningHost).toBe(nativeShellHost);
    await window.StremioLightningHost!.invoke('toggle_devtools');
    await window.StremioLightningHost!.invoke('open_external_url', {
      url: 'https://example.test',
    });
    await window.StremioLightningHost!.listen('server-started', vi.fn());
    await window.StremioLightningHost!.window.minimize();
    await window.StremioLightningHost!.window.toggleMaximize();
    await window.StremioLightningHost!.window.close();
    await window.StremioLightningHost!.window.isMaximized();
    await window.StremioLightningHost!.window.isFullscreen();
    await window.StremioLightningHost!.window.setFullscreen(true);
    await window.StremioLightningHost!.window.startDragging();
    await window.StremioLightningHost!.webview.setZoom(1.25);

    expect(nativeShellHost.invoke).toHaveBeenCalledWith('toggle_devtools');
    expect(nativeShellHost.invoke).toHaveBeenCalledWith('open_external_url', {
      url: 'https://example.test',
    });
    expect(nativeShellHost.listen).toHaveBeenCalledWith('server-started', expect.any(Function));
    expect(appWindow.minimize).toHaveBeenCalled();
    expect(appWindow.toggleMaximize).toHaveBeenCalled();
    expect(appWindow.close).toHaveBeenCalled();
    expect(appWindow.isMaximized).toHaveBeenCalled();
    expect(appWindow.isFullscreen).toHaveBeenCalled();
    expect(appWindow.setFullscreen).toHaveBeenCalledWith(true);
    expect(appWindow.startDragging).toHaveBeenCalled();
    expect(webview.setZoom).toHaveBeenCalledWith(1.25);
  });

  it('logs once and exits when no host adapter is available', () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});

    runBridge();

    expect(window.StremioLightningHost).toBeUndefined();
    expect(error).toHaveBeenCalledWith(
      '[StremioLightning] host adapter not available - bridge not loaded',
    );
    expect(error).toHaveBeenCalledTimes(1);
  });

  it('installs desktop shell transport compatibility shims', async () => {
    let shellTransportCallback:
      | ((event: { event: 'shell-transport-message'; payload: string }) => void)
      | undefined;
    const nativeShellHost = {
      invoke: vi.fn().mockResolvedValue(undefined),
      listen: vi.fn().mockImplementation((event, callback) => {
        if (event === 'shell-transport-message') shellTransportCallback = callback;
        return Promise.resolve(() => {});
      }),
      window: appWindow,
      webview,
    };
    window.StremioLightningHost = nativeShellHost as unknown as StremioLightningHost;

    runBridge();

    expect(nativeShellHost.listen).toHaveBeenCalledWith(
      'shell-transport-message',
      expect.any(Function),
    );
    expect((window as any).qt.webChannelTransport.send).toEqual(expect.any(Function));
    expect((window as any).chrome.webview.postMessage).toEqual(expect.any(Function));

    await (window as any).qt.webChannelTransport.send({
      id: 10,
      type: 6,
      args: ['mpv-observe-prop', 'pause'],
    });

    expect(nativeShellHost.invoke).toHaveBeenCalledWith('shell_transport_send', {
      message: JSON.stringify({
        id: 10,
        type: 6,
        args: ['mpv-observe-prop', 'pause'],
      }),
    });
    expect(shellTransportCallback).toEqual(expect.any(Function));
  });

  it('dispatches shell transport messages to Qt, chrome listeners, and PiP events', () => {
    let shellTransportCallback:
      | ((event: { event: 'shell-transport-message'; payload: string }) => void)
      | undefined;
    const nativeShellHost = {
      invoke: vi.fn().mockResolvedValue(undefined),
      listen: vi.fn().mockImplementation((event, callback) => {
        if (event === 'shell-transport-message') shellTransportCallback = callback;
        return Promise.resolve(() => {});
      }),
      window: appWindow,
      webview,
    };
    const qtHandler = vi.fn();
    const chromeListener = vi.fn();
    const pipHandler = vi.fn();

    window.StremioLightningHost = nativeShellHost as unknown as StremioLightningHost;
    runBridge();

    (window as any).qt.webChannelTransport.onmessage = qtHandler;
    (window as any).chrome.webview.addEventListener('message', chromeListener);
    document.addEventListener('sl-pip-enabled', pipHandler);

    const payload = JSON.stringify({ args: ['showPictureInPicture', {}] });
    shellTransportCallback?.({ event: 'shell-transport-message', payload });

    expect(qtHandler).toHaveBeenCalledWith({ data: payload });
    expect(chromeListener).toHaveBeenCalledWith({ data: payload });
    expect(pipHandler).toHaveBeenCalledTimes(1);
  });
});
