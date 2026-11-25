# Windows Development

This guide covers Windows-specific setup and local development. See the
[packaging guide](../packaging.md#windows) for portable and installer artifacts.

## Prerequisites

Install the common prerequisites from the
[developer guide](../developer-guide.md#prerequisites), plus:

- The MSVC Rust target and toolchain (`x86_64-pc-windows-msvc`)
- Visual Studio Build Tools with the Visual C++ workload
- `bash` (such as Git Bash) and `7z` for running `cargo xtask setup-windows`

## Setup

Download the Windows runtime dependencies:

```bash
cargo xtask setup-windows
```

The command populates `crates/stremio-lightning/` with `libmpv` (`mpv-dev`), `stream-server.exe`,
`ffmpeg.exe`, and `ffprobe.exe`.

## Streaming Engine

Windows launches the open-source headless `stream-server` engine.

- `STREMIO_LIGHTNING_STREAM_SERVER_BIN` overrides the `stream-server` binary path.
- If `stream-server.exe` is missing, the app starts without a streaming server.
- The app prepends its resources directory to the engine's `PATH` so the bundled `ffmpeg` and
  `ffprobe` binaries are found.
- `cargo xtask setup-windows` downloads a pinned, checksum-verified `stream-server.exe`.

## Run Locally

```powershell
cargo run -p stremio-lightning
```

## Release Compile Check

Build the native executable directly with the MSVC target:

```powershell
cargo build -p stremio-lightning --release --target x86_64-pc-windows-msvc
```

The executable is written to:

```text
target/x86_64-pc-windows-msvc/release/stremio-lightning.exe
```

This executable is not a standalone distributable package. Runtime resources (including the
`libmpv` DLLs and streaming server binaries) must sit beside it; use
`cargo xtask package-windows-portable` or `cargo xtask package-windows-installer` for a
distributable build.
