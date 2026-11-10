//! The Plugins page: the plugin files installed on this computer, the switch that turns one on,
//! and the button that removes it.
//!
//! The switch writes a name into `enabled_plugins` in the shell's own `settings.json`, and the
//! plugin runtime starts or stops the file when the page is left. What the page says about a
//! plugin is what the runtime says: running, stopped with the sentence the interpreter gave, or
//! on the way up. A plugin that failed stays failed and says why until it is switched off and on
//! again, so a broken file never looks like a working one.

use std::path::Path;

use super::catalogue::{self, Kind, Notice};
use crate::screens::settings::LocalExtras;

/// What an empty Plugins folder shows.
pub const NONE_INSTALLED: &str = "No plugins are installed.";

/// What the Plugins page says above its list once the update check has answered.
pub fn note(cards: &[super::catalogue::ModCard]) -> String {
    super::marketplace::update_note(cards)
}

/// A press on the switch of the plugin `filename`. With `enabled` true the file is added to
/// `enabled_plugins`, with false it is taken out. The list keeps the order files were enabled in,
/// so one plugin never reorders another. Returns whether the list changed: the switch is the whole
/// of what the page says about it.
pub fn set_enabled(extras: &mut LocalExtras, filename: &str, enabled: bool) -> bool {
    if let Some(builtin) = super::builtin::get(filename) {
        if builtin.switch.reads(extras) == enabled {
            return false;
        }
        builtin.switch.writes(extras, enabled);
        return true;
    }
    let list = &mut extras.enabled_plugins;
    let known = list.iter().any(|name| name == filename);
    if known == enabled {
        return false;
    }
    if enabled {
        list.push(filename.to_owned());
    } else {
        list.retain(|name| name != filename);
    }
    true
}

/// Removes the plugin file. A plugin that was named in `enabled_plugins` stops being named, so the
/// settings do not point at a file that is gone.
pub fn remove(data_dir: &Path, extras: &mut LocalExtras, filename: &str) -> Notice {
    if let Some(builtin) = super::builtin::get(filename) {
        return Notice::failed(format!(
            "{} is built into the app, so there is no file to remove.",
            builtin.name
        ));
    }
    let notice = catalogue::remove(data_dir, Kind::Plugin, filename);
    if !notice.failed {
        extras.enabled_plugins.retain(|name| name != filename);
    }
    notice
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_keeps_one_plugin_from_reordering_another() {
        let mut extras = LocalExtras::default();
        set_enabled(&mut extras, "a.plugin.js", true);
        set_enabled(&mut extras, "b.plugin.js", true);
        set_enabled(&mut extras, "a.plugin.js", false);
        set_enabled(&mut extras, "a.plugin.js", true);
        assert_eq!(extras.enabled_plugins, ["b.plugin.js", "a.plugin.js"]);
        assert!(!set_enabled(&mut extras, "a.plugin.js", true));
    }
}
