# ⚡ Stremio Lightning

A powerful desktop wrapper for [Stremio](https://www.stremio.com/) built with Rust-native shell crates and [Svelte](https://svelte.dev/), adding support for plugins, themes, Discord Rich Presence, and more.

---

## ✨ Features

- 🧩 **Plugin System** — Install and manage community-made plugins via a built-in marketplace
- 🎨 **Theme Support** — Customize the look of Stremio with downloadable themes
- 🎮 **Discord Rich Presence** — Show what you're watching on Discord
- 🖥️ **Native Player** — Integrated MPV-based native media player (Windows)
- 📡 **Streaming Server Control** — Start, stop, and restart the Stremio streaming server from within the app
- 🔗 **Deep Link Support** — Handle `stremio://` protocol links natively
- 🔄 **Auto-update Checking** — Built-in app update notifications

---

## 🛠️ Tech Stack

| Layer     | Technology                        |
|-----------|-----------------------------------|
| Frontend  | Svelte 5, TypeScript, Vite        |
| Backend   | Rust native shell crates          |
| Player    | libmpv2 (Windows)                 |
| Packaging | Native crate packaging scripts    |

---

## 🚀 Getting Started

### Prerequisites

- [Node.js](https://nodejs.org/) (LTS recommended)
- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain)
- Linux: WebKitGTK 4.1 development/runtime packages for the native shell
- Windows: WebView2 Runtime and MSVC Rust target/toolchain

### Installation

1. **Clone the repository**
   ```
   git clone https://github.com/your-username/stremio-lightning.git
   cd stremio-lightning
   ```

2. **Install dependencies**
   ```
   npm install
   ```

3. **Run setup script**
   ```
   npm run setup
   ```

### Development

Use `npm run dev:ui` for the injected UI bundle and run the native shell crate with Cargo.

### Build

```bash
npm run build:ui
npm run build:linux-appimage
cargo build -p stremio-lightning-windows --release
```

---

## 🧩 Plugin API

Plugins have access to the global `window.StremioEnhancedAPI` object, which exposes:

- **Window management** — minimize, maximize, close, drag
- **Streaming server** — start, stop, restart, get status
- **Mod management** — list, download, delete, and update plugins & themes
- **Settings** — get, save, and register plugin-specific settings
- **Events** — subscribe to fullscreen, maximize, and server state changes
- **Logging** — `info`, `warn`, `error` helpers

---

## 📦 Platform Support

| Platform | Notes                                    |
|----------|------------------------------------------|
| Windows  | Requires `libmpv-2.dll`, ffmpeg, ffprobe |
| macOS    | Minimum macOS 10.15 (Catalina)           |
| Linux    | Requires `libwebkit2gtk-4.1`             |

---

## 📄 License

This project is licensed under the [MIT License](LICENSE).
