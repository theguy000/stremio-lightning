//! Plumbing: the outside world. mpv, the streaming server, image downloads, browser links and
//! theme files each live behind a couple of functions here.
//!
//! Every module in this folder reports plain values and never Slint types or window
//! properties, so a screen can use it without knowing it exists.

pub(crate) mod images;
pub(crate) mod links;
pub(crate) mod player;
pub(crate) mod stream_server;
pub(crate) mod theme;
