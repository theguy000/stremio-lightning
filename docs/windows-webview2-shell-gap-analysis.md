# Windows WebView2 Shell Gap Analysis

## Purpose

This document tracks what is already done for the direct Windows shell migration, what is missing, and what is needed to reach feature parity with the old Windows Tauri path.

The implementation reference is `https://github.com/Stremio/stremio-shell-ng`, a Windows-only Rust shell using native WebView2 and MPV. It is not a drop-in source copy for Stremio Lightning because this project has a shared Rust core, shared injected bridge, mod/plugin behavior, and existing Tauri compatibility path. It is the best reference for the Windows shell shape because it already proves the direct WebView2 + native MPV architecture that Phase 6 targets.

## Reference Architecture From stremio-shell-ng

`stremio-shell-ng` demonstrates these relevant patterns:

- A native Windows application owns the main `HWND` instead of delegating window ownership to Tauri or Qt.
- Web rendering is hosted through WebView2, created against the native parent window.
- MPV renders directly into the same native window by initializing libmpv with the Windows `HWND` through the `wid` property.
- WebView2 is configured with app-specific browser flags, disabled status bar, disabled zoom controls, disabled host objects, disabled script dialogs, and optional devtools.
- Shell IPC uses WebView2 post-message handling and a Qt-compatible web channel shim for the upstream Stremio web app.
- Web messages are parsed as RPC requests and routed to shell features such as fullscreen, quit, focus, open external URL, updater click, and MPV commands.
- MPV commands and observed properties are parsed through typed Rust enums before reaching libmpv.
- MPV events are converted back into web-compatible shell events such as `mpv-prop-change` and `mpv-event-ended`.
- The Stremio server/runtime process is launched as a child process, attached to a Windows job object, and watched through stdout/stderr.
- Single-instance/open-media behavior uses a Windows named pipe.
- Window helpers manage center-on-start, min size, fullscreen style changes, topmost state, foreground activation, minimized/maximized/active state reporting, and tray visibility.
- Packaging embeds Windows resources and extracts/selects architecture-specific libmpv artifacts during build.

Important reference files in `stremio-shell-ng`:

- `src/stremio_app/app.rs`: main app composition, message routing, updater trigger, player/web channel bridging, window state notifications.
- `src/stremio_app/stremio_wevbiew/wevbiew.rs`: WebView2 environment/controller creation, browser flags, navigation, message handling, script injection, resize/focus/media-key handling.
- `src/stremio_app/stremio_player/player.rs`: MPV initialization with `wid`, MPV event thread, MPV command thread, event-to-web response conversion.
- `src/stremio_app/stremio_player/communication.rs`: typed incoming MPV command/property schema and outgoing MPV event schema.
- `src/stremio_app/stremio_server/server.rs`: child Stremio runtime process startup, Windows job object setup, stdout/stderr capture, crash notification.
- `src/stremio_app/ipc.rs`: web RPC request/response shapes and handshake response.
- `src/stremio_app/window_helper.rs`: Win32 fullscreen/topmost/focus/window-state helpers.
- `src/stremio_app/named_pipe.rs`: named-pipe single-instance/open-media IPC.
- `build.rs`: Windows manifest/icon/resource embedding and architecture-specific libmpv extraction/link setup.

## Current Stremio Lightning Progress

The migration has started but is still scaffolding-heavy on Windows.

Completed so far:

- Added `crates/stremio-lightning-windows` as a workspace crate.
- Added Windows shell entry points and module boundaries:
  - `src/main.rs`
  - `src/lib.rs`
  - `src/host.rs`
  - `src/player.rs`
  - `src/webview.rs`
  - `src/window.rs`
  - `src/server.rs`
  - `src/single_instance.rs`
  - `src/resources.rs`
  - `src/settings.rs`
  - `build.rs`
