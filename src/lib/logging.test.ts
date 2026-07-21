import { get } from 'svelte/store';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { LogRecord } from './logging';

const { clearDiagnostics, getDiagnosticReport, getLogs, setExtendedDiagnostics } = vi.hoisted(() => ({
  clearDiagnostics: vi.fn(),
  getDiagnosticReport: vi.fn(),
  getLogs: vi.fn(),
  setExtendedDiagnostics: vi.fn(),
}));

vi.mock('./ipc', () => ({
  clearDiagnostics,
  getDiagnosticReport,
  getLogs,
  setExtendedDiagnostics,
}));

function installBrowserLogger(initialEntries: StremioLightningLogEntry[] = []): void {
  let entries = [...initialEntries];
  let nextId = entries[entries.length - 1]?.id ?? 0;
  const listeners = new Set<(
    entry: StremioLightningLogEntry | null,
    initialEntries?: StremioLightningLogEntry[],
  ) => void>();
  const write = (level: StremioLightningLogLevel, source: string, values: unknown[]) => {
    const entry = {
      id: ++nextId,
      timestamp: Date.now(),
      level,
      source,
      message: values.join(' '),
    };
    entries = [...entries, entry].slice(-2_000);
    for (const listener of listeners) listener(entry);
  };
  const logger = {
    debug: (source: string, ...values: unknown[]) => write('debug', source, values),
    info: (source: string, ...values: unknown[]) => write('info', source, values),
    warn: (source: string, ...values: unknown[]) => write('warn', source, values),
    error: (source: string, ...values: unknown[]) => write('error', source, values),
    bind: (source: string) => ({
      debug: (...values: unknown[]) => write('debug', source, values),
      info: (...values: unknown[]) => write('info', source, values),
      warn: (...values: unknown[]) => write('warn', source, values),
      error: (...values: unknown[]) => write('error', source, values),
    }),
    entries: () => [...entries],
    clear: () => {},
    configure: () => {},
    flush: () => Promise.resolve(),
    setExtendedDiagnostics: () => {},
    clearDiagnostics: () => {},
    subscribe: (listener: (
      entry: StremioLightningLogEntry | null,
      initialEntries?: StremioLightningLogEntry[],
    ) => void) => {
      listeners.add(listener);
      listener(null, [...entries]);
      return () => listeners.delete(listener);
    },
  } satisfies StremioLightningLogger;
  window.StremioLightningLogger = logger;
}

beforeEach(() => {
  vi.resetModules();
  clearDiagnostics.mockReset();
  getDiagnosticReport.mockReset();
  getLogs.mockReset();
  setExtendedDiagnostics.mockReset();
  setExtendedDiagnostics.mockResolvedValue(undefined);
  installBrowserLogger();
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  delete window.StremioLightningLogger;
});

