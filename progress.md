# Progress

## Current State

- Branch: `master`
- Remote: `origin` (`https://github.com/theguy000/stremio-lightning.git`)
- Last pushed commit before this progress update: `52ec3fe Add Windows bridge host contract`
- Active track: Phase 6 direct Windows WebView2 shell migration
- Active plan: `docs/windows-webview2-shell-crate-plan.md`
- Official Windows reference checkout: `/tmp/stremio-shell-ng-reference`

## Completed In This Change

- Completed Milestone 5: Native MPV Baseline.
- Added a Windows-only `libmpv2` backend in `crates/stremio-lightning-windows/src/player.rs`.
- Initialized MPV from the native Win32 `HWND` using `wid` and baseline options for app title, audio client name, terminal/log level, quiet mode, hardware decoding, and audio fallback.
- Routed `mpv-command`, `mpv-set-prop`, `mpv-observe-prop`, and `native-player-stop` from shell transport to MPV.
- Converted MPV property/end events into shared `mpv-prop-change` and `mpv-event-ended` shell transport messages.
- Wired WebView2 UI-thread wake handling so native MPV events drain back to JavaScript listeners through the existing Windows IPC response/event channel.
- Added clean MPV shutdown by sending `quit` when the backend is dropped.
- Made shared player API types cloneable so shell backends can both record/test and dispatch commands.
- Updated Windows migration progress docs:
  - `docs/windows-webview2-shell-crate-plan.md`;
  - `docs/windows-webview2-shell-gap-analysis.md`;
  - `progress.md`.

## Completed Previously On Windows Track

- Milestone 1: Windows shell crate foundation, module boundaries, shared bridge relocation, resource/settings/server/single-instance scaffolding, and core feature gating.
- Milestone 2: raw Win32 native window baseline with direct `HWND` ownership, app class registration, message loop, resize/close/dpi handling, and UI-thread wake message reservation.
- Milestone 3: WebView2 environment/controller creation attached to the native `HWND`, client-rect resizing, configured URL navigation, baseline WebView2 settings, document-created script injection, and simple native/WebView2 smoke plumbing.
- Milestone 4: Promise-based Windows WebView2 IPC, structured request/response errors, listener registration, native event dispatch, shell transport handshake routing, baseline host command behavior, and JSON host contract fixture coverage.

## Verification

- `cargo fmt --all` passed.
- `cargo test -p stremio-lightning-windows` passed: 15 tests.
- `cargo check -p stremio-lightning-windows --target x86_64-pc-windows-msvc` passed.
- `cargo test --workspace` passed.

## Runtime Status

The direct Windows shell can now own a native Win32 window, host WebView2, inject the Windows adapter and shared bridge at document-created time, navigate to the configured Stremio Web URL, route baseline host IPC, initialize native MPV against the owned `HWND`, send baseline MPV commands/properties, and forward MPV property/end events back to the web app.

Runtime testing still requires Windows:

- `npm run setup:windows-shell`
- `cargo run -p stremio-lightning-windows`

## Next Work

Immediate next milestone:

1. Execute Milestone 6: Server Runtime Baseline from `docs/windows-webview2-shell-crate-plan.md`.
2. Define Windows-owned runtime asset paths for the local streaming server.
3. Start/stop the bundled runtime process with readiness/status events and kill-on-close behavior.
