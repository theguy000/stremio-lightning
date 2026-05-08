# Progress

## Current State

- Branch: `master`
- Remote: `origin` (`https://github.com/theguy000/stremio-lightning.git`)
- Last pushed commit before this progress update: `d3ce251 Add Windows resource packaging baseline`
- Active track: Phase 6 direct Windows WebView2 shell migration
- Active plan: `docs/windows-webview2-shell-crate-plan.md`
- Official Windows reference checkout: `/tmp/stremio-shell-ng-reference`

## Completed In This Change

- Executed the Linux-side portion of Milestone 10: Verification And Exit.
- Re-ran the full available static verification set from this Linux workspace.
- Updated `docs/windows-webview2-shell-crate-plan.md` to mark completed Milestone 10 verification commands and keep Windows-only smoke/portable validation open.
- Updated `docs/windows-webview2-shell-gap-analysis.md` so it reflects Milestones 5 through 9 completion and the current Milestone 10 boundary.
- Confirmed `docs/stremio-community-feature-parity-todo.md` remains blocked until Windows runtime smoke validation is complete.

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

1. Run the Windows-only Milestone 10 smoke checklist on Windows: setup, launch, WebView2 UI, mods panel, server lifecycle, native MPV playback, fullscreen restore, window controls, media keys, open-media/second-instance handoff, external links, and shutdown cleanup.
2. Manually assemble and launch the portable layout on Windows, then confirm runtime/server/ffmpeg/ffprobe resources resolve from `resources/` beside the executable and `libmpv-2.dll` resolves beside the executable.
3. Finish Milestone 6 readiness hardening only if Windows runtime validation shows the web app needs stdout/HTTP readiness detection.
4. Do not start `docs/stremio-community-feature-parity-todo.md` until Windows runtime smoke and portable layout validation are complete.
