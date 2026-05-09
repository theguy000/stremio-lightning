# macOS WKWebView Shell Crate Plan

## Goal

Create `crates/stremio-lightning-macos` as the native macOS shell for Stremio Lightning. The crate should use the shared Rust core, shared injected JavaScript bridge, `WKWebView` for the web layer, and native `libmpv` for playback.

This plan intentionally does not copy the official Qt shell. Official Stremio desktop macOS currently uses the older `Stremio/stremio-shell` architecture: Qt5, QML, QtWebEngine, QtWebChannel, app bundle `Info.plist`, OpenSSL, and `libmpv` rendered through a Qt OpenGL framebuffer object. That implementation proves the product needs native MPV, a web UI shell, a streaming-server sidecar, file/deeplink handling, and macOS bundle metadata. It should be used as a behavior reference, not as our technical stack.

Architecture type: Direct macOS WKWebView shell.

Our target stack is:

```text
Rust crate
|-- Cocoa/AppKit window ownership
|-- WKWebView web layer
|-- WKUserScript document-start injection
|-- WKScriptMessageHandler-style IPC bridge
|-- shared stremio-lightning-core host/runtime logic
`-- native libmpv player backend
```

## Non-Goals

- Do not introduce Qt as a dependency.
- Do not fork Stremio Web.
- Do not create a macOS-only JavaScript bridge unless WKWebView requires a tiny transport shim.
- Do not implement signing/notarization before the local debug app can launch, inject, and play.
- Do not move Linux or Windows code while scaffolding the macOS crate unless a shared abstraction is clearly reusable.

## References

- Existing repo target architecture: `docs/platform-shell-migration-plan.md`.
- Linux crate contract shape: `crates/stremio-lightning-linux`.
- Official legacy desktop shell reference: `https://github.com/Stremio/stremio-shell`.
- Official new Linux shell reference: `https://github.com/Stremio/stremio-linux-shell`.

## Naming And Architecture Parity

Use names that match the existing crate architecture unless macOS needs a platform-specific name. This keeps Linux, Windows, and macOS easy to compare.

Architecture labels:

- Linux: Linux GTK/WebKitGTK shell.
- Windows: Direct Windows WebView2 shell.
- macOS: Direct macOS WKWebView shell.

Workspace and package names:

- Crate directory: `crates/stremio-lightning-macos`.
- Cargo package: `stremio-lightning-macos`.
- Binary name: `stremio-lightning-macos`.
- Bundle executable: `stremio-lightning-macos`.
- Bundle identifier candidate: `com.stremio-lightning.macos` until final branding is decided.

Module/file names:

- `src/main.rs`: binary entry point.
- `src/lib.rs`: public module exports.
- `src/app.rs`: CLI parsing, `AppConfig`, startup orchestration, and `run`.
- `src/host.rs`: macOS host command dispatcher wrapper around shared/core behavior.
- `src/webview_runtime.rs`: headless/testable injection and IPC runtime contract.
- `src/native_window.rs`: AppKit `NSApplication`, `NSWindow`, and real `WKWebView` ownership.
- `src/player.rs`: native/fake player backend and MPV command mapping.
- `src/streaming_server.rs`: process spawning and server lifecycle integration.
- `src/resources.rs`: bundle/development resource path resolution if needed.
- `src/deeplink.rs`: `stremio://`, `magnet:`, and file-open parsing if it grows beyond `native_window.rs`.
- `build.rs`: macOS link/bundle metadata helper only if Cargo build-time setup is required.

Rust types and functions:

- `AppConfig`: same role as Linux app config.
- `ShellSettings`: type alias to `AppConfig` if parity with Linux remains useful.
- `DEFAULT_URL`: default startup URL.
- `STREMIO_WEB_URL`: direct Stremio Web URL used for normalization.
- `parse_args`: CLI parser.
- `normalize_startup_url`: Stremio Web to local proxy normalization helper.
- `run`: crate-level app startup function.
- `Host`: macOS shell host dispatcher type.
- `MacosWebviewRuntime`: headless/testable runtime type.
- `WebviewRuntime`: type alias to `MacosWebviewRuntime` if native/non-native cfgs need a stable public name.
- `WebviewLoadState`: load-state test struct matching Linux shape.
- `InjectionScript`: injected script struct.
- `InjectionBundle`: deterministic injected script bundle.
- `PlayerBackend`: trait for fake/native player parity.
- `MpvPlayerBackend`: native MPV backend.
- `ProcessSpawner`: trait for fake/real server process tests.
- `RealProcessSpawner`: production process spawner.
- `StreamingServer`: server lifecycle wrapper.

