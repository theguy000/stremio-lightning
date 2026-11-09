//! The Marketplace page: what the registry offers, what is installed from it, and the presses that
//! install, uninstall and update.
//!
//! The registry is `stremio_lightning_core::mods::fetch_registry`, read by the shell off the
//! window's thread; this module never starts a thread and never touches a window. Everything that
//! reads the folder or the registry has a plain function that works on values already in hand
//! (`data`, `state`, `update_note`, `with_updates`, `wanted`), which is what the tests drive.
//!
//! What "installed" means is decided by the folder, not by the registry. A card is Installed when
//! a file in the Mods folder is the one the entry's download is stored as: the same file name
//! (`mods::installed_filename`), or a file whose own `@updateUrl` is that download. Matching the
//! entry's name and author against a file's metadata would also mark as installed a different mod
//! that happens to share them.

use std::collections::HashMap;
use std::path::Path;

use stremio_lightning_core::mods::{
    self, installed_filename, list_mods, same_mod_file, ModType, Registry, RegistryEntry,
    UpdateInfo,
};

use super::catalogue::{self, Kind, ModCard, Notice};
use crate::screens::settings::LocalExtras;

/// The sentence above the Marketplace list, which says what a press does and what it does not.
pub const NOTE: &str = "The registry lists plugins and themes anyone can publish. An installed mod is off until you turn it on in Plugins or Themes.";
/// What the page says for a registry that answered with nothing in it.
pub const EMPTY: &str = "The registry is reachable, but it lists nothing.";
/// What the search box shows when nothing matches what was typed.
pub const NO_MATCH: &str = "No mod in the registry matches that search.";
/// What an installed mod's card says when its version is the registry's.
pub const UP_TO_DATE: &str = "Up to date.";
/// What a card says before a check has answered for it.
pub const CHECKING: &str = "Checking for updates.";

/// The registry as the page last read it, or the line that says why it could not be read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Listing {
    pub plugins: Vec<RegistryEntry>,
    pub themes: Vec<RegistryEntry>,
    /// The line about a read that failed, empty when the last read answered.
    pub error: String,
    /// Whether a read has finished at all, which is what tells "not fetched yet" from "empty".
    pub loaded: bool,
}

impl Listing {
    pub fn from_registry(registry: Registry) -> Self {
        Self {
            plugins: registry.plugins,
            themes: registry.themes,
            error: String::new(),
            loaded: true,
        }
    }

    /// What a failed read leaves: the sentence to show, and nothing to list.
    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
            loaded: true,
            ..Self::default()
        }
    }

    pub fn entries(&self, kind: Kind) -> &[RegistryEntry] {
        match kind {
            Kind::Plugin => &self.plugins,
            Kind::Theme => &self.themes,
        }
    }

    /// The entries of `kind` whose name, author or description holds `query`, in the registry's
    /// own order. An empty query keeps every entry.
    pub fn matching(&self, kind: Kind, query: &str) -> Vec<RegistryEntry> {
        let query = query.trim().to_lowercase();
        self.entries(kind)
            .iter()
            .filter(|entry| {
                query.is_empty()
                    || entry.name.to_lowercase().contains(&query)
                    || entry.author.to_lowercase().contains(&query)
                    || entry
                        .description
                        .as_deref()
                        .is_some_and(|text| text.to_lowercase().contains(&query))
            })
            .cloned()
            .collect()
    }
}

/// The file the folder holds for `entry`, if it holds one.
#[must_use]
pub fn installed_file(data_dir: &Path, kind: Kind, entry: &RegistryEntry) -> Option<String> {
    let files = list_mods(data_dir, kind.mod_type()).ok()?;
    if let Some(file) = files
        .iter()
        .find(|file| same_mod_file(&file.filename, file.metadata.as_ref(), &entry.download))
    {
        return Some(file.filename.clone());
    }
    // A file saved under the name this entry would be installed as, whose metadata does not name
    // the download. It is the file this entry installed, because that is the name installing it
    // chooses; the name in its metadata is what keeps a different mod with the same file name out.
    let name = installed_filename(entry, kind.mod_type()).ok()?;
    files
        .iter()
        .find(|file| {
            file.filename == name
                && file
                    .metadata
                    .as_ref()
                    .is_some_and(|metadata| metadata.name == entry.name)
        })
        .map(|file| file.filename.clone())
}

