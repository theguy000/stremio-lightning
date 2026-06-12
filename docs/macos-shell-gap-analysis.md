# macOS Shell Gap Analysis

Status snapshot for the macOS shell after the initial shipping work
(ARM + Intel DMGs built and published from CI).

## Current State

- The macOS shell crate (`stremio-lightning-macos`) builds and packages into
  `dist/Stremio Lightning.app` via `cargo xtask package-macos`.
- `cargo xtask setup-macos --arch <arch>` downloads all runtime dependencies
  with pinned SHA-256 verification:
  - **Both arches:** `server.cjs` from Stremio `stremio-service` `v0.1.21`
    (`stremio-service-macos.zip`).
  - **x86_64:** `stremio-runtime`, `ffmpeg`, and `ffprobe` come directly from
    the same `stremio-service-macos.zip` (Intel binaries).
  - **arm64:** the runtime is Node.js `v20.18.1` (darwin-arm64), and
    `ffmpeg`/`ffprobe` come from jellyfin-ffmpeg `v7.1.4-3`
    (`portable_macarm64-gpl`).
- `cargo xtask package-macos-dmg --arch <arch>` produces
  `dist/Stremio_Lightning_macOS-<arch>.dmg` with an `/Applications` symlink.
- libmpv and its non-system dylib dependencies are bundled recursively into
  `Contents/Frameworks` with `@rpath` rewrites, so the app runs without a
  Homebrew mpv install.
- CI (`.github/workflows/publish.yml`) builds both architectures on
  `macos-latest` (arm64) and `macos-15-intel` (x86_64) and attaches both DMGs
  to draft releases.
- Mach-O architecture of the app executable, runtime, ffmpeg, ffprobe, and all
  bundled dylibs is verified with `lipo` during packaging.

## Signing and Notarization

- Bundles are **ad-hoc signed by default** (`codesign --sign -`).
- CI contains a gated signing/notarization step that activates only when the
  `MACOS_CERT_P12`, `MACOS_CERT_PASSWORD`, `APPLE_ID`, `APPLE_TEAM_ID`, and
  `NOTARIZATION_PWD` secrets are configured. Without them the step is a no-op
  and the ad-hoc signed DMG ships as-is.
- Until notarization is configured, users must clear the quarantine flag once:

  ```bash
  xattr -cr "/Applications/Stremio Lightning.app"
  ```

## Known Gaps

- **No universal binary.** Two separate DMGs are published per release
  (`arm64` and `x86_64`); there is no `lipo`-merged universal app.
- **No Developer ID signing by default.** Requires the secrets above plus an
  Apple Developer account.
- **Real-hardware smoke tests pending.** CI verifies bundle structure and
  Mach-O architectures, but playback, server lifecycle, Discord Rich Presence,
  and auto-update flows still need manual verification on physical Apple
  Silicon and Intel Macs.
- **arm64 runtime is plain Node.js**, not the Stremio service runtime (no
  arm64 build of `stremio-service` is published upstream). Behavioral parity
  relies on `server.cjs` being runtime-agnostic.
- **No auto-update channel for macOS yet.** Updates are manual DMG downloads.
