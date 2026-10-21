//! The app itself: what state it holds, how that state is built, how core's events turn it
//! into what the window shows, and how the window is repainted.
//!
//! Nothing here knows about a particular screen. Screens live in `crate::screens`; the wiring
//! that connects them to the window lives in `crate::shell`.

pub(crate) mod build;
pub(crate) mod events;
pub(crate) mod model;

pub(crate) use model::{AppModel, AppRuntime, Snapshot};