Injection constants:

- `MACOS_HOST_ADAPTER_NAME`: `"macos-host-adapter"`.
- `HOST_ADAPTER_NAME`: alias to `MACOS_HOST_ADAPTER_NAME`.
- `BRIDGE_UTILS_NAME`: `"bridge/utils.js"`.
- `BRIDGE_CAST_FALLBACK_NAME`: `"bridge/cast-fallback.js"`.
- `BRIDGE_SHELL_TRANSPORT_NAME`: `"bridge/shell-transport.js"`.
- `BRIDGE_EXTERNAL_LINKS_NAME`: `"bridge/external-links.js"`.
- `BRIDGE_SHELL_DETECTION_NAME`: `"bridge/shell-detection.js"`.
- `BRIDGE_BACK_BUTTON_NAME`: `"bridge/back-button.js"`.
- `BRIDGE_SHORTCUTS_NAME`: `"bridge/shortcuts.js"`.
- `BRIDGE_PIP_NAME`: `"bridge/pip.js"`.
- `BRIDGE_DISCORD_RPC_NAME`: `"bridge/discord-rpc.js"`.
- `BRIDGE_UPDATE_BANNER_NAME`: `"bridge/update-banner.js"`.
- `BRIDGE_NAME`: `"bridge.js"`.
- `MOD_UI_NAME`: `"mod-ui-svelte.iife.js"`.

JavaScript globals and message names:

- Public host API: `window.StremioLightningHost`.
- Plugin API compatibility surface: `window.StremioEnhancedAPI`.
- Native event dispatch global: `window.__STREMIO_LIGHTNING_MACOS_DISPATCH__`.
- WKScriptMessageHandler name: `ipc` unless a WebKit conflict requires a more specific name.

Command-line flags:

- `--url <url>` and `--url=<url>`: override startup URL.
- `--devtools`: enable Web Inspector/devtools behavior where available.
- `--headless-bootstrap`: initialize host/runtime contract without opening AppKit UI.
- Optional later parity alias: `--webui-url=<url>` if Windows-style naming becomes the shared developer workflow.

Path and bundle names:

- Development resources root: `crates/stremio-lightning-macos/resources` if crate-local runtime files are needed.
- Development MPV root: `crates/stremio-lightning-macos/mpv-dev` if vendored development linking is needed.
- App bundle: `Stremio Lightning.app` or final branded name.
- Bundle executable path: `Contents/MacOS/stremio-lightning-macos`.
- Bundle resources path: `Contents/Resources`.
- Bundle native library path: `Contents/Frameworks`.

## Phase 0: Architecture Decisions

Objective: lock down the macOS technical direction before adding runtime code.

Decisions:

- Use direct `WKWebView + native MPV` as the macOS shell architecture.
- Treat official Qt shell as behavior reference only, not as a dependency or implementation model.
- Set the initial minimum macOS target to `12.0`.
- Set `MACOSX_DEPLOYMENT_TARGET=12.0` for builds.
- Set app bundle `LSMinimumSystemVersion` to `12.0`.
- Use the `objc2` ecosystem as the primary macOS binding layer:
  - `objc2`;
  - `objc2-foundation`;
  - `objc2-app-kit`;
  - `objc2-web-kit`;
  - `block2`.
- Prefer typed `objc2-*` framework APIs for Foundation, AppKit, and WebKit objects.
- Use small local `unsafe` wrappers with `objc2::msg_send!` only for APIs missing from typed crates.
- Keep all Objective-C runtime calls, delegate glue, and unsafe framework interaction inside `stremio-lightning-macos` wrapper modules.
- Do not use `wry`, `tao`, Qt, Tauri, Swift, or Objective-C helper targets for the initial crate.
- Use `libmpv2` for native player integration unless macOS linking/runtime constraints force a thinner local FFI wrapper.

Crate boundary:

- Put shared host command/request/response types in `stremio-lightning-core`.
- Put shared host event types in `stremio-lightning-core`.
- Put mod, plugin, settings, streaming-server command contracts, IPC validation, player command/event structs, shared serialization tests, and platform-independent path/config logic in `stremio-lightning-core`.
- Put AppKit lifecycle, `NSApplication` delegate glue, `NSWindow` creation, window commands, `WKWebView` setup, document-start user scripts, script message handling, macOS external URL opening, deeplink/file open handlers, macOS MPV view/rendering integration, `.app` bundle metadata, rpath/dylib handling, codesign, and notarization packaging logic in `stremio-lightning-macos`.
- If code knows about AppKit, WebKit framework objects, Objective-C selectors, Cocoa delegates, dylib rpaths, entitlements, or `.app` bundles, it belongs in `stremio-lightning-macos`.
- If code can be tested without macOS frameworks and is useful to Linux or Windows, it probably belongs in `stremio-lightning-core`.

