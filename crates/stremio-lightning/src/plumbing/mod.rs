//! Plumbing: the outside world. mpv, the streaming server, image downloads, browser links, theme
//! files, skip times, plugin interpreters and the app's own log records each live behind a
//! couple of functions here.
//!
//! Every module in this folder reports plain values and never Slint types or window
//! properties, so a screen can use it without knowing it exists.

pub mod clipboard;
pub mod images;
pub mod links;
pub mod logs;
pub mod player;
pub mod plugin;
pub mod skip;
pub mod stream_server;
pub mod theme;
