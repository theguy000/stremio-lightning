//! What is installed, as the Mods pages list it: one card per plugin file or theme file.
//!
//! The reading is `stremio_lightning_core::mods`, which owns both mod folders. A file whose
//! metadata comment is missing or incomplete cannot be named, so its card shows the file name
//! instead.

use std::path::Path;

use stremio_lightning_core::mods::{list_mods, read_mod_content, ModType};

use crate::plumbing::theme;
use crate::screens::settings::LocalExtras;

/// Which of the two folders to list.
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

/// One plugin or theme, as one card on its page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModCard {
    /// The key the card's buttons name it by: its file name, which is what a mod file is named by
    /// everywhere else in the app.
    pub id: String,
    pub filename: String,
    pub name: String,
    pub author: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
    /// Whether the file carries a settings page. Plugin settings arrive with the plugin runtime,
    /// so every card says no for now.
    pub has_settings: bool,
    /// What reading the file said it does not understand, one line each. Themes fill this in;
    /// plugins are not read yet.
    pub warnings: Vec<String>,
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
/// Every plugin or theme installed under `data_dir`, sorted by file name, with the enabled list
/// applied. A file that will not read still gets a card: the folder is the answer, not the file.
pub fn cards(data_dir: &Path, kind: Kind, extras: &LocalExtras) -> Result<Vec<ModCard>, String> {
    let enabled = match kind {
        Kind::Plugin => &extras.enabled_plugins,
        Kind::Theme => &extras.enabled_themes,
    };
    let mut installed = list_mods(data_dir, kind.mod_type())?;
    installed.sort_by(|a, b| a.filename.cmp(&b.filename));
    Ok(installed
        .into_iter()
        .map(|file| {
            let is_on = enabled.contains(&file.filename);
            // A theme's card can say what the file does, so the file is read here. A plugin is not
            // read at all: nothing loads it in this build.
            let warnings = match kind {
                Kind::Theme => read_mod_content(data_dir, &file.filename, ModType::Theme)
                    .map(|css| theme::warnings(&css))
                    .unwrap_or_else(|error| vec![error]),
                Kind::Plugin => Vec::new(),
            };
            let (name, author, version, description) = match file.metadata {
                Some(metadata) => (
                    non_empty_or(metadata.name, &file.filename),
                    metadata.author,
                    metadata.version,
                    metadata.description,
                ),
                // Nothing in the file names it. The file name is what the user sees in the folder,
                // so that is what the card shows, and the rest of the card says nothing.
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
            id: "example.plugin.js".to_owned(),
            filename: "example.plugin.js".to_owned(),
            name: name.to_owned(),
            author: author.to_owned(),
            version: version.to_owned(),
            ..ModCard::default()
        }
    }

    #[test]
    fn a_card_is_titled_by_its_metadata_or_by_its_file_name() {
        assert_eq!(title(&card("Example", "Alice", "1.0.0")), "Example");
        assert_eq!(title(&card("", "", "")), "example.plugin.js");
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
