//! The features the app ships, listed on the Plugins page beside the installed files.
//!
//! A built-in is not a file: nothing installs, updates or removes it, and it has no author to
//! name. Its card shows three things and nothing else -- a name, a version and one line about
//! what it does -- so a new built-in is a row in `ALL`, not a change to the page.
//!
//! The version is the app's version: a built-in ships with the app. If a card ever needs a fourth
//! thing shown, that is a change to this contract, not a field to sneak in.

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

/// Every built-in, in the order the Plugins page lists them.
pub const ALL: &[Builtin] = &[Builtin {
    id: "builtin:aniskip",
    name: "AniSkip",
    version: env!("CARGO_PKG_VERSION"),
    description: "Skip anime openings, endings and recaps, read from AniSkip v2.",
    switch: Switch::SkipIntros,
}];

/// The built-in `id` names, if there is one.
#[must_use]
pub fn get(id: &str) -> Option<&'static Builtin> {
    ALL.iter().find(|builtin| builtin.id == id)
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
            has_settings: false,
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
}
