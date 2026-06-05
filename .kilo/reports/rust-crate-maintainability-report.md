# Rust Crate Code Maintainability Report

Generated: 2026-06-05
Workspace: stremio-lightning
Focus: local Rust crate/module code, not third-party crate maintenance
Data sources: workspace inventory, `git log` file activity, line-count scan, `rg` smell scans, manual source review, `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`

## Executive Summary

- Local crates reviewed: 5 (`stremio-lightning-core`, `stremio-lightning-linux`, `stremio-lightning-windows`, `stremio-lightning-macos`, `xtask`).
- Review units covered: 38 planned modules plus Linux integration tests and workflow files.
- Critical/high findings: 8.
- Highest-value improvement: isolate native window/WebView/player lifecycle code behind smaller typed wrappers, then extract shared bridge injection and streaming-server command construction into core/platform-neutral helpers.
- Current validation health is good for the default Linux environment: formatting, Clippy, and workspace Rust tests all pass.
- Main maintainability risk is not dependency health. It is local code concentration in native UI/player modules, stringly typed host protocols, lifecycle/shutdown paths that suppress failures, and uneven real platform coverage.

## Activity And Static Signals

| Signal | Result |
| --- | --- |
| Rust source size under `crates` | 16,525 LOC |
| Largest files | `linux/native_window.rs` 1,402 LOC, `core/host_api.rs` 1,113 LOC, `windows/host.rs` 1,097 LOC, `macos/host.rs` 992 LOC, `windows/webview.rs` 932 LOC |
| Highest 12-month file activity | `linux/native_window.rs` 37 touches, `xtask/main.rs` 30, `windows/host.rs` 24, `linux/host.rs` 21, `macos/host.rs` 17, `windows/webview.rs` 16 |
| Unsafe/FFI marker hotspots | `windows/window.rs` 41, `linux/native_window.rs` 33, `linux/player.rs` 21, `windows/webview.rs` 19, `windows/single_instance.rs` 14 |
| Panic-prone marker note | Many `unwrap`/`expect` occurrences are test-only. Production examples are cited in detailed findings. |
| Lint suppressions/TODO density | Only 3 broad markers found: `linux/player.rs:208`, `windows/webview.rs:109`, `macos/diagnostics.rs:48`; no useful TODO/FIXME/HACK density signal. |
| Test layout | Linux has 2 integration test files. Windows and macOS have no `tests/**/*.rs` integration directories. `xtask` has 0 tests. |

## Ranked Findings

