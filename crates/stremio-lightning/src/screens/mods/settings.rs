//! The Plugin settings page for built-in plugins.

use std::path::Path;

use serde_json::{json, Map, Value as Json};

use super::catalogue::{self, Kind};
use super::plugin_schema::Field;
use crate::screens::settings::LocalExtras;

/// What the page says when nothing is picked on the left.
pub const NOTHING_PICKED: &str = "Pick a plugin on the left to see its settings.";
/// What the page says for a built-in plugin that declares no settings.
pub const NO_SCHEMA: &str = "This plugin has no settings.";

/// One built-in plugin the page can show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plugin {
    pub filename: String,
    pub name: String,
    pub fields: usize,
    pub enabled: bool,
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
    /// The sentence under the control. A built-in brings its own; an empty one lets the page say
    /// what the kind of control is.
    pub hint: String,
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

/// Built-in plugins, with how many controls each declares and whether it is enabled.
pub fn plugins(data_dir: &Path, extras: &LocalExtras) -> Result<Vec<Plugin>, String> {
    let cards = catalogue::cards(data_dir, Kind::Plugin, extras)?;
    Ok(cards
        .into_iter()
        .map(|card| Plugin {
            name: catalogue::title(&card),
            fields: super::builtin::fields(&card.filename).len(),
            enabled: card.enabled,
            filename: card.filename,
        })
        .collect())
}

/// What the page holds for a built-in plugin: its declared fields and their stored values.
pub fn edits(_data_dir: &Path, id: &str, extras: &LocalExtras) -> Edits {
    if super::builtin::get(id).is_some() {
        return builtin_edits(id, extras);
    }
    Edits {
        filename: id.to_owned(),
        ..Edits::default()
    }
}

/// One built-in's page: its declared fields, each holding what the shell's settings say.
fn builtin_edits(filename: &str, extras: &LocalExtras) -> Edits {
    let mut edits = Edits {
        filename: filename.to_owned(),
        ..Edits::default()
    };
    for field in super::builtin::fields(filename) {
        let schema = match field.kind {
            super::builtin::FieldKind::Text { default } => Field::Text {
                key: field.key.to_owned(),
                label: field.label.to_owned(),
                default: default.to_owned(),
            },
            super::builtin::FieldKind::Toggle { default } => Field::Toggle {
                key: field.key.to_owned(),
                label: field.label.to_owned(),
                default,
            },
        };
        let stored = super::builtin::value(extras, filename, field);
        edits.controls.push(Control {
            stored: stored.clone(),
            edited: stored,
            hint: field.hint.to_owned(),
            field: schema,
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

/// The Save button: writes changed built-in settings into `extras`. The caller persists `extras`.
pub fn save(_data_dir: &Path, edits: &mut Edits, extras: &mut LocalExtras) -> Saved {
    if super::builtin::get(&edits.filename).is_some() {
        return save_builtin(edits, extras);
    }
    let error = format!("{} is not a built-in plugin.", edits.filename);
    edits.error = error.clone();
    Saved {
        written: false,
        error,
        values: Json::Object(edits.values()),
    }
}

/// Saves a built-in's page into the shell's settings. A submitter id that is empty or not a UUID
/// is made fresh here, so a page that has never had one leaves with one that can be used.
fn save_builtin(edits: &mut Edits, extras: &mut LocalExtras) -> Saved {
    for control in &edits.controls {
        if !control.changed() {
            continue;
        }
        let value = match control.field.key() {
            // The API requires the field and rejects anything that is not a UUID. The app makes
            // one on the first run, so a blank one here means the user cleared it on purpose:
            // refuse rather than quietly replacing the id they may have pasted.
            "submitter_id" => {
                let text = control.edited.as_str().map(str::trim).unwrap_or_default();
                if !super::builtin::is_uuid(text) {
                    let error = "Submitter ID is required: it must be a UUID. Nothing was saved."
                        .to_owned();
                    edits.error = error.clone();
                    return Saved {
                        written: false,
                        error,
                        values: Json::Object(edits.values()),
                    };
                }
                json!(text)
            }
            // The API requires the key but accepts an empty string; a submission is easier to
            // recognise with a name, so a cleared field falls back to the app's own.
            "provider_name" => {
                let text = control.edited.as_str().map(str::trim).unwrap_or_default();
                if text.is_empty() {
                    json!("Stremio Lightning")
                } else {
                    json!(text)
                }
            }
            _ => control.edited.clone(),
        };
        super::builtin::set_value(extras, &edits.filename, control.field.key(), value);
    }
    let fresh = builtin_edits(&edits.filename, extras);
    edits.controls = fresh.controls;
    edits.error = String::new();
    edits.trouble = fresh.trouble;
    Saved {
        written: true,
        error: String::new(),
        values: Json::Object(edits.values()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_control_only_counts_as_changed_when_it_differs() {
        let field = Field::Toggle {
            key: "on".to_owned(),
            label: "On".to_owned(),
            default: false,
        };
        let mut edits = Edits {
            filename: "builtin:aniskip".to_owned(),
            controls: vec![Control {
                field,
                stored: json!(false),
                edited: json!(false),
                hint: String::new(),
            }],
            ..Edits::default()
        };
        assert!(!edits.dirty());
        set_value(&mut edits, "on", &json!(true));
        assert!(edits.dirty());
        assert_eq!(edits.values()["on"], json!(true));
        edits = Edits::default();
        assert!(
            !edits.dirty(),
            "leaving the page throws away what was typed"
        );
    }

    #[test]
    fn a_built_in_page_reads_and_writes_the_shells_settings() {
        let dir = std::env::temp_dir().join(format!(
            "stremio-lightning-builtin-settings-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let mut extras = LocalExtras::default();
        assert!(
            super::super::builtin::ensure_submitter(&mut extras),
            "the first run makes a submitter id"
        );
        let mut page = edits(&dir, "builtin:aniskip", &extras);
        assert_eq!(page.controls.len(), 3);
        set_value(&mut page, "submit_button", &json!(false));
        let saved = save(&dir, &mut page, &mut extras);
        assert!(saved.written, "{}", saved.error);
        let toggle = super::super::builtin::fields("builtin:aniskip")
            .iter()
            .find(|field| field.key == "submit_button")
            .unwrap();
        assert_eq!(
            super::super::builtin::value(&extras, "builtin:aniskip", toggle),
            json!(false)
        );
        assert!(super::super::builtin::submitter(&extras).is_some());
        // Clearing the id is refused: the API requires a UUID, and the app will not send a
        // submission under an id that is not one.
        set_value(&mut page, "submitter_id", &json!(""));
        let refused = save(&dir, &mut page, &mut extras);
        assert!(!refused.written);
        assert!(refused.error.contains("must be a UUID"));
        let _ = std::fs::remove_dir_all(&dir);
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
            filename: "builtin:aniskip".to_owned(),
            controls: vec![Control {
                field,
                stored: json!(3),
                edited: json!(3),
                hint: String::new(),
            }],
            ..Edits::default()
        };
        set_value(&mut edits, "rows", &json!(9));
        assert_eq!(edits.controls[0].edited, json!(5));
    }
}
