# Linux Development

This guide covers Linux-specific setup and local development. See the
[packaging guide](../packaging.md#linux) for AppImage, Debian, and Flatpak
artifacts.

## Prerequisites

Install the common prerequisites from the
[developer guide](../developer-guide.md#prerequisites), plus:

- `libmpv` development/runtime libraries and OpenGL / X11 / Wayland libraries required by Slint
- `clang` and [`mold`](https://github.com/rui314/mold)
- `bash`, `dpkg-deb`, `curl`, and `tar` for dependency setup and packaging

Cargo uses `clang` and `mold` for `x86_64-unknown-linux-gnu` builds through
`.cargo/config.toml`:

```toml
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]
```

Check that both linker tools are available:

```bash
command -v clang
command -v mold
```

Install missing tools with your distribution's package manager:

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

If `mold` is unavailable, linking fails with an error mentioning `-fuse-ld=mold` or a missing
`mold` executable.

## Setup

Download the Linux runtime dependencies:

```bash
cargo xtask setup-linux
```

The command populates `crates/stremio-lightning/resources/` with the headless `stream-server`
binary and static `ffmpeg` and `ffprobe`, pinned by SHA-256 in
`scripts/download-linux-shell-deps.sh`.

## Run Locally

```bash
cargo run -p stremio-lightning
```

This runs the native client from Cargo's target directory without producing a distributable
artifact.
