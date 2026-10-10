pub const SHELL_VERSION: &str = env!("STREMIO_LIGHTNING_VERSION");

#[cfg(feature = "app-updates")]
pub mod app_update;
pub mod bridge_assets;
pub mod discord_rpc;
pub mod host_api;
#[cfg(any(feature = "app-updates", feature = "mods"))]
pub(crate) mod http;
pub mod launch_intent;
pub mod logging;
#[cfg(feature = "mods")]
pub mod mods;
pub mod navigation;
pub mod pip;
pub mod player_api;
pub mod settings;
pub mod streaming_logs;
pub mod streaming_server;
pub mod validation;
pub mod webview_runtime;
