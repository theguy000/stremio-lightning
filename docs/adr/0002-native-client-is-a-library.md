# The native client is a library of screens, wiring and plumbing

The native client was one 1,586-line `main.rs` plus five side files. It held the app state, every screen's data, the window mapping, image loading, the streaming server, the sign-in flow and twenty callbacks, so adding a screen meant editing it in five places. It was also a binary, so tests could not call it at all; two suites had responded by keeping their own copies of the logic they wanted to test.

The crate is now a library with a three-line launcher, and its code is one of three kinds, in three places. `screens/` holds one module per screen: plain data and the functions that build it from the model, with no window, no Slint types and no async. `shell/` holds the window's own state, the snapshot-to-window mapping, and one function per callback. `plumbing/` holds the outside world — mpv, the streaming server, image downloads, browser links, themes — each behind a couple of functions, reporting plain values. `app/` holds the state and turns core's events into a snapshot between the other two.

## Consequences

- A file belongs to exactly one kind. A wiring file may name screens and actions but decides nothing; a screen's data function may not mention the window.
- Adding a screen is local: a module under `screens/`, one field on `Snapshot`, one line in `app::events`, and the properties it fills in `shell/view`.
- The tests call the app. Screen data is the main doorway, and needs no window, display or network. Do not copy behaviour into a test to assert on it — a test that is a copy passes while the app breaks.
- A screen's Rust module and its Slint file share a name.
- Slint keeps the short-lived view state (which screen shows, which overlay is open, whether video plays). The shell owns it; screens do not write each other's flags.
