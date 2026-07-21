# Plugin API

Plugins access native features through the global
`window.StremioEnhancedAPI` object.

## Capabilities

The API provides:

- Window management: minimize, maximize, close, and drag
- Streaming-server controls: start, stop, restart, and status
- Mod management: list, download, delete, and update plugins and themes
- Settings: get, save, and register plugin-specific settings
- Events: subscribe to fullscreen, maximize, and server-state changes
- Logging: `debug`, `info`, `warn`, and `error`

## Logging

Plugin log calls are tagged as `plugin.<name>`. They appear in the Mods panel's
Logs tab and in the browser developer console.

Logs are retained in bounded session memory and are not persisted across
application restarts.

## Host Bridge

Platform shells expose native operations through
`window.StremioLightningHost`. The shared bridge at `web/bridge/bridge.js`
adapts that host contract into `window.StremioEnhancedAPI` for plugins and the
injected UI.

See [runtime architecture](runtime-architecture.md) for the injection flow and
repository boundaries.
