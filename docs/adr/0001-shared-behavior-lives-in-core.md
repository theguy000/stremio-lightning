# Shared behavior lives in core; each shell supplies only an adapter

> **Status: Superseded.** The platform web shells (`stremio-lightning-{windows,linux,macos}`) and `stremio-lightning-core` were retired. The workspace now ships a single native Slint crate (`crates/stremio-lightning`); see [ADR 0002](0002-native-client-is-a-library.md) and [ADR 0003](0003-deep-app-and-screen-ui-globals.md).

Linux, macOS and Windows each had their own copy of the streaming-server supervisor, the player backend trait and transport handling, the webview injection bundle and load state, and the `--url`/`--devtools` startup flags. The copies drifted. We moved the shared behavior into `stremio-lightning-core` (`streaming_server`, `player_api`, `webview_runtime`, `startup`). A shell keeps only what is genuinely platform-specific: the command builder, the JS host adapters, native window and player code, and its own flags.

## Consequences

- Fix shared behavior once, in core. Do not copy it back into a shell.
- Some rules are now the same everywhere. For example, `validate_load_url` is case-insensitive on every platform.
- Windows keeps its job-object child process and its own player backend. They differ from the other shells on purpose.
- The macOS player trait extends the core one with `drain_events`.
