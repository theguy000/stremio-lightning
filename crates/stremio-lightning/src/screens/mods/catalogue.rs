//! Pure Mods catalogue types and projections for built-in plugins and pre-loaded `.theme.css`
//! files. Zero filesystem I/O.

use crate::plumbing::local_settings::LocalExtras;
use crate::plumbing::theme::{self, store::InstalledTheme};

pub const NONE_ON: &str = "No theme is on, so the app draws its own colours.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Plugin,
    Theme,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModCard {
    pub id: String,
    pub filename: String,
    pub name: String,
    pub author: String,
    pub version: String,
    pub description: String,
    pub enabled: bool,
    pub has_settings: bool,
    pub warnings: Vec<String>,
    pub removable: bool,
    pub builtin: bool,
}

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

    pub fn unreadable(text: impl Into<String>) -> Self {
        Self::failed(text)
    }
}

/// Projects built-in themes + pre-loaded disk `.theme.css` files into `ModCard`s.
pub fn theme_cards(installed: &[InstalledTheme], extras: &LocalExtras) -> Vec<ModCard> {
    let mut list = super::builtin::theme_cards(extras);
    list.extend(installed.iter().map(|file| {
        let is_on = extras.enabled_themes.contains(&file.filename);
        let warnings = theme::warnings(&file.css);
        let (name, author, version, description) = match &file.metadata {
            Some(m) => (
                non_empty_or(&m.name, &file.filename),
                m.author.clone(),
                m.version.clone(),
                m.description.clone(),
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
            filename: file.filename.clone(),
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
    }));
    list
}

fn non_empty_or(value: &str, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_owned()
    } else {
        value.to_owned()
    }
}

pub fn themes_note(any_on: bool) -> String {
    if any_on {
        "This theme is on.".to_owned()
    } else {
        NONE_ON.to_owned()
    }
}

/// Switches a theme on (exclusively) or off in `extras`. Returns `None` if already in that state.
pub fn set_theme_enabled(
    extras: &mut LocalExtras,
    filename: &str,
    enabled: bool,
) -> Option<Notice> {
    let list = &mut extras.enabled_themes;
    if enabled {
        if list.as_slice() == [filename.to_owned()] {
            return None;
        }
        *list = vec![filename.to_owned()];
        Some(Notice::said(format!(
            "{filename} is now the theme this app draws with."
        )))
    } else {
        if !list.iter().any(|name| name == filename) {
            return None;
        }
        list.clear();
        Some(Notice::said(format!(
            "{filename} is off, so the app draws its own colours."
        )))
    }
}

/// Unified built-in non-removable error notice for plugins and built-in themes.
pub fn builtin_not_removable(name: &str) -> Notice {
    Notice::failed(format!(
        "{name} is built into the app, so there is no file to remove."
    ))
}

pub fn non_removable_plugin_notice(id: &str) -> Notice {
    match super::builtin::get(id) {
        Some(b) => builtin_not_removable(b.name),
        None => Notice::failed(format!("{id} is not a built-in plugin.")),
    }
}

/// Formats the result of deleting a theme file from disk (updating `extras.enabled_themes` on ok).
pub fn finish_theme_remove(
    extras: &mut LocalExtras,
    filename: &str,
    delete_result: Result<(), String>,
) -> Notice {
    match delete_result {
        Ok(()) => {
            extras.enabled_themes.retain(|name| name != filename);
            Notice::said(format!("Removed {filename}."))
        }
        Err(error) => Notice::failed(format!(
            "Could not remove {filename}: {error} Delete it from the theme folder yourself."
        )),
    }
}

pub fn title(card: &ModCard) -> String {
    if card.name.is_empty() {
        card.filename.clone()
    } else {
        card.name.clone()
    }
}

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

pub fn unreadable(card: &ModCard) -> String {
    if card.name == card.filename && card.author.is_empty() && card.version.is_empty() {
        "This file carries no readable metadata comment, so its name is its file name.".to_owned()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(name: &str, author: &str, version: &str) -> ModCard {
        ModCard {
            id: "example.theme.css".into(),
            filename: "example.theme.css".into(),
            name: name.into(),
            author: author.into(),
            version: version.into(),
            ..ModCard::default()
        }
    }

    #[test]
    fn card_title_byline_and_unreadable_formatting() {
        assert_eq!(title(&mk("Example", "Alice", "1.0.0")), "Example");
        assert_eq!(title(&mk("", "", "")), "example.theme.css");
        assert_eq!(byline(&mk("Example", "Alice", "1.0.0")), "Alice v1.0.0");
        assert_eq!(byline(&mk("Example", "Alice", "v2.1")), "Alice v2.1");
        assert_eq!(byline(&mk("Example", "", "1.0.0")), "v1.0.0");
        assert_eq!(byline(&mk("Example", "Alice", "")), "Alice");
        assert_eq!(byline(&mk("Example", "", "")), "");
        let mut missing = mk("", "", "");
        missing.name = missing.filename.clone();
        assert!(!unreadable(&missing).is_empty());
        assert!(unreadable(&mk("Example", "Alice", "1.0.0")).is_empty());
    }
}
