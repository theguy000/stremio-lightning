//! The features the app ships, listed on the Plugins page beside the installed files.
//!
//! A built-in is not a file: nothing installs, updates or removes it, and it has no author to
//! name. Its card shows three things and nothing else -- a name, a version and one line about
//! what it does -- so a new built-in is a row in `ALL`, not a change to the page.
//!
//! A built-in may also declare settings. `fields` is that contract: a key, a label and the kind
//! of control. The Plugin settings page draws any built-in from its own list, and the values are
//! kept in the shell's `settings.json` under the built-in's id, because there is no file of its
//! own to keep them in. A fourth kind of control would be a change to the page's own contract,
//! not a field to sneak in.
//!
//! The version is the app's version: a built-in ships with the app.

use serde_json::{json, Value as Json};

use super::catalogue::ModCard;
use crate::screens::settings::LocalExtras;

/// One feature the app ships.
pub struct Builtin {
    /// What the card's switch names it by. Never a file name, and never one a file could take:
    /// the prefix keeps the two apart.
    pub id: &'static str,
    /// What the card's title says.
    pub name: &'static str,
    /// The app's version.
    pub version: &'static str,
    /// One line about what the feature does.
    pub description: &'static str,
    /// Where the feature keeps the switch the page draws.
    pub switch: Switch,
    /// What its settings page draws, in order. Empty when it has none, which draws no gear on its
    /// card and no page behind it.
    pub fields: &'static [Field],
}

/// Where a built-in keeps its on/off switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Switch {
    /// `skip_intros` in the shell's own `settings.json`: AniSkip's on/off.
    SkipIntros,
}

impl Switch {
    /// Whether the feature is on.
    #[must_use]
    pub fn reads(self, extras: &LocalExtras) -> bool {
        match self {
            Self::SkipIntros => extras.skip_intros,
        }
    }

    /// Turns the feature on or off, in the shell's own settings.
    pub fn writes(self, extras: &mut LocalExtras, enabled: bool) {
        match self {
            Self::SkipIntros => extras.skip_intros = enabled,
        }
    }
}

/// One control a built-in's settings page draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Field {
    /// What the value is stored under, beside the built-in's id.
    pub key: &'static str,
    pub label: &'static str,
    /// A sentence under the control: what the value means, or what leaving it empty does.
    pub hint: &'static str,
    pub kind: FieldKind,
}

/// The kinds of control a built-in may ask for. The settings page already draws these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Text { default: &'static str },
    Toggle { default: bool },
}

impl Field {
    /// What the control holds before the user has saved a value.
    #[must_use]
    pub fn default_value(self) -> Json {
        match self.kind {
            FieldKind::Text { default } => json!(default),
            FieldKind::Toggle { default } => json!(default),
        }
    }

    /// Whether a stored value is something this control can hold.
    #[must_use]
    pub fn accepts(self, value: &Json) -> bool {
        match (self.kind, value) {
            (FieldKind::Text { .. }, Json::String(_))
            | (FieldKind::Toggle { .. }, Json::Bool(_)) => true,
            _ => false,
        }
    }
}

/// Every built-in, in the order the Plugins page lists them.
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
            kind: FieldKind::Text {
                default: "Stremio Lightning",
            },
        },
        Field {
            key: "submitter_id",
            label: "Submitter ID",
            hint: "A UUID. Leave it empty to make one on the first submission, or paste the one you already use to keep your votes together.",
            kind: FieldKind::Text { default: "" },
        },
        Field {
            key: "submit_button",
            label: "Submit button in the player",
            hint: "Draws a button beside the skip disc so a wrong or missing skip time can be sent back.",
            kind: FieldKind::Toggle { default: true },
        },
    ],
}];

/// The built-in `id` names, if there is one.
#[must_use]
pub fn get(id: &str) -> Option<&'static Builtin> {
    ALL.iter().find(|builtin| builtin.id == id)
}

/// The fields `id` declares, empty when it is not a built-in or declares none.
#[must_use]
pub fn fields(id: &str) -> &'static [Field] {
    get(id).map(|builtin| builtin.fields).unwrap_or(&[])
}

/// What the control `field` of `id` holds, or its default when nothing is saved or the saved
/// value is of the wrong kind.
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

/// Writes one value, only when the field `key` names exists on `id`.
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

/// The submitter id a submission should carry: the saved one, or a new UUID when none is saved.
/// A malformed saved value is a new UUID too, and the caller is told to save what it got.
#[must_use]
pub fn submitter(extras: &LocalExtras) -> Option<String> {
    let field = Field {
        key: "submitter_id",
        label: "Submitter ID",
        hint: "",
        kind: FieldKind::Text { default: "" },
    };
    let saved = value(extras, "builtin:aniskip", &field);
    saved
        .as_str()
        .map(str::trim)
        .filter(|text| is_uuid(text))
        .map(str::to_owned)
}