/// One thing the registry offers, as one card on the Marketplace page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MarketCard {
    /// The entry's download, which is what a registry entry is named by everywhere here.
    pub id: String,
    /// Which page the mod belongs to: "plugin" or "theme".
    pub kind: String,
    pub name: String,
    pub author: String,
    pub version: String,
    pub description: String,
    /// The file it is installed as, empty when it is not installed.
    pub filename: String,
    /// The card's one button: "install", "uninstall", or "update" when a newer version is known.
    pub action: String,
}

impl MarketCard {
    fn new(kind: Kind, entry: &RegistryEntry) -> Self {
        Self {
            id: entry.download.clone(),
            kind: kind.mod_type().as_str().to_owned(),
            name: entry.name.clone(),
            author: entry.author.clone(),
            version: entry.version.clone(),
            description: entry.description.clone().unwrap_or_default(),
            filename: String::new(),
            action: "install".to_owned(),
        }
    }
}

/// The page: what the registry holds, what the listener typed, and what the presses said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Data {
    pub note: String,
    /// What the installs and updates said, one sentence a line, newest last.
    pub summary: Vec<Notice>,
    /// The download being fetched or written right now, empty when none is.
    pub installing: String,
    /// The line about a registry that could not be read, empty when the last read answered.
    pub trouble: String,
    pub plugins: Vec<MarketCard>,
    pub themes: Vec<MarketCard>,
    /// What the page shows in place of either list when there is nothing to show.
    pub empty: String,
}

impl Default for Data {
    fn default() -> Self {
        Self {
            note: NOTE.to_owned(),
            summary: Vec::new(),
            installing: String::new(),
            trouble: String::new(),
            plugins: Vec::new(),
            themes: Vec::new(),
            empty: String::new(),
        }
    }
}

impl Data {
    /// The page before the registry has been read.
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn cards(&self, kind: Kind) -> &[MarketCard] {
        match kind {
            Kind::Plugin => &self.plugins,
            Kind::Theme => &self.themes,
        }
    }

    /// The same page with a new line in its summary.
    #[must_use]
    pub fn with_summary(mut self, notice: Notice) -> Self {
        self.summary.push(notice);
        self
    }
}

/// The whole page, from the registry the shell last read and the folder as it is now.
#[must_use]
pub fn data(data_dir: &Path, listing: &Listing, search: &str, updates: &Updates) -> Data {
    // Each entry looks its file up in the folder and its update in the check's answer.
    let known = |kind: Kind| updates.of(kind);
    let card_of = |kind: Kind, entry: &RegistryEntry| -> MarketCard {
        let mut card = MarketCard::new(kind, entry);
        if let Some(file) = installed_file(data_dir, kind, entry) {
            card.filename = file.clone();
            // The update the check found for this file decides whether the one button installs,
            // updates or uninstalls.
            card.action = match known(kind).get(&file) {
                Some(info) if info.has_update && info.update_url.is_some() => "update",
                _ => "uninstall",
            }
            .to_owned();
        }
        card
    };
    let plugins: Vec<MarketCard> = listing
        .matching(Kind::Plugin, search)
        .iter()
        .map(|entry| card_of(Kind::Plugin, entry))
        .collect();
    let themes: Vec<MarketCard> = listing
        .matching(Kind::Theme, search)
        .iter()
        .map(|entry| card_of(Kind::Theme, entry))
        .collect();
    let empty = if !listing.loaded || !listing.error.is_empty() {
        String::new()
    } else if listing.plugins.is_empty() && listing.themes.is_empty() {
        EMPTY.to_owned()
    } else if plugins.is_empty() && themes.is_empty() {
        NO_MATCH.to_owned()
    } else {
        String::new()
    };
    Data {
        note: NOTE.to_owned(),
        summary: Vec::new(),
        installing: String::new(),
        trouble: listing.error.clone(),
        plugins,
        themes,
        empty,
    }
}

