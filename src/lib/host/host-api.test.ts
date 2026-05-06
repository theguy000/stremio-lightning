import { afterEach, describe, expect, it, vi } from 'vitest';
import type { StremioLightningHost } from './host-api';

afterEach(() => {
  delete window.StremioLightningHost;
  vi.resetModules();
});

function createHost(): StremioLightningHost {
  return {
    invoke: vi.fn().mockResolvedValue(undefined) as StremioLightningHost['invoke'],
    listen: vi.fn().mockResolvedValue(() => {}) as StremioLightningHost['listen'],
    window: {
      minimize: vi.fn().mockResolvedValue(undefined),
      toggleMaximize: vi.fn().mockResolvedValue(undefined),
      close: vi.fn().mockResolvedValue(undefined),
      isMaximized: vi.fn().mockResolvedValue(false),
      isFullscreen: vi.fn().mockResolvedValue(false),
      setFullscreen: vi.fn().mockResolvedValue(undefined),
      startDragging: vi.fn().mockResolvedValue(undefined),
    },
    webview: {
      setZoom: vi.fn().mockResolvedValue(undefined),
    },
  };
}

describe('host-api', () => {
  it('returns the installed host', async () => {
    const host = createHost();
    window.StremioLightningHost = host;
    const { getHost, hasHost } = await import('./host-api');

    expect(hasHost()).toBe(true);
    expect(getHost()).toBe(host);
  });

  it('logs a missing host once and then fails closed', async () => {
    const error = vi.spyOn(console, 'error').mockImplementation(() => {});
    const { getHost, hasHost } = await import('./host-api');

    expect(hasHost()).toBe(false);
    expect(hasHost()).toBe(false);
    expect(() => getHost()).toThrow('Stremio Lightning host adapter is not available');
    expect(error).toHaveBeenCalledTimes(1);
    expect(error).toHaveBeenCalledWith('[StremioLightning] host adapter not available');
  });
});
