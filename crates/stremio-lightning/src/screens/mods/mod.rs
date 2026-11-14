//! What the Mods section shows: built-in plugins, local `.theme.css` themes, logs, and about.
//!
//! The window's Mods section is a menu of four pages on the left (`Plugins`, `Themes`, `Logs`,
//! `About`) and one page's content on the right. A built-in plugin's settings button opens its
//! settings inside the Plugins page.

use std::path::Path;

use crate::screens::settings::LocalExtras;

pub mod about;
pub mod builtin;
pub mod catalogue;
pub mod logs;
pub mod plugin_schema;
pub mod plugins;
pub mod settings;
pub mod themes;

pub use catalogue::{ModCard, Notice};

/// The menu's pages, in the order it lists them. A plugin's settings are opened from its card's
/// gear button inside the Plugins page, so the menu never lists them.
pub const PAGES: [(&str, &str); 4] = [
    ("plugins", "Plugins"),
    ("themes", "Themes"),
    ("logs", "Logs"),
    ("about", "About"),
];

/// The page the window shows. Kept here so the window and this screen agree on the choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Page {
    #[default]
    Plugins,
    Themes,
    Settings,
    Logs,
    About,
}

impl Page {
    /// The page with this id, or the first page when nothing has that id.
    pub fn from_id(id: &str) -> Self {
        match id {
            "themes" => Self::Themes,
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
    /// Where the themes folder is.
    pub data_dir: std::path::PathBuf,
    /// The id of the page on screen.
    pub page: String,
    /// The four menu pages, with the one on screen marked.
    pub menu: Vec<NavItem>,
    /// One line about the last thing the user did, empty until they do something.
    pub notice: Notice,
    pub plugins: Vec<ModCard>,
    pub themes: Vec<ModCard>,
    /// What the Themes page says above its list.
    pub themes_note: String,
    /// Built-in plugins available for settings, and what the picked one holds.
    pub settings_plugins: Vec<settings::Plugin>,
    pub plugin: String,
    pub edits: settings::Edits,
    /// The Logs page: the filter it is showing, the records it keeps, the choices of each part of
    /// the filter, the header that says how many are held, and the one record whose full text is
    /// open.
    pub log_filter: logs::Filter,
    pub log_sources: Vec<logs::Choice>,
    pub log_levels: Vec<logs::Choice>,
    pub log_rows: Vec<logs::Row>,
    pub log_held: String,
    pub log_empty: String,
    pub log_detail: Vec<String>,
    pub log_clear_armed: bool,
    /// The About page: what this build is, where it keeps themes, counts, and diagnostics path.
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
            themes_note: themes::NONE_ON.to_owned(),
            settings_plugins: Vec::new(),
            plugin: String::new(),
            edits: settings::Edits::default(),
            log_filter: logs::Filter::default(),
            log_sources: logs::sources(&logs::Filter::default()),
            log_levels: logs::levels(&logs::Filter::default()),
            log_rows: Vec::new(),
            log_held: logs::NOTE.to_owned(),
            log_empty: logs::NONE.to_owned(),
            log_detail: Vec::new(),
            log_clear_armed: false,
            about: about::Data::default(),
        }
    }

    /// The same screen with what the Plugin settings page holds: the plugin picked and its values.
    #[must_use]
    pub fn with_edits(mut self, plugin: String, edits: settings::Edits) -> Self {
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
    pub fn with_about(mut self, extras: &LocalExtras, export: Option<&String>) -> Self {
        let sources = about::Sources {
            data_dir: &self.data_dir,
            extras,
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
        self.plugins.sort_by(|a, b| a.filename.cmp(&b.filename));
        if let Ok(list) = settings::plugins(&self.data_dir, extras) {
            self.settings_plugins = list;
        }
        self.themes_note = themes::note(self.themes.iter().any(|card| card.enabled));
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

/// Re-reads the cards after a press, leaving the page and the notice alone.
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
                data.plugins = cards;
                data.plugins.sort_by(|a, b| a.filename.cmp(&b.filename));
            }
            catalogue::Kind::Theme => {
                data.themes = cards;
                data.themes_note = themes::note(data.themes.iter().any(|card| card.enabled));
            }
        }
    }
    if let Ok(list) = settings::plugins(data_dir, &extras) {
        data.settings_plugins = list;
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
    fn the_menu_lists_the_four_pages_in_order() {
        let items = menu(Page::Themes);
        let ids: Vec<&str> = items.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids, ["plugins", "themes", "logs", "about"]);
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
        assert_eq!(Page::from_id("marketplace"), Page::Plugins);
        assert_eq!(Page::from_id(""), Page::Plugins);
        assert_eq!(Page::from_id("plugin settings"), Page::Plugins);
        for (id, _) in PAGES {
            assert_eq!(Page::from_id(id).id(), id, "{id} round-trips");
        }
    }

    #[test]
    fn only_plugins_and_themes_pages_have_cards() {
        let data = Data::empty();
        assert!(switchable(&data));
        assert!(!switchable(&data.clone().with_page(Page::Logs)));
        assert!(data.clone().with_page(Page::Themes).cards().is_empty());
    }

    #[test]
    fn no_plugin_is_picked_until_a_card_asks_for_one() {
        let dir = std::env::temp_dir().join(format!(
            "stremio-lightning-mods-picked-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);

        let opened = data(&dir);
        assert!(opened.plugin.is_empty(), "no card has been pressed");
        let listed: Vec<&str> = opened
            .settings_plugins
            .iter()
            .map(|plugin| plugin.filename.as_str())
            .collect();
        assert_eq!(listed, ["builtin:aniskip"]);

        let picked = opened.with_edits("builtin:aniskip".to_owned(), settings::Edits::default());
        assert_eq!(picked.plugin, "builtin:aniskip");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
