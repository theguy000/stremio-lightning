# Windows WebView2 Shell Crate Plan

## Purpose

Create the direct Windows shell crate first, before copying optional Stremio Community features. This plan is the foundation for Phase 6. The feature parity backlog in `docs/stremio-community-feature-parity-todo.md` starts after this crate can launch, host WebView2, communicate with the web app, start the local server, and play through MPV.

## Scope

This plan covers only the minimum direct Windows shell needed to replace the Tauri runtime path.

In scope:

- Workspace crate structure.
- Windows-only Win32/WebView2 runtime.
- Shared bridge injection.
- Host command transport.
- Native MPV playback through `HWND`/`wid`.
- Local server process lifecycle.
- Single-instance/open-media baseline.
- Dependency/resource layout owned by the Windows crate.
- Tests for platform-neutral command mapping and contract behavior.

Out of scope until the crate works:

- Browser extensions.
- Webmods folder compatibility.
- Anime4K/AnimeJaNai bundles.
- ThumbFast previews.
- Discord Rich Presence.
- Built-in updater.
- Scoop/Chocolatey/Winget packaging.
- Advanced tray menu styling.

## Current State

- `crates/stremio-lightning-windows` exists.
- It is included in the root workspace.
- It has initial modules: `main.rs`, `lib.rs`, `host.rs`, `player.rs`, `webview.rs`, `window.rs`, `server.rs`, `single_instance.rs`, `resources.rs`, `settings.rs`, and `build.rs`.
- The shared injected bridge has moved to `web/bridge/bridge.js`.
- Tauri and Linux include paths were updated to the shared bridge location.
- Windows dependency setup exists through `scripts/download-windows-shell-deps.sh` and `npm run setup:windows-shell`.
- Milestone 1 is complete: crate shape, build boundaries, non-Windows stubs/tests, and developer command documentation are in place.
- Current Windows runtime code is mostly scaffolded/stubbed; it does not yet create a real Win32 window, create WebView2, route real messages, start real MPV playback, or supervise the server.

## Developer Command Sequence

From a fresh checkout on Windows:

1. Install Rust with the MSVC toolchain and ensure `x86_64-pc-windows-msvc` is available.
2. Install Node.js project dependencies with `npm install` if the web app dependencies are not already installed.
3. Install helper tools used by the dependency script: GitHub CLI, `curl`, `unzip`, and 7-Zip or `7zz`.
4. Run `npm run setup:windows-shell` from the repository root to populate `crates/stremio-lightning-windows/resources` and `crates/stremio-lightning-windows/mpv-dev`.
5. Validate the Windows crate with `cargo test -p stremio-lightning-windows`.
6. Validate the Windows target with `cargo check -p stremio-lightning-windows --target x86_64-pc-windows-msvc`.
7. Launch the direct shell with `cargo run -p stremio-lightning-windows` once Milestone 2 and Milestone 3 are implemented.

Linux/non-Windows development remains limited to platform-neutral checks such as `cargo test -p stremio-lightning-windows`; runtime launch is Windows-only.

## Milestone 1: Crate Shape And Build Boundaries

- [x] Add `crates/stremio-lightning-windows` to workspace members.
- [x] Add Windows shell entry points and module boundaries.
- [x] Add Windows-only dependencies behind target gates where needed.
- [x] Add build script for Windows libmpv link/resource setup.
- [x] Move shared injected bridge out of `src-tauri`.
- [x] Split Windows modules by responsibility if needed: `window`, `webview`, `host`, `player`, `server`, `single_instance`, `resources`, `settings`.
- [x] Keep non-Windows builds compiling with stubs and platform-neutral tests.
- [x] Document developer command sequence for Windows setup/build/run.

Acceptance:

- `cargo test -p stremio-lightning-windows` passes on Linux for non-Windows tests.
- `cargo check -p stremio-lightning-windows --target x86_64-pc-windows-msvc` is the expected Windows validation command.
- No Windows shell runtime code depends on `src-tauri` paths.

