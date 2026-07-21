import { writable } from 'svelte/store';
import {
  clearDiagnostics as clearNativeDiagnostics,
  getDiagnosticReport as getNativeDiagnosticReport,
  getLogs,
  setExtendedDiagnostics as setNativeExtendedDiagnostics,
} from './ipc';

const MAX_ENTRIES = 2_000;

export type LogLevel = StremioLightningLogLevel;
export type NativeLogState = 'idle' | 'loading' | 'available' | 'unavailable';
export type LogOrigin = 'adapter' | 'browser' | 'native';

export interface LogRecord extends Omit<StremioLightningLogEntry, 'id'> {
  id: string;
  origin: LogOrigin;
  sequence: number;
}

export interface LogFilters {
  query: string;
  level: LogLevel | 'all';
  source: string;
}

export const logRecords = writable<LogRecord[]>([]);
export const nativeLogState = writable<NativeLogState>('idle');
export const extendedDiagnosticsEnabled = writable(false);

const records = new Map<string, LogRecord>();
const orderedRecords: LogRecord[] = [];
let nativeCursor = 0;
let nativeGeneration = 0;
let nativeUnavailableReported = false;

export function safeFormat(value: unknown): string {
  if (value instanceof Error) {
    try {
      return value.stack || `${value.name}: ${value.message}`;
    } catch {
      return '[Error]';
    }
  }
  if (typeof value === 'string') return value;
  if (value === null || value === undefined) return String(value);
  if (typeof value === 'bigint') return `${value}n`;
  if (typeof value !== 'object') {
    try {
      return String(value);
    } catch {
      return '[Unserializable]';
    }
  }

  try {
    const seen = new WeakSet<object>();
    const formatted = JSON.stringify(value, (_key, nestedValue: unknown) => {
      if (typeof nestedValue === 'bigint') return `${nestedValue}n`;
      if (typeof nestedValue === 'object' && nestedValue !== null) {
        if (seen.has(nestedValue)) return '[Circular]';
        seen.add(nestedValue);
      }
      return nestedValue;
    });
    return formatted ?? String(value);
  } catch {
    try {
      return String(value);
    } catch {
      return '[Unserializable]';
    }
  }
}

export function filterLogRecords(logs: LogRecord[], filters: LogFilters): LogRecord[] {
  const normalizedQuery = filters.query.trim().toLowerCase();
  return logs.filter((record) => {
    if (filters.level !== 'all' && record.level !== filters.level) return false;
    if (filters.source !== 'all' && record.source !== filters.source) return false;
    if (!normalizedQuery) return true;
    return `${record.source} ${record.level} ${record.message}`
      .toLowerCase()
      .includes(normalizedQuery);
  });
}

function compareRecords(left: LogRecord, right: LogRecord): number {
  return right.timestamp - left.timestamp
    || left.origin.localeCompare(right.origin)
    || right.sequence - left.sequence;
}

function insertOrdered(record: LogRecord): void {
  let start = 0;
  let end = orderedRecords.length;
  while (start < end) {
    const middle = (start + end) >>> 1;
    if (compareRecords(record, orderedRecords[middle]) < 0) {
      end = middle;
    } else {
      start = middle + 1;
    }
  }
  orderedRecords.splice(start, 0, record);
}

function trimRecords(): void {
  for (const record of orderedRecords.splice(MAX_ENTRIES)) {
    records.delete(record.id);
  }
}

export function clearLogRecords(): void {
  records.clear();
  orderedRecords.length = 0;
  nativeCursor = 0;
  nativeGeneration++;
  nativeUnavailableReported = false;
  logRecords.set([]);
}

export async function setExtendedDiagnostics(enabled: boolean): Promise<void> {
  const browserLogger = typeof window === 'undefined'
    ? undefined
    : window.StremioLightningLogger;
  browserLogger?.setExtendedDiagnostics(enabled);
  try {
    await setNativeExtendedDiagnostics(enabled);
    extendedDiagnosticsEnabled.set(enabled);
  } catch (error) {
    browserLogger?.setExtendedDiagnostics(false);
    extendedDiagnosticsEnabled.set(false);
    try {
      await setNativeExtendedDiagnostics(false);
    } catch {
      // Keep both producers disabled even when the host is unavailable.
    }
    throw error;
  }
}

export async function getDiagnosticReport(): Promise<string> {
  await window.StremioLightningLogger?.flush();
  return getNativeDiagnosticReport();
}

export async function clearDiagnostics(): Promise<void> {
  let nativeError: unknown;
  try {
    await clearNativeDiagnostics();
  } catch (error) {
    nativeError = error;
  }
  window.StremioLightningLogger?.clearDiagnostics();
  clearLogRecords();
  if (nativeError) throw nativeError;
}

function ingest(origin: LogOrigin, entries: StremioLightningLogEntry[]): void {
  const additions: LogRecord[] = [];
  for (const entry of entries) {
    const id = `${origin}:${entry.id}`;
    if (records.has(id)) continue;
    const record = { ...entry, id, origin, sequence: entry.id };
    records.set(id, record);
    additions.push(record);
  }
  if (additions.length === 0) return;

  if (additions.length === 1) {
    insertOrdered(additions[0]);
  } else {
    orderedRecords.push(...additions);
    orderedRecords.sort(compareRecords);
  }
  trimRecords();
  logRecords.set([...orderedRecords]);
}

function connectBrowserLogger(): void {
  if (typeof window === 'undefined') return;
  window.StremioLightningLogger?.subscribe((entry, initialEntries) => {
    if (initialEntries) {
      ingest('browser', initialEntries);
    } else if (entry) {
      ingest('browser', [entry]);
    }
  });
}

function writeFallback(level: LogLevel, values: unknown[]): void {
  const method = globalThis.console?.[level];
  method?.apply(globalThis.console, values);
}

export function createLogger(source: string): StremioLightningBoundLogger {
  const write = (level: LogLevel, values: unknown[]) => {
    const browserLogger = typeof window === 'undefined'
      ? undefined
      : window.StremioLightningLogger;
    if (browserLogger) {
      browserLogger[level](source, ...values);
    } else {
      writeFallback(level, values);
    }
  };

  return {
    debug: (...values) => write('debug', values),
    info: (...values) => write('info', values),
    warn: (...values) => write('warn', values),
    error: (...values) => write('error', values),
  };
}

function reportNativeUnavailable(error: unknown): void {
  if (nativeUnavailableReported) return;
  nativeUnavailableReported = true;
  const message = `Native logs unavailable: ${safeFormat(error)}`;
  ingest('adapter', [{
    id: 1,
    timestamp: Date.now(),
    level: 'warn',
    source: 'ui.logs',
    message,
  }]);
  writeFallback('warn', [message]);
}

export function startNativeLogPolling(): () => void {
  let stopped = false;
  let inFlight = false;
  nativeLogState.set(nativeCursor === 0 ? 'loading' : 'available');

  const poll = async () => {
    if (stopped || inFlight) return;
    inFlight = true;
    const generation = nativeGeneration;
    try {
      const entries = await getLogs(nativeCursor);
      if (stopped || generation !== nativeGeneration) return;
      ingest('native', entries);
      for (const entry of entries) nativeCursor = Math.max(nativeCursor, entry.id);
      nativeLogState.set('available');
    } catch (error) {
      if (stopped || generation !== nativeGeneration) return;
      nativeLogState.set('unavailable');
      reportNativeUnavailable(error);
    } finally {
      inFlight = false;
    }
  };

  void poll();
  const interval = window.setInterval(poll, 1_000);
  return () => {
    stopped = true;
    window.clearInterval(interval);
  };
}

connectBrowserLogger();