/// The registry read off the app's thread.
///
/// # Errors
/// Returns the sentence core gives for a registry that could not be fetched or parsed.
pub async fn fetch() -> Result<Registry, String> {
    mods::fetch_registry().await
}

/// Installs `entry` into `data_dir` and returns the file name it was saved as. Nothing is turned
/// on: a new mod is off until its switch is pressed.
///
/// # Errors
/// Returns the sentence for what went wrong: the download failed, the file was not a plugin or a
/// theme, or it could not be written.
pub async fn install(data_dir: &Path, kind: Kind, entry: &RegistryEntry) -> Result<String, String> {
    let filename = installed_filename(entry, kind.mod_type())?;
    mods::download_mod_as(data_dir, &entry.download, kind.mod_type(), Some(&filename)).await?;
    let content = mods::read_mod_content(data_dir, &filename, kind.mod_type())?;
    if !mods::looks_like(&content, kind.mod_type()) {
        // A page that answered where a mod should be is not left in the folder as a mod.
        let _ = mods::delete_mod(data_dir, &filename, kind.mod_type());
        return Err(mods::wrong_file_said(kind.mod_type()));
    }
    Ok(filename)
}

/// Writes `url` over the installed file `filename` and returns that name.
///
/// # Errors
/// Returns the sentence for what went wrong. A body that is not a mod is not written.
pub async fn update_file(
    data_dir: &Path,
    kind: Kind,
    filename: &str,
    url: &str,
) -> Result<String, String> {
    let content = download_bytes(url).await?;
    if !mods::looks_like(&content, kind.mod_type()) {
        return Err(format!(
            "{} The file on this computer was left as it was.",
            mods::wrong_file_said(kind.mod_type())
        ));
    }
    mods::write_mod_content(data_dir, filename, kind.mod_type(), content.as_bytes())?;
    Ok(filename.to_owned())
}

/// The body at `url`, read without being written anywhere, so an update can be checked before it
/// replaces a working file.
async fn download_bytes(url: &str) -> Result<String, String> {
    mods::fetch_text(url).await
}

/// What one check made of every installed mod of one kind.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Updates {
    plugins: HashMap<String, UpdateInfo>,
    themes: HashMap<String, UpdateInfo>,
}

impl Updates {
    /// The check for both pages, run off the window's thread.
    ///
    /// # Errors
    /// Returns the sentence for a folder that cannot be read. A registry that cannot be reached
    /// is not an error: core then checks each file against its own `updateUrl`.
    pub async fn check(data_dir: &Path) -> Result<Self, String> {
        let plugins = mods::check_mod_updates(data_dir, ModType::Plugin).await?;
        let themes = mods::check_mod_updates(data_dir, ModType::Theme).await?;
        Ok(Self { plugins, themes })
    }

