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

/// What the Plugins page says about its own switch. One sentence, above the list.
pub const NOTE: &str =
    "Switching a plugin on starts it now and at every start of the app. A plugin that fails says why on its card.";
/// What an empty Plugins folder shows.
pub const NONE_INSTALLED: &str = "No plugins are installed.";

/// What the Plugins page says above its list once the update check has answered: the page's own
/// sentence, then what the check found.
pub fn note(cards: &[super::catalogue::ModCard]) -> String {
    if cards.is_empty() {
        return NOTE.to_owned();
    }
    format!("{NOTE} {}", super::marketplace::update_note(cards))
}

fn switched_on(filename: &str) -> String {
    format!("{filename} is enabled and starting.")
}

fn switched_off_notice(filename: &str) -> String {
    format!("{filename} is off, so it will not run.")
}

/// A press on the switch of the plugin `filename`. With `enabled` true the file is added to
/// `enabled_plugins`, with false it is taken out. The list keeps the order files were enabled in,
/// so one plugin never reorders another. Returns `None` when the list already said so.
pub fn set_enabled(extras: &mut LocalExtras, filename: &str, enabled: bool) -> Option<Notice> {
    if let Some(builtin) = super::builtin::get(filename) {
        if builtin.switch.reads(extras) == enabled {
            return None;
        }
        builtin.switch.writes(extras, enabled);
        return Some(Notice::said(format!(
            "{} is {}.",
            builtin.name,
            if enabled { "on" } else { "off" }
        )));
    }
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
        assert!(set_enabled(&mut extras, "a.plugin.js", true).is_none());
    }
}