| Rank | Crate/Module | Score | Severity | Evidence | Impact | Recommendation |
| --- | --- | ---: | --- | --- | --- | --- |
| 1 | `stremio-lightning-linux::native_window` | 84 | Critical | `crates/stremio-lightning-linux/src/native_window.rs:1-28`, `107-138`, `416-677`, `838-897`, `1003-1235` | Large, high-churn file mixes GTK, WebKit, X11, libmpv, PiP, IPC, and threading. | split/refactor module; isolate unsafe/FFI behind a safer wrapper |
| 2 | `stremio-lightning-core::host_api` | 80 | Critical | `crates/stremio-lightning-core/src/host_api.rs:502-590`, `593-599`, `634-853`, `871-897` | Central shared host protocol has large string dispatch, sync/async bridging, panic-prone locks, and dropped transport errors. | split/refactor module; improve error/lifecycle handling |
| 3 | `stremio-lightning-windows::window` | 78 | Critical | `crates/stremio-lightning-windows/src/window.rs:116-117`, `153`, `520-623` | Win32 raw state and unsafe `Send`/`Sync` wrappers sit directly in event dispatch with ignored handler errors. | isolate unsafe/FFI behind a safer wrapper |
| 4 | `stremio-lightning-windows::webview` | 76 | Critical | `crates/stremio-lightning-windows/src/webview.rs:310-349`, `402-423`, `447-476`, `515-520` | WebView2 setup/teardown, host startup, player startup, and server startup are coupled in one lifecycle hotspot. | split/refactor module; improve error/lifecycle handling |
| 5 | `xtask::package_linux` | 72 | High | `crates/xtask/src/package_linux.rs:20-120`, `331-354`, `364-409`, `618-629`; `crates/xtask/src/main.rs:25` | Release packaging logic is large, command-heavy, partly best-effort, and has runtime-version drift risk. | add tests; split/refactor module |
| 6 | platform host/player/server lifecycle | 68 | High | `linux/host.rs:77-85`, `140-142`; `macos/host.rs:121-128`, `216-219`, `255-263`; `windows/single_instance.rs:122-135`; `windows/player.rs:237-243`, `281-309` | Shutdown and transport paths can silently lose errors or run detached forever, making intermittent native failures hard to diagnose. | improve error/lifecycle handling; add tests |
| 7 | cross-platform bridge and server duplication | 61 | High | `linux/webview_runtime.rs:8-21`, `64-107`; `windows/webview.rs:8-21`, `60-103`; `macos/webview_runtime.rs:8-21`, `196-239`; `linux/streaming_server.rs:172-197`; `windows/server.rs:179-197`; `macos/streaming_server.rs:336-355` | Bridge script order and sidecar process specs are duplicated across platforms and likely to drift. | extract shared platform-neutral logic |
| 8 | platform-native test coverage | 58 | High | `crates/stremio-lightning-linux/tests/e2e_host.rs:8-23`; `crates/stremio-lightning-linux/Cargo.toml:29-31`; no Windows/macOS `tests/**/*.rs`; `xtask` test output has 0 tests | Default tests validate pure Rust and mocks well but do not exercise real WebView/window/native lifecycle paths. | add tests |
| 9 | `stremio-lightning-core::discord_rpc` | 52 | Medium | `crates/stremio-lightning-core/src/discord_rpc.rs:52-76`, `80-90`, `183-229` | New single-owner feature uses a detached reconnect loop and weak observability around close/reconnect failures. | improve error/lifecycle handling |
| 10 | core update/version helpers | 46 | Medium | `crates/stremio-lightning-core/src/app_update.rs:69-106`; `crates/stremio-lightning-core/src/mods.rs:167-198` | Semver-like comparison is duplicated and network update paths are thinly isolated for tests. | extract shared platform-neutral logic; add tests |
| 11 | `stremio-lightning-macos::native_window` | 42 | Medium | `crates/stremio-lightning-macos/src/native_window.rs:121-169`; `README.md:54-58` | macOS is correctly marked in development, but the current native window path is mostly a plan/stub rather than runtime coverage. | keep and monitor; add platform smoke tests when implementation lands |

## Detailed Findings

### 1. `stremio-lightning-linux::native_window`

- Score: 84/100, Critical, confidence high.
- Files: `crates/stremio-lightning-linux/src/native_window.rs`, `crates/stremio-lightning-linux/src/player.rs`, `crates/stremio-lightning-linux/tests/e2e_host.rs`.
- Evidence: The file is 1,402 LOC and was touched 37 times in the last 12 months by 2 authors.
- Evidence: The import block spans GTK, WebKit, libmpv, X11/libc FFI, serde, GLib, channels, and PiP concerns in one module at `native_window.rs:1-28`.
- Evidence: X11 raw FFI declarations and raw pointer event structs are local to this UI module at `native_window.rs:88-138`.
- Evidence: Window construction, focus debounce, WebView creation, policy handling, IPC parsing, fullscreen, PiP, and runtime dispatch are spread through `native_window.rs:184-277` and `416-677`.
- Evidence: Always-on-top PiP reaches into X11 atoms and client messages at `native_window.rs:838-897`.
- Evidence: MPV render initialization mutates process-global locale, creates GL render context, bridges a standard channel to GLib with a detached thread, installs callbacks, and renders frames in `native_window.rs:1003-1235`.
- Evidence: Production `expect` calls can panic in native render paths at `native_window.rs:1182` and `1229`; update callbacks also drop channel send failures at `native_window.rs:1197-1199`.
- Evidence: Default tests cover helpers and serialization at `native_window.rs:1308-1402`, while the Linux E2E is ignored by default at `tests/e2e_host.rs:8-23`.
- Why it matters: This module is high-impact runtime code with high churn, unsafe/FFI boundaries, UI event timing, process-global state, and native rendering. A small feature change can unintentionally affect window focus, PiP, WebView IPC, or MPV rendering.
- Recommendation: Split into focused modules such as `window_shell`, `webkit_ipc`, `x11_window_state`, `mpv_gl_renderer`, and `pip_controller`. Put X11 and libmpv unsafe calls behind narrow safe APIs with documented invariants. Replace production render `expect` calls with error surfacing that can disable native video gracefully.
- Suggested validation: Add non-ignored smoke tests for IPC/fullscreen/PiP state transitions and manual Linux runtime checks on X11 and Wayland/Xwayland. Keep `cargo test -p stremio-lightning-linux` and `cargo clippy -p stremio-lightning-linux --all-targets` in CI.
- Effort: large.

