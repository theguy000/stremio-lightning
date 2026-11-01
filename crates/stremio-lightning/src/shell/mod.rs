//! The shell: what the window itself owns, how a snapshot becomes window content, and the wiring
//! that turns a click into an action.
//!
//! The shell owns three things no screen may touch: which screen is showing, which overlay is
//! open, and whether video is playing. Everything else a screen describes, and the shell copies
//! it in.

pub(crate) mod view;
pub(crate) mod wheel;
pub(crate) mod wiring;

pub(crate) use view::show;
pub(crate) use wiring::wire;
