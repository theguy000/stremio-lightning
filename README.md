<h1 align="center">
  <img src="assets/icons/128x128.png" alt="Stremio Lightning logo" width="32" height="32" align="absmiddle">&nbsp;&nbsp;
  Stremio Lightning
</h1>

![Built with Rust](https://img.shields.io/badge/Built_with-Rust-000000?logo=rust&logoColor=white)
![Frontend Svelte](https://img.shields.io/badge/Frontend-Svelte-FF3E00?logo=svelte&logoColor=white)

Stremio Lightning is a desktop wrapper for [Stremio](https://www.stremio.com/) built with Rust-native shell crates and [Svelte](https://svelte.dev/). It adds plugin management, theme support, Discord Rich Presence, MPV-powered native playback for a better viewing experience, and tighter control over the local streaming server.

---

## Features

| Feature | Description |
|---------|-------------|
| Plugin system | Install and manage community-made plugins through a built-in marketplace. |
| Theme support | Customize Stremio with downloadable themes. |
| Discord Rich Presence | Show what you are watching directly on Discord. |
| Native player | Use an integrated MPV-based media player on Windows. |
| Streaming server control | Start, stop, and restart the Stremio streaming server from inside the app. |
| Auto-update checking | Receive built-in notifications when app updates are available. |

---

## Tech Stack

| Layer     | Technology                        |
|-----------|-----------------------------------|
| Frontend  | Svelte 5, TypeScript, Vite        |
| Backend   | Rust native shell crates          |
| Player    | libmpv2 (Windows/Linux)           |
| Windows   | WebView2                          |
| Linux     | GTK4, WebKitGTK 6, GTK GLArea     |
| Packaging | Native crate packaging scripts    |

---

## Getting Started

### Prerequisites

- [Node.js](https://nodejs.org/) (LTS recommended)
- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain)
- Linux: WebKitGTK 4.1 development/runtime packages for the native shell
- Windows: WebView2 Runtime and MSVC Rust target/toolchain

### Installation

1. **Clone the repository**
   ```
   git clone https://github.com/theguy000/stremio-lightning.git
   cd stremio-lightning
   ```

2. **Install dependencies**
   ```
   npm install
   ```

3. **Download native shell dependencies**
   ```
   cargo xtask setup
   ```

### Development

Use `npm run dev:ui` for the injected UI bundle and run the native shell crate with Cargo.

Developer workflow details are documented in [`docs/developer-guide.md`](docs/developer-guide.md).

### Build

```bash
cargo xtask build-ui
cargo xtask build-linux-appimage
cargo xtask package-windows
```

---

## Plugin API

Plugins have access to the global `window.StremioEnhancedAPI` object, which exposes:

- **Window management** - minimize, maximize, close, drag
- **Streaming server** - start, stop, restart, get status
- **Mod management** - list, download, delete, and update plugins & themes
- **Settings** - get, save, and register plugin-specific settings
- **Events** - subscribe to fullscreen, maximize, and server state changes
- **Logging** - `info`, `warn`, `error` helpers

---

## Platform Support

| Platform | Notes                                    |
|----------|------------------------------------------|
| Windows  | Requires `libmpv-2.dll`, ffmpeg, ffprobe |
| macOS    | Minimum macOS 10.15 (Catalina)           |
| Linux    | Requires `libwebkit2gtk-4.1`             |

---

## License

This project is licensed under the [MIT License](LICENSE).
