# Runtime Architecture

Stremio Lightning is a native desktop client: a Rust shell drawn with Slint, holding
Stremio's state with `stremio-core`. The older web shells (a Svelte Mods UI injected into a
Svelte/WebKit or WebView2 window) are being retired and stay in the tree untouched.

## Repository Layout

| Path | Purpose |
| --- | --- |
| `crates/stremio-lightning` | The native client: the app, its screens, and its UI written in Slint. |
| `crates/stremio-lightning-env` | The environment `stremio-core` stores and fetches through. |
| `crates/stremio-lightning-core` | Behavior still shared with the web shells: the streaming-server supervisor, the player contract, startup flags, mods and themes. |
| `crates/xtask` | Development, validation, setup, and packaging orchestration. |
| `crates/stremio-lightning-{windows,linux,macos}` | The retiring web shells. |
| `web/`, `src/` | The retiring Mods UI and the bridge the web shells inject. |

## The native client

One crate, built as a library with a thin launcher: `main.rs` starts the app and does nothing
else. That is what lets the tests call the app's own code instead of a copy of it.

Three kinds of code, kept apart. A file belongs to exactly one of them.

**Screen data** (`screens/`) is one module per screen, holding plain structs and the functions
that build them from the model. No window, no Slint types, no async. This is the app's test
doorway: a test hands a screen a model and asks what it would show.

**Wiring** (`shell/`) is the window's own state, the mapping from a snapshot to window
properties, and one function per callback, each naming the action it performs. It decides
nothing.

**Plumbing** (`plumbing/`) is the outside world: mpv, the streaming server, image downloads,
browser links, and theme files. Each hides its details behind a couple of functions and reports
plain values, never Slint types and never window properties.

`app/` sits between them. It holds the state (`AppModel`, which is stremio-core's models), builds
it from the stored profile, turns core's events into a `Snapshot` of what the window shows, and
hands that snapshot to the shell.

### Adding a screen

Add a module under `screens/` with a `data(model) -> Data` function, add a field for it to
`Snapshot`, fill it in `app::events`, and teach `shell/view` which properties it fills. A screen
that needs callbacks gets them in `shell/wiring`, one line each.

Slint components live in `ui/`; a screen's Rust module and its Slint file share a name.

### The player

`ui/player.slint` is the one screen that is not only Slint: mpv draws its frames into the
window's own OpenGL framebuffer before Slint paints, so the player's controls are transparent
layers over the picture. That is also why `MainWindow`'s background turns transparent while the
player is open — anything opaque there would cover the video.

`plumbing/player.rs` owns mpv: it loads an address, polls what mpv is doing twice a second, and
reports a plain `Status` to whoever listens. `screens/player.rs` builds everything the screen
shows from the core's `Player` model, and `shell/wiring.rs` turns presses into core actions
(play, seek, tracks, scale, volume) and the address the core resolves into an mpv load. The core
is told the play head at most once a second, in whole seconds, which is what the web client does.

## Retiring shells

Each web shell injects its platform adapter before `web/bridge/bridge.js`, which exposes
`window.StremioEnhancedAPI` to plugins and the Mods UI. Shared host behavior lives in
`stremio-lightning-core`; see
[ADR 0001](adr/0001-shared-behavior-lives-in-core.md).

Platform implementation and setup details are documented in the
[Linux](platforms/linux.md), [macOS](platforms/macos.md), and
[Windows](platforms/windows.md) guides.

The native client's shape is recorded in
[ADR 0002](adr/0002-native-client-is-a-library.md).
