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
