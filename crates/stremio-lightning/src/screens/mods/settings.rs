//! The Plugin settings page: the plugin files on this computer, the schema each one registered,
//! and the values the user is editing before they are saved.
//!
//! The page never guesses. A value on screen is either what the plugin's settings file holds or
//! what the user has typed since the page was last saved, and the page says which. Saving is the
//! only thing that writes: a press on Save writes every changed value, then hands the plugin its
//! own `onSettingsSaved` callbacks with the values the file now holds. Revert reads the file again
//! and throws the edits away.
//!
//! Nothing here deletes: a key the schema no longer mentions stays in the file, and so does the
//! schema itself.

use std::path::Path;

use serde_json::{Map, Value as Json};
use stremio_lightning_core::mods::{self, ModType};
use stremio_lightning_core::settings;

use super::catalogue::{self, Kind};
use super::plugin_schema::{self, Field};
use crate::screens::settings::LocalExtras;

/// What the page says when nothing is picked on the left.
pub const NOTHING_PICKED: &str = "Pick a plugin on the left to see its settings.";
/// What the page says for a plugin that registered no schema.
pub const NO_SCHEMA: &str =
    "This plugin has registered no settings. Its file can still keep values of its own.";
/// What the page says about a plugin that stopped: the reason follows.
pub const STOPPED: &str = "Stopped: ";

/// One plugin the page can show, as the left list draws it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plugin {
    /// The file name, which is what every action on the page names it by.
    pub filename: String,
    /// The name its metadata gives, or the file name when it gives none.
    pub name: String,
    /// How many controls its settings page has.
    pub fields: usize,
    /// Whether the plugin is switched on, whichever way it ended up running.
    pub enabled: bool,
}

/// One plugin's state, as the window draws it: what the list needs, and what the right side says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Running {
    /// Whether the plugin is loaded and running right now.
    pub running: bool,
    /// Why it is not, empty while it is.
    pub stopped: String,
    /// The last line the plugin logged at `warn` or `error`, empty when it logged none.
    pub last_error: String,
}

impl Running {
    /// The line the page shows under the plugin's name, empty for a plugin that is not on.
    pub fn said(&self, enabled: bool) -> String {
        if !enabled {
            return String::new();
        }
        if self.running {
            return if self.last_error.is_empty() {
                "Running.".to_owned()
            } else {
                format!("Running. Last warning: {}", self.last_error)
            };
        }
        if self.stopped.is_empty() {
            "Enabled, but not running yet.".to_owned()
        } else {
            format!("{STOPPED}{}", self.stopped)
        }
    }
}

/// One control on screen: the schema's field, the value the file holds, and the value the user
/// has typed since.
#[derive(Clone, Debug, PartialEq)]
pub struct Control {
    pub field: Field,
    /// What the settings file holds, made to fit the field.
    pub stored: Json,
    /// What the user is editing. Equal to `stored` until they change it, which is what marks the
    /// page as having something to save.
    pub edited: Json,
}

impl Control {
    fn changed(&self) -> bool {
        self.edited != self.stored
    }
}

/// What the page holds for one plugin.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Edits {
    /// The plugin the page is about: its file name.
    pub filename: String,
    pub controls: Vec<Control>,
    /// Why the schema could not be used at all, empty when it could.
    pub trouble: String,
    /// Why the file could not be read or written, empty when it could.
    pub error: String,
}

impl Edits {
    /// Whether anything on the page has been typed and not saved.
    pub fn dirty(&self) -> bool {
        self.controls.iter().any(Control::changed)
    }

    /// What the plugin is handed on the save press: every control's edited value, by key.
    pub fn values(&self) -> Map<String, Json> {
        self.controls
            .iter()
            .map(|control| (control.field.key().to_owned(), control.edited.clone()))
            .collect()
    }
}

/// The plugins installed under `data_dir`, in file-name order, with how many controls each one's
/// schema has and which ones are switched on.
pub fn plugins(data_dir: &Path, extras: &LocalExtras) -> Result<Vec<Plugin>, String> {
    let cards = catalogue::cards(data_dir, Kind::Plugin, extras)?;
    let plugins_dir = mods::mods_dir(data_dir, ModType::Plugin);
    Ok(cards
        .into_iter()
        .map(|card| {
            let name = crate::plumbing::plugin::name_of(&card.filename);
            let fields = plugin_schema::stored(&plugins_dir, &name)
                .map(|fields| fields.len())
                .unwrap_or(0);
            Plugin {
                name: catalogue::title(&card),
                fields,
                enabled: card.enabled,
                filename: card.filename,
            }
        })
        .collect())
}

/// What the page holds for `filename`: its schema, and for every control the value the file holds
/// and a copy for the user to edit.
pub fn edits(data_dir: &Path, filename: &str) -> Edits {
    edits_for(&mods::mods_dir(data_dir, ModType::Plugin), filename)
}