Tasks:

- Document the selected macOS target, dependency strategy, and crate boundary in this phase.
- Revisit these decisions only if WKWebView injection, native MPV rendering, or macOS packaging hits a concrete blocker.

TDD Acceptance:

- The architecture decisions above are recorded in this plan.
- No code is added that depends on Qt or Tauri.

Exit Criteria:

- The first crate scaffold can be created without revisiting the webview/toolkit choice.

## Phase 1: Crate Scaffold

Objective: add a compiling macOS shell crate with the same testable shape as the Linux and Windows crates.

Tasks:

- Create `crates/stremio-lightning-macos` with Cargo package name `stremio-lightning-macos` and binary name `stremio-lightning-macos`.
- Add the crate to the workspace `Cargo.toml`.
- Add initial modules:
  - `main.rs`
  - `lib.rs`
  - `app.rs`
  - `host.rs`
  - `webview_runtime.rs`
  - `native_window.rs`
  - `player.rs`
  - `streaming_server.rs`
- Use Linux-compatible app/runtime names where possible: `AppConfig`, `ShellSettings`, `DEFAULT_URL`, `STREMIO_WEB_URL`, `parse_args`, `normalize_startup_url`, `run`, `InjectionScript`, `InjectionBundle`, and `WebviewLoadState`.
- Use macOS-specific type names only where the type owns platform behavior: `MacosWebviewRuntime`, `MpvPlayerBackend`, `RealProcessSpawner`, and `StreamingServer`.
- Add `#[cfg(target_os = "macos")]` guards for native AppKit/WKWebView code so Linux CI can still evaluate non-native contract tests where possible.
- Add `--url`, `--devtools`, and `--headless-bootstrap` argument parsing matching the Linux shell where useful.
- Use the same default Stremio Web/local proxy URL strategy as the Linux shell unless the macOS runtime proves direct loading is safer.

TDD Acceptance:

- `cargo test -p stremio-lightning-macos --lib` passes on macOS.
- Non-macOS builds either skip native modules cleanly or clearly report that the binary target is macOS-only.
- Argument parsing tests cover default URL, custom URL, Stremio Web URL normalization, devtools flag, and headless bootstrap flag.

Manual Runtime Acceptance:

- `cargo run -p stremio-lightning-macos -- --headless-bootstrap` initializes the host/runtime contract without opening a window.

Exit Criteria:

- Workspace contains the macOS crate and it has a stable module layout for the remaining phases.

## Phase 2: Shared Host Runtime Contract

Objective: prove the macOS crate can reuse the shared host API and injected bridge before native UI work begins.

Tasks:

- Implement `MacosWebviewRuntime` as a headless/testable runtime wrapper.
- Load the same injection bundle used by Linux:
  - platform host adapter;
  - bridge module scripts;
  - `bridge.js`;
  - mod UI bundle.
- Create a `macos-host-adapter` document-start script named by `MACOS_HOST_ADAPTER_NAME` and aliased through `HOST_ADAPTER_NAME`.
- Expose `window.StremioLightningHost` from the macOS host adapter.
- Keep host adapter behavior compatible with the shared bridge:
  - request IDs;
  - promise-based `invoke`;
  - event listeners;
  - native event dispatch;
  - window helpers where available.
- Route `invoke` calls into the Rust `Host` dispatcher.
- Implement event draining as JavaScript dispatch snippets for tests.
- Keep IPC payload validation in Rust, not JavaScript.

TDD Acceptance:

- Test that document-start injection order is deterministic.
- Test that `MACOS_HOST_ADAPTER_NAME` is loaded before `BRIDGE_NAME`.
- Test IPC dispatch with a fake host/player backend.
- Test native event drain scripts include `window.__STREMIO_LIGHTNING_MACOS_DISPATCH__`.
- Test unsupported/invalid commands return structured errors.

Manual Runtime Acceptance:

- Headless bootstrap logs the injection order and host availability.

Exit Criteria:

- macOS host/runtime contract matches Linux closely enough that future shared fixtures can run against both.

