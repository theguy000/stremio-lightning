# Runtime Architecture

Stremio Lightning uses Rust-native platform shells around Stremio Web. Shared
host behavior lives in `stremio-lightning-core`, each operating system has a
shell crate, and a Svelte/Vite bundle provides the injected Mods UI.

## Repository Layout

| Path | Purpose |
| --- | --- |
| `crates/stremio-lightning-core` | Shared host API, mod and settings validation, and common shell contracts. |
| `crates/stremio-lightning-linux` | Linux GTK/WebKit shell and AppImage runtime. |
| `crates/stremio-lightning-macos` | macOS WKWebView shell and app-bundle inputs. |
| `crates/stremio-lightning-windows` | Windows WebView2/MPV shell and portable runtime. |
| `crates/xtask` | Development, validation, setup, and packaging orchestration. |
| `web/bridge/bridge.js` | Shared bridge injected by each native shell. |
| `src/` | Svelte/TypeScript Mods UI source and tests. |
| `src/dist/mod-ui-svelte.iife.js` | Built Mods UI bundle injected into the WebView. |
| `scripts/` | Low-level dependency download scripts and compatibility wrappers. |
| `assets/` | Shared packaging assets. |

## Injection Flow

Each native shell injects its platform adapter before `web/bridge/bridge.js`.
The adapter must provide `window.StremioLightningHost`. The shared bridge then
exposes the higher-level `window.StremioEnhancedAPI` consumed by plugins and the
Mods UI.

The production UI build writes a single IIFE bundle to:

```text
src/dist/mod-ui-svelte.iife.js
```

See the [plugin API guide](plugin-api.md) for exposed capability groups.

## Platform Runtimes

- Linux uses GTK/WebKit and loads Stremio Web through the local
  streaming-server proxy by default.
- macOS uses WKWebView and packages an app-specific runtime in the `.app`
  bundle.
- Windows uses WebView2 and assembles its runtime resources beside the shell
  executable.

Platform implementation and setup details are documented in the
[Linux](platforms/linux.md), [macOS](platforms/macos.md), and
[Windows](platforms/windows.md) guides.
