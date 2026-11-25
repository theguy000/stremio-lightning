# Runtime Architecture

Stremio Lightning is a native desktop Stremio client built with Rust, Slint, `stremio-core`,
and `libmpv`.

## Workspace Layout

| Path | Purpose |
| --- | --- |
| `crates/stremio-lightning` | The native client library, launcher binary (`main.rs`), and Slint UI (`ui/`). |
| `crates/xtask` | Dependency setup, validation (`fmt`, `clippy`, `test`), version bumping, and platform packaging. |

## Layered Module Architecture

`crates/stremio-lightning` is a library with a minimal launcher (`src/main.rs`). Production code is
divided into four strict layers plus the Slint UI, with every production `.rs` and `.slint` file
kept at or below 300 lines (single justified exception: `src/screens/language_names.rs`, a static
ISO-639 lookup table):

- **`plumbing/`** — The outside world: `stremio-core` `AppEnv` (`env.rs`), image cache and decode
  pipeline (`images/`), `libmpv` session and OpenGL loader (`player/`), streaming-server supervisor
  (`stream_server.rs`, `server_child.rs`), browser OAuth polling (`oauth/`), AniSkip HTTP client
  (`skip/`), `.theme.css` parser/store (`theme/`), logs (`logs.rs`, `rotating_log.rs`), clipboard,
  browser links, local settings, and update checks. Emits plain Rust values; never imports Slint,
  `MainWindow`, `screens::*`, `app::*`, or `shell::*`.
- **`screens/`** — Pure in-process screen modules (`board`, `discover`, `library`, `details/`,
  `player/`, `addons/`, `calendar`, `settings/`, `mods/`, `auth`, `preview`, `videos`). Transforms
  `AppModel` and plain state into screen data structs and core actions. Zero Slint, zero
  `MainWindow`, zero `async`, and zero disk/HTTP/browser I/O. This is the primary test doorway.
- **`app/`** — Slint-free application core (`Arc<App>`). Owns `AppStore`, `AppModel`, `AppRuntime`,
  `Images`, `Logs`, `LocalExtras`, and the `stremio-core` event loop (`events.rs`, `project.rs`).
  Exposes typed per-screen handles (`handles.rs`, `mods_handle.rs`), fine-grained `ScreenMask`
  invalidation (`mask.rs`), structurally shared `Arc<Data>` slices in `Snapshot`, lazy
  inactive-screen projection, and `AppNotice` push notifications. Never imports Slint or `shell::*`.
- **`shell/`** — The sole Slint and `winit` adapter, entered through `shell::attach(window, app)`.
  Decides nothing:
  - `shell/mod.rs` subscribes to `AppNotice` with `SeqCst` repaint coalescing (zero polling timers).
  - `shell/winit_host.rs` handles 1-frame wheel scrolling (guarded by `active_popups()`), one-shot
    window icon installation, and `WindowEvent::Focused(true)` account sync.
  - `shell/video_host.rs` bridges Slint's OpenGL rendering callbacks and `plumbing::player::MpvSession`
    lazily on playback.
  - `shell/view/` projects `Snapshot` onto per-screen Slint globals using `ModelMirror<K, Dst>`
    (`view/sync.rs`) to update Slint `VecModel`s in place without dropping focus or scroll state.
  - `shell/wiring/` binds each `<Screen>Ui` global's callbacks to `App` handles and screen actions.
- **`ui/`** — Slint interface modules. `ui/main.slint` (`MainWindow`) owns only 19 frame-level
  members (section navigation, `details-open`, `playing`, toast/share overlays, wheel scroll
  offsets, and frame callbacks). Each screen exports its own Slint global (`BoardUi`, `DiscoverUi`,
  `LibraryUi`, `DetailsUi`, `PlayerUi`, `AddonsUi`, `CalendarUi`, `SettingsUi`, `ModsUi`,
  `AuthUi`). Reusable primitives live under `ui/primitives/` with explicit properties and callbacks.

### Adding a Screen

1. Add a pure module under `src/screens/<name>.rs` (or `<name>/`).
2. Add an `Arc<Data>` field to `Snapshot` (`app/model.rs`), a bit to `ScreenMask` (`app/mask.rs`),
   a projection arm in `app/project.rs`, and a typed handle in `app/handles.rs`.
3. Add `ui/<name>.slint` with `export global <Name>Ui` and mount it in `ui/main.slint`.
4. Project its slice in `shell/view/` and bind its callbacks in `shell/wiring/`.

### The Mods Section

Mods exposes five pages: **Plugins**, **Themes**, **Plugin settings**, **Logs**, and **About**.

- **Plugins** lists compiled-in plugins declared in `screens/mods/builtin.rs` (currently AniSkip)
  and persists toggles and field values in `LocalExtras` (`<app data>/stremio-lightning/settings.json`).
- **Themes** lists built-in themes (such as Amoled) plus `.theme.css` files in
  `<app data>/stremio-lightning/themes/`. One theme can be enabled at a time and is applied to
  `ClassStyles` and `Theme` via `shell/theme_apply.rs`. Removing a custom theme uses a two-press
  arming flow that disarms automatically after 5 seconds or on focus/page change.
- **Plugin settings** edits a built-in plugin's declared schema fields in place without recreating
  Slint text inputs so keyboard focus stays intact.
- **Logs** reads the in-memory ring buffer and rotating `app.log` from `plumbing/logs.rs`, with
  source/level/text filtering, multi-row selection, copy, and live prepend anchoring.
- **About** reports build metadata, data paths, counts, and exports a redacted diagnostics bundle.

### The Player

`ui/player.slint` (composed from `ui/player/*.slint`) renders transparent Slint controls over
`libmpv`'s OpenGL framebuffer. `MainWindow`'s background switches to `transparent` while
`root.playing` is true. `plumbing::player` initializes `libmpv` lazily on first playback and tears
down the session when playback stops, keeping idle memory low. The player's event timer runs only
while playback is active.

### Architectural Decision Records

- [ADR 0001](adr/0001-shared-behavior-lives-in-core.md) (Superseded)
- [ADR 0002](adr/0002-native-client-is-a-library.md)
- [ADR 0003](adr/0003-deep-app-and-screen-ui-globals.md)