    pub fn of(&self, kind: Kind) -> HashMap<String, UpdateInfo> {
        match kind {
            Kind::Plugin => self.plugins.clone(),
            Kind::Theme => self.themes.clone(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty() && self.themes.is_empty()
    }

    /// Whether at least one installed file has a newer version and an address to fetch it from.
    pub fn any_ready(&self) -> bool {
        self.plugins
            .values()
            .chain(self.themes.values())
            .any(|info| info.has_update && info.update_url.is_some())
    }
}

/// What an installed mod's card says about updates: "Up to date.", "Update to X", or a sentence
/// about a check that could not answer. A mod whose file carries no version has nothing to compare
/// and says so.
#[must_use]
pub fn state(kind: Kind, info: Option<&UpdateInfo>) -> String {
    let Some(info) = info else {
        return CHECKING.to_owned();
    };
    if info.installed_version.is_none() {
        return format!(
            "Could not check: this {}'s file carries no metadata comment, so its version is unknown.",
            kind.mod_type().as_str()
        );
    }
    if info.has_update {
        match &info.new_version {
            Some(version) => format!("Update to {version}"),
            None => "A newer version is ready.".to_owned(),
        }
    } else {
        UP_TO_DATE.to_owned()
    }
}

/// The Plugins and Themes cards with what the check made of each folded in, in the folder's order.
#[must_use]
pub fn with_updates(data_dir: &Path, kind: Kind, updates: &Updates) -> Vec<ModCard> {
    let extras = LocalExtras::load(data_dir);
    let known = updates.of(kind);
    let mut cards = catalogue::cards(data_dir, kind, &extras).unwrap_or_default();
    for card in &mut cards {
        // A built-in ships with the app: there is no registry entry to compare it with, and its
        // card does not carry an update line.
        if card.builtin {
            continue;
        }
        card.update = state(kind, known.get(&card.filename));
    }
    cards
}

/// The sentence above the Plugins and Themes lists after a check.
#[must_use]
pub fn update_note(cards: &[ModCard]) -> String {
    let stale = cards
        .iter()
        .filter(|card| card.update.starts_with("Update to "))
        .count();
    match stale {
        0 => UP_TO_DATE.to_owned(),
        1 => "1 update is ready.".to_owned(),
        count => format!("{count} updates are ready."),
    }
}

/// One update the page will fetch: the file it replaces and the address it comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wanted {
    pub filename: String,
    pub url: String,
}

/// The updates one press of Update or Update all fetches, and the lines for the ones it cannot:
/// a mod with a newer version but no address to fetch it from is reported rather than skipped in
/// silence.
#[must_use]
pub fn wanted(cards: &[ModCard], updates: &Updates, kind: Kind) -> (Vec<Wanted>, Vec<Notice>) {
    let known = updates.of(kind);
    let mut wanted = Vec::new();
    let mut refused = Vec::new();
    for card in cards {
        let Some(info) = known.get(&card.filename) else {
            continue;
        };
        if !info.has_update {
            continue;
        }
        match &info.update_url {
            Some(url) => wanted.push(Wanted {
                filename: card.filename.clone(),
                url: url.clone(),
            }),
            None => refused.push(Notice::failed(format!(
                "{} has a newer version, but the registry gives no address to fetch it from, so it stays as it is.",
                card.filename
            ))),
        }
    }
    (wanted, refused)
}

/// What one press of Update or Update all ends with: every file that took its new version and
/// every one that did not, each said in a sentence.
#[must_use]
pub fn updated(pressed: usize, results: Vec<Result<String, String>>) -> Notice {
    if pressed == 0 {
        return Notice::said("Nothing needed an update.");
    }
    let mut took = Vec::new();
    let mut failed = Vec::new();
    for result in results {
        match result {
            Ok(filename) => took.push(filename),
            Err(error) => failed.push(error),
        }
    }
    if failed.is_empty() {
        return Notice::said(match took.len() {
            1 => format!("Updated {}.", took[0]),
            count => format!("Updated {count} mods."),
        });
    }
    let head = match took.len() {
        0 => String::new(),
        1 => format!("Updated {}.", took[0]),
        count => format!("Updated {count} mods."),
    };
    let tail = failed.join(" ");
    Notice {
        text: if head.is_empty() {
            tail
        } else {
            format!("{head} {tail}")
        },
        failed: true,
    }
}

/// The sentence for a download that failed, and what the user can do about it.
#[must_use]
pub fn download_failed(name: &str, error: &str) -> String {
    format!("{name} was not installed: {error} Press Install to try again.")
}

/// Removes the file an entry installed and takes its name out of the list it was on. The sentence
/// is the one the Plugins and Themes pages use for Remove, so the two pages cannot disagree.
pub fn uninstall(data_dir: &Path, kind: Kind, filename: &str) -> (Row, Notice) {
    let mut extras = LocalExtras::load(data_dir);
    let notice = match kind {
        Kind::Plugin => super::plugins::remove(data_dir, &mut extras, filename),
        Kind::Theme => super::themes::remove(data_dir, &mut extras, filename),
    };
    if !notice.failed {
        extras.save(data_dir);
    }
    let plugins = catalogue::cards(data_dir, Kind::Plugin, &extras).unwrap_or_default();
    let themes = catalogue::cards(data_dir, Kind::Theme, &extras).unwrap_or_default();
    (Row { plugins, themes }, notice)
}

/// The two installed lists as a press leaves them, read from the folder: what an uninstall takes
/// off the Plugins or Themes page is the card the page draws.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Row {
    pub plugins: Vec<ModCard>,
    pub themes: Vec<ModCard>,
}

