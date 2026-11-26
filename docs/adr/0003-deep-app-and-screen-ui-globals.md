# Deep `App` handles, lazy screen projection, and per-screen Slint globals

After ADR 0002 split the native client into `plumbing/`, `screens/`, `app/`, and `shell/`, `MainWindow` still acted as a 233-member pass-through surface for every screen, `app/` rebuilt every screen on every `stremio-core` event, `shell/` polled counters with background timers, and several Rust and Slint files grew past 1,000–2,700 lines.

We redesigned the seams across all four layers and the Slint UI:

1. **Deep `App` with per-screen handles and push notices (`app/`).** `Arc<App>` stays Slint-free and exposes direct typed per-screen handles (`board()`, `discover()`, `library()`, `details()`, `player()`, `addons()`, `calendar()`, `settings()`, `mods()`, `auth()`, `viewport()`). Core events mark fine-grained `ScreenMask` bits and emit `AppNotice` push notifications directly to the shell; no polling timers exist.
2. **Structurally shared snapshots and lazy inactive-screen projection.** Each screen slice in `Snapshot` sits behind an `Arc<Data>`, so reading or updating one screen is O(1) and never clones unaffected screens. Inactive screens keep their dirty bits set until selected, avoiding wasted projection work off-screen.
3. **Per-screen Slint `<Screen>Ui` globals (`ui/`).** Each screen owns an exported Slint global (`BoardUi`, `DiscoverUi`, `LibraryUi`, `DetailsUi`, `PlayerUi`, `AddonsUi`, `CalendarUi`, `SettingsUi`, `ModsUi`, `AuthUi`). `MainWindow` holds only 19 frame-level members (active section, overlay flags, toast/share modal state, wheel scroll offsets, and frame callbacks). Reusable primitives under `ui/primitives/` keep explicit properties and callbacks so they remain context-free.
4. **Single shell entry point and unified host seam (`shell/`).** `shell::attach(window, app)` wires the whole window. `shell/winit_host.rs` unifies wheel scrolling (guarded by `active_popups()`), one-shot window icon setup, and `WindowEvent::Focused(true)` account sync. `shell/view/sync.rs` reconciles Slint models in place through `ModelMirror<K, Dst>` keyed on plain Rust values plus image revisions.
5. **Module size and strict layer discipline.** Every production `.rs` and `.slint` file stays at or below 300 lines (single justified exception: `screens/language_names.rs`, a 433-line static ISO-639 lookup table). `plumbing/` has zero Slint, `MainWindow`, `screens::*`, `app::*`, or `shell::*` imports; `screens/` is pure in-process Rust with zero Slint, `MainWindow`, async, or disk/HTTP I/O; `app/` has zero Slint or `shell::*` imports.

## Consequences

- Adding or changing a screen stays local to `screens/<name>`, its `App` handle/projection, `ui/<name>.slint` (`<Name>Ui`), `shell/view/`, and `shell/wiring/`.
- `MainWindow` changes only for app-wide frame chrome or top-level overlays.
- `shell/view/` never drops Slint row models unnecessarily, preserving input focus and scroll offsets.
- Idle and scrolling memory stays low because inactive screens do not project eagerly, portrait posters avoid per-card clip FBOs, and `libmpv` loads only when playback starts.
