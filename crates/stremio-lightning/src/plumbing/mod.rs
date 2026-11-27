//! Plumbing: the outside world. mpv, the streaming server, image downloads, browser links, theme
//! files, skip times and the app's own log records each live behind a couple of functions here.
//!
//! Every module in this folder reports plain values and never Slint types or window
//! properties, so a screen can use it without knowing it exists.

pub mod clipboard;
pub(crate) mod diagnostics;
pub mod discord;
mod discord_ipc;
pub mod env;
pub mod external_player;
pub mod file_dialog;
pub mod gamepad;
pub mod images;
pub mod links;
pub mod local_settings;
pub mod logs;
pub(crate) mod oauth;
pub mod player;
pub(crate) mod rotating_log;
pub(crate) mod sanitize;
mod server_child;
pub mod skip;
pub mod stream_server;
pub mod theme;
pub mod tray;
pub(crate) mod update;
