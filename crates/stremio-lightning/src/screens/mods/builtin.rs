//! Compiled-in plugins and built-in themes (AniSkip, Amoled), their declared settings schema,
//! and pure validation/toggle rules. Zero disk I/O and zero OS entropy calls.

use serde_json::{json, Value as Json};

use super::catalogue::ModCard;
use crate::plumbing::local_settings::LocalExtras;

pub struct Builtin {
    pub id: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub description: &'static str,
    pub switch: Switch,
    pub fields: &'static [Field],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Switch {
    SkipIntros,
}

impl Switch {
    #[must_use]
    pub fn reads(self, extras: &LocalExtras) -> bool {
        match self {
            Self::SkipIntros => extras.skip_intros,
        }
    }

    pub fn writes(self, extras: &mut LocalExtras, enabled: bool) {
        match self {
            Self::SkipIntros => extras.skip_intros = enabled,
        }
    }
}

/// Validation policy declared alongside each built-in `Field` so `mods/settings.rs` stays generic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Validation {
    None,
    RequiredUuid,
    NonEmptyDefault(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub kind: FieldKind,
    pub validation: Validation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Text { default: &'static str },
    Toggle { default: bool },
}

impl Field {
    #[must_use]
    pub fn default_value(self) -> Json {
        match self.kind {
            FieldKind::Text { default } => json!(default),
            FieldKind::Toggle { default } => json!(default),
        }
    }

    #[must_use]
    pub fn accepts(self, value: &Json) -> bool {
        matches!(
            (self.kind, value),
            (FieldKind::Text { .. }, Json::String(_)) | (FieldKind::Toggle { .. }, Json::Bool(_))
        )
    }

    /// Coerces an incoming UI value to this field's type (falling back to default if mismatched).
    #[must_use]
    pub fn conform(self, value: &Json) -> Json {
        match (self.kind, value) {
            (FieldKind::Toggle { .. }, Json::Bool(b)) => json!(b),
            (FieldKind::Text { .. }, Json::String(s)) => json!(s),
            _ => self.default_value(),
        }
    }

    #[must_use]
    pub fn default_hint(self) -> &'static str {
        match self.kind {
            FieldKind::Toggle { .. } => "On or off.",
            FieldKind::Text { .. } => "Text.",
        }
    }

    /// Validates and normalizes an edited value before saving into `LocalExtras`.
    pub fn validate(self, value: &Json) -> Result<Json, String> {
        match self.validation {
            Validation::None => Ok(value.clone()),
            Validation::RequiredUuid => {
                let text = value.as_str().map(str::trim).unwrap_or_default();
                if is_uuid(text) {
                    Ok(json!(text))
                } else {
                    Err(
                        "Submitter ID is required: it must be a UUID. Nothing was saved."
                            .to_owned(),
                    )
                }
            }
            Validation::NonEmptyDefault(fallback) => {
                let text = value.as_str().map(str::trim).unwrap_or_default();
                Ok(json!(if text.is_empty() { fallback } else { text }))
            }
        }
    }
}

pub const ALL: &[Builtin] = &[Builtin {
    id: "builtin:aniskip",
    name: "AniSkip",
    version: env!("CARGO_PKG_VERSION"),
    description: "Skip anime openings, endings and recaps, read from AniSkip v2.",
    switch: Switch::SkipIntros,
    fields: &[
        Field {
            key: "provider_name",
            label: "Submitter name",
            hint: "The name your submission is shown under on AniSkip.",
            kind: FieldKind::Text { default: "Stremio Lightning" },
            validation: Validation::NonEmptyDefault("Stremio Lightning"),
        },
        Field {
            key: "submitter_id",
            label: "Submitter ID",
            hint: "A UUID. Leave it empty to make one on the first submission, or paste the one you already use to keep your votes together.",
            kind: FieldKind::Text { default: "" },
            validation: Validation::RequiredUuid,
        },
        Field {
            key: "submit_button",
            label: "Submit button in the player",
            hint: "Draws a button beside the skip disc so a wrong or missing skip time can be sent back.",
            kind: FieldKind::Toggle { default: true },
            validation: Validation::None,
        },
    ],
}];

