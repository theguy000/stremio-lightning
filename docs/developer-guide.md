# Developer Guide

This guide covers the common development, validation, and packaging workflows for
Stremio Lightning.

Stremio Lightning is organized around Rust-native shell crates. Shared shell
behavior lives in `stremio-lightning-core`, platform shells live under `crates/`,
and the injected Svelte/Vite UI bundle lives under `src/`.

## Quick Start & Prerequisites

### Prerequisites

Before developing or packaging Stremio Lightning, ensure you have the following installed:

- **Node.js** (LTS recommended)
- **Rust** (stable toolchain)
- **Linux**: WebKitGTK development/runtime packages for the native shell, plus `clang` and [`mold`](https://github.com/rui314/mold) for Rust linking.
- **Windows**: WebView2 Runtime and MSVC Rust target/toolchain.

### Setup & Installation

Install frontend dependencies:

```bash
npm install
```

On Linux `x86_64-unknown-linux-gnu`, Cargo is configured to link Rust crates
with `clang` and [`mold`](https://github.com/rui314/mold) in `.cargo/config.toml`:

```toml
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]
```

Check whether both tools are installed before building:

```bash
command -v clang
command -v mold
```

If either command prints nothing or exits with `command not found`, install the
missing package with your distro package manager. Examples:

```bash
# Debian/Ubuntu
sudo apt install clang mold

# Fedora
sudo dnf install clang mold

# Arch Linux
sudo pacman -S clang mold

# openSUSE
sudo zypper install clang mold
```

If `mold` is missing, Linux Cargo builds will fail during linking with an error
from `clang`/the linker about `-fuse-ld=mold` or `mold` not being found.

Download native runtime dependencies for the current platform:

```bash
cargo xtask setup
```

Use the platform-specific setup commands when preparing artifacts for another platform:

```bash
cargo xtask setup-linux
cargo xtask setup-windows
```

Linux setup populates `crates/stremio-lightning-linux/`. Windows setup populates
`crates/stremio-lightning-windows/`.

## Command Model

Use `cargo xtask` for project workflows:

```bash
cargo xtask help
```

Use npm only for frontend dependency installation and direct UI watch mode:

```bash
npm install
npm run dev:ui
```

The main xtask commands are:

| Command | Purpose |
| --- | --- |
| `cargo xtask validate` | Run all formatting, linting, tests, and UI checks. |
| `cargo xtask setup` | Download native dependencies for the current platform. |
| `cargo xtask setup-linux` | Download Linux shell runtime dependencies. |
| `cargo xtask setup-macos` | Download macOS shell runtime dependencies (accepts `--arch arm64` or `--arch x86_64`). |
| `cargo xtask setup-windows` | Download Windows shell runtime dependencies. |
| `cargo xtask build-ui` | Build the injected Svelte/Vite UI bundle. |
| `cargo xtask test-ui` | Run frontend tests through Vitest. |
| `cargo xtask package-linux-appimage` | Build the Linux AppImage. |
| `cargo xtask package-linux-deb` | Build the Linux `.deb` package. |
| `cargo xtask package-linux-flatpak` | Build the Linux Flatpak bundle. |
| `cargo xtask package-macos` | Build the macOS `.app` bundle (accepts `--arch arm64` or `--arch x86_64`). |
| `cargo xtask package-macos-dmg` | Build the macOS `.app` bundle and DMG. |
| `cargo xtask package-windows-portable` | Build the Windows portable zip. |
| `cargo xtask package-windows-installer` | Build the Windows installer EXE. |

Choose commands by intent:

| If you want to... | Use this |
| --- | --- |
| Run a shell locally during development | `cargo <platform>` (e.g., `cargo linux` / `cargo macos` / `cargo windows`) |
| Check that a shell crate compiles for a target | `cargo build -p <platform-crate> --release [--target ...]` |
| Run complete project format, lint, and test validation | `cargo xtask validate` |
| Produce a distributable artifact under `dist/` | `cargo xtask package-*` |

In practice, most day-to-day work uses `cargo run` for local iteration and
`cargo xtask package-*` for release artifacts. Plain `cargo build` is mainly for
compile checks and inspecting raw binaries.

## Repository Layout

| Path | Purpose |
| --- | --- |
| `crates/stremio-lightning-core` | Shared host API, mod/settings validation, and common shell contracts. |
| `crates/stremio-lightning-linux` | Linux GTK/WebKit shell and AppImage runtime. |
| `crates/stremio-lightning-macos` | macOS WKWebView shell and app bundle packaging inputs. |
| `crates/stremio-lightning-windows` | Windows WebView2/MPV shell and portable runtime. |
| `crates/xtask` | Rust project orchestration commands. |
| `web/bridge/bridge.js` | Shared injected bridge used by native shell adapters. |
| `src/` | Svelte/TypeScript mod UI source and tests. |
| `src/dist/mod-ui-svelte.iife.js` | Built injected mod UI bundle. |
| `scripts/` | Low-level dependency download scripts and compatibility wrappers. |
| `assets/` | Shared project assets used by packaging. |

## UI Workflow

Run UI tests:

```bash
cargo xtask test-ui
```

Build the injected UI bundle:

```bash
cargo xtask build-ui
```

Use watch mode while editing UI source:

```bash
npm run dev:ui
```

The generated bundle is written to:

```text
src/dist/mod-ui-svelte.iife.js
```

## Linux Workflow

Download Linux runtime dependencies:

```bash
cargo xtask setup-linux
```

Run the Linux shell directly:

```bash
cargo linux
```

Use this for local Linux development. It runs the shell from the Cargo target
directory instead of producing a distributable package.

Build Linux packages:

```bash
cargo xtask package-linux-appimage
cargo xtask package-linux-deb
cargo xtask package-linux-flatpak          # Fast-Host bundling (for local dev)
cargo xtask package-linux-flatpak-builder  # Hermetic flatpak-builder compilation (for release build)
```

Use these when you need Linux artifacts under `dist/`.

Linux outputs:

```text
dist/Stremio_Lightning_Linux-x86_64.AppImage
dist/stremio-lightning-linux-amd64.deb
dist/Stremio_Lightning_Linux-x86_64.flatpak
```

### Flatpak Workflows: Fast-Host vs. Hermetic Builder

Stremio Lightning supports two distinct Flatpak packaging methods:

1. **Fast-Host Bundling (`cargo xtask package-linux-flatpak`)**
   * **Use Case:** Fast local development (takes <10 seconds).
   * **Mechanism:** Bundles host binaries and host shared libraries into a basic Flatpak.
   * **Requirements:** `flatpak` CLI.

2. **Hermetic Builder (`cargo xtask package-linux-flatpak-builder`)**
   * **Use Case:** Release packaging & CI pipeline (what runs in GitHub Actions).
   * **Mechanism:** Compiles everything hermetically inside the GNOME 50 sandbox.
   * **Requirements:** `flatpak-builder` and GNOME 50 platform/SDK.

```bash
flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak install flathub org.gnome.Platform//50 org.gnome.Sdk//50 org.freedesktop.Sdk.Extension.rust-stable//25.08
```

It currently exports an X11-only sandbox because Linux PiP always-on-top is
implemented with X11 `_NET_WM_STATE_ABOVE`; KDE Wayland users should run the
Flatpak through Xwayland or create a KDE Window Rule if they want forced
always-on-top behavior.

Install and run a local Flatpak bundle:

```bash
flatpak install --user --bundle dist/Stremio_Lightning_Linux-x86_64.flatpak
flatpak run io.github.theguy000.StremioLightning
```

If `appimagetool` is missing, place it at the default cache path:

```bash
mkdir -p "$HOME/.cache/appimage"
curl -L "https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-x86_64.AppImage" -o "$HOME/.cache/appimage/appimagetool-x86_64.AppImage"
chmod +x "$HOME/.cache/appimage/appimagetool-x86_64.AppImage"
```

Or point `APPIMAGE_TOOL` at a custom binary:

```bash
APPIMAGE_TOOL=/path/to/appimagetool cargo xtask package-linux-appimage
```

## macOS Workflow

Run the macOS shell directly on macOS:

```bash
cargo macos
```

Use this for local macOS development.

Download the macOS runtime dependencies (Stremio service runtime, `server.cjs`,
`ffmpeg`, `ffprobe`) for a specific architecture:

```bash
cargo xtask setup-macos --arch arm64
cargo xtask setup-macos --arch x86_64
```

Omit `--arch` to target the host architecture. The setup command needs `gh`,
`curl`, `tar`, `unzip`, and `shasum`, and every download is verified against a
pinned SHA-256 checksum.

Build the macOS app bundle on macOS:

```bash
cargo xtask package-macos --arch arm64
```

Build the app bundle and a distributable DMG:

```bash
cargo xtask package-macos-dmg --arch arm64
```

Before packaging, make sure these runtime files exist (created by
`cargo xtask setup-macos`):

```text
crates/stremio-lightning-macos/binaries/stremio-runtime-macos
crates/stremio-lightning-macos/resources/server.cjs
crates/stremio-lightning-macos/resources/ffmpeg
crates/stremio-lightning-macos/resources/ffprobe
```

The package command also needs `libmpv.dylib`. It looks in `MPV_DIR`,
`STREMIO_LIGHTNING_MPV_DIR`, `crates/stremio-lightning-macos/mpv-dev`, and the
target architecture's Homebrew `mpv` prefix. libmpv and its non-system dylib
dependencies are copied into `Contents/Frameworks` and rewritten to `@rpath`
references, so the bundle is self-contained.

macOS output:

```text
dist/Stremio Lightning.app
dist/Stremio_Lightning_macOS-<arch>.dmg
```

Bundles are ad-hoc signed by default. CI signs and notarizes the app when
Developer ID secrets are configured; otherwise the published DMGs stay ad-hoc
signed and need a one-time Gatekeeper override:

```bash
xattr -cr "/Applications/Stremio Lightning.app"
```

The macOS package command must run on macOS because it uses host Apple tooling
such as `install_name_tool`, `codesign`, `otool`, `lipo`, and `hdiutil`, and it
needs platform-matching bundled libraries.

See [`macos-shell-gap-analysis.md`](macos-shell-gap-analysis.md) for the
current state of the macOS shell and remaining gaps.

## Windows Workflow

Download Windows runtime dependencies:

```bash
cargo xtask setup-windows
```

Build the Windows shell directly with the MSVC target:

```bash
cargo build -p stremio-lightning-windows --release --target x86_64-pc-windows-msvc
```

Use this when you only want to confirm that the Windows shell crate compiles, or
when you want to inspect the raw executable produced by Cargo.

This command does not create a distributable artifact. It only builds the crate output at:

```text
target/x86_64-pc-windows-msvc/release/stremio-lightning-windows.exe
```

Build the portable Windows artifact:

```bash
cargo xtask package-windows-portable
```

Use this when you want a runnable Windows distribution zip. It builds the shell
and assembles the required runtime files into the portable layout before
creating:

```text
dist/stremio-lightning-windows-portable.zip
```

Run the generated Windows executable with DevTools enabled:

```powershell
.\stremio-lightning-windows.exe --devtools
```

Build the Windows installer EXE:

```bash
cargo xtask package-windows-installer
```

Use this when you want the Windows setup installer. It builds the shell and
prepares the portable layout internally before creating the installer.

Windows outputs:

```text
dist/stremio-lightning-windows-portable.zip
dist/stremio-lightning-windows-setup.exe
```

On Windows, the portable build uses the MSVC toolchain and requires Visual Studio
Build Tools with the Visual C++ workload.

On Linux/macOS, the portable build uses `cargo-xwin` for MSVC cross-compilation:

```bash
cargo install cargo-xwin
```

The installer EXE is currently supported only on Windows. The xtask command uses
Inno Setup's `iscc` compiler from `PATH` to create the installer.

The GitHub release workflow builds this in the `windows-latest` job after
installing Inno Setup.

Cross-building from Linux can compile and package the portable Windows layout,
but runtime validation still needs Windows with WebView2 and MPV DLL loading
available.

## Validation

To run all formatting checks, compiler lints, workspace tests, and frontend checks with a single command, use:

```bash
cargo xtask validate
```

This sequentially performs format checks (`cargo fmt`), workspace-wide compiler linting (`cargo clippy`), all Rust package tests (`cargo test`), Vitest frontend verification (`cargo xtask test-ui`), and production UI bundling (`cargo xtask build-ui`).

OR, you can run individual subsets manually before pushing native shell changes:

```bash
cargo test -p stremio-lightning-core
cargo test -p stremio-lightning-linux
cargo test -p stremio-lightning-macos
cargo test -p stremio-lightning-windows
cargo test -p xtask
cargo xtask test-ui
cargo xtask build-ui
```

For Linux packaging changes, also run:

```bash
cargo xtask package-linux-appimage
./dist/Stremio_Lightning_Linux-x86_64.AppImage --appimage-help
```

For Windows packaging changes, also run the Windows packaging command relevant to
the change:

```bash
cargo xtask package-windows-portable
cargo xtask package-windows-installer
```

Only run `cargo xtask package-windows-installer` on Windows with Inno Setup installed.

## Plugin API

Plugins have access to the global `window.StremioEnhancedAPI` object, which exposes:

- **Window management** - minimize, maximize, close, drag
- **Streaming server** - start, stop, restart, get status
- **Mod management** - list, download, delete, and update plugins & themes
- **Settings** - get, save, and register plugin-specific settings
- **Events** - subscribe to fullscreen, maximize, and server state changes
- **Logging** - `info`, `warn`, `error` helpers

---

## Runtime Notes

Native shell crates inject platform adapters before `web/bridge/bridge.js`. The
bridge expects `window.StremioLightningHost` to be provided by the shell adapter.

Linux loads Stremio Web through the local streaming-server proxy by default:

```text
http://127.0.0.1:11470/proxy/d=https%3A%2F%2Fweb.stremio.com/
```

Windows uses the direct WebView2 shell and packages runtime resources beside the
executable in the portable layout.
