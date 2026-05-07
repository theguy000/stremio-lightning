# Progress

## Current State

- Branch: `master`
- Remote: `origin` (`https://github.com/theguy000/stremio-lightning.git`)
- Last pushed commit before this progress update: `2c86feb Add Windows native MPV baseline`
- Active track: Phase 6 direct Windows WebView2 shell migration
- Active plan: `docs/windows-webview2-shell-crate-plan.md`
- Official Windows reference checkout: `/tmp/stremio-shell-ng-reference`

## Completed In This Change

- Completed most of Milestone 6: Server Runtime Baseline.
- Added a Windows streaming server supervisor in `crates/stremio-lightning-windows/src/server.rs` with start, stop, restart, running-state checks, and drop-time cleanup.
- Built the server command as `resources/stremio-runtime.exe resources/server.cjs` and explicitly passed `NO_CORS=1`, `FFMPEG_BIN`, and `FFPROBE_BIN` for compatibility with the existing Tauri/Linux launchers.
- Added stdout/stderr log file capture under the Windows shell log directory.
- Added Windows Job Object setup with kill-on-close behavior for the spawned server process.
- Wired Windows host commands for `get_streaming_server_status`, `start_streaming_server`, `stop_streaming_server`, and `restart_streaming_server`.
- Wired `server-started` and `server-stopped` host events.
- Started the server during WebView2 shell window initialization unless `--streaming-server-disabled` is passed.
- Updated Windows migration progress docs:
  - `docs/windows-webview2-shell-crate-plan.md`;
  - `docs/windows-webview2-shell-gap-analysis.md`;
  - `progress.md`.

## Completed Previously On Windows Track

- Milestone 1: Windows shell crate foundation, module boundaries, shared bridge relocation, resource/settings/server/single-instance scaffolding, and core feature gating.
- Milestone 2: raw Win32 native window baseline with direct `HWND` ownership, app class registration, message loop, resize/close/dpi handling, and UI-thread wake message reservation.
- Milestone 3: WebView2 environment/controller creation attached to the native `HWND`, client-rect resizing, configured URL navigation, baseline WebView2 settings, document-created script injection, and simple native/WebView2 smoke plumbing.
- Milestone 4: Promise-based Windows WebView2 IPC, structured request/response errors, listener registration, native event dispatch, shell transport handshake routing, baseline host command behavior, and JSON host contract fixture coverage.
- Milestone 5: native MPV baseline with `libmpv2`, direct `HWND`/`wid` rendering, MPV command/property transport, event forwarding, and clean MPV shutdown.

## Verification

- `cargo fmt --all` passed.
- `cargo test -p stremio-lightning-windows` passed: 19 tests.
- `cargo check -p stremio-lightning-windows --target x86_64-pc-windows-msvc` passed.
- `cargo test --workspace` passed.

## Runtime Status

The direct Windows shell can now own a native Win32 window, host WebView2, inject the Windows adapter and shared bridge at document-created time, navigate to the configured Stremio Web URL, route baseline host IPC, initialize native MPV against the owned `HWND`, send baseline MPV commands/properties, forward MPV property/end events back to the web app, and supervise the bundled local streaming server process.

Runtime testing still requires Windows:

- `npm run setup:windows-shell`
- `cargo run -p stremio-lightning-windows`

## Next Work

Immediate next milestone:

1. Finish Milestone 6 hardening by adding stdout/HTTP readiness detection if Windows runtime validation shows the web app needs it.
2. Execute Milestone 7: Single Instance And Open Media Baseline from `docs/windows-webview2-shell-crate-plan.md`.
3. Validate direct shell startup, server launch, MPV playback, and shutdown cleanup on Windows.
