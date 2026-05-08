# Progress

## Current State

- Branch: `master`
- Remote: `origin` (`https://github.com/theguy000/stremio-lightning.git`)
- Last pushed commit before this progress update: `6b8b2c3 Add Windows single-instance launch handoff`
- Active track: Phase 6 direct Windows WebView2 shell migration
- Active plan: `docs/windows-webview2-shell-crate-plan.md`
- Official Windows reference checkout: `/tmp/stremio-shell-ng-reference`

## Completed In This Change

- Completed Milestone 8: Window Behavior Baseline.
- Added real native window commands for the Windows WebView2 bridge: minimize, maximize/restore, close, drag, fullscreen, `isMaximized`, and `isFullscreen`.
- Added fullscreen placement restore so leaving fullscreen returns the window to its prior style and placement.
- Added window state/focus/visibility event emission from Win32 messages.
- Added `WM_APPCOMMAND` media-key handling and route media keys through shell transport as `["media-key", action]`, matching `stremio-shell-ng`.
- Added safe external URL policy and Windows OS-browser opening for allowed `http`, `https`, and `mailto` URLs.
- Added WebView2 navigation blocking so the shell cannot be navigated away from the configured web UI origin.
- Added tests for media-key shell transport, window maximize state, external URL policy, and WebView2 navigation policy.

## Completed Previously On Windows Track

- Milestone 1: Windows shell crate foundation, module boundaries, shared bridge relocation, resource/settings/server/single-instance scaffolding, and core feature gating.
- Milestone 2: raw Win32 native window baseline with direct `HWND` ownership, app class registration, message loop, resize/close/dpi handling, and UI-thread wake message reservation.
- Milestone 3: WebView2 environment/controller creation attached to the native `HWND`, client-rect resizing, configured URL navigation, baseline WebView2 settings, document-created script injection, and simple native/WebView2 smoke plumbing.
- Milestone 4: Promise-based Windows WebView2 IPC, structured request/response errors, listener registration, native event dispatch, shell transport handshake routing, baseline host command behavior, and JSON host contract fixture coverage.
- Milestone 5: native MPV baseline with `libmpv2`, direct `HWND`/`wid` rendering, MPV command/property transport, event forwarding, and clean MPV shutdown.
- Milestone 6: Windows streaming server supervisor, explicit runtime/FFmpeg/FFprobe command setup, server lifecycle host commands/events, and Job Object cleanup.
- Milestone 7: Windows single-instance gate, named-pipe launch handoff, first/secondary launch open-media routing, and file/link launch intent normalization.

## Verification

- `cargo fmt --all` passed.
- `cargo test -p stremio-lightning-windows` passed: 28 tests.
- `cargo check -p stremio-lightning-windows --target x86_64-pc-windows-msvc` passed.
- `cargo test --workspace` passed.

## Runtime Status

The direct Windows shell can now own a native Win32 window, host WebView2, inject the Windows adapter and shared bridge at document-created time, navigate to the configured Stremio Web URL, route baseline host IPC, initialize native MPV against the owned `HWND`, send baseline MPV commands/properties, forward MPV property/end events back to the web app, supervise the bundled local streaming server process, route first/secondary app launches into shell transport open-media messages, execute native window controls, route media keys, and block unsafe external/navigation behavior.

Runtime testing still requires Windows:

- `npm run setup:windows-shell`
- `cargo run -p stremio-lightning-windows`

## Next Work

Immediate next milestone:

1. Runtime validate Milestones 7 and 8 on Windows by testing second-instance launch handoff, fullscreen restore, window controls, media keys, and external links.
2. Finish Milestone 6 hardening by adding stdout/HTTP readiness detection if Windows runtime validation shows the web app needs it.
3. Continue with Milestone 9: Resource Setup And Packaging Baseline from `docs/windows-webview2-shell-crate-plan.md`.
