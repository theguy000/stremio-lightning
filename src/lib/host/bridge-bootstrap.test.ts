import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { StremioLightningHost } from './host-api';

const testDir = dirname(fileURLToPath(import.meta.url));
const bridgeModuleNames = [
  'logging.js',
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
  vi.useRealTimers();
  delete window.StremioLightningHost;
  delete (window as typeof window & { StremioEnhancedAPI?: unknown }).StremioEnhancedAPI;
  delete (window as typeof window & { qt?: unknown }).qt;
  delete (window as typeof window & { chrome?: unknown }).chrome;
  delete (window as typeof window & { StremioLightningLogger?: unknown }).StremioLightningLogger;
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

  it('retains formatted records in order and caps retention', () => {
    window.eval(bridgeModuleSources[0]);
    const logger = (window as any).StremioLightningLogger;
    const circular: { self?: unknown } = {};
    circular.self = circular;
    let initialEntries: unknown[] | undefined;
    let incrementalSnapshots = 0;
    const unsubscribe = logger.subscribe((entry: unknown, entries?: unknown[]) => {
      if (entry === null) initialEntries = entries;
      else if (entries) incrementalSnapshots++;
    });

    logger.info('bridge.test', new Error('boom'), circular, 1n);
    expect(logger.entries()[0].message).toContain('boom');
    expect(logger.entries()[0].message).toContain('[Circular]');
    expect(logger.entries()[0].message).toContain('1n');
    for (let index = 0; index < 2000; index++) {
      logger.debug('bridge.test', index);
    }
    unsubscribe();

    const entries = logger.entries();
    expect(entries).toHaveLength(2000);
    expect(entries[0]).toMatchObject({ id: 2, level: 'debug', source: 'bridge.test' });
    expect(entries.at(-1)).toMatchObject({ id: 2001, message: '1999' });
    expect(initialEntries).toEqual([]);
    expect(incrementalSnapshots).toBe(0);
    expect(logger.entries()[0].message).not.toContain('[Circular]');
  });

  it('formats circular values and mirrors through the original console method', () => {
    const info = vi.spyOn(console, 'info').mockImplementation(() => {});
    window.eval(bridgeModuleSources[0]);
    const circular: { self?: unknown } = {};
    circular.self = circular;

    (window as any).StremioLightningLogger.info('bridge.test', circular);

    expect((window as any).StremioLightningLogger.entries()[0]).toMatchObject({
      source: 'bridge.test',
      message: '{self: [Circular]}',
    });
    expect(info).toHaveBeenCalledWith(circular);
  });

  it('bounds retained message size and object traversal', () => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    window.eval(bridgeModuleSources[0]);
    const logger = (window as any).StremioLightningLogger;
    let nested: Record<string, unknown> = {};
    for (let index = 0; index < 10; index++) nested = { nested };

    logger.info('bridge.test', 'x'.repeat(20_000));
    logger.info('bridge.test', nested);

    expect(logger.entries()[0].message).toHaveLength(16_384);
    expect(logger.entries()[0].message.endsWith('... [truncated]')).toBe(true);
    expect(logger.entries()[1].message).toContain('[Max depth]');
  });

  it('retains uncaught errors, promise rejections, and code resource failures', () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    window.eval(bridgeModuleSources[0]);

    window.dispatchEvent(new ErrorEvent('error', {
      message: 'browse failed https://media.example.test/video?token=secret',
      error: new Error('browse failed https://media.example.test/video?token=secret'),
    }));
    const rejection = new Event('unhandledrejection');
    Object.defineProperty(rejection, 'reason', {
      value: { message: 'play failed', accessToken: 'secret-value' },
    });
    window.dispatchEvent(rejection);
    const image = document.createElement('img');
    document.body.appendChild(image);
    image.dispatchEvent(new Event('error'));
    const script = document.createElement('script');
    document.body.appendChild(script);
    script.dispatchEvent(new Event('error'));
    const stylesheet = document.createElement('link');
    stylesheet.rel = 'stylesheet';
    document.body.appendChild(stylesheet);
    stylesheet.dispatchEvent(new Event('error'));

    const entries = (window as any).StremioLightningLogger.entries();
    expect(entries).toHaveLength(4);
    expect(entries[0]).toMatchObject({
      level: 'error',
      source: 'bridge.browser',
    });
    expect(entries[0].message).toContain('browse failed');
    expect(entries[1].message).toContain('play failed');
    expect(entries[2].message).toContain('script');
    expect(entries[3].message).toContain('stylesheet');
    expect(entries.map((entry: { message: string }) => entry.message).join('\n'))
      .not.toMatch(/media\.example\.test|token=secret|secret-value/);
  });

  it('retains failed addon requests without request details', async () => {
    const frame = document.createElement('iframe');
    document.body.appendChild(frame);
    const frameWindow = frame.contentWindow as Window & typeof globalThis;
    const failedFetch = vi.fn().mockResolvedValue({
      ok: false,
      status: 503,
      statusText: 'Service Unavailable',
      url: 'https://addon.example.test/stream?token=secret',
    });
    Object.defineProperty(frameWindow, 'fetch', {
      configurable: true,
      value: failedFetch,
      writable: true,
    });
    vi.spyOn(frameWindow.console, 'info').mockImplementation(() => {});
    vi.spyOn(frameWindow.console, 'error').mockImplementation(() => {});
    frameWindow.eval(bridgeModuleSources[0]);

    await frameWindow.fetch('https://addon.example.test/stream?token=secret');

    const entries = (frameWindow as any).StremioLightningLogger.entries();
    expect(entries).toHaveLength(2);
    expect(entries[0]).toMatchObject({
      level: 'info',
      source: 'bridge.network',
      message: expect.stringContaining(
        'Stream request #1 started: GET stream discovery via fetch from addon #1',
      ),
    });
    expect(entries[1]).toMatchObject({
      level: 'error',
      source: 'bridge.network',
      message: expect.stringContaining(
        'request #1 failed: GET stream discovery via fetch from addon #1 -> HTTP 503 Service Unavailable after',
      ),
    });
    expect(entries.map((entry: { message: string }) => entry.message).join('\n'))
      .not.toMatch(/addon\.example\.test|token=secret/);
  });

  it('retains successful stream discovery without logging other successes', async () => {
    const frame = document.createElement('iframe');
    document.body.appendChild(frame);
    const frameWindow = frame.contentWindow as Window & typeof globalThis;
    Object.defineProperty(frameWindow, 'fetch', {
      configurable: true,
      value: vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        statusText: 'OK',
      }),
      writable: true,
    });
    vi.spyOn(frameWindow.console, 'info').mockImplementation(() => {});
    frameWindow.eval(bridgeModuleSources[0]);

    await frameWindow.fetch(
      'https://addon.example.test/stream/movie/tt-secret.json?token=secret',
    );

    const entries = (frameWindow as any).StremioLightningLogger.entries();
    expect(entries).toHaveLength(2);
    expect(entries[0].message).toContain('Stream request #1 started');
    expect(entries[1].message).toMatch(
      /Stream request #1 completed: GET stream discovery \(movie\) via fetch from addon #1 -> HTTP 200 OK in \d+ ms/,
    );
    expect(entries.map((entry: { message: string }) => entry.message).join('\n'))
      .not.toMatch(/addon\.example\.test|tt-secret|token=secret/);
  });

  it('warns when stream discovery remains pending', async () => {
    vi.useFakeTimers();
    const frame = document.createElement('iframe');
    document.body.appendChild(frame);
    const frameWindow = frame.contentWindow as Window & typeof globalThis;
    Object.defineProperty(frameWindow, 'fetch', {
      configurable: true,
      value: vi.fn().mockReturnValue(new Promise(() => {})),
      writable: true,
    });
    vi.spyOn(frameWindow.console, 'info').mockImplementation(() => {});
    vi.spyOn(frameWindow.console, 'warn').mockImplementation(() => {});
    frameWindow.eval(bridgeModuleSources[0]);

    void frameWindow.fetch('https://addon.example.test/stream/movie/tt-secret.json');
    await vi.advanceTimersByTimeAsync(15_000);

    const entries = (frameWindow as any).StremioLightningLogger.entries();
    expect(entries).toHaveLength(2);
    expect(entries[1]).toMatchObject({
      level: 'warn',
      source: 'bridge.network',
      message: expect.stringContaining(
        'Stream request #1 is still pending after 15000 ms',
      ),
    });
  });

  it('does not retain successful addon requests or route changes', async () => {
    const frame = document.createElement('iframe');
    document.body.appendChild(frame);
    const frameWindow = frame.contentWindow as Window & typeof globalThis;
    Object.defineProperty(frameWindow, 'fetch', {
      configurable: true,
      value: vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        statusText: 'OK',
      }),
      writable: true,
    });
    vi.spyOn(frameWindow.console, 'info').mockImplementation(() => {});
    frameWindow.eval(bridgeModuleSources[0]);

    await frameWindow.fetch('https://addon.example.test/meta/movie/tt-secret.json?token=secret');
    frameWindow.location.hash = '#/detail/movie/tt-secret/secret-title';
    frameWindow.dispatchEvent(new frameWindow.HashChangeEvent('hashchange'));
    frameWindow.location.hash = '#/unknown/secret-value';
    frameWindow.dispatchEvent(new frameWindow.HashChangeEvent('hashchange'));

    expect((frameWindow as any).StremioLightningLogger.entries()).toEqual([]);
  });

  it('does not retain transport payloads or external URLs in failure logs', async () => {
    const nativeShellHost = {
      invoke: vi.fn().mockImplementation((command: string) => {
        if (command === 'shell_transport_send' || command === 'open_external_url') {
          return Promise.reject(new Error('denied'));
        }
        return Promise.resolve(undefined);
      }),
      listen: vi.fn().mockResolvedValue(() => {}),
      window: appWindow,
      webview,
    };
    window.StremioLightningHost = nativeShellHost as unknown as StremioLightningHost;
    runBridge();

    const secretPayload = 'https://media.example.test/video?token=secret';
    await (window as any).qt.webChannelTransport.send(secretPayload);
    window.open('https://external.example.test/?token=secret');
    await vi.waitFor(() => {
      expect(window.StremioLightningLogger?.entries().filter(
        (entry) => entry.source === 'bridge.external-links',
      )).toHaveLength(1);
    });

    const messages = window.StremioLightningLogger?.entries().map((entry) => entry.message) || [];
    expect(messages.join('\n')).not.toContain('media.example.test');
    expect(messages.join('\n')).not.toContain('external.example.test');
    expect(messages.join('\n')).not.toContain('token=secret');
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

    const observedProperties = nativeShellHost.invoke.mock.calls
      .filter(([command]) => command === 'shell_transport_send')
      .map(([, payload]) => JSON.parse(payload.message).args)
      .filter(([method]) => method === 'mpv-observe-prop')
      .map(([, name]) => name);
    expect(observedProperties).toEqual([
      'time-pos',
      'duration',
      'pause',
      'paused-for-cache',
      'seeking',
      'eof-reached',
      'cache-buffering-state',
      'demuxer-cache-time',
    ]);

    await (window as any).qt.webChannelTransport.send({
      id: 10,
      type: 6,
      args: ['mpv-observe-prop', 'pause'],
    });

    await (window as any).qt.webChannelTransport.send({
      id: 11,
      type: 6,
      args: ['mpv-command', ['loadfile', 'https://media.example.test/?token=secret']],
    });

    expect(nativeShellHost.invoke).toHaveBeenCalledWith('shell_transport_send', {
      message: JSON.stringify({
        id: 10,
        type: 6,
        args: ['mpv-observe-prop', 'pause'],
      }),
    });
    const playbackMessages = window.StremioLightningLogger?.entries()
      .filter((entry) => entry.source === 'bridge.shell-transport')
      .map((entry) => entry.message)
      .join('\n') || '';
    expect(playbackMessages).toContain('Forwarding MPV loadfile command');
    expect(playbackMessages).not.toMatch(/media\.example\.test|token=secret/);
    expect(shellTransportCallback).toEqual(expect.any(Function));
  });

  it('routes a pre-existing native chrome transport through the host adapter', async () => {
    const nativePostMessage = vi.fn();
    const nativeChromeWebview = {
      postMessage: nativePostMessage,
      addEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    };
    const nativeShellHost = {
      invoke: vi.fn().mockResolvedValue(undefined),
      listen: vi.fn().mockResolvedValue(() => {}),
      window: appWindow,
      webview,
    };
    window.StremioLightningHost = nativeShellHost as unknown as StremioLightningHost;
    (window as any).chrome = { webview: nativeChromeWebview };

    runBridge();
    const payload = {
      id: 12,
      type: 6,
      args: ['mpv-command', ['loadfile', 'https://media.example.test/?token=secret']],
    };
    await (window as any).chrome.webview.postMessage(payload);

    expect(nativePostMessage).not.toHaveBeenCalled();
    expect(nativeShellHost.invoke).toHaveBeenCalledWith('shell_transport_send', {
      message: JSON.stringify(payload),
    });
    const messages = window.StremioLightningLogger?.entries()
      .map((entry) => entry.message)
      .join('\n') || '';
    expect(messages).toContain('Forwarding MPV loadfile command');
    expect(messages).not.toMatch(/media\.example\.test|token=secret/);
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
