pub const SHELL_VERSION: &str = env!("STREMIO_LIGHTNING_VERSION");

#[cfg(feature = "app-updates")]
pub mod app_update;
#[cfg(feature = "app-updates")]
pub(crate) mod http;
pub mod logging;
#[cfg(feature = "mods")]
pub mod mods;
pub mod streaming_logs;
pub mod streaming_server;
pub mod validation;
