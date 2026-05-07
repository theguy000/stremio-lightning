# Stremio Community Feature Parity Todo

## Purpose

Track features implemented by the unofficial Stremio Community desktop shell that should be copied, adapted, or explicitly skipped after the direct Windows shell crate is functional.

Reference project:

- Repository: `https://github.com/Zaarrg/stremio-community-v5`
- Branch inspected online: `webview-windows`
- Description: WebView2 and Qt6-based shell desktop app for Stremio with latest web UI support.

This is the second plan. The first plan is `docs/windows-webview2-shell-crate-plan.md`, which creates the Windows crate and the minimum direct WebView2 + MPV shell. Do not start this feature-parity work until the crate-first plan reaches its exit criteria.

This is a feature-parity checklist, not a source-copy plan. Stremio Lightning should adapt ideas into the existing Rust/shared-core architecture, with tests and security review, instead of importing C++ code directly.

## Dependency On Crate-First Plan

- [ ] Complete `docs/windows-webview2-shell-crate-plan.md`.
- [ ] Confirm `crates/stremio-lightning-windows` launches a native Win32/WebView2 window.
- [ ] Confirm the shared bridge works in WebView2.
- [ ] Confirm native MPV playback works through direct `HWND`/`wid`.
- [ ] Confirm the local server lifecycle works.
- [ ] Confirm single-instance/open-media baseline works.
- [ ] Confirm Phase 6 direct shell smoke testing has been documented.

Only after these are complete should this document drive implementation work.

## Reference Files Inspected

- `README.md`: public feature list, install modes, MPV config notes, app settings, common issues.
- `src/main.cpp`: DPI awareness, CLI args, duplicate-process warning, updater, Discord, hotkeys, boot order.
- `src/ui/mainwindow.cpp`: tray commands, window placement, dark theme, PiP, pause policies, drag/drop/deeplink handling.
- `src/webview/webview.cpp`: URL fallback, cache clear/reload, extension loading, webmods injection, navigation policy, new-window handling.
- `src/mpv/player.cpp`: portable MPV config, `gpu-next`, MPV scripts/config loading, command/event extensions.
- `src/node/server.cpp`: server process fallback lookup, stdout pipe, Windows Job Object cleanup.
- `src/core/globals.cpp`: fallback URLs, MPV defaults, app settings, extension state, updater state.
- `src/utils/*`: settings, crashlog, Discord RPC, WebView2 extensions, helper functions.
- `src/tray/*`: tray icon and custom tray menu implementation.
- `src/updater/*`: built-in updater.
- `utils/*`: MPV configs/addons, webmods, Windows packaging helpers, Scoop and Chocolatey metadata.

## Priority Legend

- P1: high-value feature to copy/adapt after the baseline crate works.
- P2: useful optional enhancement.
- P3: likely skip unless explicitly requested.

## Portable MPV Configuration

- [ ] P1 Add `portable_config/mpv.conf` support.
- [ ] P1 Add `portable_config/input.conf` support.
- [ ] P1 Add `portable_config/scripts/` support.
- [ ] P1 Add `portable_config/scripts-conf/` support if MPV script configs are used.
- [ ] P1 Add `portable_config/shaders/` support.
- [ ] P1 Document supported MPV config paths in Windows shell docs.
- [ ] P1 Allow user override of `hwdec`, `gpu-api`, `gpu-context`, `vo`, shader chains, subtitle styling, and audio behavior.
- [ ] P1 Add startup validation/logging for invalid MPV config files.
- [ ] P1 Preserve user config without overwriting it during app updates.

## Advanced MPV Features

- [ ] P2 Bundle optional Anime4K shader presets only if licensing and size are acceptable.
- [ ] P2 Support AnimeJaNai-style external shader/model packages as user-installed extras, not bundled by default.
- [ ] P2 Support RTX/VSR-style MPV config examples in docs.
- [ ] P2 Support ThumbFast integration through MPV scripts.
- [ ] P2 Add web-to-native `seek-hover` and `seek-leave` events for ThumbFast preview control.
- [ ] P2 Add a setting for ThumbFast preview height/offset.
- [ ] P2 Add docs warning that heavy shader/model initialization may freeze first playback.
- [ ] P2 Add example `input.conf` shortcuts for toggling shaders.

## Local Files, Drag And Drop, Subtitles