### 2. `stremio-lightning-core::host_api`

- Score: 80/100, Critical, confidence high.
- Files: `crates/stremio-lightning-core/src/host_api.rs`.
- Evidence: The file is 1,113 LOC and holds shared listener state, IPC parsing, command dispatch, platform differences, transport serialization, async runtime creation, and tests.
- Evidence: The public IPC dispatch matrix is stringly typed at `host_api.rs:502-590`.
- Evidence: Async commands are run by a shared current-thread Tokio runtime through `runtime.block_on` at `host_api.rs:593-599`, with runtime construction at `host_api.rs:984-992`.
- Evidence: The synchronous command matrix spans many unrelated responsibilities from init and windows to mods, settings, PiP, native player, and Discord RPC at `host_api.rs:634-853`.
- Evidence: Production lock unwraps exist in player/focus preference paths at `host_api.rs:452`, `456`, `475`, `795`, `798`, `803`, `810`, and `878`.
- Evidence: Auto-pause transport failures are dropped with `.ok()` at `host_api.rs:893-896`.
- Evidence: Tests validate protocol shapes at `host_api.rs:1010-1113`, but the command matrix and lifecycle transitions are mostly tested through platform crates rather than this core API boundary.
- Why it matters: `host_api` is the shared contract across all shells. Stringly dispatch and platform-specific branches inside the core path make future commands harder to review and increase regression risk across platforms.
- Recommendation: Extract typed command handlers by area: window, webview, server, mods/settings, player transport, PiP, Discord RPC. Replace `Mutex::lock().unwrap()` in production with contextual errors. Prefer a single typed transport event enum and eliminate platform-name conditionals where possible.
- Suggested validation: Add direct core tests for every `HostCommand`/IPC kind, invalid payload behavior, auto-pause state transitions, and bridge error propagation.
- Effort: large.

### 3. `stremio-lightning-windows::window`

- Score: 78/100, Critical, confidence high.
- Files: `crates/stremio-lightning-windows/src/window.rs`.
- Evidence: The file is 658 LOC with 41 unsafe/FFI markers.
- Evidence: `UiThreadNotifier` is marked `unsafe impl Send` and `unsafe impl Sync` at `window.rs:116-117`; `NativeWindowController` is marked `unsafe impl Send` at `window.rs:153`.
- Evidence: Win32 message dispatch stores and retrieves raw `WindowState` through `GWLP_USERDATA`, dereferences raw pointers, and drops the box on `WM_NCDESTROY` at `window.rs:520-623`.
- Evidence: Several handler failures are ignored in the message procedure, including resize, state, focus, media key, and UI wake callbacks at `window.rs:547-581`.
- Evidence: Non-Windows builds return a stub at `window.rs:630-638`, so default Linux validation cannot exercise the real Win32 loop.
- Why it matters: The window procedure owns the process-level event loop and native handle lifetimes. Unsound `Send`/`Sync` assumptions or swallowed callback failures can manifest as focus, resize, shutdown, or media-key bugs that are hard to reproduce.
- Recommendation: Wrap raw window state in a smaller `WindowStateHandle` abstraction with safety comments on thread affinity and lifetime. Convert handler return failures into logged structured diagnostics or a queued fatal event instead of `let _ =`.
- Suggested validation: Add Windows CI smoke coverage for create/resize/focus/close events and targeted tests around `NativeWindowController` state transitions where possible.
- Effort: medium to large.

