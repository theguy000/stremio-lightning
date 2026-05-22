<h1 align="center">
  <img src="assets/icons/128x128.png" alt="Stremio Lightning logo" width="40" height="40" align="absmiddle">&nbsp;
  Stremio Lightning
</h1>

<p align="center">
  <img src="https://img.shields.io/badge/Built_with-Rust-000000?logo=rust&logoColor=white" alt="Built with Rust">
  <img src="https://img.shields.io/badge/Frontend-Svelte-FF3E00?logo=svelte&logoColor=white" alt="Frontend Svelte">
</p>

<p align="center">
  Stremio Lightning is a fast, lightweight, and modern desktop wrapper for <a href="https://www.stremio.com/">Stremio</a> built with Rust-native shell crates and Svelte.
</p>


## Key Features

- **Plugin System** – Install and manage community plugins.
- **Theme Support** – Customize the appearance with downloadable themes.
- **Discord Rich Presence** – Show what you are watching directly on Discord.
- **Native Player** – Enjoy native MPV-powered media playback.
- **Server Control** – Start, stop, and restart the local streaming server.
- **Auto-Updates** – Built-in notifications for app updates.


## Tech Stack

- **Frontend:** Svelte 5, TypeScript, Vite
- **Backend:** Rust native shell crates
- **Media Player:** libmpv2 (Windows/Linux)
- **Native WebViews:** WebView2 (Windows), GTK4 & WebKitGTK 6 (Linux)


## Getting Started

### Installation

- **Windows (Installer/Portable)**
  Download and run the installer or portable version from the [latest release](https://github.com/theguy000/stremio-lightning/releases/latest).

- **Linux**
  One-click install with Flatpak:
  ```bash
  curl -fsSL https://raw.githubusercontent.com/theguy000/stremio-lightning/master/scripts/install-linux-flatpak.sh | bash
  ```
  *(Debian `.deb` and AppImage packages are also available in the [latest release](https://github.com/theguy000/stremio-lightning/releases/latest).)*


## Development

Developer guide, build instructions, and plugin API documentation are located in [`docs/developer-guide.md`](docs/developer-guide.md).


## Platform Support

- **Windows:** Supported (Installer & Portable `.zip`)
- **Linux:** Supported (Flatpak, AppImage, & Debian `.deb`)
- **macOS:** *In Development* (No release available yet)


## License

Licensed under the [MIT License](LICENSE).
