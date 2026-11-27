# Settings UI Parity & Wiring Audit Report

Reference: `Stremio/stremio-web` (`development`, `src/routes/Settings/*`, checked 2026-11-27).

## 1. UI Parity Overview

`stremio-lightning` previously used Stremio Web's legacy 2024 single-page scrolling Settings screen (`General / Interface / Player / Streaming / Shortcuts` with a bottom version footer). It now matches Stremio Web's 2026 paged Settings architecture:

- **Sidebar (`ui/settings/menu.slint`):** `AccountRow` at top (opens `Account` when signed in, sign-in modal when guest), live `SearchField` (`Search settings`), and 3 uppercase nav groups:
  - **PREFERENCES:** `Interface`, `Player`, `Integrations`
  - **THIS DEVICE:** `Playback and app`, `Streaming`
  - **HELP:** `Shortcuts`, `Help and about`
- **Page & Card Primitives (`ui/settings/primitives.slint`, `ui/settings/scale.slint`):** `SettingsPageHeader` (`1.8rem` title + `"Saved on this device"` scope), `SettingsGroup` (`1.25rem` rounded cards over `Theme.overlay`), `SettingsRow` with `1px` dividers and search-hit highlight pulse, `SettingsRowButton` (`primary` / `danger` / external-link icon), `SettingsLinkRow`, animated green `SettingsToggle`, stepped `SettingsScale` (`75%..175%`), and live `SubtitlesPreview` over Big Buck Bunny (CC BY 3.0).
- **Live Settings Search (`src/screens/settings/search.rs`, `SearchResultsPage`):** Multi-token prefix matching with breadcrumbs (`Preferences › Player`), guest/offline gating, and click-to-jump + 1.9s row highlight.

## 2. Setting-by-Setting Wiring Audit