- [ ] P1 Support drag-and-drop local media files into the app.
- [ ] P1 Support `Open With > Stremio Lightning` local media launch.
- [ ] P1 Support local subtitle drag-and-drop while video is playing.
- [ ] P1 Convert dropped subtitle files into `mpv-command sub-add <path> select <title> <group>`.
- [ ] P1 Emit a web event when a subtitle is dropped.
- [ ] P1 Maintain a subtitle extension allowlist.
- [ ] P1 Support `.torrent` file open from command line or drag/drop if aligned with product goals.
- [ ] P1 Support `magnet:` links if aligned with existing Stremio behavior.
- [ ] P1 Support `stremio://detail` links and addon install links.
- [ ] P1 Add path decoding and path traversal safeguards around dropped/opened files.

## App Settings

- [ ] P1 Add Windows shell settings file or integrate with existing shared settings.
- [ ] P1 Support settings equivalent to `stremio-settings.ini` while preserving our schema style.
- [ ] P1 Add `CloseOnExit`.
- [ ] P1 Add `UseDarkTheme`.
- [ ] P1 Add `PauseOnMinimize`.
- [ ] P1 Add `PauseOnLostFocus`.
- [ ] P1 Add `AllowZoom`.
- [ ] P1 Add `ThumbFastHeight` if ThumbFast is supported.
- [ ] P1 Add `DiscordRichPresence` enable/disable if RPC is supported.
- [ ] P1 Regenerate defaults if settings file is missing or invalid.
- [ ] P1 Validate settings values and avoid crashing on malformed config.

## Window And Tray Enhancements

- [ ] P1 Save and restore window placement.
- [ ] P1 Add per-monitor DPI awareness.
- [ ] P1 Handle `WM_DPICHANGED` correctly.
- [ ] P1 Add dark/light titlebar handling with `DwmSetWindowAttribute`.
- [ ] P1 Add close-to-tray behavior controlled by a setting.
- [ ] P1 Add tray show/hide window action.
- [ ] P1 Add tray always-on-top action.
- [ ] P1 Add tray close-on-exit action.
- [ ] P1 Add tray dark-theme toggle.
- [ ] P1 Add pause-on-minimize behavior.
- [ ] P1 Add pause-on-lost-focus behavior.
- [ ] P1 Register media play/pause hotkey if `WM_APPCOMMAND` is insufficient.
- [ ] P2 Implement custom tray menu styling only if native menu is insufficient.
- [ ] P2 Add custom tray menu font support only if needed.

## Picture In Picture

- [ ] P1 Investigate Stremio Community PiP behavior and exact implementation path.
- [ ] P1 Define our PiP behavior: separate always-on-top mini window, resized main window, or WebView2/MPV composition mode.
- [ ] P1 Add tray command and host command for PiP.
- [ ] P1 Preserve MPV controls and web event flow in PiP.
- [ ] P2 Persist PiP size/position if implemented.

## Web UI Fallback And Recovery

- [ ] P1 Implement URL reachability fallback before initial navigation.
- [ ] P1 Consider fallback order inspired by Stremio Community, but choose Lightning-owned defaults.
- [ ] P1 Add retry/backoff when the initial web page cannot be reached.
- [ ] P1 Show a user-visible error if all configured web UI endpoints fail.
- [ ] P1 Add web cache clear for stale hosted UI problems.
- [ ] P1 Add `F5` reload and `Ctrl+F5` cache-clear reload behavior.
- [ ] P2 Add a visible “Back to Stremio” helper when navigation leaves the main UI, if still useful.

## Web Mods And Browser Extensions

- [ ] P1 Support Lightning's existing mods through the shared bridge first.
- [ ] P2 Add optional `portable_config/webmods` CSS/JS injection compatibility.
- [ ] P2 Load webmods recursively in deterministic path order.
- [ ] P2 Ignore `.map`, `.bak`, and `.tmp` files.
- [ ] P2 Add injected CSS/JS IDs derived from relative paths to prevent duplicates.
- [ ] P2 Support unpacked WebView2 browser extensions from `portable_config/extensions`.
- [ ] P2 Expose loaded extension IDs to web only if needed and safe.
- [ ] P2 Add extension failure logging.
- [ ] P3 Do not enable arbitrary browser extensions by default; require explicit opt-in.

## Discord Rich Presence