fn edits_for(plugins_dir: &Path, filename: &str) -> Edits {
    let name = crate::plumbing::plugin::name_of(filename);
    let mut edits = Edits {
        filename: filename.to_owned(),
        ..Edits::default()
    };
    let fields = match plugin_schema::stored(plugins_dir, &name) {
        Ok(fields) => fields,
        Err(error) => {
            edits.error = format!("Its settings could not be read: {error}");
            return edits;
        }
    };
    for field in fields {
        // A value the field cannot hold is the field's default: drawing it would change it.
        let stored = settings::setting(plugins_dir, &name, field.key())
            .ok()
            .filter(|value| field.accepts(value))
            .unwrap_or_else(|| field.default());
        edits.controls.push(Control {
            stored: stored.clone(),
            edited: stored,
            field,
        });
    }
    edits
}

/// A press in a control: the value goes into the page, not the file.
pub fn set_value(edits: &mut Edits, key: &str, value: &Json) {
    if let Some(control) = edits.controls.iter_mut().find(|c| c.field.key() == key) {
        control.edited = control.field.conform(value);
    }
}

/// The Revert button: everything typed since the last save goes back to what the file holds.
pub fn revert(edits: &mut Edits) {
    for control in &mut edits.controls {
        control.edited = control.stored.clone();
    }
}

/// What saving did, as the plugin needs to hear it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Saved {
    /// Whether the file took every value.
    pub written: bool,
    /// Why not, empty when it did.
    pub error: String,
    /// The plugin's settings file afterwards, which is what its callbacks are handed.
    pub values: Json,
}

/// The Save button: every changed value is written to the plugin's settings file, and the page is
/// read back from it, so what is on screen and what the file holds are the same again.
pub fn save(data_dir: &Path, edits: &mut Edits) -> Saved {
    let name = crate::plumbing::plugin::name_of(&edits.filename);
    let plugins_dir = mods::mods_dir(data_dir, ModType::Plugin);
    let mut error = String::new();
    for control in &edits.controls {
        if !control.changed() {
            continue;
        }
        if let Err(why) = settings::save_setting(
            &plugins_dir,
            &name,
            control.field.key(),
            control.edited.clone(),
        ) {
            error = format!("{} could not be saved: {why}", control.field.label());
            break;
        }
    }
    let values = file_of(&plugins_dir, &name);
    if !error.is_empty() {
        edits.error = error.clone();
        return Saved {
            written: false,
            error,
            values,
        };
    }
    // Read back through `edits`, so a control whose value the file normalised shows what the file
    // holds rather than what was typed.
    let fresh = edits_for(&plugins_dir, &edits.filename);
    edits.controls = fresh.controls;
    edits.error = String::new();
    edits.trouble = fresh.trouble;
    Saved {
        written: true,
        error: String::new(),
        values,
    }
}

/// The plugin's settings file as it stands, the schema included: `{}` when it has none.
pub fn file_of(plugins_dir: &Path, name: &str) -> Json {
    settings::plugin_settings_path(plugins_dir, name)
        .ok()
        .and_then(|path| settings::load_settings_file(&path).ok())
        .filter(Json::is_object)
        .unwrap_or_else(|| Json::Object(Map::new()))
}

/// The first `warn` or `error` line a plugin has logged, put where the page can show it. Empty for
/// a line at another level.
pub fn last_error(logs: &[crate::plumbing::plugin::LogLine]) -> String {
    logs.iter()
        .rev()
        .find(|line| line.level == "warn" || line.level == "error")
        .map(|line| line.text.clone())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_plugin_that_is_off_says_nothing_about_running() {
        let running = Running::default();
        assert_eq!(running.said(false), "");
        assert_eq!(running.said(true), "Enabled, but not running yet.");
        let up = Running {
            running: true,
            ..Running::default()
        };
        assert_eq!(up.said(true), "Running.");
        let warned = Running {
            running: true,
            last_error: "slow".to_owned(),
            ..Running::default()
        };
        assert_eq!(warned.said(true), "Running. Last warning: slow");
        let down = Running {
            stopped: "it threw".to_owned(),
            ..Running::default()
        };
        assert_eq!(down.said(true), "Stopped: it threw");
    }

    #[test]
    fn a_control_only_counts_as_changed_when_it_differs() {
        let field = Field::Toggle {
            key: "on".to_owned(),
            label: "On".to_owned(),
            default: false,
        };
        let mut edits = Edits {
            filename: "a.plugin.js".to_owned(),
            controls: vec![Control {
                field,
                stored: json!(false),
                edited: json!(false),
            }],
            ..Edits::default()
        };
        assert!(!edits.dirty());
        set_value(&mut edits, "on", &json!(true));
        assert!(edits.dirty());
        assert_eq!(edits.values()["on"], json!(true));
        revert(&mut edits);
        assert!(!edits.dirty(), "revert throws away what was typed");
    }

    #[test]
    fn a_range_control_reports_a_clamped_number() {
        let field = Field::Range {
            key: "rows".to_owned(),
            label: "Rows".to_owned(),
            min: 1.0,
            max: 5.0,
            step: 1.0,
            default: 3.0,
        };
        let mut edits = Edits {
            filename: "a.plugin.js".to_owned(),
            controls: vec![Control {
                field,
                stored: json!(3),
                edited: json!(3),
            }],
            ..Edits::default()
        };
        set_value(&mut edits, "rows", &json!(9));
        assert_eq!(edits.controls[0].edited, json!(5));
    }
}