/// A fresh UUID v4, in the form the API wants for `submitterId`.
#[must_use]
pub fn new_submitter() -> String {
    let mut bytes = [0u8; 16];
    if getrandom::fill(&mut bytes).is_err() {
        return String::new();
    }
    // Version 4, variant 1: the two nibbles the UUID format reserves.
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        bytes[8], bytes[9], bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

/// Whether `text` is the canonical UUID form the API's `submitterId` field accepts.
#[must_use]
pub fn is_uuid(text: &str) -> bool {
    text.len() == 36
        && text.chars().enumerate().all(|(at, c)| match at {
            8 | 13 | 18 | 23 => c == '-',
            _ => c.is_ascii_hexdigit(),
        })
}

/// The cards the Plugins page shows for the built-ins. Every field a file-backed card fills from
/// its file -- author, warnings, update, unreadable -- stays empty here.
#[must_use]
pub fn cards(extras: &LocalExtras) -> Vec<ModCard> {
    ALL.iter()
        .map(|builtin| ModCard {
            id: builtin.id.to_owned(),
            // A built-in has no file. The id stands in wherever a file name would go, so a
            // switch press carries one name through the page.
            filename: builtin.id.to_owned(),
            name: builtin.name.to_owned(),
            author: String::new(),
            version: builtin.version.to_owned(),
            description: builtin.description.to_owned(),
            enabled: builtin.switch.reads(extras),
            has_settings: !builtin.fields.is_empty(),
            running: crate::screens::mods::settings::Running {
                running: true,
                stopped: String::new(),
                last_error: String::new(),
            },
            warnings: Vec::new(),
            update: String::new(),
            removable: false,
            builtin: true,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::mods::catalogue;

    #[test]
    fn a_built_in_card_shows_a_name_a_version_and_a_description_only() {
        let cards = cards(&LocalExtras::default());
        assert_eq!(cards.len(), 1);
        let card = &cards[0];
        assert_eq!(card.name, "AniSkip");
        assert_eq!(card.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(
            catalogue::byline(card),
            format!("v{}", env!("CARGO_PKG_VERSION"))
        );
        assert!(card.description.starts_with("Skip anime"));
        assert!(card.author.is_empty());
        assert!(card.warnings.is_empty());
        assert!(card.update.is_empty());
        assert!(catalogue::unreadable(card).is_empty());
        assert!(!card.removable);
        assert!(card.builtin);
        assert!(card.has_settings);
    }

    #[test]
    fn the_switch_is_the_shells_skip_intros() {
        let mut extras = LocalExtras::default();
        assert!(extras.skip_intros);
        Switch::SkipIntros.writes(&mut extras, false);
        assert!(!Switch::SkipIntros.reads(&extras));
        assert!(!cards(&extras)[0].enabled);
        Switch::SkipIntros.writes(&mut extras, true);
        assert!(cards(&extras)[0].enabled);
    }

    #[test]
    fn the_fields_start_at_their_defaults() {
        let extras = LocalExtras::default();
        for field in fields("builtin:aniskip") {
            assert_eq!(
                value(&extras, "builtin:aniskip", field),
                field.default_value()
            );
        }
    }

    #[test]
    fn a_saved_value_of_the_wrong_kind_is_the_default() {
        let mut extras = LocalExtras::default();
        set_value(
            &mut extras,
            "builtin:aniskip",
            "submit_button",
            json!("yes"),
        );
        let field = fields("builtin:aniskip")
            .iter()
            .find(|field| field.key == "submit_button")
            .unwrap();
        assert_eq!(value(&extras, "builtin:aniskip", field), json!(true));
        set_value(
            &mut extras,
            "builtin:aniskip",
            "submit_button",
            json!(false),
        );
        assert_eq!(value(&extras, "builtin:aniskip", field), json!(false));
    }

    #[test]
    fn a_new_submitter_is_a_uuid_and_the_saved_one_is_kept() {
        let fresh = new_submitter();
        assert!(is_uuid(&fresh), "{fresh}");
        let mut extras = LocalExtras::default();
        assert!(submitter(&extras).is_none());
        set_value(&mut extras, "builtin:aniskip", "submitter_id", json!(fresh));
        assert_eq!(submitter(&extras), Some(fresh));
        set_value(
            &mut extras,
            "builtin:aniskip",
            "submitter_id",
            json!("nope"),
        );
        assert!(submitter(&extras).is_none());
    }
}
