//! What is installed, as the Mods pages list it: built-in plugins and local `.theme.css`
//! files.
//!
//! Theme files are read through `stremio_lightning_core::mods`. A file whose metadata comment is
//! missing or incomplete cannot be named, so its card shows the file name instead.

use std::path::Path;

use stremio_lightning_core::mods::{list_mods, read_mod_content, ModType};

use crate::plumbing::theme;
use crate::screens::settings::LocalExtras;

/// Which of the two Mods lists to show.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Plugin,
    Theme,
}

impl Kind {
    pub(crate) fn mod_type(self) -> ModType {
        match self {
            Self::Plugin => ModType::Plugin,
            Self::Theme => ModType::Theme,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Plugin => "plugin",
            Self::Theme => "theme",
        }
    }
}

/// One built-in plugin or local theme, as one card on its page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModCard {
    /// The key the card's buttons name it by: the built-in id or theme file name.
    pub id: String,
    pub filename: String,
    pub name: String,
    pub author: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
    /// Whether the built-in declares a settings page. Always false for a theme.
    pub has_settings: bool,
    /// What reading the theme file said it does not understand, one line each.
    pub warnings: Vec<String>,
    /// Whether the card has a Remove button. A feature built into the app has none: there is no
    /// file behind it.
    pub removable: bool,
    /// Whether this card is a feature the app ships rather than a file in the folder. A built-in
    /// has no author and nothing to remove: its card shows its name, its version and what it
    /// does, and nothing else. See `mods::builtin` for the list.
    pub builtin: bool,
}

/// One line a Mods page shows about the last action. `failed` draws it in the warning colour.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Notice {
    pub text: String,
    pub failed: bool,
}

impl Notice {
    pub(crate) fn said(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            failed: false,
        }
    }

    pub(crate) fn failed(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            failed: true,
        }
    }

    /// A folder that could not be read: every card is missing until it can.
    pub fn unreadable(text: impl Into<String>) -> Self {
        Self::failed(text)
    }
}
/// Built-in plugins or local `.theme.css` files under `data_dir`, sorted by file name, with
/// the enabled state applied.
pub fn cards(data_dir: &Path, kind: Kind, extras: &LocalExtras) -> Result<Vec<ModCard>, String> {
    if kind == Kind::Plugin {
        return Ok(super::builtin::cards(extras));
    }
    let mut installed = list_mods(data_dir, ModType::Theme)?;
    installed.sort_by(|a, b| a.filename.cmp(&b.filename));
    Ok(installed
        .into_iter()
        .map(|file| {
            let is_on = extras.enabled_themes.contains(&file.filename);
            let warnings = read_mod_content(data_dir, &file.filename, ModType::Theme)
                .map(|css| theme::warnings(&css))
                .unwrap_or_else(|error| vec![error]);
            let (name, author, version, description) = match file.metadata {
                Some(metadata) => (
                    non_empty_or(metadata.name, &file.filename),
                    metadata.author,
                    metadata.version,
                    metadata.description,
                ),
                None => (
                    file.filename.clone(),
                    String::new(),
                    String::new(),
                    String::new(),
                ),
            };
            ModCard {
                id: file.filename.clone(),
                filename: file.filename,
                name,
                author,
                version,
                description,
                enabled: is_on,
                has_settings: false,
                warnings,
                removable: true,
                builtin: false,
            }
        })
        .collect())
}

/// A name the file left empty is no name, so the file name stands in for it.
fn non_empty_or(value: String, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_owned()
    } else {
        value
    }
}

/// The card for `filename` on this page, if the page lists it.
pub fn card<'a>(cards: &'a [ModCard], filename: &str) -> Option<&'a ModCard> {
    cards.iter().find(|card| card.filename == filename)
}

/// What a card's title line says: the name the file gives, which is the file name when it gives
/// none.
pub fn title(card: &ModCard) -> String {
    if card.name.is_empty() {
        card.filename.clone()
    } else {
        card.name.clone()
    }
}

/// What a card says under the title: the author and the version, each left out when the file does
/// not name it.
pub fn byline(card: &ModCard) -> String {
    let version = if card.version.is_empty() {
        String::new()
    } else if card.version.starts_with('v') {
        card.version.clone()
    } else {
        format!("v{}", card.version)
    };
    match (card.author.is_empty(), version.is_empty()) {
        (true, true) => String::new(),
        (true, false) => version,
        (false, true) => card.author.clone(),
        (false, false) => format!("{} {}", card.author, version),
    }
}

/// What a card says when its file carries no metadata comment. Empty for a file that has one.
pub fn unreadable(card: &ModCard) -> String {
    if !metadata_missing(card) {
        return String::new();
    }
    "This file carries no readable metadata comment, so its name is its file name.".to_string()
}

/// Whether the card's file named itself. `cards` gives a file without metadata its own file name
/// as the name, so the two matching with no author or version is what says the metadata was
/// missing.
fn metadata_missing(card: &ModCard) -> bool {
    card.name == card.filename && card.author.is_empty() && card.version.is_empty()
}

/// Removes one mod file. The notice says either that the file is gone or why it is still there.
pub(crate) fn remove(data_dir: &Path, kind: Kind, filename: &str) -> Notice {
    match stremio_lightning_core::mods::delete_mod(data_dir, filename, kind.mod_type()) {
        Ok(()) => Notice::said(format!("Removed {filename}.")),
        Err(error) => Notice::failed(format!(
            "Could not remove {filename}: {error} Delete it from the {} folder yourself.",
            kind.label()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(name: &str, author: &str, version: &str) -> ModCard {
        ModCard {
            id: "example.theme.css".to_owned(),
            filename: "example.theme.css".to_owned(),
            name: name.to_owned(),
            author: author.to_owned(),
            version: version.to_owned(),
            ..ModCard::default()
        }
    }

    #[test]
    fn a_card_is_titled_by_its_metadata_or_by_its_file_name() {
        assert_eq!(title(&card("Example", "Alice", "1.0.0")), "Example");
        assert_eq!(title(&card("", "", "")), "example.theme.css");
    }

    #[test]
    fn the_byline_leaves_out_what_the_file_does_not_say() {
        assert_eq!(byline(&card("Example", "Alice", "1.0.0")), "Alice v1.0.0");
        // A version the file writes with its own v is not given a second one.
        assert_eq!(byline(&card("Example", "Alice", "v2.1")), "Alice v2.1");
        assert_eq!(byline(&card("Example", "", "1.0.0")), "v1.0.0");
        assert_eq!(byline(&card("Example", "Alice", "")), "Alice");
        assert_eq!(byline(&card("Example", "", "")), "");
    }

    #[test]
    fn a_file_with_no_metadata_says_so() {
        let mut file = card("", "", "");
        file.name = file.filename.clone();
        assert!(!unreadable(&file).is_empty());
        assert!(unreadable(&card("Example", "Alice", "1.0.0")).is_empty());
    }
}
