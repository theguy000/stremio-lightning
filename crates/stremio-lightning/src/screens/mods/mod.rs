//! What the Mods section shows: its six pages, the plugins and the themes installed on this
//! computer, and the actions that turn one on, turn it off or remove it.
//!
//! The window's Mods section is a menu of six pages on the left and one page's content on the
//! right. This file owns the menu and the two pages that work offline: Plugins and Themes.
//! `catalogue` reads what is installed and turns it into cards; `plugins`, `themes` and `settings`
//! hold what pressing a page's controls does. The Marketplace, Logs and About pages come later and
//! get their own files beside these.
//!
//! Nothing here executes a plugin. Turning one on writes its file name to `enabled_plugins` in
//! the shell's own `settings.json`; what reading that list means is the plugin runtime's business.
//! The Plugin settings page edits the plugin's own settings file, and the runtime is what hands
//! the saved values back to the plugin.

use std::path::Path;

use crate::screens::settings::LocalExtras;

pub mod catalogue;
pub mod plugin_schema;
pub mod plugins;
pub mod settings;
pub mod themes;

pub use catalogue::{ModCard, Notice};

/// The six pages of the Mods section, in the order its menu lists them.
pub const PAGES: [(&str, &str); 6] = [
    ("plugins", "Plugins"),
    ("themes", "Themes"),
    ("marketplace", "Marketplace"),
    ("settings", "Plugin settings"),
    ("logs", "Logs"),
    ("about", "About"),
];

/// The page the window shows. Kept here so the window and this screen agree on the choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Plugins,
    Themes,
    Marketplace,
    Settings,
    Logs,
    About,
}

impl Page {
    /// The page with this id, or the first page when nothing has that id.
    pub fn from_id(id: &str) -> Self {
        match id {
            "themes" => Self::Themes,
            "marketplace" => Self::Marketplace,
            "settings" => Self::Settings,
            "logs" => Self::Logs,
            "about" => Self::About,
            // "plugins", and anything the window may still hold from an older build.
            _ => Self::Plugins,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Plugins => "plugins",
            Self::Themes => "themes",
            Self::Marketplace => "marketplace",
            Self::Settings => "settings",
            Self::Logs => "logs",
            Self::About => "about",
        }
    }

    /// The list of mods this page shows, if it shows one.
    pub fn kind(self) -> Option<catalogue::Kind> {
        match self {
            Self::Plugins => Some(catalogue::Kind::Plugin),
            Self::Themes => Some(catalogue::Kind::Theme),
            _ => None,
        }
    }

    /// Whether a card on this page has a switch to turn it on or off.
    fn switchable(self) -> bool {
        self.kind().is_some()
    }
}

/// One item of the Mods menu.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavItem {
    pub id: String,
    pub label: String,
    pub selected: bool,
}

#[derive(Clone, Debug)]
pub struct Data {
    /// Where the plugins and themes folders are. The plugin runtime is asked for the state of
    /// each plugin, so the screen needs the folder to name them.
    pub data_dir: std::path::PathBuf,
    /// The id of the page on screen.
    pub page: String,
    /// The six pages, with the one on screen marked.
    pub menu: Vec<NavItem>,
    /// One line about the last thing the user did, empty until they do something.
    pub notice: Notice,
    pub plugins: Vec<ModCard>,
    pub themes: Vec<ModCard>,
    /// What each of the two pages says above its list: the Plugins page about its switch, the
    /// Themes page when no theme is on. Fixed sentences rather than state, so they read the same
    /// way whichever way the list looks.
    pub plugins_note: String,
    pub themes_note: String,
    /// What every plugin the app is running is doing, by file name. Read from the plugin runtime
    /// rather than from the folder, because a file can only say what it was written to do.
    pub running: std::collections::HashMap<String, settings::Running>,
    /// The Plugin settings page: the plugins on its left list, and what it holds for the one
    /// picked.
    pub settings_plugins: Vec<settings::Plugin>,
    pub plugin: String,
    pub edits: settings::Edits,
    /// Whether the plugin picked on that page is running, and why not: the same state its card
    /// carries, said in full on the page where its settings are. `picked_enabled` is whether that
    /// plugin is switched on, which is what decides whether the page says anything about running
    /// at all.
    pub picked_state: settings::Running,
    pub picked_enabled: bool,
}

