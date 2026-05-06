# Handover

## Context

This repo is `stremio-lightning`, a Tauri 2 + Svelte desktop wrapper around `https://web.stremio.com/`.

The Linux debugging session started because the app worked on Windows but failed on Linux with:

- WebKitGTK mixed-content/access-control blocks for `http://127.0.0.1:11470/...`
- repeated native MPV errors: `Native MPV backend is not initialized`
- Stremio showing streaming-server/player failures / “streaming server is not available”

Important repo facts discovered:

- The app creates the Stremio webview in `src-tauri/src/lib.rs` using:
  - `WebviewBuilder::new(player::MAIN_APP_LABEL, WebviewUrl::External("https://web.stremio.com/"))`
  - `.initialization_script(&native_player_flag_js)`
  - `.initialization_script(bridge_js)` from `src-tauri/scripts/bridge.js`
  - `.initialization_script(mod_ui_js)` from `src/dist/mod-ui-svelte.iife.js`
- Native MPV is currently Windows-only:
  - `src-tauri/Cargo.toml` has `libmpv2` and `libmpv2-sys` only under `[target.'cfg(windows)'.dependencies]`.
  - `src-tauri/src/player.rs` gates the real MPV platform implementation with `#[cfg(windows)]`.
  - `player::native_player_enabled()` returns `cfg!(windows) && ...`.
- On Linux, `native_player_enabled()` is false, so the MPV backend is not initialized by design.
- The Stremio streaming server sidecar **does run** on Linux:
  - `127.0.0.1:11470` listens.
  - `curl -i http://127.0.0.1:11470/settings` returns `200 OK`.
  - `curl -i http://127.0.0.1:11470/casting` returns `200 OK`.
  - Therefore the current issue is not server startup; it is the website/worker being unable to connect to the HTTP loopback server from WebKitGTK.

## User constraints / decisions

The user wants:

- Linux MPV support eventually.
- No Electron.
- Concern about Chromium heaviness.

Recommended architecture after discussion:

- Stay on Tauri/WebKitGTK for now.
- Add Linux `libmpv` support later, preferably starting with an external native MPV window rather than embedded MPV.
- Use Rust/Tauri-side proxying for WebKitGTK localhost issues where possible.

## Changes made in this session

### 1. Added Rust streaming-server proxy command

File: `src-tauri/src/commands.rs`

Added:

- `use std::collections::HashMap;`
- `ProxyStreamingServerResponse`
- `is_hop_by_hop_header()`
- `validate_streaming_server_proxy_path()`
- `proxy_streaming_server_request(...)`

The command:

- only proxies to `http://127.0.0.1:11470`
- validates that paths are relative and start with `/`
- rejects paths with `://`, `//`, backslashes, or NUL chars
- supports methods: `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `OPTIONS`, `HEAD`
- strips hop-by-hop headers like `host`, `connection`, `content-length`, `transfer-encoding`, etc.
- uses `reqwest::Client` to perform the actual local HTTP request
- returns status, status text, headers, and body to JS
- logs requests as:

```text
[StreamingServerProxy] GET /settings -> 200
```

### 2. Registered the proxy command

File: `src-tauri/src/lib.rs`

Added to `tauri::generate_handler![...]`:

```rust
commands::proxy_streaming_server_request,
```

### 3. Added main-page fetch proxy

File: `src-tauri/scripts/bridge.js`

Added a `window.fetch` wrapper that intercepts:

```text
http://127.0.0.1:11470/...
http://localhost:11470/...
```

and routes those requests through:

```js
invoke("proxy_streaming_server_request", { method, path, headers, body })
```

Then returns a normal `Response` object to the caller.

Important limitation: this only affects fetches in the main page. Stremio Core performs the relevant streaming-server checks from `worker.js`, so this alone does not fix the worker-side mixed-content failures.

### 4. Worker-level proxy history and current implementation

Problem found:

- The blocked localhost requests are coming from `worker.js`, shown by logs like:

```text
ERROR stremio-core-web/src/env.rs:318 "Load failed" (worker.js)
Method: GET Url: http://127.0.0.1:11470/settings
```

- Tauri `initialization_script` runs in the main page, not inside Stremio's worker.
- Therefore, the main-page `window.fetch` proxy does not intercept `worker.js` fetches.

#### Failed/abandoned approaches

1. A first blob Worker wrapper was attempted, but it caused Stremio to show “something went wrong.” It was disabled.
2. A Linux WebKitGTK `settings.set_disable_web_security(true)` attempt was tried in `src-tauri/src/lib.rs` with a temporary `webkit2gtk` dependency. It logged as enabled but:
   - did not stop mixed-content blocks;
   - caused Tauri IPC failures like `missing Origin header` / `plugin:event|listen 500`.
   This approach was reverted, including removal of the `webkit2gtk` dependency.

#### Current worker-level implementation

File: `src-tauri/scripts/bridge.js`

There is now a redesigned `installStreamingServerWorkerProxy()` implementation.

It:

- wraps `window.Worker`
- only intercepts worker URLs matching `/scripts/worker.js`
- synchronously fetches the original worker source using `XMLHttpRequest`, because `new Worker(...)` must return a Worker synchronously
- injects a worker-local fetch shim before the original worker source
- intercepts only `http://127.0.0.1:11470/...` and `http://localhost:11470/...`
- forwards those requests from the worker to the main page via `postMessage`
- the main page then calls the Rust `proxy_streaming_server_request` command
- returns a normal `Response` inside the worker
- patches the worker source string:

```js
'n.g.importScripts&&(e=n.g.location+"");'
```
to use the original worker URL instead of `blob:` for webpack publicPath resolution.

After the user retested, the app opened far enough to show the injected UI, but WebKit logged:

```text
TypeError: "/stremio_core_web.js" cannot be parsed as a URL.
Not allowed to load local resource: blob://nullhttps//web.stremio.com/worker.js.map
```

Root cause:

- Stremio's generated worker sets `self.document={baseURI:self.location.href}` during `self.init(...)`.
- In the patched blob worker, `self.location.href` is a `blob:` URL, so later WASM glue code (`new URL("/stremio_core_web.js", document.baseURI)`) failed URL parsing under WebKitGTK.
- The original worker sourcemap directive also made WebKit try to resolve `worker.js.map` relative to the blob URL, producing the noisy `blob://nullhttps//web.stremio.com/worker.js.map` local-resource error.

Additional fix now applied in `bridge.js`:

```js
.replace(
  "self.document={baseURI:self.location.href}",
  "self.document={baseURI:" + JSON.stringify(absoluteUrl) + "}",
)
.replace(/\n?\/\/# sourceMappingURL=worker\.js\.map\s*$/g, "")
```

The blob source no longer appends a custom `//# sourceURL=...` line. This avoids WebKit's bad blob sourcemap/resource resolution while keeping worker asset resolution anchored to the real `worker.js` URL.

Expected console log if this path is used:

```text
[StremioLightning] Patched Stremio worker fetch proxy installed
```

Latest user runtime feedback: after the `document.baseURI` and sourcemap fixes, **the app opens now**. The remaining runtime state still needs checking: whether Stremio considers the streaming server available and whether Rust proxy logs appear for `/settings`, `/casting`, `/network-info`, and `/device-info`.

### 5. Shell transport restored; MPV commands ignored on Linux

Originally the fake desktop shell transport was disabled on Linux by setting:

```js
var shellTransportEnabled = nativePlayerEnabled;
```

That stopped MPV errors but caused/kept Stremio showing “streaming server is not available,” because Stremio appears to need desktop shell detection (`qt.webChannelTransport` / `chrome.webview`) for streaming-server integration.

Current state:

File: `src-tauri/scripts/bridge.js`

```js
var shellTransportEnabled = true;
```

The desktop shell bridge is now exposed on Linux too.

File: `src-tauri/src/shell_transport.rs`

For methods:

```text
mpv-observe-prop
mpv-set-prop
mpv-command
```

Rust now checks `player::native_player_enabled()`. If false, it returns `Ok(())` and drops the MPV command instead of calling `player::handle_transport(...)` and producing `Native MPV backend is not initialized`.

This keeps the shell transport alive for streaming-server integration while avoiding MPV error spam until Linux MPV is implemented.

## Validation already run

After the latest `bridge.js` fixes, the following passed:

```bash
npm run build:ui
cd src-tauri && cargo check
```
`diagnostics_scan` also reported no issues for:

```text
src-tauri/scripts/bridge.js
```

Earlier in the session, diagnostics also reported no issues for `src-tauri/src/lib.rs` and `src-tauri/Cargo.toml`. `src-tauri/Cargo.toml` had already been reverted from the abandoned `webkit2gtk` attempt.

Runtime checks already performed:

```bash
curl -i --max-time 5 http://127.0.0.1:11470/settings
curl -i --max-time 5 http://127.0.0.1:11470/casting
```

Both returned `200 OK`, confirming `server.cjs` is running and reachable from the OS.

## Current known state / issues

Most recent user runtime feedback:

- Earlier, before the final blob-worker fix, app startup reached the injected UI but showed:

```text
TypeError: "/stremio_core_web.js" cannot be parsed as a URL.
Error: window.cast api not available
Not allowed to load local resource: blob://nullhttps//web.stremio.com/worker.js.map
```

- The `"/stremio_core_web.js" cannot be parsed as a URL` error was fixed by patching Stremio's worker `self.document={baseURI:self.location.href}` assignment to use the original `worker.js` URL.
- The `blob://null...worker.js.map` noise was fixed by stripping the worker sourcemap directive and not appending a custom `sourceURL` to the blob worker.
- Latest user confirmation after these fixes: **"it opens now."**

Still unknown / should be checked next:

- Is “streaming server is not available” gone or still present?
- Do Rust logs show proxy hits like `[StreamingServerProxy] GET /settings -> 200`, `/casting`, `/network-info`, `/device-info`?
- Do WebKit blocked localhost warnings still appear?
- Is MPV error spam gone?