Validation completed:

- `cargo fmt --all`
- `cargo test -p stremio-lightning-windows`
- `cargo check -p stremio-lightning-windows --target x86_64-pc-windows-msvc`
- `cargo test --workspace`

## Milestone 2: Native Window And Message Loop

- [x] Choose the implementation layer: raw `windows` crate Win32, `native-windows-gui`, or a small wrapper.
- [x] Create the main native window and own the parent `HWND`.
- [x] Register app class, title, icon, default size, minimum size, and dark background.
- [x] Implement the Win32 message loop.
- [x] Handle `WM_SIZE`, `WM_CLOSE`, `WM_DESTROY`, `WM_ACTIVATE`, and `WM_DPICHANGED` at minimum.
- [x] Add a safe channel or event mechanism for background threads to notify the UI thread.
- [x] Add clean shutdown ordering for WebView2, MPV, server process, and IPC resources.

Acceptance:

- On Windows, `cargo run -p stremio-lightning-windows` opens a native blank window.
- The window closes cleanly without orphaned processes or panics.
- The app can receive internal messages from background tasks on the UI thread.

Implementation notes:

- The crate uses raw Win32 through the `windows` crate to preserve direct `HWND` ownership for WebView2 and MPV.
- `src/window.rs` registers the app class, creates the main overlapped window, stores window state in `GWLP_USERDATA`, enforces the minimum track size through `WM_GETMINMAXINFO`, and owns the message loop.
- `WM_SIZE`, `WM_CLOSE`, `WM_DESTROY`, `WM_ACTIVATE`, and `WM_DPICHANGED` are handled as the baseline event surface for later WebView2/MPV work.
- `UI_THREAD_WAKE_MESSAGE` reserves a `WM_APP` message for background-thread-to-UI-thread notifications.
- `src/webview.rs` now routes the Windows shell run path to the native blank window until Milestone 3 creates the WebView2 controller.

Validation completed:

- `cargo fmt --all`
- `cargo test -p stremio-lightning-windows`
- `cargo check -p stremio-lightning-windows --target x86_64-pc-windows-msvc`

## Milestone 3: WebView2 Host

- [x] Create WebView2 environment.
- [x] Create WebView2 controller attached to the native `HWND`.
- [x] Resize WebView2 to the client rect on `WM_SIZE`.
- [x] Navigate to configured web UI URL.
- [x] Add `--webui-url=<url>` command-line support.
- [x] Configure basic settings: autoplay, status bar off, zoom policy, devtools policy.
- [x] Add WebView2 initialization error reporting.
- [x] Add native-to-web message posting.
- [x] Add web-to-native message receiving.

Acceptance:

- The hosted Stremio web UI loads in WebView2.
- Native can receive a simple test message from JS.
- Native can post a simple test message to JS.
- WebView2 resizes with the native window.

Implementation notes:

- The Windows shell now uses `webview2-com` and initializes COM/WebView2 from the UI thread.
- `src/window.rs` exposes a small native-window handler hook so `src/webview.rs` can attach a WebView2 controller after the parent `HWND` exists and resize it from `WM_SIZE`.
- The WebView2 environment uses browser arguments for autoplay and baseline WebView2 UI suppression.
- The hosted WebView2 injects the existing Windows adapter, native-player flag, and shared bridge through document-created scripts.
- The first native-to-web smoke message is posted as `native-ready`; incoming WebView2 messages are received and logged until Milestone 4 wires the full host contract.

Validation completed:

- `cargo fmt --all`
- `cargo test -p stremio-lightning-windows`
- `cargo check -p stremio-lightning-windows --target x86_64-pc-windows-msvc`

## Milestone 4: Bridge Injection And Host Contract

