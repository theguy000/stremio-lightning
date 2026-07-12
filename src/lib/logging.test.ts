import { get } from 'svelte/store';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const { getLogs } = vi.hoisted(() => ({ getLogs: vi.fn() }));

vi.mock('./ipc', () => ({ getLogs }));

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
  getLogs.mockReset();
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