| Page / Group | Control / Setting | Core / Runtime Wiring | Status |
|---|---|---|---|
| **Interface — Language & Display** | `UI Language` (`interfaceLanguage`) | Saved to `Profile.settings` (`ActionCtx::UpdateSettings`), used for subtitle language sort priority in `player_tracks.rs`. Native UI strings remain English-only (no i18n catalog in Slint yet). | Partially wired (persists & sorts tracks; UI copy not localized) |
| **Interface — Language & Display** | `Interface zoom` (`interfaceScale`) | **Newly wired:** 5-stop `SettingsScale` (`75/100/125/150/175%`) persists via `Profile.settings` and updates `Theme.ui-scale` live across the whole app. | Fully wired |
| **Interface — Behavior** | `Blur unwatched episodes image` (`hideSpoilers`) | **Newly wired:** Persists via `Profile.settings` and applies `images::blur_key` to unwatched episode thumbnails in Details episode list, Details picked episode hero, and Player side drawer. | Fully wired |
| **Player — Subtitles** | Live `SubtitlesPreview` | **Newly added & wired:** Updates live with `subtitlesSize`, `subtitlesOffset`, `subtitlesTextColor`, `subtitlesBackgroundColor`, `subtitlesOutlineColor`, and `subtitlesBold`. | Fully wired |
| **Player — Subtitles** | `Default Subtitles Language` (`subtitlesLanguage`) | Persists in `Profile.settings` -> `PlayerConfig` -> mpv `slang` & `sub-visibility`. | Fully wired |
| **Player — Subtitles** | `Default Subtitles Size` (`subtitlesSize`) | Persists in `Profile.settings` -> `PlayerConfig` -> mpv `sub-scale`. | Fully wired |
| **Player — Subtitles** | `Vertical Position` (`subtitlesOffset`) | **Newly added & wired:** `0%..100%` (step 5%) -> mpv `sub-pos = 100 - offset`. | Fully wired |
| **Player — Subtitles** | `Subtitles color / background / outline` | Persists in `Profile.settings` -> mpv `sub-color`, `sub-back-color`, `sub-border-color`. | Fully wired |
| **Player — Subtitles** | `Bold` (`subtitlesBold`) | **Newly added & wired:** Persists in `Profile.settings` -> mpv `sub-bold`. | Fully wired |
| **Player — Subtitles** | `Style ASS subtitles` (`assSubtitlesStyling`) | **Newly added & wired:** Persists in `Profile.settings` -> mpv `sub-ass-override` (`force` / `no`). | Fully wired |
| **Player — Audio** | `Default Audio Track` (`audioLanguage`) | Persists in `Profile.settings` -> mpv `alang`. | Fully wired |
| **Player — Controls** | `Arrow keys seek time` (`seekTimeDuration`) / `(Shift)` (`seekShortTimeDuration`) | Persists in `Profile.settings` -> `PlayerUi.seek-step` / `seek-short-step` -> `PlayerKeyHandler`. | Fully wired |
| **Player — Auto-Play** | `Auto-Play Next Episode` (`bingeWatching`) | Persists in `Profile.settings` -> read in `shell/wiring/player.rs` on EOF. | Fully wired |
| **Player — Auto-Play** | `Next video popup duration` (`nextVideoNotificationDuration`) | Extended to `0..90s` matching Stremio Web -> read in `screens/player/mod.rs`. | Fully wired |
| **Player — Advanced** | `Play in external player` (`playerType`) | Persists in `Profile.settings` (`Disabled / M3U / VLC / MPV`), **not yet consumed** by `screens/player/open.rs` (currently only `StreamSource::External` opens outside mpv). | Stored only — **needs external-launcher wiring** |
| **Integrations** | `Trakt Scrobbling` | **Newly wired** (button previously had no `clicked` handler): opens `https://www.strem.io/trakt/auth/{user_id}` when disconnected, dispatches `ActionCtx::LogoutTrakt` when connected. | Fully wired |
| **Integrations** | `Subscribe to calendar` | **Newly added & wired:** Opens `https://www.strem.io/calendar/{user_id}.ics` + toast. | Fully wired |
| **Playback and app — Playback** | `Surround sound` (`surroundSound`) | **Newly wired:** Persists in `Profile.settings` -> mpv `audio-channels` (`auto-safe` vs `stereo`). | Fully wired |
| **Playback and app — Playback** | `Audio passthrough` (`audioPassthrough`) | **Newly added & wired:** Persists in `Profile.settings` -> mpv `audio-spdif` (`ac3,eac3,dts,dts-hd,truehd` vs `""`). | Fully wired |
| **Playback and app — Playback** | `Hardware-accelerated decoding` (`hardwareDecoding`) | Persists in `Profile.settings` -> mpv `hwdec` (`auto-safe` vs `no`). | Fully wired |
| **Playback and app — Playback** | `Pause playback when minimized` (`pauseOnMinimize`) | **Newly wired:** Persists in `Profile.settings` -> `winit_host.rs` detects minimize (`WindowEvent::Resized` zero-size / `is_minimized()`) and calls `video.set_paused(true)`. | Fully wired |
| **Playback and app — Desktop app** | `Quit on close` (`quitOnClose`) | Persists in `Profile.settings`. `stremio-lightning` has no system-tray resident mode, so closing the window always quits. | Stored only (no tray mode) |
| **Playback and app — Desktop app** | `Escape key exit full screen` (`escExitFullscreen`) | **Newly wired:** Propagated to `PlayerUi.esc-exit-fullscreen` -> `PlayerKeyHandler` exits fullscreen on `Esc` before navigating back. | Fully wired |
| **Streaming — URLsManager** | Server URL list, Add URL, Delete URL, Select URL, Online/Offline status, Reload | **Newly wired:** Added `stremio_core::models::streaming_server::StreamingServer` to `AppModel`; dispatches `ActionCtx::AddServerUrl`, `ActionCtx::DeleteServerUrl`, `UpdateSettings(streaming_server_url)`, and `ActionStreamingServer::Reload`. | Fully wired |
| **Streaming — Remote URL** | Remote URL row + Copy button | **Newly added & wired:** Reads `StreamingServer.remote_url`, copies via clipboard + toast. | Fully wired |
| **Streaming — Server options** | `Streaming HTTPS endpoint`, `Cache size`, `Torrent profile`, `Transcode profile` | **Newly wired** (previously saved only to local `settings.json` overrides and never sent to the server): now reads live `StreamingServer.settings` / `network_info` / `device_info` and dispatches `ActionStreamingServer::UpdateSettings`. | Fully wired |
| **Streaming — Offline notice** | Offline warning card | **Newly added & wired:** Shown when `StreamingServer.settings` is `Loadable::Err`. | Fully wired |
| **Account** | Identity card (`Log in / Sign up`, `Log out`) | Dispatches `ActionCtx::Logout` / opens `AuthUi`. | Fully wired |
| **Account — Security** | `Change password` | **Newly added & wired:** Opens `https://www.strem.io/reset-password/{email}`. | Fully wired |
| **Account — Your data** | `Export user data` | **Newly added & wired:** Added `stremio_core::models::data_export::DataExport` to `AppModel`, dispatches `ActionLoad::DataExport`, opens returned export URL in browser. | Fully wired |
| **Account — Danger** | `Delete account` | **Newly added & wired:** Opens `https://www.stremio.com/acc-settings`. | Fully wired |
| **Shortcuts** | Keyboard shortcuts list | Updated to match real wired `PlayerKeyHandler` bindings (added `F11`, `-`/`=`, `G`/`H`, `[`/`]`, `V`, `C`, `S`, `A`, `I`, `R`, `Shift+N`). | Fully wired |
| **Help and about** | `Contact support`, `App / Build / Server Version`, `Copy details`, `Terms`, `Privacy`, `Source code`, `Big Buck Bunny CC BY 3.0` | **Newly added & wired:** All links verified in `plumbing/links.rs` + `ui_shape_tests.rs`; `Copy details` copies version block to clipboard. | Fully wired |