#[must_use]
pub fn get(id: &str) -> Option<&'static Builtin> {
    ALL.iter().find(|b| b.id == id)
}

#[must_use]
pub fn fields(id: &str) -> &'static [Field] {
    get(id).map(|b| b.fields).unwrap_or(&[])
}

/// Toggles a built-in plugin switch in `extras`; returns whether the state changed.
pub fn set_enabled(extras: &mut LocalExtras, id: &str, enabled: bool) -> bool {
    let Some(builtin) = get(id) else {
        return false;
    };
    if builtin.switch.reads(extras) == enabled {
        return false;
    }
    builtin.switch.writes(extras, enabled);
    true
}

#[must_use]
pub fn value(extras: &LocalExtras, id: &str, field: &Field) -> Json {
    extras
        .builtin_settings
        .get(id)
        .and_then(|values| values.get(field.key))
        .filter(|value| field.accepts(value))
        .cloned()
        .unwrap_or_else(|| field.default_value())
}

pub fn set_value(extras: &mut LocalExtras, id: &str, key: &str, value: Json) {
    if !fields(id).iter().any(|field| field.key == key) {
        return;
    }
    extras
        .builtin_settings
        .entry(id.to_owned())
        .or_default()
        .insert(key.to_owned(), value);
}

#[must_use]
pub fn submitter(extras: &LocalExtras) -> Option<String> {
    let field = fields("builtin:aniskip")
        .iter()
        .find(|f| f.key == "submitter_id")?;
    value(extras, "builtin:aniskip", field)
        .as_str()
        .map(str::trim)
        .filter(|text| is_uuid(text))
        .map(str::to_owned)
}

/// Ensures AniSkip has a valid UUID submitter id, storing `fresh_id` if none is saved yet.
pub fn ensure_submitter(extras: &mut LocalExtras, fresh_id: &str) -> bool {
    if submitter(extras).is_some() || !is_uuid(fresh_id) {
        return false;
    }
    set_value(extras, "builtin:aniskip", "submitter_id", json!(fresh_id));
    true
}

#[must_use]
pub fn is_uuid(text: &str) -> bool {
    text.len() == 36
        && text.chars().enumerate().all(|(at, c)| match at {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

#[must_use]
pub fn cards(extras: &LocalExtras) -> Vec<ModCard> {
    ALL.iter()
        .map(|b| ModCard {
            id: b.id.to_owned(),
            filename: b.id.to_owned(),
            name: b.name.to_owned(),
            author: String::new(),
            version: b.version.to_owned(),
            description: b.description.to_owned(),
            enabled: b.switch.reads(extras),
            has_settings: !b.fields.is_empty(),
            warnings: Vec::new(),
            removable: false,
            builtin: true,
        })
        .collect()
}

pub struct BuiltinTheme {
    pub id: &'static str,
    pub name: &'static str,
    pub version: &'static str,
    pub description: &'static str,
}

pub const THEMES: &[BuiltinTheme] = &[BuiltinTheme {
    id: "builtin:amoled",
    name: "Amoled Theme",
    version: env!("CARGO_PKG_VERSION"),
    description: "A theme that uses amoled pitch black color.",
}];

#[must_use]
pub fn get_theme(id: &str) -> Option<&'static BuiltinTheme> {
    THEMES.iter().find(|t| t.id == id)
}

#[must_use]
pub fn theme_cards(extras: &LocalExtras) -> Vec<ModCard> {
    THEMES
        .iter()
        .map(|t| ModCard {
            id: t.id.to_owned(),
            filename: t.id.to_owned(),
            name: t.name.to_owned(),
            author: String::new(),
            version: t.version.to_owned(),
            description: t.description.to_owned(),
            enabled: extras.enabled_themes.iter().any(|name| name == t.id),
            has_settings: false,
            warnings: Vec::new(),
            removable: false,
            builtin: true,
        })
        .collect()
}

#[cfg(test)]
#[path = "builtin_tests.rs"]
mod tests;
