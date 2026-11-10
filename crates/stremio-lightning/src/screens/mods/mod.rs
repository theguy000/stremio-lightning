//! What the Mods section shows: its six pages, the plugins and the themes installed on this
//! computer, and the actions that turn one on, turn it off or remove it.
//!
//! The window's Mods section is a menu of six pages on the left and one page's content on the
//! right. This file owns the menu and the two pages that work offline: Plugins and Themes.
//! `catalogue` reads what is installed and turns it into cards; `plugins`, `themes` and `settings`
//! hold what pressing a page's controls does. `logs` reads what the app recorded and `about` reads
//! what this build is; the Marketplace page gets its own file beside these.
//!
//! Nothing here executes a plugin. Turning one on writes its file name to `enabled_plugins` in
//! the shell's own `settings.json`; what reading that list means is the plugin runtime's business.
//! The Plugin settings page edits the plugin's own settings file, and the runtime is what hands
//! the saved values back to the plugin.

use std::path::Path;

use crate::screens::settings::LocalExtras;

pub mod about;
pub mod builtin;
pub mod catalogue;
pub mod logs;
pub mod marketplace;
pub mod plugin_schema;
pub mod plugins;
pub mod settings;
pub mod themes;

pub use catalogue::{ModCard, Notice};

/// The six pages of the Mods section, in the order its menu lists them.
/// The menu's pages. A plugin's settings are not a page of their own: a card's settings button
/// opens them inside the Plugins page, so the menu never lists them.
pub const PAGES: [(&str, &str); 5] = [
    ("plugins", "Plugins"),
    ("themes", "Themes"),
    ("marketplace", "Marketplace"),
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
    /// at all. Both are read from the folder and the runtime with every snapshot, never carried as
    /// a choice: a switch pressed here and one pressed on the Plugins page are the same press.
    pub picked_state: settings::Running,
    pub picked_enabled: bool,
    /// The Logs page: the filter it is showing, the records it keeps, the choices of each part of
    /// the filter, the header that says how many are held, and the one record whose full text is
    /// open. Read from the app's recorder with every snapshot, so a line a plugin logs from a timer
    /// is on the page without a press.
    pub log_filter: logs::Filter,
    pub log_sources: Vec<logs::Choice>,
    pub log_levels: Vec<logs::Choice>,
    pub log_rows: Vec<logs::Row>,
    pub log_held: String,
    pub log_empty: String,
    pub log_detail: Vec<String>,
    pub log_clear_armed: bool,
    /// The Marketplace page: the registry the shell last read, what the search box holds, the
    /// lines under its list, and the card being fetched. Read from the shell's own state, because
    /// the registry is not on disk.
    pub market: marketplace::Data,
    pub market_search: String,
    /// What the update check made of the installed files, so a card says where it stands.
    pub updates: marketplace::Updates,
    /// The About page: what this build is, where it keeps the two folders, what is installed, and
    /// the path of the diagnostics file written last.
    pub about: about::Data,
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
            plugins_note: String::new(),
            themes_note: themes::NONE_ON.to_owned(),
            running: std::collections::HashMap::new(),
            settings_plugins: Vec::new(),
            plugin: String::new(),
            edits: settings::Edits::default(),
            picked_state: settings::Running::default(),
            picked_enabled: false,
            log_filter: logs::Filter::default(),
            log_sources: logs::sources(&logs::Filter::default()),
            log_levels: logs::levels(&logs::Filter::default()),
            log_rows: Vec::new(),
            log_held: logs::NOTE.to_owned(),
            log_empty: logs::NONE.to_owned(),
            log_detail: Vec::new(),
            log_clear_armed: false,
            market: marketplace::Data::default(),
            market_search: String::new(),
            updates: marketplace::Updates::default(),
            about: about::Data::default(),
        }
    }

    /// is what turns Update all on: with nothing ready, the press has nothing to do.
    #[must_use]
    pub fn updates_ready(&self) -> bool {
        self.updates.any_ready()
    }

    /// The same screen with the registry the shell last read on its Marketplace page, and what
    /// the search box holds. The cards come from the folder as it is now, so a mod installed
    /// since the registry was read already says so.
    #[must_use]
    pub fn with_market(mut self, listing: &marketplace::Listing, search: &str) -> Self {
        self.market = marketplace::data(&self.data_dir, listing, search, &self.updates);
        self.market_search = search.to_owned();
        self
    }

    /// The same screen with everything the Marketplace page draws that is not in the folder: the
    /// registry it read, its search box, the lines under its list and the card being fetched. A
    /// registry that has not been read yet leaves the page as the folder alone makes it.
    #[must_use]
    pub fn with_market_parts(
        mut self,
        listing: Option<&marketplace::Listing>,
        search: &str,
        summary: &[Notice],
        installing: &str,
    ) -> Self {
        if let Some(listing) = listing {
            self = self.with_market(listing, search);
        }
        self.market.summary = summary.to_vec();
        self.market.installing = installing.to_owned();
        self
    }

    /// The same screen with the registry read again, except that it has not been read yet. A card
    /// read here still says nothing about running, so the window is handed the cards it had.
    #[must_use]
    pub fn with_listing(mut self, listing: &marketplace::Listing) -> Self {
        let search = self.market_search.clone();
        let market = marketplace::data(&self.data_dir, listing, &search, &self.updates);
        self.market = marketplace::Data {
            summary: self.market.summary,
            installing: self.market.installing,
            ..market
        };
        self
    }

    /// The same screen with what the update check made of the installed files: the cards on both
    /// pages say it, and the Marketplace cards offer Update where it applies.
    #[must_use]
    pub fn with_updates(mut self, updates: marketplace::Updates) -> Self {
        self.updates = updates;
        self.read_update_states();
        self
    }

    /// Puts the check's sentence on each of the two pages' cards, and on the Marketplace's cards,
    /// without reading the folder again: the lists are already what the folder holds.
    fn read_update_states(&mut self) {
        for kind in [catalogue::Kind::Plugin, catalogue::Kind::Theme] {
            let known = self.updates.of(kind);
            for card in self.cards_of_mut(kind) {
                // A built-in ships with the app: no registry entry, no update line.
                if card.builtin {
                    continue;
                }
                card.update = marketplace::state(kind, known.get(&card.filename));
            }
        }
        // The sentence above each list says what the check found, and nothing about it before the
        // first check has answered.
        self.plugins_note = if self.updates.is_empty() {
            String::new()
        } else {
            plugins::note(&self.plugins)
        };
        self.themes_note = if self.updates.is_empty() {
            themes::NONE_ON.to_owned()
        } else {
            themes::note(self.themes.iter().any(|card| card.enabled), &self.themes)
        };
    }

    fn cards_of_mut(&mut self, kind: catalogue::Kind) -> &mut Vec<ModCard> {
        match kind {
            catalogue::Kind::Plugin => &mut self.plugins,
            catalogue::Kind::Theme => &mut self.themes,
        }
    }

    /// The same screen with what the plugin runtime holds folded in: the Plugins page says which
    /// plugins run, and a card's settings button opens that plugin's page.
    ///
    /// No plugin is picked here. The settings page is opened by a press on a card, so it holds a
    /// plugin only when that press has happened.
    #[must_use]
    pub fn with_plugins(mut self, plugins: &crate::app::plugins::Runtime) -> Self {
        let extras = LocalExtras::load(&self.data_dir);
        self.read_folder(&extras, plugins);
        self.update(String::new(), settings::Edits::default());
        self
    }

    /// The same screen for a window that has been running for a while: the folder is read again,
    /// and what the user chose and typed stays. The window is drawn again on every core event,
    /// and this is the part of it the folder can change without the user: a plugin installed or
    /// removed, a plugin that failed to load. Nothing typed is thrown away.
    #[must_use]
    pub fn refresh(mut self, plugins: &crate::app::plugins::Runtime) -> Self {
        let extras = LocalExtras::load(&self.data_dir);
        self.read_folder(&extras, plugins);
        let (plugin, edits) = (self.plugin.clone(), self.edits.clone());
        self.update(plugin, edits);
        self
    }

    /// Reads the two folders and what the runtime holds for each plugin into this screen.
    fn read_folder(&mut self, extras: &LocalExtras, plugins: &crate::app::plugins::Runtime) {
        match settings::plugins(&self.data_dir, extras) {
            Ok(list) => self.settings_plugins = list,
            Err(error) => self.notice = Notice::unreadable(error),
        }
        for plugin in &self.settings_plugins {
            // A built-in has no file and no runtime to ask: it ships running, switched on as its
            // own switch says.
            let state = if builtin::get(&plugin.filename).is_some() {
                settings::Running {
                    running: true,
                    ..settings::Running::default()
                }
            } else {
                plugins.state(&plugin.filename, plugin.enabled)
            };
            self.running.insert(plugin.filename.clone(), state);
        }
        for card in &mut self.plugins {
            if let Some(state) = self.running.get(&card.filename) {
                card.running = state.clone();
            }
        }
        self.plugins.sort_by(|a, b| a.filename.cmp(&b.filename));
    }

    /// The same screen with what the Plugin settings page holds: the plugin picked, and its
    /// schema with the values on screen. Whether that plugin is on comes from the folder, so this
    /// cannot put the page out of step with the card on the Plugins page.
    #[must_use]
    pub fn with_edits(mut self, plugin: String, edits: settings::Edits) -> Self {
        self.update(plugin, edits);
        self
    }

    /// Puts in place what the Plugin settings page holds: the plugin picked, what is typed for it,
    /// and where that plugin stands. It is a method on the fields alone, so calling it cannot move
    /// the screen and leave a stale copy of it behind.
    fn update(&mut self, plugin: String, edits: settings::Edits) {
        self.picked_state = self.running.get(&plugin).cloned().unwrap_or_default();
        self.picked_enabled = self
            .settings_plugins
            .iter()
            .any(|entry| entry.filename == plugin && entry.enabled);
        self.plugin = plugin;
        self.edits = edits;
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

    /// The same screen with the app's log records folded in: the Logs page's rows, the header that
    /// says how many are held, and the record whose full text is open. Nothing is recorded here;
    /// the page only reads. `armed` is the window's own state for the Clear button.
    #[must_use]
    pub fn with_logs(mut self, logs: &crate::plumbing::logs::Logs, armed: bool) -> Self {
        self.log_filter = logs.filter();
        self.log_sources = logs::sources(&self.log_filter);
        self.log_levels = logs::levels(&self.log_filter);
        self.log_rows = logs::rows(logs, &self.log_filter);
        self.log_held = logs::held(logs, &self.log_filter);
        self.log_empty = if logs.is_empty() {
            logs::NONE.to_owned()
        } else {
            logs::NONE_MATCH.to_owned()
        };
        self.log_detail = logs::detail(logs);
        self.log_clear_armed = armed;
        self
    }

    /// The same screen with the About page read from the two folders, this shell's settings and
    /// the plugin runtime, and the diagnostics file written last. The path is the window's, read
    /// back on every snapshot so a press on another page does not take it off the page.
    #[must_use]
    pub fn with_about(
        mut self,
        extras: &LocalExtras,
        plugins: &crate::app::plugins::Runtime,
        export: Option<&String>,
    ) -> Self {
        let sources = about::Sources {
            data_dir: &self.data_dir,
            extras,
            plugins,
        };
        self.about = about::data(&sources);
        if let Some(path) = export {
            self.about = about::exported(self.about, std::path::Path::new(path));
        }
        self
    }

    /// The same screen after a press on the Logs page's Clear button. The first press arms it and
    /// removes nothing; the second empties the list, which deletes no file and stops no record
    /// being recorded afterwards. Returns whether the list is now armed, which is what the window
    /// holds until the next press.
    pub fn press_clear(&mut self, logs: &crate::plumbing::logs::Logs, armed: bool) -> bool {
        if !armed {
            return true;
        }
        logs.clear();
        false
    }

    /// The same screen after a press on one of the Logs page's records: the full text of that
    /// record is on screen, or taken back when it is the one already open.
    pub fn press_record(&mut self, logs: &crate::plumbing::logs::Logs, id: u64) {
        let open = logs.detail().is_some_and(|entry| entry.id == id);
        logs.set_detail(if open { None } else { Some(id) });
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
    data.cards_from_folder(&extras)
}

impl Data {
    /// The cards the two pages draw, read from the folder, and the line about a folder that cannot
    /// be read. The data the window gets at boot and the data it gets on every later event both
    /// come through here.
    fn cards_from_folder(mut self, extras: &LocalExtras) -> Self {
        for kind in [catalogue::Kind::Plugin, catalogue::Kind::Theme] {
            match catalogue::cards(&self.data_dir, kind, extras) {
                Ok(list) if kind == catalogue::Kind::Plugin => self.plugins = list,
                Ok(list) => self.themes = list,
                Err(error) => self.notice = Notice::unreadable(error),
            }
        }
        self
    }
}

/// What a press on a card's switch means. `enabled` is the state the switch asks for; the screen
/// writes the choice down and reads the two lists back from disk, so the card and the file never
/// disagree.
pub fn set_enabled(data: &mut Data, data_dir: &Path, filename: &str, enabled: bool) {
    let mut extras = LocalExtras::load(data_dir);
    let changed = match Page::from_id(&data.page) {
        Page::Plugins => plugins::set_enabled(&mut extras, filename, enabled),
        Page::Themes => themes::set_enabled(&mut extras, filename, enabled).is_some(),
        _ => false,
    };
    if changed {
        extras.save(data_dir);
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
    fn the_menu_lists_the_five_pages_in_order() {
        let items = menu(Page::Themes);
        let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, ["plugins", "themes", "marketplace", "logs", "about"]);
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

    /// A card's settings button is what picks a plugin: nothing opens the settings page on its own,
    /// and a plugin the folder names is still off until it is switched on.
    #[test]
    fn no_plugin_is_picked_until_a_card_asks_for_one() {
        let dir = std::env::temp_dir().join(format!(
            "stremio-lightning-mods-picked-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let plugins = stremio_lightning_core::mods::mods_dir(
            &dir,
            stremio_lightning_core::mods::ModType::Plugin,
        );
        std::fs::create_dir_all(&plugins).unwrap();
        std::fs::write(plugins.join("a.plugin.js"), "/**\n * @name A\n */\n").unwrap();
        std::fs::write(plugins.join("b.plugin.js"), "/**\n * @name B\n */\n").unwrap();

        let mut page = Data::empty();
        page.data_dir = dir.clone();
        let opened = page.with_plugins(&crate::app::plugins::Runtime::new(dir.clone()));
        assert!(opened.plugin.is_empty(), "no card has been pressed");
        let listed: Vec<&str> = opened
            .settings_plugins
            .iter()
            .map(|plugin| plugin.filename.as_str())
            .collect();
        assert!(listed.contains(&"a.plugin.js"), "listed {listed:?}");
        assert!(listed.contains(&"b.plugin.js"), "listed {listed:?}");

        let picked = opened.with_edits("a.plugin.js".to_owned(), settings::Edits::default());
        assert_eq!(picked.plugin, "a.plugin.js");
        assert!(!picked.picked_enabled);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
