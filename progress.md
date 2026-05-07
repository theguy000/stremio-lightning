# Progress

## Current State

- Branch: `master`
- Remote: `origin` (`https://github.com/theguy000/stremio-lightning.git`)
- Last base commit before this progress update: `0cb8737 Port mods and settings to Linux host`

## Completed In This Change

- Configured Cargo on Linux to use `clang` with `mold` for faster linking.
- Removed the abandoned streaming-server proxy path:
  - removed `proxy_streaming_server_request` from the shared host command contract;
  - removed shared proxy request/response validation code;
  - removed the Tauri proxy command and command registration;
  - removed the injected fetch/worker proxy shims from `bridge.js`;
  - removed proxy types from the frontend host API.
- Updated Linux shell server lifecycle handling:
  - starts the streaming server before opening the Linux shell window;
  - stops the child process on drop/app exit;
  - supports restart;
  - detects exited children;
  - writes stdout/stderr to log files.
- Updated docs and manual smoke checks to use direct local server access instead of the removed proxy.

## Verification

- `npm run test:ui` passed.
- `cargo test --workspace` passed.

## Important Runtime Finding

The Linux shell binary is not yet a usable Stremio shell. It opens a placeholder GL window and does not currently create a real CEF/webview instance or load `https://web.stremio.com/`.

This means the migration plan is out of sync with implementation reality:

- Phase 3 is not complete because the real Linux webview runtime, document-start injection, and JS-to-Rust IPC bridge are still missing.
- Phase 4 cannot be considered complete at runtime because mods/plugin UI cannot be exercised inside the Linux shell without the webview.
- Phase 5 cleanup has been applied, but its runtime acceptance depends on completing Phase 3 first.

## Next Work

Complete Phase 3 before continuing later phases:

1. Add the real Linux webview runtime.
2. Load `https://web.stremio.com/` in the Linux shell.
3. Inject the Linux host adapter, bridge script, and mod UI at document start.
4. Wire JS IPC to `LinuxHost`.
5. Run the Linux manual smoke checks against the actual loaded web UI.