describe('frontend logging adapter', () => {
  it('formats errors, circular values, and bigint without throwing', async () => {
    const { safeFormat } = await import('./logging');
    const circular: { self?: unknown; count: bigint } = { count: 2n };
    circular.self = circular;

    expect(safeFormat(new Error('boom'))).toContain('boom');
    expect(safeFormat(circular)).toBe('{"count":"2n","self":"[Circular]"}');
  });

  it('combines text, level, and source filters case-insensitively', async () => {
    const { filterLogRecords } = await import('./logging');
    const records = [
      { id: 'browser:1', sequence: 1, origin: 'browser', timestamp: 1, level: 'info', source: 'ui.marketplace', message: 'Loaded Registry' },
      { id: 'native:1', sequence: 1, origin: 'native', timestamp: 2, level: 'error', source: 'native.player', message: 'MPV failed' },
    ] as const;

    expect(filterLogRecords([...records], {
      query: 'mpv',
      level: 'error',
      source: 'native.player',
    })).toEqual([records[1]]);
    expect(filterLogRecords([...records], {
      query: 'REGISTRY',
      level: 'all',
      source: 'all',
    })).toEqual([records[0]]);
  });

  it('ingests early browser records and caps the merged view', async () => {
    installBrowserLogger(Array.from({ length: 2_001 }, (_, index) => ({
      id: index + 1,
      timestamp: index + 1,
      level: 'debug',
      source: 'bridge.test',
      message: String(index + 1),
    })));

    const { logRecords } = await import('./logging');
    const records = get(logRecords);

    expect(records).toHaveLength(2_000);
    expect(records[0].id).toBe('browser:2001');
    expect(records[records.length - 1]?.id).toBe('browser:2');
  });

  it('ingests incremental browser records without snapshots', async () => {
    const { createLogger, logRecords } = await import('./logging');

    createLogger('ui.test').info('new record');

    expect(get(logRecords)[0]).toMatchObject({
      id: 'browser:1',
      source: 'ui.test',
      message: 'new record',
    });
  });

  it('publishes a new record snapshot for every live entry', async () => {
    const { createLogger, logRecords } = await import('./logging');
    const snapshots: LogRecord[][] = [];
    const unsubscribe = logRecords.subscribe((records) => snapshots.push(records));

    createLogger('ui.test').info('first');
    createLogger('ui.test').info('second');
    unsubscribe();

    const previous = snapshots[snapshots.length - 2];
    const latest = snapshots[snapshots.length - 1];
    expect(previous).not.toBe(latest);
    expect(previous).toHaveLength(1);
    expect(latest).toHaveLength(2);
  });

  it('clears displayed records without disconnecting future entries', async () => {
    const { clearLogRecords, createLogger, logRecords } = await import('./logging');

    createLogger('ui.test').info('before clear');
    clearLogRecords();
    expect(get(logRecords)).toEqual([]);

    createLogger('ui.test').info('after clear');
    expect(get(logRecords)).toHaveLength(1);
    expect(get(logRecords)[0]).toMatchObject({ message: 'after clear' });
  });

  it('keeps Extended diagnostics session-only and rolls browser mode back on failure', async () => {
    setExtendedDiagnostics.mockRejectedValueOnce(new Error('offline'));
    const { extendedDiagnosticsEnabled, setExtendedDiagnostics: setMode } = await import('./logging');
    const setBrowserMode = vi.spyOn(window.StremioLightningLogger!, 'setExtendedDiagnostics');

    await expect(setMode(true)).rejects.toThrow('offline');

    expect(setBrowserMode).toHaveBeenNthCalledWith(1, true);
    expect(setBrowserMode).toHaveBeenNthCalledWith(2, false);
    expect(get(extendedDiagnosticsEnabled)).toBe(false);
  });

  it('flushes queued browser records before requesting a diagnostic report', async () => {
    getDiagnosticReport.mockResolvedValue('report');
    const flush = vi.spyOn(window.StremioLightningLogger!, 'flush');
    const { getDiagnosticReport: report } = await import('./logging');

    await expect(report()).resolves.toBe('report');

    expect(flush).toHaveBeenCalledOnce();
    expect(flush.mock.invocationCallOrder[0]).toBeLessThan(
      getDiagnosticReport.mock.invocationCallOrder[0],
    );
  });

  it('clears native diagnostics before resetting browser and UI records', async () => {
    clearDiagnostics.mockResolvedValue(undefined);
    const { clearDiagnostics: clearAll, createLogger, logRecords } = await import('./logging');
    const clearBrowser = vi.spyOn(window.StremioLightningLogger!, 'clearDiagnostics');
    createLogger('ui.test').info('before clear');

    await clearAll();

    expect(clearDiagnostics).toHaveBeenCalledOnce();
    expect(clearBrowser).toHaveBeenCalledOnce();
    expect(get(logRecords)).toEqual([]);
  });

  it('clears browser and UI records while surfacing a retained-file failure', async () => {
    clearDiagnostics.mockRejectedValueOnce(new Error('file locked'));
    const { clearDiagnostics: clearAll, createLogger, logRecords } = await import('./logging');
    const clearBrowser = vi.spyOn(window.StremioLightningLogger!, 'clearDiagnostics');
    createLogger('ui.test').info('before failed clear');

    await expect(clearAll()).rejects.toThrow('file locked');

    expect(clearBrowser).toHaveBeenCalledOnce();
    expect(get(logRecords)).toEqual([]);
  });

  it('polls incrementally and deduplicates repeated native snapshots', async () => {
    vi.useFakeTimers();
    getLogs.mockResolvedValue([{
      id: 1,
      timestamp: 100,
      level: 'info',
      source: 'native.application',
      message: 'started',
    }]);
    const { logRecords, startNativeLogPolling } = await import('./logging');

    const stop = startNativeLogPolling();
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(1_000);
    stop();

    expect(getLogs).toHaveBeenNthCalledWith(1, 0);
    expect(getLogs).toHaveBeenNthCalledWith(2, 1);
    expect(get(logRecords).filter((record) => record.origin === 'native')).toHaveLength(1);
  });

  it('ignores a stale native poll and accepts live records after Clear', async () => {
    vi.useFakeTimers();
    let resolveStale: ((entries: StremioLightningLogEntry[]) => void) | undefined;
    getLogs
      .mockImplementationOnce(() => new Promise((resolve) => { resolveStale = resolve; }))
      .mockResolvedValueOnce([{
        id: 9,
        timestamp: 200,
        level: 'info',
        source: 'native.application',
        message: 'after clear',
      }]);
    clearDiagnostics.mockResolvedValue(undefined);
    const { clearDiagnostics: clearAll, logRecords, startNativeLogPolling } = await import('./logging');

    const stop = startNativeLogPolling();
    await vi.advanceTimersByTimeAsync(0);
    await clearAll();
    resolveStale?.([{
      id: 4,
      timestamp: 100,
      level: 'warn',
      source: 'native.application',
      message: 'before clear',
    }]);
    await Promise.resolve();
    await vi.advanceTimersByTimeAsync(1_000);
    stop();

    expect(get(logRecords).map((record) => record.message)).toEqual(['after clear']);
  });

  it('ignores a stale native poll failure after Clear', async () => {
    let rejectStale: ((error: Error) => void) | undefined;
    getLogs.mockImplementationOnce(() => new Promise((_resolve, reject) => {
      rejectStale = reject;
    }));
    clearDiagnostics.mockResolvedValue(undefined);
    const {
      clearDiagnostics: clearAll,
      logRecords,
      nativeLogState,
      startNativeLogPolling,
    } = await import('./logging');

    const stop = startNativeLogPolling();
    await Promise.resolve();
    await clearAll();
    rejectStale?.(new Error('stale failure'));
    await Promise.resolve();
    stop();

    expect(get(logRecords)).toEqual([]);
    expect(get(nativeLogState)).not.toBe('unavailable');
  });

  it('reports repeated native retrieval failures only once', async () => {
    vi.useFakeTimers();
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    getLogs.mockRejectedValue(new Error('offline'));
    const { logRecords, nativeLogState, startNativeLogPolling } = await import('./logging');

    const stop = startNativeLogPolling();
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(2_000);
    stop();

    expect(get(nativeLogState)).toBe('unavailable');
    expect(get(logRecords).filter((record) => record.source === 'ui.logs')).toHaveLength(1);
  });
});