- Moved the shared injected bridge from `src-tauri/scripts/bridge.js` to `web/bridge/bridge.js`.
- Updated existing Tauri and Linux include paths to use the neutral shared bridge location.
- Added Windows-shell-owned dependency setup through `scripts/download-windows-shell-deps.sh`.
- Added `setup:windows-shell` to `package.json`.
- Added `.gitignore` entries for downloaded Windows shell resources and MPV dev files.
- Added Windows-only dependencies for libmpv/windows bindings in the new crate.
- Added build-script link setup for the Windows shell crate.
- Added initial host/player/webview tests that compile on Linux by keeping real Windows runtime code gated or stubbed.
- Completed Milestone 1 of `docs/windows-webview2-shell-crate-plan.md`.
- Split the Windows crate into the responsibility modules needed by later milestones without implementing runtime behavior early.
- Added Windows-resource layout helpers, baseline server config scaffolding, launch argument classification, shell settings, and default window config.
- Made `stremio-lightning-windows` depend on `stremio-lightning-core` with default features disabled so the Windows target check only pulls platform-neutral host/player/settings contracts.
- Moved shared filename validation into `stremio-lightning-core::validation`, keeping mods behind the core `mods` feature.
- Completed Milestone 2 of `docs/windows-webview2-shell-crate-plan.md`.
- Added a raw Win32 native window baseline in `src/window.rs` with app class registration, direct `HWND` ownership, minimum size handling, message loop ownership, and a reserved UI-thread wake message.
- Routed the Windows shell run path through the native blank window while WebView2 remains a Milestone 3 task.
- Verified the workspace after the scaffold with `cargo fmt --all`, `cargo test -p stremio-lightning-windows`, and `cargo test --workspace`.
- Verified Milestone 1 with `cargo fmt --all`, `cargo test -p stremio-lightning-windows`, `cargo check -p stremio-lightning-windows --target x86_64-pc-windows-msvc`, and `cargo test --workspace`.
- Verified Milestone 2 with `cargo fmt --all`, `cargo test -p stremio-lightning-windows`, and `cargo check -p stremio-lightning-windows --target x86_64-pc-windows-msvc`.
- Completed Milestone 3 of `docs/windows-webview2-shell-crate-plan.md`.
- Added WebView2 environment/controller creation against the native parent `HWND`, client-rect resizing on `WM_SIZE`, navigation to the configured web UI URL, and `--webui-url=<url>` override support.
- Added baseline WebView2 settings and browser flags for autoplay, status bar suppression, zoom control suppression, host-object disablement, script-dialog suppression, and debug-build devtools policy.
- Added document-created injection of the Windows adapter, native-player flag, and shared bridge script.
- Added simple WebView2 native-to-web and web-to-native smoke plumbing; full request/response host routing remains Milestone 4.
- Completed Milestone 4 of `docs/windows-webview2-shell-crate-plan.md`.
- Added Promise-based Windows WebView2 IPC with request IDs, native responses, structured errors, listener registration/unregistration, and native event dispatch back into the shared bridge.
- Added a Windows host contract JSON fixture at `crates/stremio-lightning-windows/tests/fixtures/host_contract.json`.

Current status:

- The Windows crate is structurally present.
- Milestone 1, Milestone 2, Milestone 3, and Milestone 4 are complete.
- Shared bridge ownership is no longer under `src-tauri`.
- The direct shell can now create and own a native Win32 window on Windows, attach WebView2 to it, and navigate to the configured web UI.
- The direct shell can route the baseline host-contract request/response messages and structured errors.
- The direct shell cannot yet render MPV video, supervise the local server, or provide full window behavior parity.

## Missing Work By Area

## 1. Native Windows App And Window Ownership

Missing:

- Center-on-start behavior.
- Full close/minimize/show-hide policy beyond blank-window shutdown.
- Runtime icon resource wiring.
- Splash behavior, if still desired.
- Cleanup integration for future WebView2, MPV, server, and IPC resources.

Needed:

- Extend the raw `windows` crate Win32 wrapper chosen in Milestone 2.
- Add a `WindowController` or equivalent module that exposes a small tested surface for `show`, `hide`, `focus`, `quit`, `set_fullscreen`, `toggle_topmost`, `window_state`, and `client_bounds`.
- Expand the Win32 message dispatch model as WebView2 callbacks and MPV background threads are wired in.
- Preserve a clean separation: windowing code stays in `crates/stremio-lightning-windows`; shared command schemas stay in core if they are platform-neutral.

Reference behavior to copy conceptually:

- `stremio-shell-ng` composes `MainWindow` with partial components for tray, splash, server, player, and webview.
- `window_helper.rs` stores old style/position before fullscreen and restores it when leaving fullscreen.
- `OnMinMaxInfo`, paint/resize, focus, minimize/maximize, and close events are explicitly handled.

