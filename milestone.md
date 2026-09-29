# macOS Milestones

Implementation phases should be one focused commit each and leave the project building with relevant tests passing. Validation phases may produce test evidence or documentation instead of code.

## Milestone 1 - Functional native shell

### Phase 1.1 - AppKit lifecycle
- Create the `NSApplication`, application delegate, native window, and event loop.
- Keep native objects alive until orderly application shutdown.

### Phase 1.2 - WKWebView
- Create and configure the transparent WKWebView.
- Load the configured web UI and inject the existing bridge assets.

### Phase 1.3 - IPC bridge
- Register the WKWebView IPC message handler.
- Route requests to the existing host and return responses and events to JavaScript.

### Phase 1.4 - MPV video layer
- Create a native video layer behind the transparent WKWebView.
- Attach and retain the MPV renderer before loading the web UI.

### Phase 1.5 - Navigation policy
- Connect the existing navigation policy decisions to the WKWebView navigation delegate.

**Exit:** The app opens a persistent native window, loads the web UI, completes an IPC round trip, and renders MPV through the native video layer.

## Milestone 2 - Desktop integration and parity

### Phase 2.1 - Native window controls
- Connect minimize, maximize, fullscreen, close, focus, and webview zoom to AppKit and WKWebView.
- Connect picture-in-picture state and restoration to the native window.

### Phase 2.2 - Lifecycle events
- Emit activation, focus, visibility, and shutdown events from AppKit callbacks.

### Phase 2.3 - Launch callbacks
- Implement AppKit open, reopen, URL, and file callbacks for initial and subsequent launches.
- Deliver stremio, magnet, torrent, and file intents to the web UI.

### Phase 2.4 - Intent routing and file associations
- Route stremio and magnet navigation as launch intents instead of external URLs.
- Register supported local media file associations in `Info.plist`.

### Phase 2.5 - Sidecar resource paths
- Make local sidecar resource resolution match the documented macOS crate paths.

### Phase 2.6 - Sidecar startup behavior
- Prevent loading the local proxy when required sidecar startup fails.
- Emit `server-started` after successful startup.

### Phase 2.7 - Native developer controls
- Implement native DevTools toggling and title-bar dragging.

### Phase 2.8 - WKWebView diagnostics
- Capture navigation, process, HTTP, and network failures.
- Report real shell and WKWebView engine metadata.

### Phase 2.9 - Media keys
- Add macOS media-key handling and route commands through the existing media transport.

**Exit:** Native window behavior, launch intents, sidecar lifecycle, diagnostics, and media controls work without state-only placeholders.

## Milestone 3 - Reproducible packaging

### Phase 3.1 - Portable packaging tests
- Make the macOS packaging command test use platform-independent paths so the crate test suite passes on Windows.

### Phase 3.2 - Dependency setup
- Add reproducible macOS dependency setup.
- Verify SHA-256 checksums for downloaded service and media-runtime archives.

### Phase 3.3 - Bundle metadata and icon
- Generate `Info.plist` bundle versions from the checked-in release version.
- Add and package `AppIcon.icns`.

### Phase 3.4 - Architecture validation
- Verify that the app, Stremio runtime, FFmpeg, FFprobe, and libmpv match the requested architecture.

### Phase 3.5 - Dylib resolution
- Discover and bundle required non-system dylib dependencies.
- Fail packaging when a dependency cannot be resolved.

### Phase 3.6 - Dylib rewrite verification
- Verify bundled dylib references after `install_name_tool` rewrites.

**Exit:** A clean checkout can produce a complete, versioned app bundle with validated dependencies for Apple Silicon and Intel.

## Milestone 4 - Signed release and updates

### Phase 4.1 - macOS release job
- Add a macOS CI build and packaging job.
- Upload an unsigned artifact for pipeline validation.

### Phase 4.2 - Developer ID signing
- Import Developer ID credentials from release secrets.
- Sign the application and bundled native components with the hardened runtime.

### Phase 4.3 - Notarized distribution
- Submit the signed application for notarization and staple the result.
- Produce and publish the final archive or disk image.

### Phase 4.4 - Release smoke test
- Launch the packaged application in release CI and fail when startup does not reach the native shell.

### Phase 4.5 - Update artifact metadata
- Extend the existing update manifest with the macOS artifact URL, version, architecture, and integrity metadata.

### Phase 4.6 - Update download
- Download the matching macOS update artifact and verify its integrity and signature.

### Phase 4.7 - Update installation
- Install the verified update, relaunch the application, and report failures without replacing a working installation.

**Exit:** Release CI publishes a signed, notarized, launch-tested macOS artifact that the updater can install.

## Milestone 5 - Release validation

### Phase 5.1 - Native shell integration tests
- Cover AppKit launch and clean shutdown.
- Cover WKWebView injection and an IPC round trip.

### Phase 5.2 - Runtime integration tests
- Cover MPV attachment and playback startup.
- Cover launch-intent delivery and sidecar lifecycle.

### Phase 5.3 - Apple Silicon validation
- Validate playback, server lifecycle, Discord Rich Presence, and updates on Apple Silicon hardware.

### Phase 5.4 - Intel validation
- Validate playback, server lifecycle, Discord Rich Presence, and updates on Intel hardware.

### Phase 5.5 - Release gate
- Keep macOS marked as in development until all functional and hardware validation phases pass.
- Record completed validation and update platform support documentation only when the gate passes.

**Exit:** Automated integration checks and real-hardware validation pass on both supported architectures.
