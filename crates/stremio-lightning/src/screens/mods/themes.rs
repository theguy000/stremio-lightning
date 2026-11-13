//! The Themes page: the theme files installed on this computer, the switch that turns one on, and
//! the button that removes it.
//!
//! One theme is on at a time. Turning a theme on writes its file name and nothing else into
//! `enabled_themes`, which is the list `plumbing::theme` reads when it restyles the window.

use std::path::Path;

use super::catalogue::{self, Kind, Notice};
use crate::screens::settings::LocalExtras;

/// What the Themes page says when no theme is on.
pub const NONE_ON: &str = "No theme is on, so the app draws its own colours.";
/// What an empty Themes folder shows.
pub const NONE_INSTALLED: &str = "No themes are installed.";

/// What the Themes page says above its list.
pub fn note(only_on: bool) -> String {
    if only_on {
        "This theme is on.".to_owned()
    } else {
        NONE_ON.to_owned()
    }
}

fn switched_on(filename: &str) -> String {
    format!("{filename} is now the theme this app draws with.")
}

fn switched_off_notice(filename: &str) -> String {
    format!("{filename} is off, so the app draws its own colours.")
}

/// A press on `filename`'s switch. Turning one on makes it the only name in `enabled_themes`;
/// turning it off leaves the list empty. Returns `None` when the list already said so.
pub fn set_enabled(extras: &mut LocalExtras, filename: &str, enabled: bool) -> Option<Notice> {
    let list = &mut extras.enabled_themes;
    if enabled {
        if list.as_slice() == [filename.to_owned()] {
            return None;
        }
        *list = vec![filename.to_owned()];
        Some(Notice::said(switched_on(filename)))
    } else {
        if !list.iter().any(|name| name == filename) {
            return None;
        }
        list.clear();
        Some(Notice::said(switched_off_notice(filename)))
    }
}

/// Removes the theme file. A theme that was on stops being named, so the app goes back to its own
/// colours once the removal is applied.
pub fn remove(data_dir: &Path, extras: &mut LocalExtras, filename: &str) -> Notice {
    let notice = catalogue::remove(data_dir, Kind::Theme, filename);
    if !notice.failed {
        extras.enabled_themes.retain(|name| name != filename);
    }
    notice
}
