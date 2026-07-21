# macOS Development

This guide covers macOS-specific local development. See the
[packaging guide](../packaging.md#macos) for `.app` bundle requirements.

## Prerequisites

Install the common prerequisites from the
[developer guide](../developer-guide.md#prerequisites). Native development and
packaging must run on macOS with the Apple command-line tools available.

The project does not currently provide a macOS dependency setup xtask. Before
packaging, provide these runtime files:

```text
crates/stremio-lightning-macos/binaries/stremio-runtime-macos
crates/stremio-lightning-macos/resources/server.cjs
crates/stremio-lightning-macos/resources/ffmpeg
crates/stremio-lightning-macos/resources/ffprobe
```

Packaging also requires `libmpv.dylib`. The packaging command searches:

- `MPV_DIR`
- `STREMIO_LIGHTNING_MPV_DIR`
- `crates/stremio-lightning-macos/mpv-dev`
- Homebrew's `mpv` prefixes

## Run Locally

```bash
cargo macos
```

This runs the shell from Cargo's target directory without producing an app
bundle.
