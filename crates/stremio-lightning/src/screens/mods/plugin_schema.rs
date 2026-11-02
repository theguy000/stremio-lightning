//! The schema a plugin registers, read into the fields the Plugin settings page draws.
//!
//! A schema is one JSON array. Each entry names a key, a label and a type:
//!
//! ```json
//! [
//!   { "key": "enabled", "label": "Turn it on", "type": "toggle", "default": true },
//!   { "key": "quality", "label": "Quality", "type": "select", "options": ["720p", "1080p"], "default": "1080p" },
//!   { "key": "server", "label": "Server", "type": "text", "default": "" },
//!   { "key": "rows", "label": "Rows", "type": "range", "min": 1, "max": 10, "step": 1, "default": 3 }
//! ]
//! ```
//!
//! The four types are the four controls the page has: toggle, text, select and range. A field of
//! another type is reported and left out, so the page never draws a control it cannot use. A
//! schema is kept in the plugin's own settings file under [`SCHEMA_KEY`], so the page can draw it
//! after a restart without running the plugin first.

use std::path::Path;

use serde_json::{json, Map, Value as Json};
use stremio_lightning_core::settings;

/// The settings key a registered schema is stored under. No plugin field may use it.
pub const SCHEMA_KEY: &str = "__schema";

/// One control on the Plugin settings page.
#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    Toggle {
        key: String,
        label: String,
        default: bool,
    },
    Text {
        key: String,
        label: String,
        default: String,
    },
    Select {
        key: String,
        label: String,
        options: Vec<String>,
        default: String,
    },
    Range {
        key: String,
        label: String,
        min: f64,
        max: f64,
        step: f64,
        default: f64,
    },
}

impl Field {
    pub fn key(&self) -> &str {
        match self {
            Self::Toggle { key, .. }
            | Self::Text { key, .. }
            | Self::Select { key, .. }
            | Self::Range { key, .. } => key,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Toggle { label, .. }
            | Self::Text { label, .. }
            | Self::Select { label, .. }
            | Self::Range { label, .. } => label,
        }
    }

    /// The value this field holds before the plugin or the user has saved one.
    pub fn default(&self) -> Json {
        match self {
            Self::Toggle { default, .. } => json!(default),
            Self::Text { default, .. } => json!(default),
            Self::Select { default, .. } => json!(default),
            Self::Range { default, .. } => json!(default),
        }
    }

    /// Whether a stored value is something this field can hold. A value of the wrong type is
    /// treated as the default rather than drawn in a control that would change it by being seen.
    pub fn accepts(&self, value: &Json) -> bool {
        match (self, value) {
            (Self::Toggle { .. }, Json::Bool(_)) | (Self::Range { .. }, Json::Number(_)) => true,
            (Self::Text { .. }, Json::String(_)) => true,
            (Self::Select { options, .. }, Json::String(text)) => options.contains(text),
            _ => false,
        }
    }

    /// The value a control reports, made into something this field can hold. A slider reports a
    /// number outside its range or between its steps; that number is brought inside the range and
    /// onto a step. A value that is not of the field's own kind at all becomes the default.
    pub fn conform(&self, value: &Json) -> Json {
        match (self, value) {
            (Self::Range { min, max, step, .. }, Json::Number(number)) => {
                let step = if *step > 0.0 { *step } else { 1.0 };
                let number = number.as_f64().unwrap_or(*min).clamp(*min, *max);
                let number = min + ((number - min) / step).round() * step;
                // A slider set in whole numbers stays in whole numbers: the third position of a
                // 1-to-10 slider is "3", not "3.0000000001".
                let number = if min.fract() == 0.0 && step.fract() == 0.0 {
                    number.round()
                } else {
                    number
                };
                match serde_json::Number::from_f64(number) {
                    Some(number) => Json::Number(number),
                    None => self.default(),
                }
            }
            (Self::Toggle { .. }, Json::Number(number)) => Json::Bool(number.as_f64() != Some(0.0)),
            (_, other) if self.accepts(other) => other.clone(),
            (field, _) => field.default(),
        }
    }
}

