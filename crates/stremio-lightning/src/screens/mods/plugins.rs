//! The Plugins page: the plugin files installed on this computer, the switch that turns one on,
//! and the button that removes it.
//!
//! The switch only writes a name into `enabled_plugins` in the shell's own `settings.json`. This
//! build does not load or run plugin code, and the page says so above the list, so nobody reads
//! the switch as "this plugin is working now".

use std::path::Path;

use super::catalogue::{self, Kind, Notice};
use crate::screens::settings::LocalExtras;

/// What the Plugins page says about its own switch. One sentence, above the list.
pub const NOT_RUNNING: &str =
    "This build does not run plugin code yet, so enabling a plugin only remembers the choice.";
/// What an empty Plugins folder shows.
pub const NONE_INSTALLED: &str = "No plugins are installed.";

fn switched_on(filename: &str) -> String {
    format!("{filename} is enabled. It will run once plugin support arrives.")
}

fn switched_off_notice(filename: &str) -> String {
    format!("{filename} is off, so it will not run.")
}

/// A press on the switch of the plugin `filename`. With `enabled` true the file is added to
/// `enabled_plugins`, with false it is taken out. The list keeps the order files were enabled in,
/// so one plugin never reorders another. Returns `None` when the list already said so.
pub fn set_enabled(extras: &mut LocalExtras, filename: &str, enabled: bool) -> Option<Notice> {
    let list = &mut extras.enabled_plugins;
    let known = list.iter().any(|name| name == filename);
    if known == enabled {
        return None;
    }
    if enabled {
        list.push(filename.to_owned());
    } else {
        list.retain(|name| name != filename);
    }
    Some(Notice::said(if enabled {
        switched_on(filename)
    } else {
        switched_off_notice(filename)
    }))
}

/// Removes the plugin file. A plugin that was named in `enabled_plugins` stops being named, so the
/// settings do not point at a file that is gone.
pub fn remove(data_dir: &Path, extras: &mut LocalExtras, filename: &str) -> Notice {
    let notice = catalogue::remove(data_dir, Kind::Plugin, filename);
    if !notice.failed {
        extras.enabled_plugins.retain(|name| name != filename);
    }
    notice
}
