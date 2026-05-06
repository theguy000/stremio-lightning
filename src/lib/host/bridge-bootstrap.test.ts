import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { StremioLightningHost } from './host-api';

const testDir = dirname(fileURLToPath(import.meta.url));
const bridgeSource = readFileSync(
  resolve(testDir, '../../../src-tauri/scripts/bridge.js'),
  'utf8',
);

type TauriMock = {
  core: { invoke: ReturnType<typeof vi.fn> };
  event: { listen: ReturnType<typeof vi.fn> };
  window: { getCurrentWindow: ReturnType<typeof vi.fn> };
  webview: { getCurrentWebview: ReturnType<typeof vi.fn> };
};

declare global {
  interface Window {
    __TAURI__?: TauriMock;
  }
}

let appWindow: StremioLightningHost['window'];
let webview: StremioLightningHost['webview'];
let tauri: TauriMock;

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
  tauri = {
    core: {
      invoke: vi.fn().mockResolvedValue(undefined),
    },
    event: {
      listen: vi.fn().mockResolvedValue(() => {}),
    },
    window: {
      getCurrentWindow: vi.fn(() => appWindow),
    },
    webview: {
      getCurrentWebview: vi.fn(() => webview),
    },
  };
});

afterEach(() => {
  delete window.__TAURI__;
  delete window.StremioLightningHost;
});

describe('bridge host bootstrap', () => {
  it('creates StremioLightningHost from the Tauri global', async () => {
    window.__TAURI__ = tauri;

    window.eval(bridgeSource);

    expect(window.StremioLightningHost).toBeTruthy();
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

    expect(tauri.core.invoke).toHaveBeenCalledWith('toggle_devtools');
    expect(tauri.core.invoke).toHaveBeenCalledWith('open_external_url', {
      url: 'https://example.test',
    });
    expect(tauri.event.listen).toHaveBeenCalledWith('server-started', expect.any(Function));
    expect(appWindow.minimize).toHaveBeenCalled();
    expect(appWindow.toggleMaximize).toHaveBeenCalled();
    expect(appWindow.close).toHaveBeenCalled();
    expect(appWindow.isMaximized).toHaveBeenCalled();
    expect(appWindow.isFullscreen).toHaveBeenCalled();
    expect(appWindow.setFullscreen).toHaveBeenCalledWith(true);
    expect(appWindow.startDragging).toHaveBeenCalled();
    expect(webview.setZoom).toHaveBeenCalledWith(1.25);
  });

  it('reuses an existing host instead of reading the Tauri global', () => {
    const existingHost = {
      invoke: vi.fn().mockResolvedValue(undefined),
      listen: vi.fn().mockResolvedValue(() => {}),
      window: appWindow,
      webview,
    };
    window.StremioLightningHost = existingHost as unknown as StremioLightningHost;
    window.__TAURI__ = tauri;

    window.eval(bridgeSource);

    expect(window.StremioLightningHost).toBe(existingHost);
    expect(tauri.window.getCurrentWindow).not.toHaveBeenCalled();
    expect(tauri.webview.getCurrentWebview).not.toHaveBeenCalled();
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
