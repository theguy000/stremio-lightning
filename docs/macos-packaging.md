# macOS Packaging

Phase 8 packaging is centered on a local `.app` bundle built by `cargo xtask package-macos`.

## Local Prerequisites

- Run on macOS.
- Install Xcode command line tools so `install_name_tool` and `codesign` are available.
- Install or provide `libmpv.dylib` with Homebrew or `MPV_DIR` / `STREMIO_LIGHTNING_MPV_DIR`.
- Provide macOS sidecar files under `crates/stremio-lightning-macos`:
  - `binaries/stremio-runtime-macos`
  - `resources/server.cjs`
  - `resources/ffmpeg`
  - `resources/ffprobe`

## Bundle Layout

`cargo xtask package-macos` creates `dist/Stremio Lightning.app` with:

- `Contents/MacOS/stremio-lightning-macos`
- `Contents/Info.plist`
- `Contents/Resources/entitlements.plist`
- `Contents/Resources/binaries/stremio-runtime-macos`
- `Contents/Resources/resources/server.cjs`
- `Contents/Resources/resources/ffmpeg`
- `Contents/Resources/resources/ffprobe`
- `Contents/Frameworks/libmpv.dylib` or `libmpv.2.dylib`

The app resolves its bundled streaming-server root from `Contents/Resources` when launched from a `.app` bundle.

## Signing

The local bundle command applies ad-hoc signing with hardened runtime options and the crate-local entitlements file. Developer ID signing and notarization should be layered on top only after a local unsigned/ad-hoc bundle launches, injects, starts the sidecar, and plays video reliably.
