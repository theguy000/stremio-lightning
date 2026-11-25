<h1 align="center">
  <img src="assets/icons/128x128.png" alt="Stremio Lightning logo" width="40" height="40" align="absmiddle">&nbsp;
  Stremio Lightning
</h1>

<p align="center">
  <img src="https://img.shields.io/badge/Built_with-Rust-000000?logo=rust&logoColor=white" alt="Built with Rust">
  <img src="https://img.shields.io/badge/UI-Slint-2379F4" alt="UI Slint">
</p>

<p align="center">
  Stremio Lightning is a fast, low-memory native desktop client for <a href="https://www.stremio.com/">Stremio</a> built with Rust, Slint, <code>stremio-core</code>, and <code>libmpv</code>.
</p>


## Key Features

- **Native Slint UI** – Fast, virtualized catalog browsing with low idle and scrolling memory usage.
- **Native Player** – Direct OpenGL <code>libmpv</code> playback with lazy initialization, subtitles, audio tracks, Next Video popup, and AniSkip support.
- **Mods & Themes** – Built-in plugins (such as AniSkip), <code>.theme.css</code> theming, plugin settings, live application logs, and diagnostics export.
- **Streaming Server Control** – Manages the local headless <code>stream-server</code> automatically.
- **Cross-Platform Packaging** – Portable and installer builds on Windows; AppImage, Debian <code>.deb</code>, and Flatpak on Linux; macOS bundle support.


## Tech Stack

- **UI:** Slint (native desktop renderer)
- **Core State:** <code>stremio-core</code> with a Rust <code>AppEnv</code>
- **Media Player:** <code>libmpv</code> (loaded lazily on playback)
- **Streaming Engine:** Headless <code>stream-server</code> + bundled <code>ffmpeg</code> / <code>ffprobe</code>


## Getting Started

### Installation

- **Windows (Installer/Portable)**
  Download and run the installer or portable archive from the [latest release](https://github.com/theguy000/stremio-lightning/releases/latest).

- **Linux**
  One-click install with Flatpak:
  ```bash
  curl -fsSL https://raw.githubusercontent.com/theguy000/stremio-lightning/master/scripts/install-linux-flatpak.sh | bash
  ```
  *(Debian `.deb` and AppImage packages are also available in the [latest release](https://github.com/theguy000/stremio-lightning/releases/latest).)*


## Development

### Quick Start

```bash
# 1. Download native runtime dependencies (libmpv, stream-server, ffmpeg)
cargo xtask setup

# 2. Run the native client
cargo run -p stremio-lightning
```

For build workflows, validation, and packaging, see the [developer guide](docs/developer-guide.md). Detailed documentation is also available for [packaging](docs/packaging.md), [mods and plugins](docs/plugin-api.md), and [runtime architecture](docs/runtime-architecture.md).


## Platform Support

- **Windows:** Supported (Installer & Portable `.zip`)
- **Linux:** Supported (Flatpak, AppImage, & Debian `.deb`)
- **macOS:** *In Development* (No release available yet)


## License

Licensed under the [GNU General Public License v3.0](LICENSE) (GPL-3.0-only).

Releases up to and including v0.1.6 were published under the MIT License and stay available under it.
