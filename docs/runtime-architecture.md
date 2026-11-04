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

### The Mods section

Mods has a page menu on the left and one page on the right. The menu lists six pages: Plugins,
Themes, Marketplace, Plugin settings, Logs and About. `screens/mods/` owns the menu, the two pages
that list installed files, the settings page and the Marketplace page; Logs and About show a
heading and a line saying they arrive later.
`screens/mods/catalogue.rs` reads the plugin and theme folders under `<app data>/stremio-lightning/`
and turns each file into a card. Everything a card says comes from the file: its metadata comment,
or its file name when the comment is missing, plus the theme `parse` warnings for a theme.

The window holds two pieces of state the snapshot does not. `mods-page` is the page on screen, so
a press on the menu only says which page was picked and the wiring repaints with it. An armed
Remove is window state too: the first press of a card's Remove button arms that card and the
second press, on the same card, deletes the file. Pressing any other card, changing page, or five
seconds passing takes the arm back.

### The Marketplace

`screens/mods/marketplace.rs` is the page's data and its actions. It reads the folder and the
registry, but it never starts a thread and never touches a window: its fetches are `async`
functions the shell runs for it. The registry is
`stremio_lightning_core::mods::fetch_registry`, which reads `REGISTRY_URL` unless
`STREMIO_LIGHTNING_REGISTRY_URL` names another address; core reads that variable on every fetch,
and only tests set it. The registry is read the first time the page is
opened, never at boot, and again when Check for updates is pressed. `shell/wiring` runs both the
read and the check on the shell's runtime handle and repaints when they land, so the window's
thread never waits on the network. A read that fails becomes one sentence on the page,
`Listing::failed`, and the list stays empty rather than showing the registry it could not read.

What "installed" means is decided by the folder, not by the registry. An entry is installed when a
file in its folder is the one its download is stored as, which is the file name
`mods::installed_filename` gives (the URL's own name when that name carries the extension,
otherwise the entry's name with spaces and punctuation replaced) or a file whose `@updateUrl` is
that download. A different mod with the same name and author is not counted, which is what
matching metadata would have done.

Install is `mods::download_mod_as` under that name. The body is then checked with
`mods::looks_like`: it must carry a metadata comment and must not start with `<!doctype` or
`<html` for a plugin. A page that answered where a mod should be is deleted and the sentence says
what arrived. Nothing is switched on afterwards: a new mod is off until its switch is pressed on
Plugins or Themes, and the page says so in its summary line.

The update check is `mods::check_mod_updates` for both kinds, off the window's thread, when the
Marketplace page is shown and on Check for updates. Each installed card on Plugins and Themes
carries the result as a sentence (`Up to date.`, `Update to 2.0.0`, or what the check could not
read), and the page's note counts the ones that are ready. Update and Update all fetch a file's
new version into memory with `mods::fetch_text`, check it the same way an install is checked, and
only then write it over the old file, so a bad body never replaces a working mod. Uninstall is the
same two-press arm the installed pages use, and the file is deleted by the same function they
call.

Plugins and themes are switched on by writing their file names to `enabled_plugins` and
`enabled_themes` in the shell's own `settings.json`. A name that is absent means off. Turning a
theme on replaces the whole theme list with that one name, so one theme is on at a time.
`plumbing::theme::apply_enabled` reads that list and draws the window with it, both at boot and
whenever the choice changes.

Turning a plugin on starts it: `shell/wiring` calls `app::plugins::Runtime::start`, which reads the
file and hands it to `plumbing/plugin`, an embedded JavaScript engine (rquickjs, a vendored build of
QuickJS). Every plugin runs on its own thread. Nothing a plugin does runs on the window's thread,
and a plugin that keeps running past its budget is abandoned rather than waited for. What a plugin
can reach is fixed in `plumbing/plugin/globals.rs` and written down in `docs/plugin-api.md`. A line
a plugin logs, a settings schema it registers and rules it asks the window to draw arrive at
`app::plugins` as plain data, and `shell/wiring` acts on them: lines land on the plugin's state,
rules go through `plumbing::theme::install`, and a schema is stored in the plugin's own settings
file. Nothing is written to stderr.

`Runtime` keeps what it knows about each plugin in three places: the running ones with the lines
they logged, the ones that stopped with the sentence that says why, and a count that changes on
every log line and every start. The Mods pages read the runtime when a snapshot is built, so a
Plugins card says `Running.`, `Stopped: ...` or the last warning the plugin logged, and a plugin
that is off says nothing about running.

The Plugin settings page lists the installed plugins on the left and the picked one's schema on the
right. A schema is read from the plugin's settings file (`__schema`), so the page draws it after a
restart without running the plugin first. A control is a toggle, a text box, a select row or a
slider; a field of any other kind is reported and left out. A press changes the page and writes
nothing. Save writes the changed values through `settings::save_setting`, keeps every key the schema
does not name, and hands the file to the running plugin's `onSettingsSaved`. Revert reads the file
again and drops the typing. Nothing on the page deletes a value or the schema.

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