- [x] Inject the Windows WebView2 adapter at document start or earliest supported equivalent.
- [x] Inject/load `web/bridge/bridge.js` without duplicating shell-specific logic in the shared bridge.
- [x] Implement handshake/initialization expected by the hosted web app.
- [x] Implement request/response IDs.
- [x] Implement structured native errors for invalid commands.
- [x] Implement host commands currently used by the app and mods panel.
- [x] Implement native event listener registration/unregistration.
- [x] Add JSON fixtures for shared host contract tests.

Acceptance:

- The mods panel can call native host APIs through the shared bridge.
- Unsupported native commands fail with structured errors.
- Contract tests are shared with or mirrored from Linux/Tauri host behavior.

Implementation notes:

- The Windows WebView2 adapter now owns a Promise-based request/response map and sends `{ id, kind, payload }` messages to native through `chrome.webview.postMessage`.
- `src/host.rs` mirrors the Linux IPC envelope for `invoke`, `listen`, `unlisten`, window commands, and `webview.setZoom`, returning structured `{ kind: "response", id, ok, value }` messages.
- Native event listener registration is tracked by ID and drained back to WebView2 as `{ kind: "event", event, payload }` messages.
- Shell transport handshake messages are emitted through the shared `shell-transport-message` event so `web/bridge/bridge.js` keeps the same Qt-compatible transport path.
- The JSON contract fixture lives at `crates/stremio-lightning-windows/tests/fixtures/host_contract.json` and is exercised by platform-neutral unit tests.

Validation completed:

- `cargo fmt --all`
- `cargo test -p stremio-lightning-windows`
- `cargo check -p stremio-lightning-windows --target x86_64-pc-windows-msvc`

## Milestone 5: Native MPV Baseline

- [x] Load/link libmpv from the Windows shell resource layout.
- [x] Initialize MPV in `crates/stremio-lightning-windows`.
- [x] Pass the native `HWND` to MPV via `wid`.
- [x] Set baseline MPV options: app title, audio client name, terminal/log level, hwdec, audio fallback.
- [x] Implement `mpv-command`.
- [x] Implement `mpv-set-prop`.
- [x] Implement `mpv-observe-prop`.
- [x] Implement MPV event loop and wakeup handling.
- [x] Emit `mpv-prop-change` to the web app.
- [x] Emit `mpv-event-ended` to the web app.
- [x] Implement `native-player-stop`.
- [x] Cleanly shut down MPV on app exit.

Implementation notes:

- `crates/stremio-lightning-windows/src/player.rs` now owns a Windows-only `libmpv2` backend initialized from the native Win32 `HWND`.
- The backend sets `wid`, app/audio title, terminal/log level, `quiet`, `hwdec`, and audio fallback options during MPV initialization.
- Shell transport methods `mpv-command`, `mpv-set-prop`, `mpv-observe-prop`, and `native-player-stop` are mapped to MPV calls.
- MPV `PropertyChange` and `EndFile` events are converted to shared `mpv-prop-change` and `mpv-event-ended` transport messages and posted back through WebView2 listener dispatch.
- The WebView2 window host initializes MPV after native window creation and drains MPV events on the reserved UI-thread wake message.
- Linux-side validation completed with `cargo test -p stremio-lightning-windows` and `cargo check -p stremio-lightning-windows --target x86_64-pc-windows-msvc`; runtime playback validation still requires Windows.

Acceptance:

- `mpv-command loadfile <url-or-path>` starts playback in the native Windows shell.
- Observed MPV properties reach the web app.
- End/error events reach the web app.
- MPV does not leak after window close.

## Milestone 6: Server Runtime Baseline

- [x] Define resource paths for Windows runtime assets owned by the Windows shell crate.
- [x] Start bundled `stremio-runtime.exe server.cjs`.
- [x] Attach server process to a Windows Job Object with kill-on-close behavior.
- [x] Pipe stdout/stderr to Windows shell log files.
- [ ] Detect readiness from stdout or HTTP health check.
- [x] Emit server status events to the web app.
- [x] Stop server on app exit.
- [x] Add `--streaming-server-disabled` for development.

Implementation notes:

- `crates/stremio-lightning-windows/src/server.rs` now owns the Windows streaming server supervisor with `start`, `stop`, `restart`, and running-state checks.
- The server command launches `resources/stremio-runtime.exe resources/server.cjs` and explicitly passes `NO_CORS=1`, `FFMPEG_BIN`, and `FFPROBE_BIN` to match the existing Tauri/Linux launcher behavior while keeping the shell-ng-compatible flat resource layout.
- On Windows, the spawned server process is assigned to a Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` so it is terminated with the shell process.
- `src/host.rs` wires `get_streaming_server_status`, `start_streaming_server`, `stop_streaming_server`, and `restart_streaming_server`, and emits `server-started` / `server-stopped` through the existing host event channel.
- The WebView2 shell starts the server during native window initialization unless `--streaming-server-disabled` is passed.
- Readiness detection remains a follow-up hardening item because the current app contract mirrors Linux/Tauri boolean status and start/stop events.

Acceptance:

- The app starts the local server on Windows.
- The web UI receives server status/address.
- Closing the app terminates the server process.

## Milestone 7: Single Instance And Open Media Baseline

- [ ] Choose IPC: named pipe or mutex plus `WM_COPYDATA`.
- [ ] Detect second instance.
- [ ] Focus/restore existing window.
- [ ] Forward first non-option argument to existing instance.
- [ ] Classify file paths, `stremio://`, `magnet:`, and torrent arguments.
- [ ] Queue launch/open-media events until web app is ready.
- [ ] Emit open-media/deeplink events to the web app.

Acceptance:

- A second invocation does not start a duplicate shell/server/player stack.
- Opening a media/deeplink while the app is running focuses the existing window and routes the event to the web app.

## Milestone 8: Window Behavior Baseline

- [ ] Implement fullscreen toggle and restore prior placement.
- [ ] Implement focus command.
- [ ] Implement minimize/maximize/restore commands if used by the bridge.
- [ ] Emit window state/visibility events.
- [ ] Handle media keys through `WM_APPCOMMAND` or registered hotkeys.
- [ ] Implement safe external URL open policy.
- [ ] Implement navigation blocking for untrusted destinations.

Acceptance:

- Fullscreen works and restores correctly.
- Window state reaches the web app.
- Media keys trigger expected web/player actions.
- External links cannot launch arbitrary local commands.

## Milestone 9: Resource Setup And Packaging Baseline

- [ ] Finish `scripts/download-windows-shell-deps.sh` to produce the exact dev resource layout.
- [ ] Ensure `libmpv-2.dll` is found in dev and packaged layouts.
- [ ] Ensure ffmpeg/ffprobe and runtime assets are available where needed.
- [ ] Add Windows icon and manifest resources.
- [ ] Document fresh Windows checkout setup.
- [ ] Add minimal installer or portable archive notes after the runtime works.

Acceptance:

- A clean Windows checkout can run setup, build, and launch the direct shell.
- Runtime dependencies are not loaded from `src-tauri`.
- Packaged/dev layouts use the same documented resource resolution rules.

## Milestone 10: Verification And Exit

- [ ] Run `cargo fmt --all`.
- [ ] Run `cargo test -p stremio-lightning-windows`.
- [ ] Run `cargo test --workspace` when changes affect shared code.
- [ ] Run Windows build/check command.
- [ ] Run manual Windows smoke checklist: launch, web UI, mods panel, server, playback, fullscreen, media keys, open-media, shutdown.
- [ ] Update `docs/windows-webview2-shell-gap-analysis.md` with completed work.
- [ ] Only then begin `docs/stremio-community-feature-parity-todo.md` P1/P2 feature expansion.

Exit criteria:

- Windows runs through `crates/stremio-lightning-windows` with no Tauri runtime dependency.
- WebView2 hosts the app and loads the shared bridge.
- Native MPV playback works through direct `HWND` ownership.
- Local server lifecycle works.
- Single-instance/open-media baseline works.
- Tests and manual smoke are documented.
