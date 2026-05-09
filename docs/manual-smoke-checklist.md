# Manual Smoke Checklist

Use this checklist before and after migration work that touches shell startup, injection, mods, settings, streaming server, or playback.

## Setup

- Run `npm install` if dependencies are missing.
- Run `npm run setup` if sidecar resources are missing.
- Run `cargo test --manifest-path src-tauri/Cargo.toml shell_transport -- --nocapture`.
- Run `cargo test --manifest-path src-tauri/Cargo.toml streaming_server_proxy -- --nocapture`.
- Run `cargo test --manifest-path src-tauri/Cargo.toml mod_manager -- --nocapture`.

## App startup

- Start the app with `npm run tauri dev`.
- Confirm the main window opens.
- Confirm Stremio Web loads. In the native Linux shell, it should load through `http://127.0.0.1:11470/proxy/d=https%3A%2F%2Fweb.stremio.com/`.
- Open devtools and confirm there are no repeated bridge initialization errors.

## Injected UI

- Confirm the mods button appears after Stremio Web loads.
- Open the mods panel.
- Switch across Plugins, Themes, Marketplace, Settings, and About.
- Close the mods panel and confirm the hosted page remains usable.

## Plugins

- Install a plugin from the marketplace or add a local `.plugin.js` file.
- Confirm it appears in the Plugins tab.
- Enable/load it and confirm `window.StremioEnhancedAPI` calls work.
- Disable/unload it and confirm the page does not need a full app restart.
- Delete it and confirm it disappears after refresh/reload.

## Themes

- Install a theme from the marketplace or add a local `.theme.css` file.
- Apply the theme and confirm the hosted page styling changes.
- Remove/disable the theme and confirm the original styling returns.
- Delete it and confirm it disappears after refresh/reload.

## Settings

- Save a plugin setting.
- Close and restart the app.
- Confirm the setting is loaded from disk.
- Change the setting again and confirm the settings JSON remains valid.

## Streaming server

- Confirm server status reports running after startup.
- Stop the server from the UI and confirm status changes to stopped.
- Start the server and confirm status changes to running.
- Trigger a streaming-server reload from the hosted UI.
- Confirm direct local server requests work for `/settings`, `/casting`, `/network-info`, and `/device-info`.
- Confirm no `proxy_streaming_server_request` calls appear in devtools.

## Playback

- On Linux/macOS, confirm web playback or external fallback remains available and native player status reports disabled.
- On Windows, confirm native player status reports enabled.
- On Windows, start playback and confirm MPV receives `loadfile`.
- On Windows, pause/seek/stop and confirm MPV property changes are reflected in the web UI.

## macOS WKWebView Shell

- Build the local app bundle with `cargo xtask package-macos` on macOS.
- Launch `dist/Stremio Lightning.app` by double-clicking it.
- Confirm the web UI loads through the streaming-server proxy URL.
- Confirm `window.StremioLightningHost`, `window.StremioEnhancedAPI`, and the mods button are available after document-start injection.
- Confirm `stremio://`, `magnet:`, and `.torrent` opens focus the existing app and dispatch `open-media` through `shell-transport-message`.
- Confirm server status reports online and `Contents/Resources` sidecar files are used instead of development paths.
- Start local sample playback and a real Stremio stream; confirm native MPV receives `loadfile`, first frame appears, pause/seek/stop events reach the web UI, and fullscreen enter/exit works.
- Check diagnostics for webview URL/load state, injection order, IPC errors, MPV backend/options, first-frame timing, and streaming-server stdout/stderr log paths.
- Test with a fresh user profile and an existing user profile under `~/Library/Application Support`.
- Compare startup, URL selection, server launch, playback, file/deeplink handling, fullscreen, and shutdown cleanup against Linux/Windows native shells and the official shell behavior.
- Note WKWebView differences during smoke testing: document-start injection timing, one-way `WKScriptMessageHandler` IPC with evaluated JavaScript responses, HTTPS/local proxy rules, and Web Inspector availability.

## Shutdown

- Close the app window.
- Confirm the streaming server sidecar exits.
- Confirm Discord RPC stops if it was running.
