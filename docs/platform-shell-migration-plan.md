# Platform Shell Migration Plan

## Goal

Move Stremio Lightning from a Tauri-centered shell to platform-specific native shells with a shared Rust core and shared injected web/mod layer.

This plan chooses option 4:

```text
Shared Rust core + shared injected JS
|-- Windows shell: WebView2 + native MPV
|-- Linux shell: GTK4 + WebKitGTK 6 + native MPV
`-- macOS shell: WKWebView + native MPV
```

Tauri can remain as a temporary compatibility shell while the new architecture is built. It should not be the long-term Linux native playback path.

## Non-Goals

- Do not rewrite the Stremio web UI.
- Do not fork Stremio Web unless hosted UI instability becomes unmanageable.
- Do not make arbitrary plugin JS privileged by default.
- Do not continue investing in Tauri-specific WebKitGTK proxy/workaround behavior as the final Linux solution.

## Architecture Principles

- The injected JavaScript must not know which native shell is hosting it.
- Shell-specific code owns windowing, webview, native player, and OS integration.
- Shared Rust code owns business logic: mods, settings, server lifecycle, updater, Discord RPC, IPC types, validation.
- Every native command exposed to JavaScript must have a typed request/response shape and tests.
- Plugins run with an intentionally small capability surface.
- Linux native playback is proved first because it is the current failing platform.

## Target Layout

```text
crates/
  stremio-lightning-core/
    src/
      host_api/
      mods/
      settings/
      streaming_server/
      player_api/
      discord/
      updater/
      deeplink/
  stremio-lightning-tauri/
  stremio-lightning-linux/
  stremio-lightning-windows/
  stremio-lightning-macos/

web/
  host/
    host-api.ts
    tauri-adapter.ts
    shell-adapter.ts
  bridge/
    bridge.js
  mod-ui/
    ...
```

This can be introduced incrementally inside the current repo. The final folder names can change, but the separation should not.

## Test Strategy

Use TDD at three layers.

### Unit Tests

Scope:

- command validation
- mod filename/type validation
- settings persistence
- registry parsing
- streaming-server path/header sanitization
- shell transport message parsing
- player command mapping

Rules:

- Write failing tests before moving behavior into shared crates.
- Shared core tests must not require a GUI.
- Tests must use temp directories instead of real user app data.

### Contract Tests

Scope:

- JavaScript host API command names and payloads
- native IPC request/response schemas
- MPV event names sent back to web
- plugin API compatibility

Rules:

- The same contract fixtures must run against Tauri shell adapter and new shell adapter.
- Any command exposed to plugins needs both success and rejection tests.

### Integration / Smoke Tests

Scope:

- webview loads `https://web.stremio.com/`
- injection appears before app route logic needs it
- mods button appears
- local server starts and responds
- streaming-server proxy works
- MPV receives `observe`, `set_property`, `command loadfile`
- MPV sends property changes back to web

Rules:

- GUI smoke tests can be platform-specific.
- Linux playback smoke test is required before deleting the Tauri Linux path.
- Full video playback can use a local static sample file or controlled HTTP fixture, not a real addon stream.

## Phase 0: Stabilize Current Baseline

Objective: freeze what currently works before migration starts.

Tasks:

- Document current supported flows:
  - app startup
  - injected bridge load
  - mods panel open/close
  - plugin install/load/unload
  - theme apply/remove
  - local streaming server start/stop/status
  - native player command flow
- Add or fix baseline tests for existing code:
  - `shell_transport` parsing
  - streaming-server proxy path validation
  - mod manager path/type validation
  - settings load/save
- Add a simple smoke checklist script or markdown for manual runtime validation.
- Disable or hide the broken Linux native MPV path behind an explicit opt-in env var if it blocks normal usage.

TDD Acceptance:

- `cargo test --manifest-path src-tauri/Cargo.toml shell_transport -- --nocapture` passes.
- Streaming-server proxy validation has tests for invalid absolute URLs, backslashes, null bytes, hop-by-hop headers, and supported methods.
- Mod manager rejects path traversal filenames in tests.
- Current Tauri app still starts and injects the mods UI.

Exit Criteria:

- Engineers have a known-good baseline.
- Linux can still use web playback or external fallback while native shell work proceeds.

## Phase 1: Introduce Host API Abstraction

Objective: remove direct dependency on `window.__TAURI__` from the injected app code.

Current problem:

```js
window.__TAURI__.core.invoke(...)
window.__TAURI__.event.listen(...)
window.__TAURI__.window.getCurrentWindow(...)
```

Target:

```js
window.StremioLightningHost.invoke(command, payload)
window.StremioLightningHost.listen(event, callback)
window.StremioLightningHost.window.minimize()
```

Tasks:

- Create a host API TypeScript module with typed command names.
- Create a Tauri adapter that maps the host API to `window.__TAURI__`.
- Update `bridge.js`, `plugin-api.ts`, and mod UI code to call `StremioLightningHost`.
- Keep `window.StremioEnhancedAPI` as the plugin-facing API for compatibility.
- Add a startup guard that logs a clear error if no host adapter exists.

TDD Acceptance:

- Unit test the host adapter with a mocked `window.__TAURI__`.
- Contract test every command currently used by `plugin-api.ts`.
- Test that plugin code still sees the same `StremioEnhancedAPI` surface.
- Test that missing host adapter fails closed instead of throwing repeated runtime errors.

Exit Criteria:

- Tauri remains functional.
- Injected JS is shell-agnostic.
- No plugin-facing API regression.

## Phase 2: Extract Shared Rust Core

Objective: move shell-independent behavior out of Tauri command handlers.

Initial shared modules:

- `mods`
- `settings`
- `streaming_server`
- `host_api`
- `player_api`
- `discord`
- `updater`
- `deeplink`

Tasks:

- Create `crates/stremio-lightning-core`.
- Move pure validation and filesystem logic first:
  - mod directory layout
  - mod download validation
  - mod listing
  - registry parsing
  - settings schema persistence
  - streaming-server proxy request validation
- Keep Tauri-specific command wrappers thin.
- Define typed IPC enums/structs:
  - `HostCommand`
  - `HostEvent`
  - `PlayerCommand`
  - `PlayerEvent`
  - `StreamingServerCommand`
- Add serde tests for IPC payload compatibility.

TDD Acceptance:

- Each moved function has tests in the shared crate before the Tauri wrapper is changed.
- Tauri command tests, where present, become wrapper tests only.
- IPC JSON snapshots are stable.
- Temp-dir tests prove no shared code writes outside its assigned app data directory.

Exit Criteria:

- Tauri is one shell implementation over shared core.
- New shells can reuse mod/server/settings logic without importing Tauri.

## Phase 3: Build Linux Shell Prototype

Status: runtime shell path implemented against the current upstream Stremio Linux shell direction. The old black GL placeholder is gone. The Linux shell now opens a GTK4 window with a native MPV `GLArea` layer under a transparent WebKitGTK 6 webview, injects the host adapter at document start, and routes WebKit script messages into the Rust host runtime.

Objective: prove the new Linux rendering architecture before porting all app features.

Prototype scope:

- one GTK4 window: implemented;
- WebKitGTK 6 webview loading Stremio Web through the official local server proxy (`http://127.0.0.1:11470/proxy/d=https%3A%2F%2Fweb.stremio.com/`): implemented through the native Linux shell startup path;
- document-start injection of host adapter + bridge script: implemented and tested;
- local server commands: implemented and tested;
- MPV player setup: native libmpv backend is initialized for the GTK `GLArea`; command/event contract is implemented and tested;
- MPV render loop: represented as MPV GLArea under transparent WebKitGTK overlay;
- IPC roundtrip: web -> Rust -> web implemented and tested through the Linux host adapter contract.

Tasks:

- Create `crates/stremio-lightning-linux`: done.
- Use Stremio Linux Shell style event ownership:
  - GTK4 application/window lifecycle
  - WebKitGTK 6 webview layer
  - GTK `GLArea`
  - libmpv render context through `libmpv2`
- Implement shell adapter for `StremioLightningHost`: done.
- Implement minimal commands:
  - `init`
  - `open_external_url`
  - `start_streaming_server`
  - `get_streaming_server_status`
  - `shell_transport_send`
  - `get_native_player_status`
- Route Stremio MPV IPC to native player:
  - observe property
  - set property
  - command
  - property change event
  - end event
- Add dev flag for opening WebKit devtools: implemented by enabling developer extras when `--devtools` is passed.

TDD Acceptance:

- Unit tests cover IPC parsing and command dispatch without launching the GUI webview: done.
- Player command mapping tests confirm Stremio command payloads become expected MPV calls: done.
- A fake player backend can run integration tests without libmpv: done.
- A smoke test loads a local test HTML page and confirms injection/IPC roundtrip: represented by the Linux webview runtime tests using a `file://` smoke URL and fake host/player backend.

Manual Runtime Acceptance:

- Linux shell starts: `timeout 12s cargo run -p stremio-lightning-linux` reaches the GTK4/WebKitGTK 6 load path and remains alive until killed by the timeout.
- Web UI loads: pending real Linux web layer.
- Mods button appears: pending real Linux web layer.
- Local server startup/reload handoff: implemented in the replacement Linux shell; online status still needs manual smoke against the visible shell.
- MPV initializes: pending final libmpv runtime backend.
- A local sample video renders inside the shell: pending final compositor.
- Web controls remain visible and clickable: pending final compositor.
- Stop/end clears the player layer without stale black rectangles: pending final compositor.

Exit Criteria:

- Code-contract scaffolding is in place for the shell prototype.
- Runtime exit criteria are not met until the real Linux web layer and libmpv compositor work are completed and manually smoked.

## Phase 4: Port Mods and Plugin Manager to Linux Shell

Objective: make the Linux shell usable for the product, not just playback.

Tasks:

- Wire all mod commands from shared core:
  - get plugins/themes
  - download mod
  - delete mod
  - get mod content
  - check updates
  - get registry
  - register settings
  - get/save settings
- Inject the existing Svelte mod UI bundle.
- Verify plugin lifecycle:
  - install
  - enable
  - disable
  - reload page
  - persist enabled list
  - settings callback
- Add shell event delivery:
  - window maximize/fullscreen
  - server started/stopped
  - native player started/stopped/property change

TDD Acceptance:

- Contract fixtures for all plugin API commands pass against Linux shell command dispatcher.
- Plugin settings persistence has roundtrip tests.
- A sample plugin fixture can register settings and receive saved values.
- Theme fixture can apply and clear CSS custom properties.

Manual Runtime Acceptance:

- Mods panel opens on Linux shell.
- Marketplace/registry loads.
- A plugin can be installed and enabled.
- A theme can be installed and applied.
- Plugin settings survive app restart.

Exit Criteria:

- Linux shell has feature parity with current Tauri mod system.

## Phase 5: Streaming Server and WebKitGTK Workaround Retirement

Objective: remove Tauri/WebKitGTK-specific workarounds from the Linux path.

Tasks:

- Use direct local-server access from the Linux shell; do not keep a native fetch proxy.
- Remove WebKitGTK fetch/worker proxy branches from the injected bridge.
- Remove the native proxy command and shared proxy validation types.
- Validate server lifecycle:
  - start on app start
  - stop on app exit
  - restart command
  - stdout/stderr logging
  - crash detection if implemented

TDD Acceptance:

- Server process lifecycle tests use a dummy process or fixture script.
- Host command contracts no longer include `proxy_streaming_server_request`.

Manual Runtime Acceptance:

- Stremio settings show server online.
- `/settings`, `/casting`, `/network-info`, and `/device-info` work.
- Playback sources that require the server continue to resolve.

Exit Criteria:

- Linux shell no longer depends on Tauri/WebKitGTK behavior.

## Phase 6: Direct Windows WebView2 Shell

Objective: replace the Windows Tauri shell with a direct WebView2 shell that uses the shared Rust core, shared injected JS host adapter, and native MPV directly.

Tasks:

- Reuse shared core and JS host adapter.
- Move reusable Windows shell code out of `src-tauri` into `crates/stremio-lightning-windows` or shared core crates as appropriate: started by moving the shared injected bridge to `web/bridge/bridge.js`, adding `crates/stremio-lightning-windows`, and adding a Windows-shell-owned dependency downloader/build link path.
- Use `https://github.com/Stremio/stremio-shell-ng` as the direct Windows WebView2 + MPV architecture reference. Its implementation proves the target shape: native Windows window ownership, WebView2 controller hosted inside the native window, MPV initialized directly with the window `HWND` through `wid`, shell-to-web RPC over WebView2 messages, native server process management, tray/window/fullscreen helpers, named-pipe single-instance IPC, and Windows resource/libmpv packaging.
- Execute the foundational crate-first plan in `docs/windows-webview2-shell-crate-plan.md` before feature expansion.
- Track current progress, missing implementation, and direct shell gaps in `docs/windows-webview2-shell-gap-analysis.md`.
- Track Stremio Community feature parity separately in `docs/stremio-community-feature-parity-todo.md` after the direct Windows shell crate is functional.
- Implement WebView2 host adapter: baseline complete in `crates/stremio-lightning-windows` with WebView2 environment/controller creation, `HWND` attachment, resize handling, configured URL navigation, request/response IPC IDs, structured native errors, and native event listener dispatch through the shared bridge.
- Implement native MPV embedding/rendering using the currently stable Windows approach.
- Preserve installer/dependency download behavior for libmpv.
- Retire the Windows Tauri runtime path after direct WebView2 feature parity is reached.