/// The sentence for a registry that could not be read, and what the user can do about it.
#[must_use]
pub fn registry_failed(error: &str) -> String {
    format!(
        "The registry could not be read, so nothing is listed. Check this computer's connection and press Check for updates to try again. ({error})"
    )
}

/// The sentence for an update that failed, and what the user can do about it.
#[must_use]
pub fn update_failed(name: &str, error: &str) -> String {
    format!("{name} was not updated: {error} Press Update to try again.")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(has_update: bool, new: Option<&str>) -> UpdateInfo {
        UpdateInfo {
            has_update,
            installed_version: Some("1.0.0".to_owned()),
            new_version: new.map(str::to_owned),
            registry_version: new.map(str::to_owned),
            update_url: has_update.then(|| "https://example.com/a.plugin.js".to_owned()),
        }
    }

    fn card(update: &str) -> ModCard {
        ModCard {
            filename: "a.plugin.js".to_owned(),
            update: update.to_owned(),
            ..ModCard::default()
        }
    }

    #[test]
    fn a_state_names_the_new_version_or_says_the_mod_is_current() {
        assert_eq!(
            state(Kind::Plugin, Some(&info(true, Some("2.0.0")))),
            "Update to 2.0.0"
        );
        assert_eq!(state(Kind::Theme, Some(&info(false, None))), UP_TO_DATE);
        assert_eq!(state(Kind::Plugin, None), CHECKING);
        assert!(state(
            Kind::Plugin,
            Some(&UpdateInfo {
                installed_version: None,
                ..info(false, None)
            })
        )
        .starts_with("Could not check"));
    }

    #[test]
    fn the_note_counts_the_updates_that_are_ready() {
        assert_eq!(update_note(&[]), UP_TO_DATE);
        assert_eq!(update_note(&[card(UP_TO_DATE)]), UP_TO_DATE);
        assert_eq!(
            update_note(&[card("Update to 2.0.0")]),
            "1 update is ready."
        );
        assert_eq!(
            update_note(&[card("Update to 2.0.0"), card("Update to 3.0.0")]),
            "2 updates are ready."
        );
    }

    #[test]
    fn a_search_matches_name_author_and_description_and_ignores_case() {
        let listing = Listing::from_registry(Registry {
            plugins: vec![RegistryEntry {
                name: "Alpha".to_owned(),
                author: "Ada".to_owned(),
                description: Some("Fixes subtitles.".to_owned()),
                version: "1.0.0".to_owned(),
                repo: "https://example.com/repo".to_owned(),
                download: "https://example.com/a.plugin.js".to_owned(),
                preview: None,
            }],
            themes: Vec::new(),
        });
        for query in ["alpha", "ADA", "subtitle", ""] {
            assert_eq!(listing.matching(Kind::Plugin, query).len(), 1, "{query:?}");
        }
        assert!(listing.matching(Kind::Plugin, "nothing").is_empty());
        assert!(listing.matching(Kind::Theme, "").is_empty());
    }

    #[test]
    fn the_summary_names_what_took_its_update_and_what_did_not() {
        assert_eq!(updated(0, Vec::new()).text, "Nothing needed an update.");
        assert_eq!(
            updated(1, vec![Ok("a.plugin.js".to_owned())]).text,
            "Updated a.plugin.js."
        );
        let mixed = updated(
            2,
            vec![
                Ok("a.plugin.js".to_owned()),
                Err(update_failed("b.plugin.js", "The download failed.")),
            ],
        );
        assert_eq!(
            mixed.text,
            "Updated a.plugin.js. b.plugin.js was not updated: The download failed. Press Update to try again."
        );
        assert!(
            mixed.failed,
            "a failed update is drawn as bad news even beside a good one"
        );
    }
}
