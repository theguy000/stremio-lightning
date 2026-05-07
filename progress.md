# Progress

## Current State

- Branch: `master`
- Remote: `origin` (`https://github.com/theguy000/stremio-lightning.git`)
- Last pushed commit before this progress update: `04b4b13 Add Windows streaming server baseline`
- Active track: Phase 6 direct Windows WebView2 shell migration
- Active plan: `docs/windows-webview2-shell-crate-plan.md`
- Official Windows reference checkout: `/tmp/stremio-shell-ng-reference`

## Completed In This Change

- Completed Milestone 7: Single Instance And Open Media Baseline.
- Added a Windows single-instance gate in `crates/stremio-lightning-windows/src/single_instance.rs` using a local named mutex.
- Added named-pipe handoff so secondary launches send their launch intent to the already-running primary app and exit before creating another WebView/server/player stack.
- Added launch intent parsing for focus-only launches, local file paths, `stremio://` links, `magnet:` links, and `.torrent` arguments.
- Wired first-launch and secondary-launch delivery into the WebView window host so the primary window restores/focuses and emits shell transport `['open-media', value]` messages.
- Queued open-media shell transport in the Windows host until the web app reports `app-ready`.
- Normalized existing file path arguments to `file:///...`, matching `stremio-shell-ng` behavior.
- Added platform-neutral tests for launch argument parsing and queued open-media delivery.
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
- Milestone 6: Windows streaming server supervisor, explicit runtime/FFmpeg/FFprobe command setup, server lifecycle host commands/events, and Job Object cleanup.

## Verification

- `cargo fmt --all` passed.
- `cargo test -p stremio-lightning-windows` passed: 23 tests.
- `cargo check -p stremio-lightning-windows --target x86_64-pc-windows-msvc` passed.
- `cargo test --workspace` passed.

## Runtime Status

The direct Windows shell can now own a native Win32 window, host WebView2, inject the Windows adapter and shared bridge at document-created time, navigate to the configured Stremio Web URL, route baseline host IPC, initialize native MPV against the owned `HWND`, send baseline MPV commands/properties, forward MPV property/end events back to the web app, supervise the bundled local streaming server process, and route first/secondary app launches into shell transport open-media messages.

Runtime testing still requires Windows:

- `npm run setup:windows-shell`
- `cargo run -p stremio-lightning-windows`

## Next Work

Immediate next milestone:

1. Runtime validate Milestone 7 on Windows by launching the app twice and by launching with file/link arguments.
2. Finish Milestone 6 hardening by adding stdout/HTTP readiness detection if Windows runtime validation shows the web app needs it.
3. Continue with Milestone 8: Window Behavior Baseline from `docs/windows-webview2-shell-crate-plan.md`.
