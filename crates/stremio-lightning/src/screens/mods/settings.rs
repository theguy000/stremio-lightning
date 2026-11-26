//! Pure Plugin settings editor for compiled-in plugins.
//! Binds directly to `&'static builtin::Field`; zero disk I/O and zero hardcoded plugin keys.

use serde_json::{Map, Value as Json};

use super::builtin::{self, Field};
use super::catalogue;
use crate::plumbing::local_settings::LocalExtras;

pub const NOTHING_PICKED: &str = "Pick a plugin on the left to see its settings.";
pub const NO_SCHEMA: &str = "This plugin has no settings.";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Plugin {
    pub filename: String,
    pub name: String,
    pub fields: usize,
    pub enabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Control {
    pub field: &'static Field,
    pub stored: Json,
    pub edited: Json,
}

impl Control {
    fn changed(&self) -> bool {
        self.edited != self.stored
    }

    pub fn hint(&self) -> &'static str {
        if self.field.hint.is_empty() {
            self.field.default_hint()
        } else {
            self.field.hint
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Edits {
    pub filename: String,
    pub controls: Vec<Control>,
    pub error: String,
}

impl Edits {
    pub fn dirty(&self) -> bool {
        self.controls.iter().any(Control::changed)
    }

    pub fn values(&self) -> Map<String, Json> {
        self.controls
            .iter()
            .map(|c| (c.field.key.to_owned(), c.edited.clone()))
            .collect()
    }
}

/// Built-in plugins with their declared control counts and enabled flags.
pub fn plugins(extras: &LocalExtras) -> Vec<Plugin> {
    builtin::cards(extras)
        .into_iter()
        .map(|card| Plugin {
            name: catalogue::title(&card),
            fields: builtin::fields(&card.filename).len(),
            enabled: card.enabled,
            filename: card.filename,
        })
        .collect()
}

/// Opens the settings editor for built-in plugin `id`.
pub fn edits(id: &str, extras: &LocalExtras) -> Edits {
    let controls = builtin::fields(id)
        .iter()
        .map(|field| {
            let stored = builtin::value(extras, id, field);
            Control {
                field,
                edited: stored.clone(),
                stored,
            }
        })
        .collect();
    Edits {
        filename: id.to_owned(),
        controls,
        error: String::new(),
    }
}

pub fn set_value(edits: &mut Edits, key: &str, value: &Json) {
    if let Some(control) = edits.controls.iter_mut().find(|c| c.field.key == key) {
        control.edited = control.field.conform(value);
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Saved {
    pub written: bool,
    pub error: String,
    pub values: Json,
}

/// Validates and writes changed built-in settings into `extras`. The caller persists `extras`.
pub fn save(edits: &mut Edits, extras: &mut LocalExtras) -> Saved {
    if builtin::get(&edits.filename).is_none() {
        let error = format!("{} is not a built-in plugin.", edits.filename);
        edits.error = error.clone();
        return Saved {
            written: false,
            error,
            values: Json::Object(edits.values()),
        };
    }
    let mut validated: Vec<(&'static str, Json)> = Vec::new();
    for control in &edits.controls {
        if !control.changed() {
            continue;
        }
        match control.field.validate(&control.edited) {
            Ok(val) => validated.push((control.field.key, val)),
            Err(error) => {
                edits.error = error.clone();
                return Saved {
                    written: false,
                    error,
                    values: Json::Object(edits.values()),
                };
            }
        }
    }
    for (key, value) in validated {
        builtin::set_value(extras, &edits.filename, key, value);
    }
    let fresh = self::edits(&edits.filename, extras);
    edits.controls = fresh.controls;
    edits.error = String::new();
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
    fn builtin_settings_edit_validate_and_save_purely() {
        let mut extras = LocalExtras::default();
        assert!(builtin::ensure_submitter(
            &mut extras,
            "11111111-2222-4333-8444-555555555555"
        ));
        let mut page = edits("builtin:aniskip", &extras);
        assert_eq!(page.controls.len(), 3);
        assert!(!page.dirty());

        set_value(&mut page, "submit_button", &json!(false));
        assert!(page.dirty());
        let saved = save(&mut page, &mut extras);
        assert!(saved.written, "{}", saved.error);
        assert!(!page.dirty());
        let toggle = builtin::fields("builtin:aniskip")
            .iter()
            .find(|f| f.key == "submit_button")
            .unwrap();
        assert_eq!(
            builtin::value(&extras, "builtin:aniskip", toggle),
            json!(false)
        );

        // Blank submitter_id is rejected by Field::validate without saving.
        set_value(&mut page, "submitter_id", &json!(""));
        let refused = save(&mut page, &mut extras);
        assert!(!refused.written && refused.error.contains("must be a UUID"));
    }
}
