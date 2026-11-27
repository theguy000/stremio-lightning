# Settings UI parity and native shell wiring report

Reference: `Stremio/stremio-web` (`development`, v5.2.0) and `Stremio/stremio-shell-ng` (`master`), checked 2026-11-27.

## 1. Settings UI structure

`stremio-lightning` implements Stremio Web's 2026 paged Settings layout:

- Sidebar (`ui/settings/menu.slint`): `AccountRow` at the top, live `SearchField`, and three navigation groups (`PREFERENCES`, `THIS DEVICE`, `HELP`).
- Shared page primitives (`ui/settings/primitives.slint`, `ui/settings/scale.slint`): `SettingsPageHeader`, `SettingsGroup`, `SettingsRow`, `SettingsRowButton`, `SettingsLinkRow`, `SettingsToggle`, `SettingsScale`, and live `SubtitlesPreview`.
- Live search (`src/screens/settings/search.rs`, `SearchResultsPage`): multi-token prefix matching, guest and offline gating, and click-to-jump highlight.

## 2. Native shell feature status

| Feature | Settings location | Runtime wiring | Status |
|---|---|---|---|
| External player launch (`playerType`) | Player / Advanced | `screens/player/open.rs` resolves direct or streaming-server HTTP(S) stream URLs via `Stream::convert` and `StreamUrls::new`, dispatches `ActionMetaDetails::ExternalPlayerStreamOpened` and `MarkVideoAsWatched`, and launches VLC, MPV, or a 3-line M3U playlist via `plumbing/external_player.rs`. | Wired |
| Global keyboard shortcuts | Shortcuts / General + `Ctrl+/` modal | `shell/shortcuts.rs` hooked into `winit_host.rs` handles `Ctrl+-`/`Ctrl+=`/`Ctrl+0` interface zoom, `1`..`6` tab switching, `0` search focus, `Backspace` back navigation, `F`/`F11` fullscreen, `Ctrl+/` Shortcuts modal, and `Ctrl+G` Gamepad Guide modal. | Wired |
| Caching drive picker (`cacheRoot`) | Streaming / Server options | `screens/settings/cache_root.rs` reads `StreamingServer.settings_options` (`id == "cacheRoot"`) for loopback servers, opens a native folder picker via `plumbing/file_dialog.rs` (`rfd`), disables server selects while updating, and dispatches `ActionStreamingServer::UpdateCacheRoot`. | Wired |
| Custom `mpv.conf` file picker | Playback and app / Playback | Stored in `LocalExtras.mpv_config_path`, chosen via `plumbing/file_dialog.rs`, and loaded in `plumbing/player/mpv_config.rs` via `MpvInitializer::load_config` before re-enforcing `vo=libmpv`, `osc=no`, `keep-open=no`, `loop-file=no`, and disabled mpv keyboard bindings. | Wired |
| System tray and `quitOnClose` | Playback and app / Desktop app | `plumbing/tray.rs` (`tray-icon` with pure-Rust D-Bus `ksni` on Linux) and `shell/tray.rs` handle tray Show/Hide, Always on top, Quit, and `on_close_requested` hide-to-tray when `quitOnClose` is off (pausing playback if `pauseOnMinimize` is on). | Wired |
| Discord Rich Presence (`discordRpcEnabled`) | Playback and app / Desktop app | Pure-Rust IPC v1 client (`plumbing/discord_ipc.rs` + `plumbing/discord.rs`) uses Stremio App ID `1452620752263319665`, `ActivityType::Watching`, 5-second timestamp drift debouncing, `stremio_logo` fallback, paused timestamp formatting, idle clearing, and 15-second reconnect backoff. | Wired |
| Board Spotlight carousel and picker | Interface / Board + `SpotlightPickerModal` | `screens/board/spotlight.rs` interleaves up to 6 unique titles with background artwork from up to 3 catalogs (default first movie and first series), optionally hides picked Board rows, renders `ui/board/spotlight.slint` at the top of Board, and pins hero art in `app/images.rs`. | Wired |
| Gamepad navigation (`gamepadSupport`) | Interface / Behavior + `Ctrl+G` modal | `plumbing/gamepad.rs` (`gilrs`, started only while enabled) and `shell/gamepad.rs` detect PlayStation/Xbox/generic controllers, show connect/disconnect toasts, map navigation, player controls, and the live-highlighted `ui/gamepad-modal.slint` guide. | Wired |
| Existing Settings pages | Interface, Player, Integrations, Device, Streaming, Account, Help | Interface zoom, spoiler blur, live subtitles preview, subtitle styling and offset, surround sound, audio passthrough, hwdec, pause on minimize, Esc exit fullscreen, Trakt auth/logout, calendar `.ics`, streaming server URLs and options, password reset, GDPR data export, account deletion, and Help links. | Wired |

## 3. Intentionally omitted features

1. **Full UI localization (`interfaceLanguage`):** Persists to `Profile.settings` and prioritizes subtitle tracks, while native Slint UI copy remains English.
2. **Nvidia GPU Video Processing (`gpuVideoProcessing`) and Windows Legacy Video Mode (`videoMode`):** Specific to Stremio's Chromium/ANGLE web shell pipeline and inapplicable to `stremio-lightning`'s direct `libmpv` OpenGL renderer.
