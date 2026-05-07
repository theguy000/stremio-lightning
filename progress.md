# Progress

## Current State

- Branch: `master`
- Remote: `origin` (`https://github.com/theguy000/stremio-lightning.git`)
- Last pushed commit before this progress update: `4f48d1c Add Windows WebView2 host baseline`
- Active track: Phase 6 direct Windows WebView2 shell migration
- Active plan: `docs/windows-webview2-shell-crate-plan.md`
- Official Windows reference checkout: `/tmp/stremio-shell-ng-reference`

## Completed In This Change

- Completed Milestone 4: Bridge Injection And Host Contract.
- Implemented Promise-based Windows WebView2 IPC in the injected Windows host adapter:
  - `invoke`;
  - `listen`;
  - `unlisten`;
  - baseline window methods;
  - `webview.setZoom`.
- Added native request/response routing in `crates/stremio-lightning-windows/src/host.rs` using `{ id, kind, payload }` inbound messages and `{ kind: "response", id, ok, value }` outbound messages.
- Added structured native errors for unsupported and failed commands so JavaScript receives a single rejected Promise instead of repeated bridge exceptions.
- Added listener registration and event dispatch back to JavaScript with `{ kind: "event", event, payload }` messages.
- Routed Stremio shell transport handshake responses through the shared `shell-transport-message` event used by `web/bridge/bridge.js`.
- Added baseline Windows host command behavior for bridge/plugin-facing calls, with storage-heavy mod commands still blocked behind structured not-yet-implemented errors until the later mods/storage work.
- Added JSON host contract fixture coverage at `crates/stremio-lightning-windows/tests/fixtures/host_contract.json`.
- Updated Windows migration progress docs:
  - `docs/windows-webview2-shell-crate-plan.md`;
  - `docs/windows-webview2-shell-gap-analysis.md`;
  - `docs/platform-shell-migration-plan.md`.

## Completed Previously On Windows Track

- Milestone 1: Windows shell crate foundation, module boundaries, shared bridge relocation, resource/settings/server/single-instance scaffolding, and core feature gating.
- Milestone 2: raw Win32 native window baseline with direct `HWND` ownership, app class registration, message loop, resize/close/dpi handling, and UI-thread wake message reservation.
- Milestone 3: WebView2 environment/controller creation attached to the native `HWND`, client-rect resizing, configured URL navigation, baseline WebView2 settings, document-created script injection, and simple native/WebView2 smoke plumbing.

## Verification

- `cargo fmt --all` passed.
- `cargo test -p stremio-lightning-windows` passed: 14 tests.
- `cargo check -p stremio-lightning-windows --target x86_64-pc-windows-msvc` passed.
- `cargo test --workspace` passed.

## Runtime Status

The direct Windows shell can now own a native Win32 window, host WebView2, inject the Windows adapter and shared bridge at document-created time, navigate to the configured Stremio Web URL, and route baseline host IPC with request IDs, structured errors, listener registration, and native event dispatch.

Runtime testing still requires Windows:

- `npm run setup:windows-shell`
- `cargo run -p stremio-lightning-windows`

## Next Work

Immediate next milestone:

1. Execute Milestone 5: Native MPV Baseline from `docs/windows-webview2-shell-crate-plan.md`.
2. Load/link `libmpv` from the Windows shell resource layout.
3. Initialize MPV with the native window `HWND` and implement `mpv-command`, `mpv-set-prop`, `mpv-observe-prop`, player events, and clean shutdown.