TDD Acceptance:

- Same host contract tests pass.
- Same plugin contract tests pass.
- Player command mapping tests pass.
- Windows-specific window command tests use mocks where possible.

Manual Runtime Acceptance:

- Web UI loads.
- Mods panel works.
- Local server works.
- MPV playback works.
- Fullscreen, PiP, auto-pause, and keyboard shortcuts match current behavior.

Exit Criteria:

- Windows runs through the direct WebView2 shell with no Tauri runtime dependency.
- Windows-specific files needed by the replacement shell have been moved out of `src-tauri`.

## Phase 7: macOS Shell

Objective: add macOS after Linux and Windows patterns are proven.

Decision point:

- Start with WKWebView if Stremio Web and injection behavior are sufficient.
- Revisit the webview choice only if WKWebView cannot support the shared host adapter and native-player handoff.

Tasks:

- Implement macOS host adapter.
- Reuse shared core.
- Implement native player backend.
- Implement app bundle/signing/notarization path.

TDD Acceptance:

- Shared host contract tests pass.
- macOS window command tests use native mocks or thin wrappers.
- Player command mapping tests pass with fake backend.

Manual Runtime Acceptance:

- App loads.
- Mods work.
- Local server works.
- Native playback works.
- Fullscreen behavior follows macOS expectations.

Exit Criteria:

- Three platform shells share one core and one injected mod layer.

## Phase 8: Decommission Tauri

Objective: remove Tauri when replacement shells are production-ready.

Tasks:

- Freeze Tauri shell as legacy for one release if needed.
- Remove Tauri-only injected API usage.
- Remove Tauri command wrappers after platform shells cover all commands.
- Remove Tauri config/build scripts once installers are replaced.
- Update documentation and developer workflow.

TDD Acceptance:

- Shared core tests pass without any Tauri dependency.
- JS host contract tests pass against all active shell adapters.
- No production code imports `@tauri-apps/api`.

Exit Criteria:

- Tauri is no longer required to build or run the app.

## Continuous Quality Gates

Every phase must keep these green:

```bash
npm run build:ui
cargo test --workspace
```

If `cargo test --workspace` is not available yet, each phase must define the exact crate-level cargo test commands it owns.

Before merging shell changes:

- run formatting checks;
- run unit tests;
- run contract tests;
- run at least one platform smoke test for the touched shell;
- update this plan if the architecture changes.

## Risk Register

### Hosted Stremio Web Changes

Risk: DOM selectors or internal app behavior changes.

Mitigation:

- keep injections as route/DOM-observer based, not React-internal based;
- add smoke tests that check the mods button and shell transport wiring after page load;
- prefer official Stremio addon/deeplink behavior when installing real addons.

### Plugin Security

Risk: installed plugins can abuse privileged native APIs.

Mitigation:

- plugin API remains narrower than host API;
- validate all native command payloads;
- do not expose arbitrary filesystem/network/native process access;
- add per-command capability checks before marketplace plugins are treated as trusted.

### Native Webview Runtime Differences

Risk: GTK/WebKitGTK on Linux, WebView2 on Windows, and WKWebView on macOS expose different script injection, IPC, and media defaults.

Mitigation:

- keep all privileged calls behind the shared `StremioLightningHost` adapter;
- test document-start injection and IPC per shell;
- keep browser media disabled where native playback owns MPV handoff.

### Divergent Platform Behavior

Risk: platform shells drift apart.

Mitigation:

- shared core owns logic;
- shared host API fixtures define the contract;
- shell-specific code only handles webview/window/player plumbing.

### MPV Rendering Edge Cases

Risk: GL context/render loop issues vary by GPU, compositor, X11/Wayland.

Mitigation:

- build fake-player tests for logic;
- maintain manual GPU/compositor smoke matrix;
- keep MPV options configurable for diagnostics;
- log render backend, hwdec, VO, and first-frame timing.

## Suggested First Engineering Tickets

1. Add tests for current mod manager path/type validation.
2. Add tests for streaming-server proxy validation.
3. Add `StremioLightningHost` JS abstraction and Tauri adapter.
4. Update `bridge.js` to use host abstraction.
5. Move mod/settings validation into `stremio-lightning-core`.
6. Add IPC schema fixtures for host commands/events.
7. Create Linux shell spike with fake player and local test HTML injection.
8. Replace fake player with libmpv render loop and local sample playback.
9. Inject real bridge/mod UI into Linux shell.
10. Disable Linux native MPV in Tauri by default.