### 4. `stremio-lightning-windows::webview`

- Score: 76/100, Critical, confidence high.
- Files: `crates/stremio-lightning-windows/src/webview.rs`.
- Evidence: The file is 932 LOC with 19 unsafe/FFI markers and 16 touches in the last 12 months.
- Evidence: `on_created` binds the native window, initializes the MPV player, starts the streaming server, creates WebView2 environment/controller, configures scripts/handlers, navigates, and stores runtime state in one method at `webview.rs:310-349`.
- Evidence: Teardown ignores WebView2 event-token removal and controller close errors at `webview.rs:402-423`.
- Evidence: WebView2 async callback handoff uses `expect` on channel sends at `webview.rs:447-450` and `473-476`.
- Evidence: WebView2 settings calls drop errors via `.ok()` at `webview.rs:514-520`.
- Evidence: Non-Windows builds return a stub at `webview.rs:693-705`, so default Linux validation cannot exercise the real WebView2 COM lifecycle.
- Why it matters: Startup and teardown are tightly coupled. Failures during environment creation, script injection, host startup, or teardown can leave the native shell partially initialized with little recovery or diagnostics.
- Recommendation: Split WebView2 lifecycle into explicit phases: host startup, environment creation, controller creation, script injection, navigation, event token registration, and teardown. Track event tokens in a RAII guard and report cleanup failures once.
- Suggested validation: Add tests for lifecycle phase ordering and add Windows smoke checks for navigation, IPC response delivery, and clean close.
- Effort: large.

### 5. `xtask::package_linux`

- Score: 72/100, High, confidence high.
- Files: `crates/xtask/src/package_linux.rs`, `crates/xtask/src/main.rs`, `.github/workflows/publish.yml`.
- Evidence: `package_linux.rs` is 732 LOC and mixes AppImage, deb, Flatpak, Flatpak Builder, AppDir layout, ELF detection, ldd parsing, patchelf, desktop metadata, and shell wrapper generation.
- Evidence: AppImage/deb setup and file layout logic starts at `package_linux.rs:20-120`; AppDir preparation and shell build are at `package_linux.rs:364-409`.
- Evidence: GLIBC offender detection only checks `GLIBC_2.43` when `LINUX_FLATPAK_RUNTIME_VERSION == "25.08"` at `package_linux.rs:331-354`, but the current constant is `"50"` at `xtask/src/main.rs:25`.
- Evidence: Failed `patchelf --replace-needed` operations only print a warning and continue at `package_linux.rs:618-629`.
- Evidence: The publish workflow builds UI and packages artifacts at `.github/workflows/publish.yml:68-94` and `158-170`, but does not run `cargo xtask validate` before packaging.
- Evidence: `cargo test --workspace` reports 0 `xtask` tests.
- Why it matters: Packaging is release-critical and command-heavy. Silent best-effort behavior can ship broken artifacts, while hardcoded runtime checks can become stale as GNOME/Freedesktop versions move.
- Recommendation: Extract testable pure functions for AppDir layout, library inclusion/exclusion, GLIBC policy, and patchelf decisions. Make patchelf failure policy explicit by artifact type. Add `cargo xtask validate` or equivalent Rust validation before packaging in CI.
- Suggested validation: Add unit tests around `resolved_ldd_path`, `should_bundle_linux_library`, GLIBC policy, and package script generation. Add a CI preflight job that runs formatting, Clippy, and tests before release packaging.
- Effort: medium.

### 6. Platform host/player/server lifecycle

