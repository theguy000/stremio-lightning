declare const __APP_VERSION__: string;

type StremioLightningLogLevel = 'debug' | 'info' | 'warn' | 'error';

interface StremioLightningLogEntry {
  id: number;
  timestamp: number;
  level: StremioLightningLogLevel;
  source: string;
  message: string;
}

interface StremioLightningBoundLogger {
  debug(...values: unknown[]): void;
  info(...values: unknown[]): void;
  warn(...values: unknown[]): void;
  error(...values: unknown[]): void;
}

interface StremioLightningLogger extends StremioLightningBoundLogger {
  debug(source: string, ...values: unknown[]): void;
  info(source: string, ...values: unknown[]): void;
  warn(source: string, ...values: unknown[]): void;
  error(source: string, ...values: unknown[]): void;
  bind(source: string): StremioLightningBoundLogger;
  entries(): StremioLightningLogEntry[];
  subscribe(
    listener: (
      entry: StremioLightningLogEntry | null,
      initialEntries?: StremioLightningLogEntry[],
    ) => void,
  ): () => void;
}

interface Window {
  StremioLightningLogger?: StremioLightningLogger;
}
