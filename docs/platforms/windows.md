# Windows Development

This guide covers Windows-specific setup and local shell development. See the
[packaging guide](../packaging.md#windows) for portable and installer artifacts.

## Prerequisites

Install the common prerequisites from the
[developer guide](../developer-guide.md#prerequisites), plus:

- WebView2 Runtime
- The MSVC Rust target and toolchain
- Visual Studio Build Tools with the Visual C++ workload

## Setup

Download the Windows runtime dependencies:

```bash
cargo xtask setup-windows
```

The command populates `crates/stremio-lightning-windows/`.

## Streaming Engine

Windows launches only the open-source `stream-server` engine. The legacy
bundled `server.js` + Node runtime are no longer shipped or supported; there is
no backend selection or fallback.

- `STREMIO_LIGHTNING_STREAM_SERVER_BIN` overrides the `stream-server` binary
  path.
- There is no automatic fallback: if `stream-server.exe` is missing, the app
  starts without a streaming server.
- The shell prepends its resources directory to the engine's `PATH` so the
  bundled `ffmpeg`/`ffprobe` are found (`stream-server` reads them from `PATH`,
  not `FFMPEG_BIN`).
- `cargo xtask setup-windows` downloads a pinned, checksum-verified
  `stream-server.exe`.

See `scripts/parity/` for the endpoint parity harness. Because Windows setup no
longer installs the legacy engine, a two-engine comparison needs `--legacy-root`
pointing at a folder that still contains the legacy files; use
`--only stream-server` to snapshot the current engine alone.

## Run Locally

```powershell
cargo windows
```

The Cargo alias already forwards trailing arguments to the shell. For example,
enable WebView2 DevTools with:

```powershell
cargo windows --devtools
```

## Compile Check

Build the shell directly with the MSVC target when you only need to confirm that
the crate compiles:

```powershell
cargo build -p stremio-lightning-windows --release --target x86_64-pc-windows-msvc
```

The raw executable is written to:

```text
target/x86_64-pc-windows-msvc/release/stremio-lightning-windows.exe
```

This executable is not a distributable package. Runtime resources must be
assembled beside it; use the portable packaging command for a runnable
distribution.

Windows uses the direct WebView2 shell. Packaged runtime resources, including
the MPV DLLs, live beside the executable in the portable layout.