- Score: 68/100, High, confidence high.
- Files: `crates/stremio-lightning-linux/src/host.rs`, `crates/stremio-lightning-macos/src/host.rs`, `crates/stremio-lightning-windows/src/single_instance.rs`, `crates/stremio-lightning-windows/src/player.rs`, `crates/stremio-lightning-windows/src/server.rs`, `crates/stremio-lightning-linux/src/streaming_server.rs`, `crates/stremio-lightning-macos/src/streaming_server.rs`.
- Evidence: Linux and macOS platform bridges silently accept unknown custom transport methods by returning `Ok(())` at `linux/host.rs:77-85` and `macos/host.rs:121-128`, while Windows rejects unsupported methods at `windows/host.rs:193-200`.
- Evidence: Linux shutdown drops player-stop errors at `linux/host.rs:140-142`; macOS shutdown drops lifecycle and player stop errors at `macos/host.rs:216-219`.
- Evidence: macOS drains player events after player commands with `.ok()` at `macos/host.rs:255-263`.
- Evidence: Windows single-instance starts an infinite listener loop without a shutdown path at `single_instance.rs:122-135`.
- Evidence: Windows player shutdown sends a shutdown command but intentionally does not join the worker thread at `windows/player.rs:237-243`; the worker loop runs at `windows/player.rs:281-309`.
- Evidence: Streaming server drops ignore stop errors on Linux, Windows, and macOS at `linux/streaming_server.rs:166-169`, `windows/server.rs:173-176`, and `macos/streaming_server.rs:330-333`.
- Why it matters: These are runtime-critical paths for player control, single-instance launch behavior, streaming server processes, and shutdown. Silent success and detached loops make failures intermittent and hard to diagnose.
- Recommendation: Standardize unsupported transport behavior across platforms. Introduce explicit shutdown tokens for listener/player threads. Add structured logs for stop failures and make shutdown idempotency visible to tests.
- Suggested validation: Add tests for unsupported transport behavior, shutdown ordering, double-stop/restart, and background worker termination. Add Windows named-pipe/single-instance smoke checks in Windows CI.
- Effort: medium.

### 7. Cross-platform bridge and server duplication

- Score: 61/100, High, confidence high.
- Files: `crates/stremio-lightning-linux/src/webview_runtime.rs`, `crates/stremio-lightning-windows/src/webview.rs`, `crates/stremio-lightning-macos/src/webview_runtime.rs`, `crates/stremio-lightning-linux/src/streaming_server.rs`, `crates/stremio-lightning-windows/src/server.rs`, `crates/stremio-lightning-macos/src/streaming_server.rs`.
- Evidence: Bridge script constants are duplicated in Linux at `linux/webview_runtime.rs:8-21`, Windows at `windows/webview.rs:8-21`, and macOS at `macos/webview_runtime.rs:8-21`.
- Evidence: Bridge module list construction is duplicated in Linux at `linux/webview_runtime.rs:64-107`, Windows at `windows/webview.rs:60-103`, and macOS at `macos/webview_runtime.rs:196-239`.
- Evidence: Sidecar server command specs repeat environment keys, log file names, and resource path conventions in Linux at `linux/streaming_server.rs:172-197`, Windows at `windows/server.rs:179-197`, and macOS at `macos/streaming_server.rs:336-355`.
- Why it matters: The bridge and sidecar server are cross-platform contracts. Duplicated script order or command spec logic can drift without tests catching every platform.
- Recommendation: Move shared bridge module names/order into a core helper that accepts only the platform adapter source. Extract a shared `SidecarCommandSpec` builder with platform-specific path and argument policy hooks.
- Suggested validation: Add shared fixture tests asserting all platforms inject the same bridge module order and construct sidecar commands from the same normalized model.
- Effort: medium.

### 8. Platform-native test coverage