- [ ] P1 Reuse or extend existing Stremio Lightning Discord RPC core if present.
- [ ] P1 Implement web event `activity` to update Discord presence.
- [ ] P1 Run Discord RPC callbacks during the native message loop or on a safe background task.
- [ ] P1 Add setting to disable Discord RPC.
- [ ] P1 Clear presence on app exit and playback end.
- [ ] P2 Match Stremio Community's states: discovering, watching, paused.

## Server Runtime Extras

- [ ] P1 Warn if another `stremio.exe` or `stremio-runtime.exe` process may conflict.
- [ ] P1 Support `--streaming-server-disabled`.
- [ ] P1 Surface server startup failure in crash logs and UI.
- [ ] P1 Preserve a server crash event for the web side.
- [ ] P2 Support fallback lookup for an installed Stremio service/runtime path, only if safe and documented.

## Crash Logging And Diagnostics

- [ ] P1 Add crash log file under the Windows shell app data or portable config directory.
- [ ] P1 Install unhandled exception/panic logging for the Windows shell.
- [ ] P1 Log WebView2 initialization errors.
- [ ] P1 Log MPV create/initialize/playback errors.
- [ ] P1 Log server startup errors.
- [ ] P1 Log extension/webmod load errors.
- [ ] P1 Add a user-facing note for where logs live.
- [ ] P2 Add debug logging flag for WebView2/native IPC messages.

## Updater And Release Flow

- [ ] P2 Decide whether Windows shell should have a built-in updater or rely on package/install update flow.
- [ ] P2 Support `--autoupdater-endpoint=<url>` only if updater is implemented.
- [ ] P2 Support `--autoupdater-force-full` only if updater is implemented.
- [ ] P2 Verify downloaded installers with signatures or checksums before running.
- [ ] P2 Run installer and exit via a reviewed command path.
- [ ] P3 Do not copy unsigned remote update descriptor behavior without a security design.

## Packaging And Distribution Extras

- [ ] P1 Support installer build.
- [ ] P1 Support portable archive build.
- [ ] P1 Support local WebView2 runtime for portable build if licensing/size are acceptable.
- [ ] P2 Add Scoop manifest.
- [ ] P2 Add Chocolatey package metadata.
- [ ] P2 Add Winget packaging notes.
- [ ] P2 Add x64 first; evaluate x86 only if there is user demand.

## Security Review Items

- [ ] P1 Validate all new web-to-native feature messages before executing native actions.
- [ ] P1 Restrict external URL protocols for new navigation/fallback behavior.
- [ ] P1 Do not allow arbitrary shell execution through URLs or settings.
- [ ] P1 Sanitize local file and subtitle paths.
- [ ] P1 Review WebView2 extension support before enabling it.
- [ ] P1 Review remote web UI fallback before enabling third-party hosted pages.
- [ ] P1 Avoid unsigned remote update execution.
- [ ] P1 Add threat model notes for WebView2 native message bridge and extension loading.

## Testing Todo

- [ ] P1 Unit test settings parse/default/regeneration behavior.
- [ ] P1 Unit test external URL allowlist/protocol validation.
- [ ] P1 Unit test local file/subtitle/torrent/magnet classification.
- [ ] P1 Add fake MPV backend tests for advanced commands such as subtitle add and ThumbFast script messages.
- [ ] P1 Add fake WebView2 tests for reload/cache-clear/webmods decisions.
- [ ] P1 Add manual Windows smoke checklist for all enabled parity features.
- [ ] P2 Add Windows CI for parity feature tests if available.

## Recommended Parity Copy Order

1. P1 portable MPV config directory and user config loading.
2. P1 drag/drop local files and subtitles.
3. P1 app settings, pause policies, dark theme, and window placement refinements.
4. P1 tray enhancements.
5. P1 URL fallback, refresh/cache clear, and crash logs.
6. P1 Discord Rich Presence if aligned with Lightning's existing feature set.
7. P2 advanced MPV extras: shaders, ThumbFast, browser extensions, updater, package-manager manifests.

## Explicit Decisions Needed

- [ ] Decide whether to allow third-party web UI fallback URLs or only official/Lightning-controlled URLs.
- [ ] Decide whether WebView2 browser extensions are in scope.
- [ ] Decide whether built-in updater is in scope or packaging-only updates are preferred.
- [ ] Decide whether to bundle advanced MPV shaders/addons or only support user-installed configs.
- [ ] Decide whether x86 Windows builds are worth supporting.
- [ ] Decide whether PiP is required for Phase 6 or can follow after basic playback parity.
