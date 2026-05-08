# Developer Guide

Stremio Lightning is now organized around Rust-native shell crates. The active runtime paths are Linux and Windows shell crates, with shared behavior in `stremio-lightning-core` and injected web assets under `web/` and `src/dist/`.

The project still uses Node for the Svelte/Vite UI bundle and Vitest, but Rust `xtask` owns native setup, packaging, and release-oriented orchestration.

## Repository Layout

| Path | Purpose |
| --- | --- |
| `crates/stremio-lightning-core` | Shared host API, mod/settings validation, and common shell contracts. |
| `crates/stremio-lightning-linux` | Linux GTK/WebKit shell and AppImage runtime. |
| `crates/stremio-lightning-windows` | Windows WebView2/MPV shell and portable runtime. |
| `crates/xtask` | Rust project orchestration commands. |
| `web/bridge/bridge.js` | Shared injected bridge used by native shell adapters. |
| `src/` | Svelte/TypeScript mod UI source and tests. |
| `src/dist/mod-ui-svelte.iife.js` | Built injected mod UI bundle. |
| `scripts/` | Low-level dependency download scripts and compatibility wrappers. |
| `assets/` | Shared project assets used by packaging. |

## Tooling Boundary

Use Cargo and `xtask` for project-level workflows:

```bash
cargo xtask help
```

Use npm only for frontend dependency installation and direct frontend watch mode:

```bash
npm install
npm run dev:ui
```

The npm scripts intentionally stay small:

```bash
npm run build:ui
npm run test:ui
npm run dev:ui
```

## First-Time Setup

Install Rust stable and Node.js LTS, then install frontend dependencies:

```bash
npm install
```

Download native shell runtime dependencies for the current platform:

```bash
cargo xtask setup
```

Platform-specific setup commands are also available:

```bash
cargo xtask setup-linux
cargo xtask setup-windows
```

Linux setup downloads runtime files under `crates/stremio-lightning-linux/`. Windows setup downloads runtime files under `crates/stremio-lightning-windows/`.

## UI Development

Run the UI tests through xtask:

```bash
cargo xtask test-ui
```

Build the injected UI bundle through xtask:

```bash
cargo xtask build-ui
```

Use watch mode while editing UI source:

```bash
npm run dev:ui
```

The generated bundle is:

```text
src/dist/mod-ui-svelte.iife.js
```

## Linux Development

Build and run the Linux shell directly:

```bash
cargo run -p stremio-lightning-linux
```

Build the AppImage:

```bash
cargo xtask build-linux-appimage
```

Run the generated AppImage with DevTools enabled:

```bash
./dist/Stremio_Lightning_Linux-x86_64.AppImage --devtools
```

If packaging fails because `appimagetool` is missing, place it at the default path or set `APPIMAGE_TOOL`:

```bash
mkdir -p "$HOME/.cache/appimage"
curl -L "https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage" -o "$HOME/.cache/appimage/appimagetool-x86_64.AppImage"
chmod +x "$HOME/.cache/appimage/appimagetool-x86_64.AppImage"
```

```bash
APPIMAGE_TOOL=/path/to/appimagetool cargo xtask build-linux-appimage
```

## Windows Development

Download Windows runtime dependencies:

```bash
cargo xtask setup-windows
```

Build the Windows shell:

```bash
cargo build -p stremio-lightning-windows --release --target x86_64-pc-windows-msvc
```

Build the portable Windows artifact:

```bash
cargo xtask package-windows
```

The Windows portable output is expected at:

```text
dist/stremio-lightning-windows-portable.zip
```

Cross-building from Linux can compile the Windows crate with `cargo-xwin`, but full runtime validation still needs Windows with WebView2 and MPV DLL loading available.

## Validation Checklist

Before pushing native shell changes, run the relevant subset:

```bash
cargo test -p stremio-lightning-core
cargo test -p stremio-lightning-linux
cargo test -p stremio-lightning-windows
cargo test -p xtask
cargo xtask test-ui
cargo xtask build-ui
```

For Linux packaging changes, also run:

```bash
cargo xtask build-linux-appimage
./dist/Stremio_Lightning_Linux-x86_64.AppImage --appimage-help
```

## Runtime Architecture Notes

Native shell crates inject platform adapters before `web/bridge/bridge.js`. The bridge expects `window.StremioLightningHost` to be provided by the shell adapter.

The app loads Stremio Web through the local streaming-server proxy on Linux by default:

```text
http://127.0.0.1:11470/proxy/d=https%3A%2F%2Fweb.stremio.com/
```

Windows uses the direct WebView2 shell and packages runtime resources beside the executable for the portable layout.
