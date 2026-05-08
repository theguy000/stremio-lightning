# Progress

## Current State

- Branch: `master`
- Remote: `origin` (`https://github.com/theguy000/stremio-lightning.git`)
- Last pushed commit before this progress update: `22cddd2 Add Windows window behavior baseline`
- Active track: Phase 6 direct Windows WebView2 shell migration
- Active plan: `docs/windows-webview2-shell-crate-plan.md`
- Official Windows reference checkout: `/tmp/stremio-shell-ng-reference`

## Completed In This Change

- Completed Milestone 9: Resource Setup And Packaging Baseline.
- Hardened `scripts/download-windows-shell-deps.sh` so a fresh Windows checkout produces and validates the direct-shell `resources/` and `mpv-dev/` development layout.
- Added runtime resource resolution for both crate-local development resources and a portable packaged layout with `resources/` beside the executable.
- Added Windows build handling that copies `resources/libmpv-2.dll` beside the Cargo-built executable so MPV delay-loading can find it during development runs.
- Added direct-shell Windows executable resources: reused Stremio icon, common controls v6 manifest, per-monitor DPI awareness, long path awareness, and `asInvoker` execution level.
- Documented the fresh Windows setup sequence and minimal portable archive layout in the active Windows shell plan.

## Completed Previously On Windows Track

- Milestone 1: Windows shell crate foundation, module boundaries, shared bridge relocation, resource/settings/server/single-instance scaffolding, and core feature gating.
- Milestone 2: raw Win32 native window baseline with direct `HWND` ownership, app class registration, message loop, resize/close/dpi handling, and UI-thread wake message reservation.
- Milestone 3: WebView2 environment/controller creation attached to the native `HWND`, client-rect resizing, configured URL navigation, baseline WebView2 settings, document-created script injection, and simple native/WebView2 smoke plumbing.
- Milestone 4: Promise-based Windows WebView2 IPC, structured request/response errors, listener registration, native event dispatch, shell transport handshake routing, baseline host command behavior, and JSON host contract fixture coverage.
- Milestone 5: native MPV baseline with `libmpv2`, direct `HWND`/`wid` rendering, MPV command/property transport, event forwarding, and clean MPV shutdown.
- Milestone 6: Windows streaming server supervisor, explicit runtime/FFmpeg/FFprobe command setup, server lifecycle host commands/events, and Job Object cleanup.
- Milestone 7: Windows single-instance gate, named-pipe launch handoff, first/secondary launch open-media routing, and file/link launch intent normalization.
- Milestone 8: native window behavior for minimize/maximize/restore/close/drag/fullscreen, window state/focus/visibility events, media keys, safe external URL opening, and WebView2 navigation blocking.

## Verification

- `cargo fmt --all` passed.
- `cargo test -p stremio-lightning-windows` passed: 29 tests.
- `cargo check -p stremio-lightning-windows --target x86_64-pc-windows-msvc` passed.
- `cargo test --workspace` passed.

## Runtime Status

The direct Windows shell can now own a native Win32 window, host WebView2, inject the Windows adapter and shared bridge at document-created time, navigate to the configured Stremio Web URL, route baseline host IPC, initialize native MPV against the owned `HWND`, send baseline MPV commands/properties, forward MPV property/end events back to the web app, supervise the bundled local streaming server process, route first/secondary app launches into shell transport open-media messages, execute native window controls, route media keys, block unsafe external/navigation behavior, and resolve resources from either development or portable packaged layouts.

Runtime testing still requires Windows:

- `npm run setup:windows-shell`
- `cargo run -p stremio-lightning-windows`

## Next Work

Immediate next milestone:

1. Runtime validate Milestones 7, 8, and 9 on Windows by testing setup, launch, packaged resource lookup, second-instance launch handoff, fullscreen restore, window controls, media keys, and external links.
2. Finish Milestone 6 hardening by adding stdout/HTTP readiness detection if Windows runtime validation shows the web app needs it.
3. Continue with Milestone 10: Verification And Exit from `docs/windows-webview2-shell-crate-plan.md`.
