// Shared endpoint probe + normalization for streaming-server parity checks.
//
// The goal is a stable, comparable snapshot of the HTTP surface each desktop
// shell actually depends on, so a legacy server.js engine and the open-source
// stream-server engine can be diffed side by side.
//
// "platforms" lists which shells treat an endpoint as a critical entry point:
//   - Windows loads web.stremio.com directly and calls the local server
//     cross-origin, so CORS is what matters there (no /proxy, no /).
//   - Linux and macOS load the UI through the local server's /proxy route.
//   - Nobody loads "/" directly, so its redirect is informational only.

export const DEFAULT_BASE_URL = "http://127.0.0.1:11470";

const PROBE_TIMEOUT_MS = 10000;

export const ENDPOINTS = [
  { id: "heartbeat", path: "/heartbeat", kind: "json", platforms: ["windows", "linux", "macos"] },
  { id: "stats", path: "/stats.json", kind: "json", platforms: ["windows", "linux", "macos"] },
  { id: "network-info", path: "/network-info", kind: "json", platforms: ["windows", "linux", "macos"] },
  { id: "settings", path: "/settings", kind: "json", platforms: ["windows", "linux", "macos"] },
  {
    id: "cors-heartbeat",
    path: "/heartbeat",
    kind: "cors",
    platforms: ["windows"],
    headers: { Origin: "https://web.stremio.com" },
  },
  { id: "root-redirect", path: "/", kind: "redirect", platforms: [] },
  {
    id: "proxy-web",
    path: "/proxy/d=" + encodeURIComponent("https://web.stremio.com/"),
    kind: "document",
    platforms: ["linux", "macos"],
  },
];

export function isApplicable(endpoint, profile) {
  if (profile === "all") return endpoint.platforms.length > 0;
  return endpoint.platforms.includes(profile);
}

function lengthBucket(bytes) {
  if (bytes == null) return null;
  if (bytes < 1024) return `${bytes}B`;
  return `${Math.round(bytes / 1024)}KB`;
}

function normalizeLocation(location, baseUrl) {
  if (!location) return null;
  return location.split(baseUrl).join("{base}");
}

export async function fetchWithTimeout(url, options = {}, timeoutMs = PROBE_TIMEOUT_MS) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetch(url, { ...options, signal: controller.signal });
  } finally {
    clearTimeout(timer);
  }
}

export async function probeEndpoint(baseUrl, endpoint) {
  const url = baseUrl + endpoint.path;
  const common = { status: null, contentType: null, error: null };
  const init = endpoint.headers ? { headers: endpoint.headers } : {};

  try {
    if (endpoint.kind === "redirect") {
      const res = await fetchWithTimeout(url, { ...init, redirect: "manual" });
      return {
        ...common,
        status: res.status,
        contentType: res.headers.get("content-type"),
        location: normalizeLocation(res.headers.get("location"), baseUrl),
      };
    }

    const res = await fetchWithTimeout(url, init);
    const contentType = res.headers.get("content-type");

    if (endpoint.kind === "cors") {
      return {
        ...common,
        status: res.status,
        contentType,
        allowOrigin: res.headers.get("access-control-allow-origin"),
      };
    }

    const body = await res.text();

    if (endpoint.kind === "json") {
      let keys = null;
      let parsed = true;
      try {
        const value = JSON.parse(body);
        keys = value && typeof value === "object" ? Object.keys(value).sort() : [];
      } catch {
        parsed = false;
      }
      return { ...common, status: res.status, contentType, jsonParsed: parsed, keys };
    }

    return {
      ...common,
      status: res.status,
      contentType,
      size: lengthBucket(body.length),
      bodyHead: body.replace(/\s+/g, " ").trim().slice(0, 300),
    };
  } catch (error) {
    return { ...common, error: String(error?.message ?? error) };
  }
}

export async function probeAll(baseUrl) {
  const endpoints = {};
  for (const endpoint of ENDPOINTS) {
    endpoints[endpoint.id] = await probeEndpoint(baseUrl, endpoint);
  }
  return { baseUrl, probedAt: new Date().toISOString(), endpoints };
}

// Compare two snapshots and return a per-endpoint diff report for a profile.
export function diffSnapshots(baseline, candidate, profile = "all") {
  const rows = [];
  for (const endpoint of ENDPOINTS) {
    const a = baseline.endpoints[endpoint.id];
    const b = candidate.endpoints[endpoint.id];
    rows.push({
      id: endpoint.id,
      applicable: isApplicable(endpoint, profile),
      match: JSON.stringify(a) === JSON.stringify(b),
      baseline: a,
      candidate: b,
    });
  }
  return rows;
}