Acceptance:

- On Windows, `cargo run -p stremio-lightning-windows` opens a native app window.
- The window can be closed without orphaning background threads or child processes.
- Window state can be reported to the web side in the same event shape expected by Stremio Web or the shared bridge.

## 2. WebView2 Runtime Integration

Implemented baseline:

- WebView2 environment creation.
- WebView2 controller creation against the native parent `HWND`.
- Bounds resizing when the window resizes.
- Navigation to the target Stremio web URL.
- Baseline WebView2 settings and debug-build devtools policy.
- Document-created injection with the shared bridge.
- Simple WebView2 message receive/send plumbing.
- Promise-based request/response host contract routing through WebView2 messages.
- Native event dispatch back to JavaScript listeners.

Missing:

- Runtime devtools CLI toggle beyond debug-build policy.
- New-window/external-link handling.
- Fullscreen element handling.
- Focus handling.
- Media-key handling.
- Error handling when WebView2 is missing or fails to initialize.

Needed:

- Implement a Windows-only WebView2 backend in `crates/stremio-lightning-windows/src/webview.rs`.
- Inject `web/bridge/bridge.js` and the Windows host adapter before the hosted app needs shell APIs.
- Decide whether the injection should happen through document-created script APIs or content-loading callbacks. Prefer document-start semantics for Stremio Lightning's shared bridge where possible.
- Add browser flags equivalent to the reference where they make sense, especially autoplay and WebView2 UI suppression.
- Disable host objects unless a specific, reviewed host-object use case is introduced.
- Block or reroute unwanted `window.open` requests to the system browser with URL allowlisting/warning behavior.
- Implement a queue for native-to-web messages sent before the WebView2 controller is fully ready.

Reference behavior to copy conceptually:

- `stremio-shell-ng` uses WebView2 additional browser arguments including autoplay enablement and disabling WebView2 UI/smart-screen features.
- It disables status bar, zoom controls, built-in error page, host objects, and script dialogs.
- It receives messages through `add_web_message_received` and sends responses back through the web channel transport.
- It injects a compatibility bridge that maps `window.qt.webChannelTransport.send` to `window.chrome.webview.postMessage`.
- It resizes the WebView2 controller using `GetClientRect` and `put_bounds`.
- It handles fullscreen element changes and sends `win-set-visibility` style events.

Acceptance:

- Web UI loads in the direct shell.
- Shared bridge is available before app shell logic needs it.
- Native-to-web and web-to-native messages work with request IDs and error responses.
- Devtools can be enabled for development builds or a CLI flag.
- External links do not execute arbitrary local commands.

## 3. Host API And Shell RPC Contract

Implemented baseline:

- Windows WebView2 adapter posts `{ id, kind, payload }` requests and resolves/rejects JavaScript Promises from native responses.
- Native host dispatcher handles `invoke`, `listen`, `unlisten`, baseline window commands, and `webview.setZoom`.
- Unsupported commands return a single structured error response.
- Shell transport handshake responses are emitted as `shell-transport-message` events.
- Contract fixture coverage exists for success and invalid-command responses.

Missing:

- Complete Windows implementation of the shared host API.
- Real mods/settings filesystem behavior in the Windows crate once shared core features are enabled for it.
- Native event emission for media keys, native player status, player events, and app lifecycle.

Needed:

- Inventory every command used by `web/bridge/bridge.js`, plugin APIs, Tauri command wrappers, and Linux shell adapter.
- Implement the Windows side as a typed dispatcher instead of untyped `serde_json::Value` pass-through where possible.
- Keep plugin-facing APIs narrower than shell-facing APIs.
- Add JSON fixtures for commands and responses so Tauri/Linux/Windows adapters stay contract-compatible.
- Add explicit rejection tests for commands that Windows does not support yet.

Reference behavior to copy conceptually:

- `stremio-shell-ng` treats id `0` as a handshake and responds with a transport object.
- It routes method strings such as `win-set-visibility`, `quit`, `app-ready`, `app-error`, `open-external`, `win-focus`, and `mpv-*`.
- It emits response messages as arrays like `["win-visibility-changed", {...}]`, `["open-media", url]`, `["media-key", action]`, and MPV event arrays.