- Score: 58/100, High, confidence high.
- Files: `crates/stremio-lightning-linux/tests/e2e_host.rs`, `crates/stremio-lightning-linux/tests/process_spawner.rs`, `crates/stremio-lightning-linux/Cargo.toml`, Windows/macOS crate test layout.
- Evidence: The Linux E2E test is ignored by default and gated by `STREMIO_LIGHTNING_LINUX_E2E=1` or `STREMIO_LIGHTNING_LINUX_SMOKE=1` at `tests/e2e_host.rs:8-23`.
- Evidence: `process_spawner` is configured with `harness = false` at `linux/Cargo.toml:29-31` and manually prints/runs test functions at `tests/process_spawner.rs:8-52`.
- Evidence: No files were found under `crates/stremio-lightning-windows/tests/**/*.rs` or `crates/stremio-lightning-macos/tests/**/*.rs`.
- Evidence: `cargo test --workspace` passed 172 tests with 1 ignored Linux E2E test and 0 `xtask` tests.
- Why it matters: Unit tests are strong for pure logic, protocol shapes, and mockable state. They do not provide the same confidence for WebView2, Win32 window loops, WKWebView/AppKit, X11/GTK/libmpv rendering, named pipes, or release packaging.
- Recommendation: Add a small platform smoke tier separate from pure unit tests. Keep it opt-in for local machines but run it on the matching CI OS when native dependencies are available.
- Suggested validation: Windows: single-instance/named-pipe, WebView2 navigation, window create/close. Linux: X11/Xwayland PiP, GTK/WebKit IPC, libmpv render creation failure path. macOS: AppKit/WKWebView launch when implementation is available. Xtask: pure function tests for packaging.
- Effort: medium to large.

### 9. `stremio-lightning-core::discord_rpc`

- Score: 52/100, Medium, confidence medium-high.
- Files: `crates/stremio-lightning-core/src/discord_rpc.rs`.
- Evidence: The file has only 1 historical commit and 1 author in the scanned history.
- Evidence: `start` stores a client and spawns reconnect after a failed initial connection while still returning `Ok(())` at `discord_rpc.rs:52-76`.
- Evidence: `stop` ignores `clear_activity` and `close` failures at `discord_rpc.rs:80-90`.
- Evidence: Reconnect uses a detached thread with atomic flags and one-second polling inside a 10-second interval at `discord_rpc.rs:183-229`.
- Why it matters: RPC is not as core as window/player code, but detached reconnection and weak diagnostics can cause confusing user-facing state when Discord is unavailable or reconnecting.
- Recommendation: Track a join handle or cancellation token, expose reconnect state for diagnostics, and test start/stop/reconnect behavior behind a mock IPC client trait.
- Suggested validation: Add tests for failed initial connect, stop during reconnect wait, update while disabled, and close/clear error reporting.
- Effort: small to medium.

### 10. Core update/version helpers

- Score: 46/100, Medium, confidence high.
- Files: `crates/stremio-lightning-core/src/app_update.rs`, `crates/stremio-lightning-core/src/mods.rs`.
- Evidence: App update comparison is implemented at `app_update.rs:69-106`.
- Evidence: Mod update comparison repeats the same parse/compare pattern at `mods.rs:167-198`.
- Evidence: App update checks GitHub directly in `app_update.rs:24-63`, which makes error-path and response-shape testing harder without a seam.
- Why it matters: Version comparison is a shared business rule. Duplicating it invites drift between app updates and mod updates.
- Recommendation: Extract a shared `version` helper in core, reuse it in app and mod update checks, and add table-driven tests for prerelease, invalid numeric segments, leading `v`, and missing parts.
- Suggested validation: Add direct tests for the shared comparator and, if feasible, introduce a minimal HTTP client seam for update response/error tests.
- Effort: small.

### 11. `stremio-lightning-macos::native_window`

- Score: 42/100, Medium, confidence high.
- Files: `crates/stremio-lightning-macos/src/native_window.rs`, `README.md`.
- Evidence: README marks macOS as in development at `README.md:54-58`.
- Evidence: The target macOS path delegates to `appkit_shell::run` at `native_window.rs:121-129`, but that implementation currently logs preparation and returns `Ok(())` at `native_window.rs:146-159`.
- Evidence: Non-macOS builds return a platform error stub at `native_window.rs:162-169`.
- Why it matters: The score is lower because macOS is not advertised as supported, but this area will become high-risk once real AppKit/WKWebView lifecycle code lands.
- Recommendation: Keep and monitor while macOS is in development. Before marking macOS supported, add the same lifecycle/unsafe/test gates recommended for Windows and Linux.
- Suggested validation: Add a macOS-only smoke test for app launch, WKWebView load, host IPC, player initialization, and clean shutdown.
- Effort: medium.

## Prioritized Backlog

