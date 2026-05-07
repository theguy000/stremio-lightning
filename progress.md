# Progress

## Current State

- Branch: `master`
- Remote: `origin` (`https://github.com/theguy000/stremio-lightning.git`)
- Last base commit before this progress update: `0cb8737 Port mods and settings to Linux host`

## Completed Previously

- Configured Cargo on Linux to use `clang` with `mold` for faster linking.
- Removed the abandoned streaming-server proxy path:
  - removed `proxy_streaming_server_request` from the shared host command contract;
  - removed shared proxy request/response validation code;
  - removed the Tauri proxy command and command registration;
  - removed the injected fetch/worker proxy shims from `bridge.js`;
  - removed proxy types from the frontend host API.
- Updated Linux shell server lifecycle handling:
  - starts the streaming server before opening the Linux shell window;
  - stops the child process on drop/app exit;
  - supports restart;
  - detects exited children;
  - writes stdout/stderr to log files.
- Updated docs and manual smoke checks to use direct local server access instead of the removed proxy.

## Completed In This Change

- Completed the Phase 3 Linux runtime path against the current upstream `stremio-linux-shell` architecture:
  - replaced the fake black GL compositor and the rejected GTK3/WebKit2 experiment with GTK4 + WebKitGTK 6;
  - added the upstream-style `libepoxy` bootstrap required before using GTK GL functions;
  - created a GTK `ApplicationWindow` with a native `GLArea` MPV layer under a transparent WebKitGTK 6 webview overlay;
  - wired WebKit document-start scripts and `window.webkit.messageHandlers.ipc` to the existing Rust host runtime;
  - disabled WebKit web media so playback is routed through the native-player contract instead of browser media.
- Added Phase 3 shell-contract scaffolding:
  - added a Linux webview runtime object that owns URL loading state, devtools intent, document-start injection order, and JS event dispatch;
  - wired the runtime into Linux app startup instead of discarding the injection bundle after bootstrap logging;
  - added a JS-to-Rust IPC dispatcher for the Linux host adapter paths:
    - `invoke`;
    - `listen`;
    - `unlisten`;
    - minimal window methods;
    - `webview.setZoom`;
  - fixed listener id handling so ids created by the injected JavaScript adapter are the ids removed by `unlisten`;
  - added drainable host events so native events can be delivered back into the loaded webview through `window.__STREMIO_LIGHTNING_LINUX_DISPATCH__`;
  - ported streaming-server startup/reload behavior into the replacement Linux shell:
    - starts the sidecar during `stremio-lightning-linux` boot;
    - waits for the local server to accept connections on `127.0.0.1:11470`;
    - loads Stremio Web through the official local server proxy to avoid HTTPS-to-local-HTTP mixed-content blocking;
    - dispatches Stremio Web's `StreamingServer Reload` after the WebKit page finishes loading;
  - added unit coverage for document-start injection, IPC roundtrip, event delivery, listener removal, server commands, and MPV transport command mapping.
- Wired the Linux native-player command path to the rendered MPV layer using the official Stremio Linux shell architecture as the design reference:
  - kept the GTK `GLArea` video state as the owner of the real `libmpv2::Mpv` instance and render context;
  - changed `MpvPlayerBackend` from stub methods into a lightweight command handle attached to that rendered MPV state;
  - forwarded web-issued `mpv-command`, `mpv-set-prop`, `mpv-observe-prop`, and `native-player-stop` commands through the host backend into the GLArea-owned MPV instance;
  - added focused test coverage proving `MpvPlayerBackend` forwards commands to the attached renderer channel.

## Verification

- `cargo check -p stremio-lightning-linux` passed.
- `cargo test -p stremio-lightning-linux` passed: 30 unit tests, smoke test still ignored unless `STREMIO_LIGHTNING_LINUX_SMOKE=1`.
- `timeout 12s cargo run -p stremio-lightning-linux` reached the GTK4/WebKitGTK 6 load path and stayed alive until the timeout killed it.

## Runtime Status

Phase 3 now has a real Linux shell surface using GTK4/WebKitGTK 6 plus a native MPV GLArea layer, matching the current upstream Stremio Linux shell direction. The previous blocking runtime gap is fixed: `LinuxHost` and `MpvPlayerBackend` now send native-player transport commands to the same MPV command path used by the GTK `GLArea` renderer, so web-issued MPV commands no longer disappear into stub methods.

Runtime manual acceptance still needs an interactive pass:

- visual confirmation that the local server proxy renders Stremio Web in the Linux shell;
- visual confirmation that the mods button appears in that rendered page;
- visual confirmation that Stremio Web reports the local streaming server online after the replacement shell dispatches `StreamingServer Reload`;
- local sample playback rendered through the MPV GLArea below the WebKitGTK overlay.

## Next Work

Immediate goal for the next commit:

1. Add a local sample-video smoke path proving load, render, stop/end cleanup, and WebKit overlay visibility.
2. Run the Linux manual smoke checks against the actual rendered Stremio Web UI.
3. Forward MPV property-change/end events from the rendered MPV instance back through the Linux host event bridge.
4. Exercise Phase 4 mod/plugin flows inside the Linux shell with the real webview.
