# macOS Development

This guide covers macOS-specific local development. See the
[packaging guide](../packaging.md#macos) for `.app` bundle requirements.

## Prerequisites

Install the common prerequisites from the
[developer guide](../developer-guide.md#prerequisites). Native development and
packaging must run on macOS with the Apple command-line tools available.

The project does not currently provide a macOS dependency setup xtask. Before
packaging, provide these executable runtime files:

```text
crates/stremio-lightning/resources/stream-server
crates/stremio-lightning/resources/ffmpeg
crates/stremio-lightning/resources/ffprobe
```

Packaging also requires `libmpv.dylib`. The packaging command searches:

- `MPV_DIR`
- `STREMIO_LIGHTNING_MPV_DIR`
- `crates/stremio-lightning/mpv-dev`
- Homebrew's `mpv` prefixes

## Run Locally

```bash
cargo run -p stremio-lightning
```

This runs the native client from Cargo's target directory without producing an app bundle.
