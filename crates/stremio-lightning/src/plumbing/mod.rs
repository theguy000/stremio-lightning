//! Plumbing: the outside world. mpv, the streaming server, image downloads, browser links, theme
//! files, skip times and the app's own log records each live behind a couple of functions here.
//!
//! Every module in this folder reports plain values and never Slint types or window
//! properties, so a screen can use it without knowing it exists.

pub mod clipboard;
pub mod env;
pub mod images;
pub mod links;
pub mod logs;
pub mod player;
pub(crate) mod rotating_log;
pub(crate) mod sanitize;
mod server_child;
pub mod skip;
pub mod stream_server;
pub mod theme;
pub(crate) mod update;