/// Reads a registered schema. `Err` says why the schema cannot be used at all; a field inside a
/// usable schema that could not be read is left out, and collecting those is the caller's job.
pub fn parse(schema: &Json) -> Result<Vec<Field>, String> {
    let entries = match schema {
        Json::Array(entries) => entries,
        Json::Null => return Ok(Vec::new()),
        other => {
            return Err(format!(
                "registerSettings wants a list of fields, and was given {}.",
                type_name(other)
            ))
        }
    };
    let mut fields: Vec<Field> = Vec::new();
    for entry in entries {
        let Some(object) = entry.as_object() else {
            continue;
        };
        let Some(key) = string(object, "key").filter(|key| !key.is_empty()) else {
            continue;
        };
        if key == SCHEMA_KEY || fields.iter().any(|field| field.key() == key) {
            continue;
        }
        if let Some(field) = field(object, &key) {
            fields.push(field);
        }
    }
    Ok(fields)
}

fn field(object: &Map<String, Json>, key: &str) -> Option<Field> {
    let label = string(object, "label").unwrap_or_else(|| key.to_owned());
    let kind = string(object, "type")?;
    let default = object.get("default").cloned().unwrap_or(Json::Null);
    let (key, label) = (key.to_owned(), label);
    Some(match kind.as_str() {
        "toggle" => Field::Toggle {
            key,
            label,
            default: default.as_bool().unwrap_or(false),
        },
        "text" => Field::Text {
            key,
            label,
            default: default.as_str().unwrap_or_default().to_owned(),
        },
        "select" => {
            let options: Vec<String> = object
                .get("options")
                .and_then(Json::as_array)
                .map(|options| {
                    options
                        .iter()
                        .filter_map(|option| option.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default();
            if options.is_empty() {
                return None;
            }
            let default = default
                .as_str()
                .filter(|value| options.iter().any(|option| option == value))
                .unwrap_or(&options[0])
                .to_owned();
            Field::Select {
                key,
                label,
                options,
                default,
            }
        }
        "range" => {
            let min = number(object, "min").unwrap_or(0.0);
            let max = number(object, "max").unwrap_or(min + 10.0);
            if max <= min {
                return None;
            }
            let step = number(object, "step")
                .filter(|step| *step > 0.0)
                .unwrap_or(1.0);
            let default = number(object, "default").unwrap_or(min).clamp(min, max);
            Field::Range {
                key,
                label,
                min,
                max,
                step,
                default,
            }
        }
        _ => return None,
    })
}

fn string(object: &Map<String, Json>, key: &str) -> Option<String> {
    object.get(key).and_then(Json::as_str).map(str::to_owned)
}

fn number(object: &Map<String, Json>, key: &str) -> Option<f64> {
    object.get(key).and_then(Json::as_f64)
}

fn type_name(value: &Json) -> &'static str {
    match value {
        Json::Null => "nothing",
        Json::Bool(_) => "a true or false value",
        Json::Number(_) => "a number",
        Json::String(_) => "a string",
        Json::Array(_) => "a list",
        Json::Object(_) => "an object",
    }
}

/// The schema as the plugin's settings file holds it: the same shape `registerSettings` was given,
/// with the fields this app cannot draw already left out.
pub fn fields_json(fields: &[Field]) -> Json {
    Json::Array(fields.iter().map(field_json).collect())
}

fn field_json(field: &Field) -> Json {
    match field {
        Field::Toggle {
            key,
            label,
            default,
        } => {
            json!({ "key": key, "label": label, "type": "toggle", "default": default })
        }
        Field::Text {
            key,
            label,
            default,
        } => {
            json!({ "key": key, "label": label, "type": "text", "default": default })
        }
        Field::Select {
            key,
            label,
            options,
            default,
        } => json!({
            "key": key, "label": label, "type": "select", "options": options, "default": default
        }),
        Field::Range {
            key,
            label,
            min,
            max,
            step,
            default,
        } => json!({
            "key": key, "label": label, "type": "range",
            "min": min, "max": max, "step": step, "default": default
        }),
    }
}

/// The schema stored for the plugin whose settings file is `plugins_dir/<name>.plugin.json`.
pub fn stored(plugins_dir: &Path, name: &str) -> Result<Vec<Field>, String> {
    let schema = settings::setting(plugins_dir, name, SCHEMA_KEY)?;
    parse(&schema)
}

/// What a field says it is, for the one line the page puts under a control.
pub fn hint(field: &Field) -> String {
    match field {
        Field::Toggle { .. } => "On or off.".to_owned(),
        Field::Text { .. } => "Any text.".to_owned(),
        Field::Select { options, .. } => {
            format!("One of {}.", options.join(", "))
        }
        Field::Range { min, max, step, .. } => {
            if *step == 1.0 {
                format!("A whole number from {min} to {max}.")
            } else {
                format!("A number from {min} to {max}, in steps of {step}.")
            }
        }
    }
}