## Phase 3: WKWebView Window Prototype

Objective: open a real macOS window with `WKWebView`, inject scripts at document start, and route JavaScript messages into Rust.

Tasks:

- Implement AppKit app lifecycle in `src/native_window.rs`:
  - `NSApplication` setup;
  - application delegate;
  - main menu minimum viable setup;
  - normal app activation behavior.
- Implement native window ownership in `src/native_window.rs`:
  - `NSWindow` creation;
  - initial size similar to official shell minimums;
  - dark background before web content loads;
  - close/hide behavior appropriate for macOS.
- Implement `WKWebView` creation in `src/native_window.rs`:
  - `WKWebViewConfiguration`;
  - `WKUserContentController`;
  - document-start user scripts;
  - JavaScript message handler for IPC;
  - navigation delegate for load status and external navigation policy.
- Implement devtools support where available:
  - enable Web Inspector in debug builds or when `--devtools` is passed;
  - tolerate older macOS versions where this is unavailable.
- Implement external URL handling through AppKit/Foundation APIs.
- Add first real-page smoke path using a local `file://` page.

TDD Acceptance:

- Unit tests continue to cover the headless runtime.
- Native wrapper tests cover URL policy decisions without launching a visible app where possible.
- IPC message parsing tests cover malformed JSON, missing IDs, and unknown message kinds.

Manual Runtime Acceptance:

- `cargo run -p stremio-lightning-macos -- --url file:///.../smoke.html --devtools` opens a window.
- The smoke page can call `window.StremioLightningHost.invoke(...)` and receive a response.
- Native events can be dispatched back to the smoke page.
- External links open outside the webview.

Exit Criteria:

- The macOS crate owns a working `WKWebView` shell with document-start injection and bidirectional IPC.

## Phase 4: Streaming Server Lifecycle

Objective: make the macOS shell launch and manage the Stremio streaming server sidecar through the shared runtime model.

Tasks:

- Reuse the existing streaming server process abstraction shape: `ProcessSpawner`, `RealProcessSpawner`, and `StreamingServer`.
- Keep macOS server lifecycle code in `src/streaming_server.rs` unless it can move into `stremio-lightning-core` without platform dependencies.
- Resolve the macOS runtime server binary/script location for development and bundled app modes.
- Start the server on app launch unless explicitly disabled for development.
- Report server status through host commands.
- Stop the server on app shutdown.
- Add restart/fast-reload command support if required by shared bridge behavior.
- Capture stdout/stderr for diagnostics.
- Preserve official shell behavior where relevant:
  - default production starts the server;
  - development can use an existing local server;
  - server address is delivered to the web app.

TDD Acceptance:

- Fake process spawner tests cover start, status, restart, stop, and spawn failure.
- Server command contract tests pass with fake process backend.
- Server address events serialize consistently with Linux/Windows.

Manual Runtime Acceptance:

- App launch starts the local server.
- Web UI sees the server online.
- `/settings`, `/casting`, `/network-info`, and `/device-info` endpoints behave consistently with the other shells.

Exit Criteria:

- macOS shell can own server lifecycle without Tauri or Qt.

## Phase 5: Web UI And Mods Smoke

Objective: load the real Stremio Web UI and prove the shared mod bridge works in `WKWebView`.

Tasks:

- Load the default Stremio Web URL through the selected local proxy/direct strategy.
- Verify document-start injection happens before the web app needs shell APIs.
- Verify `window.StremioLightningHost` is available.
- Verify `window.StremioEnhancedAPI` remains plugin-facing compatible.
- Verify the mod UI bundle is injected.
- Implement webview navigation policy:
  - allow expected Stremio app origins;
  - block or externalize unexpected top-level navigations;
  - allow known embedded provider behavior only where required.
- Implement clipboard/app-active event behavior if required by existing bridge code.

TDD Acceptance:

- Host API contract tests pass for commands used by mods/plugin manager.
- Plugin-facing compatibility fixtures pass against the macOS adapter.
- Navigation policy tests cover allowed app origin, external link, and blocked unexpected main-frame navigation.

Manual Runtime Acceptance:

- Real web UI loads.
- Mods button appears.
- Mods panel opens.
- Registry loads.
- A plugin can be installed, enabled, disabled, and survives reload.
- A theme can be installed and applied.

Exit Criteria:

- The shell is usable for web/mod flows without native playback enabled.

## Phase 6: Native MPV Backend

Objective: implement macOS native playback with `libmpv` while preserving the same player command/event contract used by the other shells.

