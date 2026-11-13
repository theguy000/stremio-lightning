//! The Plugins page: built-in plugins and their switches.

use std::path::Path;

use super::catalogue::Notice;
use crate::screens::settings::LocalExtras;

/// A press on the switch of a built-in plugin. Returns whether the setting changed.
pub fn set_enabled(extras: &mut LocalExtras, id: &str, enabled: bool) -> bool {
    let Some(builtin) = super::builtin::get(id) else {
        return false;
    };
    if builtin.switch.reads(extras) == enabled {
        return false;
    }
    builtin.switch.writes(extras, enabled);
    true
}

/// Built-in plugins have no file to remove.
pub fn remove(_data_dir: &Path, _extras: &mut LocalExtras, id: &str) -> Notice {
    if let Some(builtin) = super::builtin::get(id) {
        return Notice::failed(format!(
            "{} is built into the app, so there is no file to remove.",
            builtin.name
        ));
    }
    Notice::failed(format!("{id} is not a built-in plugin."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_toggles_builtin_plugins_and_ignores_unknown_ids() {
        let mut extras = LocalExtras::default();
        assert!(extras.skip_intros);
        assert!(set_enabled(&mut extras, "builtin:aniskip", false));
        assert!(!extras.skip_intros);
        assert!(!set_enabled(&mut extras, "builtin:aniskip", false));
        assert!(set_enabled(&mut extras, "builtin:aniskip", true));
        assert!(extras.skip_intros);
        assert!(!set_enabled(&mut extras, "unknown", true));
    }
}
