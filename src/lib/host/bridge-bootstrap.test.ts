import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { StremioLightningHost } from './host-api';

const testDir = dirname(fileURLToPath(import.meta.url));
const bridgeSource = readFileSync(
  resolve(testDir, '../../../web/bridge/bridge.js'),
  'utf8',
);

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

    window.eval(bridgeSource);

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

    window.eval(bridgeSource);

    expect(window.StremioLightningHost).toBeUndefined();
    expect(error).toHaveBeenCalledWith(
      '[StremioLightning] host adapter not available - bridge not loaded',
    );
    expect(error).toHaveBeenCalledTimes(1);
  });
});