Tasks:

- Add macOS `libmpv` dependency/link strategy for development in `src/player.rs` and optional `build.rs`:
  - Homebrew `mpv`/`libmpv` for local builds, or;
  - vendored `libmpv.dylib` path for reproducible builds.
- Implement `MpvPlayerBackend` command mapping behind the `PlayerBackend` trait:
  - observe property;
  - set property;
  - command;
  - load file/stream;
  - shutdown.
- Emit MPV events through the host event system:
  - property change;
  - end file;
  - error;
  - player visible/hidden state.
- Choose first rendering path:
  - start with a native view/layer that can host MPV rendering reliably;
  - prefer a minimal OpenGL-backed view first if that is the shortest path to parity;
  - evaluate Metal/libmpv integration only after basic playback is proven.
- Place the MPV video layer behind or alongside the transparent web layer so web controls remain visible.
- Implement resize/fullscreen synchronization between window, webview, and video layer.
- Configure MPV defaults similar to official shell where useful:
  - app/audio client name;
  - cache defaults;
  - audio fallback behavior;
  - hardware decoding diagnostics.

TDD Acceptance:

- Player command mapping tests pass with fake backend.
- MPV event serialization tests match shared player event fixtures.
- Video visibility state tests cover start, property-change video detection, end, and error.

Manual Runtime Acceptance:

- A local sample video plays.
- A Stremio stream opens in native MPV.
- Web controls remain visible and clickable during playback.
- Stop/end hides or clears the player layer without stale black rectangles.
- Fullscreen playback works and exits cleanly.

Exit Criteria:

- macOS shell has functional native playback through `libmpv`.

## Phase 7: macOS App Integration

Objective: make the native shell behave like a real macOS app.

Tasks:

- Add app bundle metadata for the `stremio-lightning-macos` executable:
  - `Info.plist`;
  - bundle identifier;
  - app name;
  - icon;
  - minimum macOS version;
  - automatic graphics switching support if still relevant.
- Add URL/document handlers:
  - `stremio://`;
  - `magnet:`;
  - `.torrent` files.
- Implement second-open behavior:
  - focus existing window;
  - forward opened URL/file to the running app;
  - preserve normal macOS activation behavior.
- Implement native window commands:
  - minimize;
  - maximize/zoom;
  - fullscreen;
  - close-to-hide if product behavior requires it;
  - always-on-top only if supported and required.
- Implement app lifecycle events:
  - app became active;
  - app resigned active;
  - window focus/visibility changes;
  - shutdown cleanup.
- Implement file picker commands if still used by plugins or shell transport.
- Implement clipboard handoff behavior only if required by the web app and safe under HTTPS/WKWebView rules.

TDD Acceptance:

- URL/file open parsing tests cover `stremio://`, `magnet:`, `.torrent`, regular file paths, and unsupported schemes.
- Window command tests use mockable wrappers where possible.
- Lifecycle event serialization tests match shared host event shape.

Manual Runtime Acceptance:

- Opening a `stremio://` link focuses the app and dispatches the event to web.
- Opening a magnet link focuses the app and dispatches the event to web.
- Opening a `.torrent` file dispatches a file/open-media event.
- Close, minimize, fullscreen, app activation, and quit feel native on macOS.

Exit Criteria:

- The shell behaves like a macOS desktop application, not just a debug webview window.

## Phase 8: Packaging, Bundling, Signing

Objective: produce a distributable `.app` bundle with all required native dependencies.

Tasks:

- Define bundle layout:
  - `Contents/MacOS/stremio-lightning-macos`;
  - `Contents/Info.plist`;
  - `Contents/Resources`;
  - `Contents/Frameworks` or equivalent dylib location.
- Bundle `libmpv.dylib` and transitive native dependencies.
- Fix install names/rpaths for bundled dylibs.
- Bundle or locate the streaming server runtime files.
- Add an `xtask` command or script for local app bundle creation.
- Add codesign support for development identity/ad-hoc signing.
- Add hardened runtime entitlements required by WKWebView, JIT, networking, and MPV dependencies.
- Add notarization workflow only after bundle launch is stable.
- Document Homebrew/dev dependency setup separately from release bundle requirements.

TDD Acceptance:

- Bundle layout generation can be tested as filesystem output without signing.
- Rpath/install-name rewrite command construction is tested where practical.

Manual Runtime Acceptance:

