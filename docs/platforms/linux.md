# Linux Development

This guide covers Linux-specific setup and local shell development. See the
[packaging guide](../packaging.md#linux) for AppImage, Debian, and Flatpak
artifacts.

## Prerequisites

Install the common prerequisites from the
[developer guide](../developer-guide.md#prerequisites), plus:

- WebKitGTK development and runtime packages
- GTK4 development packages
- `clang` and [`mold`](https://github.com/rui314/mold)
- `bash`, GitHub CLI (`gh`), `dpkg-deb`, `curl`, and `tar` for dependency setup

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

If `mold` is unavailable, linking fails with an error mentioning
`-fuse-ld=mold` or a missing `mold` executable.

## Setup

Download the Linux shell runtime dependencies:

```bash
cargo xtask setup-linux
```

The command populates `crates/stremio-lightning-linux/`.

## Run Locally

```bash
cargo linux
```

This runs the shell from Cargo's target directory without producing a
distributable artifact.

## Runtime Notes

Linux loads Stremio Web through the local streaming-server proxy by default:

```text
http://127.0.0.1:11470/proxy/d=https%3A%2F%2Fweb.stremio.com/
```

The Flatpak currently exports an X11-only sandbox because Picture-in-Picture
always-on-top uses X11 `_NET_WM_STATE_ABOVE`. KDE Wayland users can run it
through Xwayland or create a KDE Window Rule for forced always-on-top behavior.