## 3. Stremio Web Features Intentionally Omitted (Not Supported in Native Shell Yet)

These controls exist in `stremio-web` (gated on platform/shell capabilities) and were **not** added as dead toggles:

1. **Board Spotlight settings (`SpotlightSettings`)** — `stremio-lightning`'s Board screen does not yet render a Spotlight hero carousel.
2. **Gamepad support (`gamepadSupport`)** — No gamepad input backend (`gilrs` etc.) is integrated in the native shell.
3. **Nvidia GPU Video Processing (`gpuVideoProcessing`) & Windows Legacy Video Mode (`videoMode`)** — Specific to `stremio-shell-ng`'s Direct3D/ANGLE pipeline; `stremio-lightning` renders exclusively via `libmpv` OpenGL render context.
4. **Custom `mpv.conf` file picker (`useMpvConfig`) & Caching drive picker (`useCacheLocation`)** — Requires native OS file/folder dialog integration and `mpv` config-file reload plumbing (`ActionStreamingServer::UpdateCacheRoot` is available in `stremio-core` when a folder picker is added).
5. **Discord Rich Presence (`discordRpcEnabled`)** — No Discord IPC client is bundled in `stremio-lightning`.
6. **Global shell keyboard shortcuts (`Ctrl+/`, `1..6`, `0`)** — Only player shortcuts are wired in `PlayerKeyHandler`; the Shortcuts page accurately lists only the working shortcuts rather than advertising unwired global hotkeys.

## 4. Recommended Follow-Up Wiring Tasks

1. **External player launcher (`playerType`):** In `screens/player/open.rs` + `shell/wiring/player.rs`, if `profile.settings.player_type` is `vlc` / `mpv` / `m3u`, hand the resolved `streaming_url` to the external binary or `.m3u` playlist instead of loading into embedded `libmpv`.
2. **Native tray / minimize-to-tray (`quitOnClose`):** Add a system-tray icon (`tray-icon`) if keeping the app running in the background when closing the window is desired; otherwise keep `quitOnClose` hidden or note that desktop close always quits.
3. **Custom `mpv.conf` & Caching drive picker:** Add `rfd` (or platform picker) to wire `Settings -> Playback and app -> Custom mpv config` and `Settings -> Streaming -> Caching drive` (`ActionStreamingServer::UpdateCacheRoot`).
4. **UI Localization (`interfaceLanguage`):** Wire a translation loader for `stremio-translations` if localized Slint UI strings are needed.