Acceptance:

- Host contract tests pass for the Windows adapter.
- The mods panel can call native commands without knowing it is hosted by WebView2.
- Unsupported commands fail once with structured errors rather than repeated JS exceptions.

## 4. Native MPV Backend

Implemented baseline:

- Real libmpv initialization in the Windows crate.
- `HWND` handoff from the native window to MPV through the `wid` option.
- Direct MPV embedding into the owned Win32 window, matching the initial `stremio-shell-ng` reference strategy.
- MPV command/event loop thread.
- Property observation handling.
- Event conversion back to web transport messages.
- Player shutdown and cleanup through MPV `quit` on backend drop.

Still needs runtime verification / later hardening:

- Runtime loading/copying of `libmpv-2.dll` beside the executable or in a known DLL search path.
- Actual Windows playback smoke testing with `mpv-command loadfile`.
- Overlay/UI behavior verification when WebView2 and MPV share the native parent window.
- More complete event/error parity with the old Tauri Windows player path.

Needed:

- Continue hardening the proven parts of the old Tauri Windows MPV path and the `stremio-shell-ng` MPV shape in `crates/stremio-lightning-windows/src/player.rs`.
- Preserve the current Stremio Lightning shell transport names: `mpv-observe-prop`, `mpv-set-prop`, `mpv-command`, `native-player-stop`, `mpv-prop-change`, and `mpv-event-ended`.
- Decide whether to render MPV into the main window, a child window, or a dedicated video host area layered with WebView2. `stremio-shell-ng` uses the parent `HWND` directly; that is the simplest reference, but Stremio Lightning must verify overlay/UI expectations with the hosted web app.
- Wake the MPV event context when new property observations are registered.
- Convert JSON-like MPV string properties such as `track-list`, `video-params`, and `metadata` into JSON values where expected.
- Ensure shutdown sends/handles MPV shutdown and joins or detaches threads safely.

Reference behavior to copy conceptually:

- `stremio-shell-ng` calls `Mpv::with_initializer` and sets `wid` to the parent `HWND` cast to `i64`.
- It uses one event thread and one incoming-message thread.
- It observes properties with typed libmpv formats: flag, int64, double, string.
- It maps `Event::PropertyChange` to `mpv-prop-change` and `Event::EndFile` to `mpv-event-ended`.
- It appends `gpu-next,` when setting `vo` if needed by the reference playback behavior.

Acceptance:

- `mpv-command loadfile` starts playback in the direct Windows shell.
- `mpv-observe-prop` returns property changes to the hosted web app.
- Stop/end/error events are visible to the web app.
- Closing the window does not leave MPV threads or handles alive.

## 5. Web UI, Bridge, And Injection Parity

Missing:

- A Windows WebView2-specific adapter that exposes the same shell-agnostic API as other shells.
- Verification that the shared `web/bridge/bridge.js` works with WebView2 native messaging.
- A reliable initialization order between WebView2, host adapter, bridge, and Stremio web app route logic.
- Compatibility behavior for Stremio Web's existing Qt shell assumptions if they are still present in hosted code.

Needed:

- Add a small Windows adapter script that maps `StremioLightningHost.invoke/listen/window` to WebView2 post messages.
- Keep the shared bridge independent of `window.__TAURI__`, `window.chrome.webview`, and `window.qt` details.
- If Stremio Web still expects `window.qt.webChannelTransport`, provide that compatibility shim only inside the Windows shell adapter layer.
- Add smoke tests or manual checklist steps proving the mods button appears, plugin APIs work, and player handoff reaches native MPV.

Reference behavior to copy conceptually:

- `stremio-shell-ng` sets `window.qt.webChannelTransport.send = window.chrome.webview.postMessage` and routes WebView2 messages back into `onmessage`.
- It calls shell communication initialization on page load after injecting the compatibility script.

Acceptance:

- The hosted UI cannot tell whether it is running in Tauri, Linux WebKitGTK, or Windows WebView2 except through intended platform metadata.
- Existing plugins still see `window.StremioEnhancedAPI` compatibility behavior.
- The bridge fails closed when native messaging is unavailable.

## 6. Stremio Server And Runtime Process

Implemented baseline:

- Windows direct-shell lifecycle for the local Stremio server/runtime.
- Child process startup from Windows shell resources.
- Explicit `NO_CORS`, `FFMPEG_BIN`, and `FFPROBE_BIN` environment setup for the bundled server.
- stdout/stderr log file capture.
- Host commands for start, stop, restart, and running-state checks.
- `server-started` and `server-stopped` event emission through the existing host event channel.
- Shutdown cleanup through the server supervisor drop path.
- Windows job object handling so child processes die with the shell.

Missing:

- Readiness detection from stdout or HTTP health check.
- Crash detection and user-visible error reporting.

Needed:

- Decide which pieces belong in shared core and which stay Windows-specific. Process creation flags, job objects, and resource paths are Windows-specific; lifecycle commands and status schemas can be shared.
- Reuse the current Stremio Lightning streaming server abstraction where possible.
- Harden the Windows process supervisor that starts bundled runtime resources downloaded by `setup:windows-shell`.
- Parse readiness from stdout or replace readiness detection with a local HTTP health check.
- Surface richer server status/address details if the web app requires more than the current Linux-compatible boolean status and start/stop events.

Reference behavior to copy conceptually:

- `stremio-shell-ng` starts `stremio-runtime server.js` with `CREATE_NO_WINDOW`.
- It configures a job object with kill-on-close behavior.
- It captures stdout/stderr, stores a rolling log, and waits until the server reports its HTTP endpoint before continuing.

Acceptance:

- The direct Windows shell starts the local server on app start.
- Settings/network pages can detect the server as online.
- Server crashes are observable and do not silently break playback resolution.
- App exit terminates child server processes.

## 7. Single Instance, Deep Links, And Open Media

Missing:

- Direct Windows replacement for Tauri single-instance/deeplink behavior.
- A way for second invocations to send open-media commands to the running app.
- Focus existing window on second invocation.

Needed:

- Implement a named-pipe or mutex-plus-pipe mechanism for single-instance behavior.
- Parse CLI/deeplink/media arguments at startup.
- If another instance is running, send the command to the existing instance and exit.
- If this is the first instance, start a pipe server and forward incoming commands to the web side as `open-media` events.

Reference behavior to copy conceptually:

- `stremio-shell-ng` implements named pipe client/server wrappers and forwards received strings to the web side as `open-media` events while focusing the app.

Acceptance:

- Opening a supported Stremio/media URL while the app is running focuses the existing window and routes the media to the web UI.
- Second invocation exits without starting a duplicate server/player stack.

## 8. Tray, Window Commands, Fullscreen, And Media Keys

Missing:

- System tray menu.
- Show/hide behavior.
- Topmost toggle.
- Fullscreen toggle and restoration.
- Minimize/maximize/focus state notifications.
- Hardware media key forwarding.
- Web fullscreen element synchronization.

Needed:

- Implement tray only if current Windows product behavior requires it. If it is required, keep it isolated from core.
- Implement window commands used by the bridge: minimize, maximize, restore, close/hide, focus, fullscreen.
- Report state changes to web in the expected event format.
- Intercept relevant Win32 app commands and forward media actions to the web side.

Reference behavior to copy conceptually:

- `stremio-shell-ng` maps `WM_APPCOMMAND` media actions to `media-key` messages.
- It toggles fullscreen by removing/restoring caption/thickframe styles and sizing to the nearest monitor.
- It uses tray menu items for show/hide, topmost, and exit.

Acceptance:

- Fullscreen works without losing previous window size/position.
- Media keys produce web-visible playback actions.
- Window state events remain accurate after minimize, maximize, fullscreen, focus, and hide/show.

## 9. External URL And Security Policy

Missing:

- Windows direct-shell external link policy.
- URL allowlist or warning behavior.
- Guardrails for native command exposure.
- WebView2 hardening settings review.

Needed:

- Define allowed protocols for `open-external`.
- Never pass arbitrary strings to `open::that` or `ShellExecute` without protocol validation.
- Decide whether untrusted URLs should be blocked, opened in the browser, or routed through a warning page.
- Disable WebView2 host objects unless explicitly needed.
- Keep plugin commands restricted to the plugin API surface, not the full host API.

Reference behavior to copy conceptually:

- `stremio-shell-ng` allows only selected URL schemes for `open-external`, though its own comment warns this area is unsafe and needs caution.
- It reroutes non-whitelisted new-window hosts through a warning URL.

Acceptance:

- `open-external` cannot launch local executables or shell commands.
- New windows do not silently open privileged local targets.
- All native command handlers validate payload types before executing.

## 10. Packaging, Resources, And Dependency Setup

Missing:

- Complete Windows resource layout for the direct shell.
- Runtime copy/layout for `libmpv-2.dll`, ffmpeg/ffprobe, stremio-service/runtime, icons, and any web assets needed by the shell.
- Installer/update packaging for the direct Windows shell.
- Architecture-specific x64/arm64 handling.
- Windows manifest and icon embedding.

Needed:

- Finish `scripts/download-windows-shell-deps.sh` so it produces exactly the files the Windows crate expects.
- Update `crates/stremio-lightning-windows/build.rs` to link and package libmpv consistently for MSVC targets.
- Add architecture detection equivalent to the reference build script.
- Embed Windows icon/manifest resources.
- Ensure runtime DLL search path works in installed and dev layouts.
- Document how a developer runs the direct Windows shell from a fresh checkout.

Reference behavior to copy conceptually:

- `stremio-shell-ng` has `bin`, `bin-arm64`, `mpv-x64`, `mpv-arm64`, zipped libmpv artifacts, and build scripts for x64/arm64.
- Its build script sets a manifest, embeds icon/splash resources, extracts libmpv, sets an `ARCH` env var, and adds architecture-specific link args.

Acceptance:

- A clean Windows checkout can run setup, build, and launch the direct shell.
- Both dev and packaged builds find `libmpv-2.dll` without relying on global PATH.
- Installer output includes all runtime dependencies.

## 11. Tests And Verification

Missing:

- Windows-only runtime tests or smoke checks.
- WebView2 IPC integration tests.
- MPV backend tests beyond command mapping.
- Server process lifecycle tests.
- Contract fixtures shared across Tauri/Linux/Windows adapters.
- Manual Windows acceptance checklist.

Needed:

- Keep non-GUI command parsing, message schema, and MPV mapping tests cross-platform.
- Add Windows-gated integration tests for components that require WebView2/Win32.
- Add fake backend tests for host routing so CI can validate behavior without launching WebView2.
- Add a manual smoke checklist for Windows direct shell until GUI automation is stable.
- Test invalid payloads, not only happy paths.

Acceptance:

- `cargo test -p stremio-lightning-windows` passes on Linux for platform-neutral tests.
- Windows CI or manual command validates `cargo test -p stremio-lightning-windows --target x86_64-pc-windows-msvc`.
- Manual smoke covers startup, mods panel, server online status, native playback, fullscreen, media keys, deep links, and shutdown cleanup.

## Recommended Implementation Order

1. Implement the native Windows main window and message loop with a minimal blank window.
2. Create WebView2 controller inside that window and navigate to the configured Stremio web URL.
3. Add WebView2 native messaging and the Windows host adapter with a handshake test fixture.
4. Inject the shared bridge and verify the mods UI appears.
5. Port MPV initialization with `wid` and prove `loadfile` against a local sample.
6. Wire MPV observe/set/command/event conversion to the existing shell transport names.
7. Implement server/runtime process supervision with job-object cleanup.
8. Add window/fullscreen/focus/media-key parity.
9. Add single-instance named-pipe/deeplink behavior.
10. Finish packaging: resource layout, DLL path, icons, manifest, installer/update path.
11. Run full Windows manual smoke and update Phase 6 status.

## Definition Of Done For Phase 6

Phase 6 is complete when:

- Windows launches through `crates/stremio-lightning-windows`, not Tauri.
- WebView2 hosts the Stremio web UI and loads the shared bridge.
- The mods panel and plugin-facing API work through the shared host API.
- The local server starts, reports status, and stops with the app.
- MPV plays video natively through the direct Windows shell.
- MPV property changes and end/error events reach the web app.
- Fullscreen, focus, minimize/maximize, topmost/show-hide behavior, and media keys match the current supported Windows behavior.
- Deep links or second invocations route to the existing app instance.
- Runtime dependencies are owned by the Windows shell crate and no longer depend on `src-tauri`.
- Tests and manual Windows smoke checks are documented and passing.
