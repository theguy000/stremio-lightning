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
  delete (window as typeof window & { __stremioLightningCapture?: unknown }).__stremioLightningCapture;
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

  it('captures direct console errors once without recursion and warnings only in Extended mode', () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    window.eval(bridgeModuleSources[0]);
    const logger = (window as any).StremioLightningLogger;

    console.error('request failed https://media.example.test/video?token=secret {"token":"secret-json"} {\\"token\\":\\"escaped-json-secret\\"} session_id: browser-session-secret at C:\\Users\\local-user\\app.js:10');
    console.warn('normal page warning');
    logger.warn('bridge.test', 'explicit warning');
    logger.configure({ extended: true });
    console.warn('extended page warning');

    const entries = logger.entries();
    expect(error).toHaveBeenCalledTimes(1);
    expect(warn).toHaveBeenCalledTimes(3);
    expect(entries.filter((entry: { source: string }) => entry.source === 'web.console'))
      .toHaveLength(2);
    expect(entries.map((entry: { message: string }) => entry.message).join('\n'))
      .toContain('https://media.example.test/video?token=[redacted]');
    expect(entries.map((entry: { message: string }) => entry.message).join('\n'))
      .not.toContain('token=secret');
    expect(entries.map((entry: { message: string }) => entry.message).join('\n'))
      .not.toContain('secret-json');
    expect(entries.map((entry: { message: string }) => entry.message).join('\n'))
      .not.toContain('escaped-json-secret');
    expect(entries.map((entry: { message: string }) => entry.message).join('\n'))
      .not.toContain('browser-session-secret');
    expect(entries.map((entry: { message: string }) => entry.message).join('\n'))
      .not.toContain('local-user');
    expect(entries.some((entry: { message: string }) => entry.message === 'explicit warning')).toBe(true);
  });

  it('resets native Extended diagnostics when the bridge is reloaded', async () => {
    vi.useFakeTimers();
    const nativeShellHost = {
      invoke: vi.fn().mockImplementation((command: string) => Promise.resolve(
        command === 'init'
          ? { diagnostics: { nativeHttpCapture: false, nativeNetworkFailureCapture: false } }
          : undefined,
      )),
      listen: vi.fn().mockResolvedValue(() => {}),
      window: appWindow,
      webview,
    };
    window.StremioLightningHost = nativeShellHost as unknown as StremioLightningHost;

    window.eval(bridgeModuleSources[0]);
    await vi.advanceTimersByTimeAsync(0);

    expect(nativeShellHost.invoke).toHaveBeenCalledWith(
      'set_extended_diagnostics',
      { enabled: false },
    );
  });

  it('clears the local ring and queued browser diagnostics', async () => {
    const nativeShellHost = {
      invoke: vi.fn().mockResolvedValue(undefined),
      listen: vi.fn().mockResolvedValue(() => {}),
      window: appWindow,
      webview,
    };
    window.StremioLightningHost = nativeShellHost as unknown as StremioLightningHost;
    vi.spyOn(console, 'info').mockImplementation(() => {});
    window.eval(bridgeModuleSources[0]);
    const logger = window.StremioLightningLogger!;

    logger.info('bridge.test', 'queued record');
    logger.clear();
    await logger.flush();

    expect(logger.entries()).toEqual([]);
    expect(nativeShellHost.invoke).not.toHaveBeenCalledWith(
      'submit_diagnostic_logs',
      expect.anything(),
    );
  });

  it('batches sanitized diagnostics and retries failed native submissions', async () => {
    vi.useFakeTimers();
    let submitAttempts = 0;
    const nativeShellHost = {
      invoke: vi.fn().mockImplementation((command: string) => {
        if (command === 'submit_diagnostic_logs') {
          submitAttempts++;
          return submitAttempts === 1
            ? Promise.reject(new Error('temporarily unavailable'))
            : Promise.resolve(undefined);
        }
        return Promise.resolve({ diagnostics: { nativeHttpCapture: true } });
      }),
      listen: vi.fn().mockResolvedValue(() => {}),
      window: appWindow,
      webview,
    };
    window.StremioLightningHost = nativeShellHost as unknown as StremioLightningHost;
    vi.spyOn(console, 'error').mockImplementation(() => {});
    window.eval(bridgeModuleSources[0]);
    const logger = window.StremioLightningLogger!;

    logger.error('bridge.test', 'failed https://media.example.test/video?token=secret');
    const flush = logger.flush();
    await vi.advanceTimersByTimeAsync(500);
    await flush;

    expect(submitAttempts).toBe(2);
    const batches = nativeShellHost.invoke.mock.calls
      .filter(([command]) => command === 'submit_diagnostic_logs')
      .map(([, payload]) => JSON.stringify(payload));
    expect(batches).toHaveLength(2);
    expect(batches.join('\n')).toContain('https://media.example.test/video?token=[redacted]');
    expect(batches.join('\n')).not.toContain('token=secret');
  });

  it('does not restore an in-flight batch after diagnostics are cleared', async () => {
    vi.useFakeTimers();
    let rejectSubmission: ((error: Error) => void) | undefined;
    const nativeShellHost = {
      invoke: vi.fn().mockImplementation((command: string) => {
        if (command === 'submit_diagnostic_logs') {
          return new Promise<void>((_resolve, reject) => {
            rejectSubmission = reject;
          });
        }
        return Promise.resolve({ diagnostics: { nativeHttpCapture: false } });
      }),
      listen: vi.fn().mockResolvedValue(() => {}),
      window: appWindow,
      webview,
    };
    window.StremioLightningHost = nativeShellHost as unknown as StremioLightningHost;
    vi.spyOn(console, 'error').mockImplementation(() => {});
    window.eval(bridgeModuleSources[0]);
    const logger = window.StremioLightningLogger!;

    logger.error('bridge.test', 'in-flight failure');
    logger.clearDiagnostics();
    rejectSubmission?.(new Error('late rejection'));
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(5_000);

    expect(nativeShellHost.invoke.mock.calls.filter(([command]) =>
      command === 'submit_diagnostic_logs'
    )).toHaveLength(1);
    expect(logger.entries()).toEqual([]);
  });

  it('bounds the browser diagnostic queue and reports drops without recursive console capture', () => {
    vi.spyOn(console, 'info').mockImplementation(() => {});
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    window.eval(bridgeModuleSources[0]);
    const logger = window.StremioLightningLogger!;

    for (let index = 0; index < 501; index++) {
      logger.info('bridge.test', `record ${index}`);
    }

    const dropped = logger.entries().filter((entry) =>
      entry.message.includes('Browser diagnostic records were dropped'),
    );
    expect(dropped).toHaveLength(1);
    expect(dropped[0]).toMatchObject({ source: 'bridge.diagnostics', level: 'warn' });
  });

  it('keeps every browser submission below the native batch byte limit', async () => {
    vi.useFakeTimers();
    const nativeShellHost = {
      invoke: vi.fn().mockImplementation((command: string) => Promise.resolve(
        command === 'init' ? { diagnostics: { nativeHttpCapture: false } } : undefined,
      )),
      listen: vi.fn().mockResolvedValue(() => {}),
      window: appWindow,
      webview,
    };
    window.StremioLightningHost = nativeShellHost as unknown as StremioLightningHost;
    vi.spyOn(console, 'info').mockImplementation(() => {});
    window.eval(bridgeModuleSources[0]);
    const logger = window.StremioLightningLogger!;

    for (let index = 0; index < 20; index++) {
      logger.info('bridge.test', `${index} ${'\u{1F680}'.repeat(4_000)}`);
    }
    await vi.advanceTimersByTimeAsync(1_000);

    const payloads = nativeShellHost.invoke.mock.calls
      .filter(([command]) => command === 'submit_diagnostic_logs')
      .map(([, payload]) => JSON.stringify(payload));
    expect(payloads.length).toBeGreaterThan(1);
    expect(payloads.every((payload) => new TextEncoder().encode(payload).length < 256 * 1024))
      .toBe(true);
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

  it('preserves non-HTTP URLs while redacting embedded credentials', () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    window.eval(bridgeModuleSources[0]);
    const logger = window.StremioLightningLogger!;

    logger.error(
      'rtsp://source-secret@source.example.test/private',
      'rtsp://user:password@media.example.test/private',
      'data:text/plain,secret',
    );

    expect(logger.entries()[0].message).toContain(
      'rtsp://[redacted]@media.example.test/private data:text/plain,secret',
    );
    expect(logger.entries()[0].message).not.toContain('password');
    expect(logger.entries()[0].source).toBe('browser.external');
  });

  it('retains uncaught errors, promise rejections, and safe resource failures', () => {
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
      .toContain('https://media.example.test/video?token=[redacted]');
    expect(entries.map((entry: { message: string }) => entry.message).join('\n'))
      .not.toMatch(/token=secret|secret-value/);
  });

  it('classifies media, font, and document resource failures without addresses', () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    window.eval(bridgeModuleSources[0]);
    const audio = document.createElement('audio');
    audio.src = 'https://media.example.test/audio?token=secret';
    const font = document.createElement('link');
    font.as = 'font';
    font.href = 'https://fonts.example.test/font.woff2?token=secret';
    document.body.append(audio, font);

    audio.dispatchEvent(new Event('error'));
    font.dispatchEvent(new Event('error'));
    document.dispatchEvent(new Event('error'));

    const messages = window.StremioLightningLogger!.entries().map((entry) => entry.message).join('\n');
    expect(messages).toContain('media');
    expect(messages).toContain('font');
    expect(messages).toContain('document');
    expect(messages).toContain('resource address redacted');
    expect(messages).not.toMatch(/media\.example\.test|fonts\.example\.test|token=secret/);
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
    expect(entries).toHaveLength(1);
    expect(entries[0]).toMatchObject({
      level: 'error',
      source: 'bridge.network',
      message: expect.stringContaining(
        'Browser request failed: GET stream discovery via fetch from origin #1 -> HTTP 503 after',
      ),
    });
    expect(entries.map((entry: { message: string }) => entry.message).join('\n'))
      .not.toMatch(/addon\.example\.test|token=secret/);
  });

  it('captures generic opensubHash HTTP failures with safe request metadata', async () => {
    const frame = document.createElement('iframe');
    document.body.appendChild(frame);
    const frameWindow = frame.contentWindow as Window & typeof globalThis;
    Object.defineProperty(frameWindow, 'fetch', {
      configurable: true,
      value: vi.fn().mockResolvedValue({ ok: false, status: 500 }),
      writable: true,
    });
    vi.spyOn(frameWindow.console, 'error').mockImplementation(() => {});
    frameWindow.eval(bridgeModuleSources[0]);

    await frameWindow.fetch('https://subtitle.example.test/opensubHash?mediaId=tt-secret&token=secret', {
      method: 'post',
      headers: { authorization: 'Bearer secret' },
      body: 'secret body',
    });

    const message = (frameWindow as any).StremioLightningLogger.entries()[0].message;
    expect(message).toMatch(/POST subtitle hash via fetch from origin #1 -> HTTP 500 after \d+ ms/);
    expect(message).not.toMatch(/subtitle\.example\.test|tt-secret|token=secret|Bearer|secret body/);
  });

  it('suppresses native-captured HTTP responses but retains browser network errors', async () => {
    const frame = document.createElement('iframe');
    document.body.appendChild(frame);
    const frameWindow = frame.contentWindow as Window & typeof globalThis;
    const fetch = vi.fn()
      .mockResolvedValueOnce({ ok: false, status: 503 })
      .mockRejectedValueOnce(new Error('https://media.example.test/video?token=secret'));
    Object.defineProperty(frameWindow, 'fetch', { configurable: true, value: fetch, writable: true });
    vi.spyOn(frameWindow.console, 'error').mockImplementation(() => {});
    frameWindow.eval(bridgeModuleSources[0]);
    const logger = (frameWindow as any).StremioLightningLogger;
    logger.configure({ nativeHttpCapture: true });

    await frameWindow.fetch('https://addon.example.test/meta/movie/tt-secret.json?token=secret');
    await expect(frameWindow.fetch('https://addon.example.test/meta/movie/tt-secret.json?token=secret'))
      .rejects.toThrow('token=secret');

    const entries = logger.entries();
    expect(entries).toHaveLength(1);
    expect(entries[0].message).toContain('network error');
    expect(entries[0].message).not.toMatch(/media\.example\.test|token=secret/);
  });

  it('suppresses browser network errors when native failure capture is active', async () => {
    const frame = document.createElement('iframe');
    document.body.appendChild(frame);
    const frameWindow = frame.contentWindow as Window & typeof globalThis;
    Object.defineProperty(frameWindow, 'fetch', {
      configurable: true,
      value: vi.fn().mockRejectedValue(new Error('rtsp://media.example.test/private')),
      writable: true,
    });
    vi.spyOn(frameWindow.console, 'error').mockImplementation(() => {});
    frameWindow.eval(bridgeModuleSources[0]);
    const logger = (frameWindow as any).StremioLightningLogger;
    logger.configure({ nativeNetworkFailureCapture: true });

    await expect(frameWindow.fetch('rtsp://media.example.test/private')).rejects.toThrow();

    expect(logger.entries()).toEqual([]);
  });

  it('records successful request lifecycle details only in Extended mode', async () => {
    const frame = document.createElement('iframe');
    document.body.appendChild(frame);
    const frameWindow = frame.contentWindow as Window & typeof globalThis;
    Object.defineProperty(frameWindow, 'fetch', {
      configurable: true,
      value: vi.fn().mockResolvedValue({ ok: true, status: 200 }),
      writable: true,
    });
    vi.spyOn(frameWindow.console, 'debug').mockImplementation(() => {});
    frameWindow.eval(bridgeModuleSources[0]);
    const logger = (frameWindow as any).StremioLightningLogger;
    logger.configure({ extended: true });

    await frameWindow.fetch('https://addon.example.test/catalog/movie/top.json?token=secret');

    expect(logger.entries()).toHaveLength(2);
    expect(logger.entries().every((entry: { level: string }) => entry.level === 'debug')).toBe(true);
    expect(logger.entries().map((entry: { message: string }) => entry.message).join('\n'))
      .not.toMatch(/addon\.example\.test|token=secret/);
  });

  it('omits successful request lifecycle records by default', async () => {
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

    expect((frameWindow as any).StremioLightningLogger.entries()).toEqual([]);
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
    expect(entries).toHaveLength(1);
    expect(entries[0]).toMatchObject({
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

  it('reports an unavailable shell transport method once without its payload', async () => {
    const nativeShellHost = {
      invoke: vi.fn().mockImplementation((command: string) => {
        if (command === 'shell_transport_send') return Promise.reject(new Error('unsupported'));
        return Promise.resolve(undefined);
      }),
      listen: vi.fn().mockResolvedValue(() => {}),
      window: appWindow,
      webview,
    };
    window.StremioLightningHost = nativeShellHost as unknown as StremioLightningHost;
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    runBridge();

    const payload = {
      args: ['unsupported-method', 'https://media.example.test/video?token=secret'],
    };
    await (window as any).qt.webChannelTransport.send(payload);
    await (window as any).qt.webChannelTransport.send(payload);

    const messages = window.StremioLightningLogger!.entries()
      .filter((entry) => entry.source === 'bridge.shell-transport')
      .map((entry) => entry.message);
    expect(messages.filter((message) => message.includes('unsupported-method'))).toHaveLength(1);
    expect(messages.join('\n')).not.toMatch(/media\.example\.test|token=secret/);
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

  it('adds the PiP button after the volume slider on navigation and control remounts', async () => {
    const nativeShellHost = {
      invoke: vi.fn().mockResolvedValue(undefined),
      listen: vi.fn().mockResolvedValue(() => {}),
      window: appWindow,
      webview,
    };
    window.StremioLightningHost = nativeShellHost as unknown as StremioLightningHost;
    window.history.replaceState({}, '', '#/');

    runBridge();

    window.history.pushState({}, '', '#/player/test');
    window.dispatchEvent(new HashChangeEvent('hashchange'));
    const oldControls = document.createElement('div');
    oldControls.className = 'control-bar-buttons-container-old';
    oldControls.innerHTML =
      '<div id="volume-button" class="control-bar-button-old"></div>' +
      '<div id="volume-slider" class="volume-slider-old"></div>' +
      '<div class="spacing-old"></div>';
    document.body.appendChild(oldControls);

    await vi.waitFor(() => {
      expect(oldControls.querySelector('#sl-pip-btn')).not.toBeNull();
    });
    expect(oldControls.querySelector('#volume-slider')?.nextElementSibling?.id).toBe('sl-pip-btn');

    const newControls = document.createElement('div');
    newControls.className = 'control-bar-buttons-container-new';
    newControls.innerHTML =
      '<div class="control-bar-button-new"></div>' +
      '<div class="volume-slider-new"></div>' +
      '<div class="spacing-new"></div>';
    oldControls.replaceWith(newControls);

    await vi.waitFor(() => {
      expect(newControls.querySelector('#sl-pip-btn')).not.toBeNull();
    });

    document.dispatchEvent(new CustomEvent('sl-pip-feature-changed', { detail: false }));
    window.history.replaceState({}, '', '#/');
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