impl Data {
    /// Nothing read yet: what the window shows before the first snapshot arrives.
    pub fn empty() -> Self {
        Self {
            data_dir: std::path::PathBuf::new(),
            page: Page::default().id().to_owned(),
            menu: menu(Page::default()),
            notice: Notice::default(),
            plugins: Vec::new(),
            themes: Vec::new(),
            plugins_note: plugins::NOTE.to_owned(),
            themes_note: themes::NONE_ON.to_owned(),
            running: std::collections::HashMap::new(),
            settings_plugins: Vec::new(),
            plugin: String::new(),
            edits: settings::Edits::default(),
            picked_state: settings::Running::default(),
            picked_enabled: false,
        }
    }

    /// The same screen with what the plugin runtime holds folded in: the Plugins page says which
    /// plugins run, and the Plugin settings page lists the plugins that can be configured.
    #[must_use]
    pub fn with_plugins(mut self, plugins: &crate::app::plugins::Runtime) -> Self {
        let extras = LocalExtras::load(&self.data_dir);
        match settings::plugins(&self.data_dir, &extras) {
            Ok(list) => self.settings_plugins = list,
            Err(error) => self.notice = Notice::unreadable(error),
        }
        for plugin in &self.settings_plugins {
            self.running.insert(
                plugin.filename.clone(),
                plugins.state(&plugin.filename, plugin.enabled),
            );
        }
        for card in &mut self.plugins {
            if let Some(state) = self.running.get(&card.filename) {
                card.running = state.clone();
            }
        }
        self.plugins.sort_by(|a, b| a.filename.cmp(&b.filename));
        self
    }

    /// The same screen with what the Plugin settings page holds: the plugin picked, its schema
    /// with the values on screen, and where that plugin stands. The switch the user pressed last
    /// is the one that counts, not the folder's copy of it, which the core's own events can be
    /// holding from before the press.
    #[must_use]
    pub fn with_edits(mut self, plugin: String, edits: settings::Edits, enabled: bool) -> Self {
        self.picked_state = self.running.get(&plugin).cloned().unwrap_or_default();
        self.picked_enabled = enabled;
        for entry in &mut self.settings_plugins {
            if entry.filename == plugin {
                entry.enabled = enabled;
            }
        }
        self.plugin = plugin;
        self.edits = edits;
        self
    }

    /// The same screen after the user chose `page`, nothing else changed.
    pub fn with_page(mut self, page: Page) -> Self {
        self.page = page.id().to_owned();
        self.menu = menu(page);
        self
    }

    /// The page this screen is showing.
    #[must_use]
    pub fn to_page(&self) -> Page {
        Page::from_id(&self.page)
    }

    /// The same screen after the user pressed something: the menu is not rebuilt, so the card
    /// whose button was pressed is not torn down under the pointer.
    pub fn with_notice(mut self, notice: Notice) -> Self {
        self.notice = notice;
        self
    }

    /// The cards of the page on screen.
    pub fn cards(&self) -> &[ModCard] {
        match self.page_kind() {
            Some(catalogue::Kind::Plugin) => &self.plugins,
            Some(catalogue::Kind::Theme) => &self.themes,
            None => &[],
        }
    }

    fn page_kind(&self) -> Option<catalogue::Kind> {
        Page::from_id(&self.page).kind()
    }
}

impl Default for Data {
    fn default() -> Self {
        Self::empty()
    }
}

/// The menu of pages, with `page` marked.
pub fn menu(page: Page) -> Vec<NavItem> {
    PAGES
        .iter()
        .map(|(id, label)| NavItem {
            id: (*id).to_owned(),
            label: (*label).to_owned(),
            selected: *id == page.id(),
        })
        .collect()
}

