//! Pure Mods hub: built-in plugins, pre-loaded `.theme.css` themes, plugin settings, logs, and
//! about. Zero filesystem I/O (theme listing/deletion and `LocalExtras` live in `plumbing/`).

use std::path::{Path, PathBuf};

use crate::plumbing::local_settings::LocalExtras;
use crate::plumbing::theme::store::InstalledTheme;

pub mod about;
pub mod builtin;
pub mod catalogue;
pub mod log_filter;
pub mod log_selection;
pub mod logs;
pub mod settings;

pub use catalogue::{ModCard, Notice};

pub const PAGES: [(&str, &str); 4] = [
    ("plugins", "Plugins"),
    ("themes", "Themes"),
    ("logs", "Logs"),
    ("about", "About"),
];

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
    pub fn from_id(id: &str) -> Self {
        match id {
            "themes" => Self::Themes,
            "settings" => Self::Settings,
            "logs" => Self::Logs,
            "about" => Self::About,
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

    pub fn kind(self) -> Option<catalogue::Kind> {
        match self {
            Self::Plugins => Some(catalogue::Kind::Plugin),
            Self::Themes => Some(catalogue::Kind::Theme),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavItem {
    pub id: String,
    pub label: String,
    pub selected: bool,
}

#[derive(Clone, Debug)]
pub struct Data {
    pub themes_dir: PathBuf,
    pub page: String,
    pub menu: Vec<NavItem>,
    pub notice: Notice,
    pub plugins: Vec<ModCard>,
    pub themes: Vec<ModCard>,
    pub themes_note: String,
    pub settings_plugins: Vec<settings::Plugin>,
    pub plugin: String,
    pub edits: settings::Edits,
    pub logs: logs::Data,
    pub about: about::Data,
}

impl Data {
    pub fn empty() -> Self {
        Self {
            themes_dir: PathBuf::new(),
            page: Page::default().id().to_owned(),
            menu: menu(Page::default()),
            notice: Notice::default(),
            plugins: Vec::new(),
            themes: Vec::new(),
            themes_note: catalogue::NONE_ON.to_owned(),
            settings_plugins: Vec::new(),
            plugin: String::new(),
            edits: settings::Edits::default(),
            logs: logs::Data::default(),
            about: about::Data::default(),
        }
    }

    #[must_use]
    pub fn with_edits(mut self, plugin: String, edits: settings::Edits) -> Self {
        self.plugin = plugin;
        self.edits = edits;
        self
    }

    pub fn with_page(mut self, page: Page) -> Self {
        self.page = page.id().to_owned();
        self.menu = menu(page);
        self
    }

    #[must_use]
    pub fn with_logs(
        mut self,
        entries: &[crate::plumbing::logs::Entry],
        sel: &log_selection::LogSelection,
        armed: bool,
    ) -> Self {
        self.logs = logs::data(entries, sel, armed);
        self
    }

    #[must_use]
    pub fn with_export(mut self, export: Option<&String>) -> Self {
        if let Some(path) = export {
            self.about.export = path.clone();
        }
        self
    }

    pub fn with_notice(mut self, notice: Notice) -> Self {
        self.notice = notice;
        self
    }
}

impl Default for Data {
    fn default() -> Self {
        Self::empty()
    }
}

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

/// Pure Mods projection from pre-loaded themes (or read error) and `LocalExtras`.
pub fn data(
    themes_dir: &Path,
    extras: &LocalExtras,
    installed_themes: Result<&[InstalledTheme], &str>,
) -> Data {
    let mut plugins = builtin::cards(extras);
    plugins.sort_by(|a, b| a.filename.cmp(&b.filename));
    let (themes, notice) = match installed_themes {
        Ok(list) => (catalogue::theme_cards(list, extras), Notice::default()),
        Err(err) => (Vec::new(), Notice::unreadable(err)),
    };
    let about = about::data(themes_dir, &plugins, &themes);
    let themes_note = catalogue::themes_note(themes.iter().any(|c| c.enabled));
    Data {
        themes_dir: themes_dir.to_path_buf(),
        plugins,
        themes,
        themes_note,
        settings_plugins: settings::plugins(extras),
        notice,
        about,
        ..Data::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_pages_and_pure_data_build_without_disk_io() {
        let items = menu(Page::Themes);
        assert_eq!(
            items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
            ["plugins", "themes", "logs", "about"]
        );
        assert!(items[1].selected && items.iter().filter(|i| i.selected).count() == 1);
        assert_eq!(Page::from_id("unknown"), Page::Plugins);
        for (id, _) in PAGES {
            assert_eq!(Page::from_id(id).id(), id);
        }

        let extras = LocalExtras::default();
        let d = data(Path::new("/themes"), &extras, Ok(&[]));
        assert!(d.plugin.is_empty());
        assert_eq!(
            d.settings_plugins
                .iter()
                .map(|p| p.filename.as_str())
                .collect::<Vec<_>>(),
            ["builtin:aniskip"]
        );
        assert_eq!(d.about.counts[0].value, "1");
    }
}
