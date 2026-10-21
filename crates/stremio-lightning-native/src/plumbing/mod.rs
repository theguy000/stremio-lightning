//! Plumbing: the outside world. mpv, the streaming server, image downloads, browser links and
//! theme files each live behind a couple of functions here.
//!
//! Every module in this folder reports plain values and never Slint types or window
//! properties, so a screen can use it without knowing it exists.

pub mod images;
pub mod links;
pub mod player;
pub mod stream_server;
pub mod theme;