/// Everything the Mods section shows, read from `data_dir`: the plugins and themes folders of
/// `stremio_lightning_core::mods`, and this shell's own `settings.json` for which of them are on.
/// The window decides which page that is; a page switch only repaints, it does not read the disk.
pub fn data(data_dir: &Path) -> Data {
    let extras = LocalExtras::load(data_dir);
    let mut data = Data::empty();
    data.data_dir = data_dir.to_path_buf();
    let mut read = |kind: catalogue::Kind, cards: &mut Vec<ModCard>| match catalogue::cards(
        data_dir, kind, &extras,
    ) {
        Ok(list) => *cards = list,
        Err(error) => data.notice = Notice::unreadable(error),
    };
    read(catalogue::Kind::Plugin, &mut data.plugins);
    read(catalogue::Kind::Theme, &mut data.themes);
    data
}

/// What a press on a card's switch means. `enabled` is the state the switch asks for; the screen
/// writes the choice down and reads the two lists back from disk, so the card and the file never
/// disagree.
pub fn set_enabled(data: &mut Data, data_dir: &Path, filename: &str, enabled: bool) {
    let mut extras = LocalExtras::load(data_dir);
    let notice = match Page::from_id(&data.page) {
        Page::Plugins => plugins::set_enabled(&mut extras, filename, enabled),
        Page::Themes => themes::set_enabled(&mut extras, filename, enabled),
        _ => None,
    };
    if let Some(notice) = notice {
        extras.save(data_dir);
        data.notice = notice;
    }
    refresh(data, data_dir);
}

/// What a press on a card's Remove button means: the file is deleted, the lists on disk are read
/// again, and the card is gone from the page.
pub fn remove(data: &mut Data, data_dir: &Path, filename: &str) {
    let mut extras = LocalExtras::load(data_dir);
    let notice = match Page::from_id(&data.page) {
        Page::Plugins => Some(plugins::remove(data_dir, &mut extras, filename)),
        Page::Themes => Some(themes::remove(data_dir, &mut extras, filename)),
        _ => None,
    };
    let Some(notice) = notice else {
        return;
    };
    if !notice.failed {
        extras.save(data_dir);
    }
    data.notice = notice;
    refresh(data, data_dir);
}

/// Re-reads the installed cards after a press, leaving the page and the notice alone.
fn refresh(data: &mut Data, data_dir: &Path) {
    let extras = LocalExtras::load(data_dir);
    for kind in [catalogue::Kind::Plugin, catalogue::Kind::Theme] {
        let cards = match catalogue::cards(data_dir, kind, &extras) {
            Ok(cards) => cards,
            Err(error) => {
                data.notice = Notice::unreadable(error);
                Vec::new()
            }
        };
        match kind {
            catalogue::Kind::Plugin => {
                // What the runtime says about each plugin is not in the folder, so it is put back
                // on the cards the read replaced.
                data.plugins = cards
                    .into_iter()
                    .map(|mut card| {
                        if let Some(state) = data.running.get(&card.filename) {
                            card.running = state.clone();
                        }
                        card
                    })
                    .collect();
            }
            catalogue::Kind::Theme => data.themes = cards,
        }
    }
}

/// Whether the page on screen has cards to switch on or off.
pub fn switchable(data: &Data) -> bool {
    Page::from_id(&data.page).switchable()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_menu_lists_the_six_pages_in_order() {
        let items = menu(Page::Themes);
        let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "plugins",
                "themes",
                "marketplace",
                "settings",
                "logs",
                "about"
            ]
        );
        assert_eq!(
            items.iter().filter(|item| item.selected).count(),
            1,
            "exactly one page is on screen"
        );
        assert!(items[1].selected);
    }

    #[test]
    fn a_page_id_the_app_does_not_know_opens_the_first_page() {
        assert_eq!(Page::from_id("themes"), Page::Themes);
        assert_eq!(Page::from_id("plugins"), Page::Plugins);
        assert_eq!(Page::from_id(""), Page::Plugins);
        assert_eq!(Page::from_id("plugin settings"), Page::Plugins);
        for (id, _) in PAGES {
            assert_eq!(Page::from_id(id).id(), id, "{id} round-trips");
        }
    }

    #[test]
    fn only_the_two_offline_pages_have_cards() {
        let data = Data::empty();
        assert!(switchable(&data));
        assert!(!switchable(&data.clone().with_page(Page::Logs)));
        assert!(data.clone().with_page(Page::Themes).cards().is_empty());
    }
}