| Priority | Action | Area | Effort | Expected Benefit |
| --- | --- | --- | --- | --- |
| P0 | Isolate Linux native X11/libmpv/GTK/WebKit responsibilities into smaller modules and safe wrappers. | Linux native window/player | Large | Reduces highest combined churn, unsafe, and runtime-impact hotspot. |
| P0 | Split core `host_api` command dispatch into typed handler groups and replace production lock unwraps. | Core host API | Large | Makes cross-platform command changes reviewable and less panic-prone. |
| P1 | Introduce lifecycle phase objects/RAII cleanup for Windows WebView2 and Win32 window event handling. | Windows window/WebView | Large | Improves teardown reliability and diagnostics around native resources. |
| P1 | Add xtask packaging tests and fail/warn policy for patching/linkage decisions. | Xtask packaging | Medium | Reduces release artifact regressions. |
| P1 | Standardize unsupported transport and shutdown error behavior across Linux, Windows, and macOS. | Platform hosts/player/server | Medium | Makes behavior predictable and failures visible. |
| P2 | Extract shared bridge injection order and sidecar command builders. | Platform WebView/server | Medium | Prevents cross-platform drift. |
| P2 | Add platform smoke tiers in matching CI OS jobs. | Linux/Windows/macOS | Medium to large | Covers native paths not exercised by default Linux tests. |
| P2 | Extract shared core version comparison helper. | Core app/mod updates | Small | Removes duplicated update business logic. |
| P3 | Add a mockable Discord RPC client seam and reconnect lifecycle tests. | Core Discord RPC | Small to medium | Improves observability for a new feature. |

## Validation Results

| Command | Result | Notes |
| --- | --- | --- |
| `cargo fmt --all -- --check` | Passed | No output. |
| `cargo clippy --workspace --all-targets` | Passed | Completed successfully. |
| `cargo test --workspace` | Passed | 172 passed, 1 ignored Linux E2E, 0 failed. |
| `cargo xtask validate` | Not run | It repeats Rust checks and runs UI test/build steps. This audit was Rust-code focused and report-only; UI build steps can rewrite generated frontend output. |

## Appendix

### Commands And Scans Used

- `git ls-files 'crates/**/*.rs' | xargs wc -l | sort -nr`
- `git log --since='12 months ago' --name-only --pretty=format: -- crates | sort | uniq -c | sort -nr`
- Hotspot history loop using `git rev-list --count HEAD -- <file>`, `git log --format='%aN' -- <file>`, and `git log -1 --format='%cs %s' -- <file>`.
- `rg --count-matches --glob '*.rs' '\bunsafe\b|extern "|\*mut|\*const|unsafe impl' crates`
- `rg --count-matches --glob '*.rs' '\bunwrap\(|\bexpect\(|panic!' crates`
- `rg --count-matches --glob '*.rs' 'thread::spawn|std::thread::spawn|tokio::spawn|spawn_local|block_on|Command::new' crates`
- `rg --count-matches --glob '*.rs' '#\[allow\(|TODO|FIXME|HACK|\.ok\(\)|let _ = ' crates`
- `cargo fmt --all -- --check`
- `cargo clippy --workspace --all-targets`
- `cargo test --workspace`

### Files Intentionally Excluded

- Third-party dependency/crate maintenance health was excluded except where local FFI/WebView/MPV/native packaging code creates local maintainability risk.
- Frontend Svelte/TypeScript source was excluded. It is only referenced because the Rust `xtask` validation flow invokes UI scripts.
- No iOS crate was reviewed because the current workspace contains Windows, Linux, macOS, core, and xtask only.

### Platform-Specific Checks Requiring Native Runtime Access

- Linux: X11/Xwayland PiP always-on-top behavior, WebKitGTK message handler behavior, GTK GLArea/libmpv render context error handling.
- Windows: WebView2 COM lifecycle, Win32 window procedure behavior, named-pipe single-instance listener, media-key dispatch, native player thread shutdown.
- macOS: real AppKit/WKWebView launch, native window loop, bundled libmpv/WKWebView integration, app bundle signing/notarization behavior.