Important: the server is not the problem right now. It was verified running; any remaining failure is likely still around WebKitGTK worker access/proxying or Stremio desktop-shell integration.

## Recommended next steps

### Immediate test

Fully quit any running Tauri app process, then run:

```bash
npm run tauri dev
```

Check the web console for:

```text
[StremioLightning] Patched Stremio worker fetch proxy installed
```

Then check:

1. Does the app open normally?
2. “Something went wrong” appears to be gone as of the latest user feedback (`it opens now`).
3. Is “streaming server is not available” gone or still present?
4. Is the MPV error spam gone?
5. Do WebKit blocked localhost warnings still appear?
6. Do Rust logs show proxy hits like `[StreamingServerProxy] GET /settings -> 200` after Stremio reloads?
7. Is the `"/stremio_core_web.js" cannot be parsed as a URL` error still gone?

### If the patched worker log does not appear

Investigate whether Stremio changed the worker URL. The wrapper currently matches URLs ending in:

```text
/scripts/worker.js
```

The current web index fetched during debugging referenced:

```html
<script src="a83788ef670cc7d09900d9070e4bd19bbbf6bdfe/scripts/worker.js"></script>
```

If the app creates a different Worker URL, adjust `shouldPatchWorkerUrl()` in `bridge.js`.

### If the patched worker log appears but Stremio breaks

Inspect console errors for:

- worker construction errors
- synchronous XHR failures
- blob worker restrictions
- `importScripts` / `stremio_core_web.js` resolution failures
- WASM loading failures

The critical patches currently rewrite both webpack publicPath source from using `n.g.location` and Stremio's generated `self.document={baseURI:self.location.href}` assignment to use the original worker URL. If Stremio changes its bundled worker, either string replacement may need updating.

### If worker proxy still fails

Alternative next approaches:

1. **Serve a patched worker through Rust/Tauri rather than blob**
   - Fetch Stremio’s real worker source.
   - Inject the same fetch shim.
   - Serve it under a compatible custom protocol/origin.
   - Override only `new Worker(...)` to point at that served patched worker.

2. **Patch Stremio's streaming server URL before it reaches the worker**
   - Find how Stremio passes `http://127.0.0.1:11470` into core/worker.
   - Replace it with a proxied URL that WebKitGTK will allow.

3. **Investigate Wry/Tauri custom protocol behavior**
   - Tauri docs mention custom `http` schemes allowing mixed content in some contexts.
   - Stock external `https://web.stremio.com` cannot simply be made to allow HTTP mixed content through a normal Tauri option, and `set_disable_web_security` was not viable.

## Linux MPV implementation plan

Linux MPV is a separate task from the proxy issue.

Recommended MVP:

1. Add Linux dependencies:
   - system packages: `libmpv-dev`, `mpv`
   - Rust/Cargo target dependencies for Linux, likely using `libmpv2`/`libmpv2-sys` if they support Linux or direct FFI if not.
2. Add Linux implementation parallel to the current Windows `platform` module in `src-tauri/src/player.rs`.
3. Start with an external MPV window, not embedded.
4. Support current shell transport methods:
   - `mpv-command`
   - `mpv-set-prop`
   - `mpv-observe-prop`
5. Emit existing events back to Stremio:
   - `mpv-prop-change`
   - `mpv-event-ended`
6. Only after external Linux MPV works, consider embedded MPV.

## Files modified

Currently modified files from this debugging session:

```text
src-tauri/scripts/bridge.js
src-tauri/src/commands.rs
src-tauri/src/lib.rs
src-tauri/src/shell_transport.rs
```

`handover.md` is also updated at repo root.

Note: `src-tauri/Cargo.toml` was temporarily modified for `webkit2gtk`, but that change was reverted. Current final code should not include the Linux `webkit2gtk` dependency.

## Useful commands

```bash
# UI build
npm run build:ui

# Rust check
cd src-tauri && cargo check

# Dev run
npm run tauri dev

# Check local server directly
curl -i --max-time 5 http://127.0.0.1:11470/settings
curl -i --max-time 5 http://127.0.0.1:11470/casting

# See current changes
git status --short
git diff -- src-tauri/scripts/bridge.js src-tauri/src/commands.rs src-tauri/src/lib.rs src-tauri/src/shell_transport.rs
```

## Caution

Do not assume the server is down just because Stremio says “streaming server is not available.” In this session, the server was verified running; the failure was WebKitGTK blocking worker access to it.

Do not reintroduce the old async blob Worker wrapper. Worker construction must remain synchronous.

Do not reintroduce the `set_disable_web_security` / `webkit2gtk` attempt without redesigning it; it caused Tauri IPC `missing Origin header` failures and did not solve the mixed-content blocks.

The current worker wrapper is a targeted workaround and should be retested whenever Stremio updates its `worker.js` bundle, because it relies on matching `/scripts/worker.js` and patching a webpack publicPath snippet.
