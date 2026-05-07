# Current Baseline

This document freezes the current Tauri baseline before the platform shell migration.

## Supported Runtime Flows

### App startup

1. `src-tauri/src/main.rs` applies Linux WebKitGTK backend workarounds before Tauri initializes.
2. `stremio_lightning_lib::run()` registers Tauri plugins, managed state, and command handlers.
3. Setup ensures plugin/theme directories exist under app data.
4. The main window and child webview load `https://web.stremio.com/`.
5. Initialization scripts are injected in this order:
   - native-player/WebKitGTK capability flags
   - `web/bridge/bridge.js`
   - bundled Svelte mod UI IIFE
6. The streaming server starts during setup and the web app receives a delayed `StreamingServer Reload` dispatch once the local server is ready.

### Injected bridge load

1. `bridge.js` installs desktop shell shims expected by Stremio Web.
2. The web app sends shell transport messages through the Tauri `shell_transport_send` command.
3. Rust parses JSON-RPC-like transport payloads in `shell_transport.rs`.
4. The bridge readiness command flips the native bridge-ready condition variable.
5. Native events are queued until the web app sends `app-ready`, then flushed as `shell-transport-message` events.

### Mods panel open/close

1. The injected Svelte mod UI adds the mods button to the Stremio Web page.
2. The button opens `ModsPanel.svelte`.
3. Tabs call Tauri commands through `src/lib/ipc.ts`.
4. Closing the panel returns control to the hosted Stremio Web UI without unloading the bridge.

### Plugin install/load/unload

1. Marketplace/install actions call `download_mod`.
2. Rust stores plugin files in app data under `stremio-lightning/plugins`.
3. Installed plugin files must use the `.plugin.js` extension.
4. `get_plugins` lists installed plugins and parses JSDoc metadata.
5. The injected plugin loader evaluates enabled plugin files and exposes `window.StremioEnhancedAPI`.
6. Disabling/unloading is handled in the injected UI layer by removing the plugin from the active runtime set.

### Theme apply/remove

1. Marketplace/install actions call `download_mod`.
2. Rust stores theme files in app data under `stremio-lightning/themes`.
3. Installed theme files must use the `.theme.css` extension.
4. `get_themes` lists installed themes and parses JSDoc metadata.
5. Applying a theme injects CSS into the hosted page.
6. Removing a theme disables the injected CSS and can delete the stored file through `delete_mod`.

### Local streaming server start/stop/status

1. `start_streaming_server` launches the `stremio-runtime` sidecar with `server.cjs`, `ffmpeg`, and `ffprobe` resource paths.
2. Logs are appended to `stremio-server.log` in the app data directory.
3. Unexpected sidecar termination auto-restarts after a short delay.
4. `stop_streaming_server` marks the stop as intentional and kills the child process.
5. `get_streaming_server_status` reports whether a child process is currently tracked.
6. The hosted UI talks to `http://127.0.0.1:11470` directly; the shell no longer exposes a Rust `proxy_streaming_server_request` command.

### Native player command flow

1. The setup script exposes `window.__STREMIO_LIGHTNING_ENABLE_NATIVE_PLAYER__`.
2. Native MPV is enabled by default only on Windows.
3. Non-Windows builds keep web playback/external fallback behavior and drop MPV transport commands without failing the shell transport.
4. Windows initializes libmpv against the main window handle and starts a background event loop.
5. Transport methods map as follows:
   - `mpv-observe-prop` observes MPV properties.
   - `mpv-set-prop` sends property updates.
   - `mpv-command` sends commands such as `loadfile` and `stop`.
6. MPV property and end-file events are emitted back to the web bridge through `shell-transport-message`.

## Platform Gate

The current Tauri native player path is Windows-only. Linux and macOS builds report native player backend `disabled`, so Linux can continue using web playback or external fallback while the new native Linux shell is developed.
