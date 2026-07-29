# macOS TODO

## Blockers
- Implement the AppKit application, native window, WKWebView, IPC handler, and event loop.
- Attach MPV to a native video layer before loading the web UI.
- Connect minimize, maximize, fullscreen, close, focus, webview zoom, and picture-in-picture state to the native window.
- Implement AppKit open/reopen URL and file callbacks, including subsequent launches, and deliver stremio, magnet, torrent, and file intents to the web UI.
- Fix local sidecar resource resolution so it matches the documented macOS crate paths.
- Prevent loading the local proxy when sidecar startup fails, and emit server-started after successful startup.
- Route stremio and magnet navigation as launch intents instead of passing them to the external URL opener.

## Release
- Add a macOS CI build/package job and publish a signed, notarized distributable archive or disk image.
- Wire Developer ID signing and notarization secrets into the release workflow.
- Fail packaging when a non-system dylib dependency cannot be resolved.
- Verify bundled dylib references after install_name_tool rewrites.
- Add reproducible macOS dependency setup with SHA-256 verification for downloaded service and media-runtime archives.
- Generate Info.plist bundle versions from the checked-in release version.
- Add and package AppIcon.icns.
- Ensure the app, Stremio runtime, FFmpeg, FFprobe, and libmpv match the requested architecture.
- Add a macOS launch smoke test to release CI.

## Platform parity
- Connect navigation policy decisions to WKWebView.
- Emit application activation, focus, visibility, and shutdown events from AppKit.
- Implement native DevTools toggling and title-bar dragging.
- Add WKWebView navigation, process, HTTP, and network-failure diagnostics with real shell and engine metadata.
- Add macOS media-key support.
- Implement automatic update download and installation; shared update checking and the update banner already exist.
- Register the supported local media file associations in Info.plist.
- Validate playback, server lifecycle, Discord Rich Presence, and updates on Apple Silicon and Intel hardware.

## Windows and Linux associations
- Register stremio and magnet protocols plus supported media file associations in the Windows installer.
- Register stremio and magnet protocols plus supported media MIME associations in Linux desktop and Flatpak metadata, and route Linux launch arguments to the web UI.

## Tests
- Add native integration coverage for AppKit launch, WKWebView IPC, MPV attachment, launch intents, and clean shutdown.
- Make the macOS packaging command test use platform-independent paths so the crate test suite passes on Windows.

## Documentation
- Release gate: keep macOS marked as in development until the native shell is functional and hardware-tested.