- Double-clicking the `.app` launches the shell.
- Bundled app finds `libmpv.dylib` without Homebrew runtime assumptions.
- Bundled app starts the streaming server.
- Bundled app plays a local sample and a Stremio stream.

Exit Criteria:

- A local distributable macOS app bundle can be built and smoke-tested.

## Phase 9: Parity And Hardening

Objective: close behavioral gaps against Linux/Windows/Tauri and stabilize for production use.

Tasks:

- Run shared host command fixture tests against macOS.
- Run shared plugin API fixture tests against macOS.
- Add macOS manual smoke checklist entries.
- Compare against official shell behavior:
  - startup;
  - server launch;
  - web UI URL selection;
  - native player command handling;
  - file/deeplink handling;
  - fullscreen behavior;
  - shutdown cleanup.
- Add diagnostics:
  - webview URL/load state;
  - injection order;
  - IPC errors;
  - MPV backend/options;
  - first-frame timing;
  - server spawn logs.
- Validate on Intel and Apple Silicon if available.
- Validate clean user profile and existing user profile migrations.
- Document known WKWebView differences from WebView2/WebKitGTK.

TDD Acceptance:

- `cargo test -p stremio-lightning-macos` passes on macOS.
- Shared core tests pass.
- Shared JS/host contract tests pass.
- Player contract tests pass.

Manual Runtime Acceptance:

- Fresh launch loads web UI.
- Login flow works.
- Mods panel works.
- Plugin install/enable/disable works.
- Theme install/apply/remove works.
- Server status is online.
- Native MPV playback works for local sample and real stream.
- Fullscreen enter/exit works.
- App quit cleans up server and player.

Exit Criteria:

- macOS is feature-complete enough to be considered alongside Linux and Windows native shells.

## Suggested Implementation Order

1. Add the crate scaffold and argument parsing tests.
2. Add headless `MacosWebviewRuntime` with injection order tests.
3. Add fake-host IPC dispatch tests.
4. Add the real `WKWebView` smoke window.
5. Load a local smoke page and prove bidirectional IPC.
6. Start the streaming server from the macOS crate.
7. Load real Stremio Web and verify mods injection.
8. Add `libmpv` command/event backend with fake tests first.
9. Add real MPV rendering and local sample playback.
10. Add bundle metadata and local `.app` creation.
11. Add deeplink/file handling.
12. Add signing/notarization after local bundle smoke is stable.

## Risk Register

### WKWebView Script Injection Timing

Risk: Stremio Web may access shell APIs before the injected adapter is available.

Mitigation:

- use `WKUserScriptInjectionTimeAtDocumentStart`;
- test injection order with a local smoke page;
- keep the host adapter minimal and synchronous to install.

### WKWebView IPC Shape

Risk: WKWebView message handlers are one-way and require a request/response wrapper.

Mitigation:

- keep promise/request ID logic in the macOS host adapter;
- route native responses back with evaluated JavaScript;
- test malformed and out-of-order responses.

### MPV Rendering On macOS

Risk: OpenGL is deprecated and Metal integration may add complexity.

Mitigation:

- start with the smallest reliable playback path;
- isolate rendering from command/event backend;
- preserve fake-player tests so UI and command logic can progress independently.

### Dylib Packaging

Risk: `libmpv` and transitive dependencies may fail outside Homebrew paths.

Mitigation:

- add bundle inspection and rpath checks;
- test launch on a machine without development paths where possible;
- keep release bundling separate from early crate runtime work.

### App Sandbox/Notarization

Risk: hardened runtime or sandbox settings may block JIT, networking, dynamic libraries, or MPV behavior.

Mitigation:

- defer notarization until the unsigned bundle works;
- document required entitlements;
- test codesigned builds incrementally.

### Platform Drift

Risk: macOS host behavior diverges from Linux and Windows.

Mitigation:

- use shared host/player fixtures;
- copy behavior from existing crates only when it matches the contract;
- keep shell-specific code limited to window/webview/player plumbing.

## Definition Of Done

The macOS shell crate is complete when:

- `crates/stremio-lightning-macos` is a workspace member.
- The crate launches a native macOS window with `WKWebView`.
- The shared bridge and mod UI inject at document start.
- IPC works both web-to-native and native-to-web.
- The streaming server starts, reports status, and shuts down cleanly.
- Native MPV playback works for local and Stremio streams.
- Mods/plugin/theme flows match the other shells.
- `stremio://`, `magnet:`, and `.torrent` open events are handled.
- A local `.app` bundle can be built and launched.
- Shared core, host contract, plugin contract, and player tests pass.
