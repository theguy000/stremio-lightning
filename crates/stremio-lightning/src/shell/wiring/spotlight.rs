//! Board Spotlight carousel & Settings Spotlight picker callbacks.

use slint::ComponentHandle;
use stremio_core::models::common::Loadable;
use stremio_core::runtime::msg::{Action, ActionCtx};
use stremio_core::runtime::RuntimeAction;

use super::Wiring;
use crate::app::ScreenMask;
use crate::plumbing::local_settings::SpotlightCatalogPick;
use crate::screens::board::spotlight::MAX_CATALOGS;
use crate::{BoardUi, MainWindow, SettingsUi, ShellModalsUi};

impl Wiring {
    fn refresh_spotlight(&self) {
        self.app.settings().refresh();
        self.app
            .mark_dirty(ScreenMask::BOARD.union(ScreenMask::SETTINGS));
        self.app.viewport().request_active_images();
    }

    pub(super) fn spotlight_toggle_library(&self, id: String, kind: String, in_library: bool) {
        if in_library {
            self.app.runtime().dispatch(RuntimeAction {
                field: None,
                action: Action::Ctx(ActionCtx::RemoveFromLibrary(id)),
            });
            return;
        }
        let preview = self.app.runtime().model().ok().and_then(|model| {
            model
                .board
                .catalogs
                .iter()
                .filter_map(|c| c.first())
                .filter_map(|p| match &p.content {
                    Some(Loadable::Ready(items)) => {
                        items.iter().find(|m| m.id == id && m.r#type == kind)
                    }
                    _ => None,
                })
                .next()
                .cloned()
        });
        if let Some(item) = preview {
            self.app.runtime().dispatch(RuntimeAction {
                field: None,
                action: Action::Ctx(ActionCtx::AddToLibrary(item)),
            });
        }
    }

    pub(super) fn set_spotlight_enabled(&self, enabled: bool) {
        {
            let mut extras = self.app.local_extras();
            extras.spotlight.enabled = enabled;
            extras.save(self.app.data_dir());
        }
        self.refresh_spotlight();
    }

    pub(super) fn set_spotlight_show_rows(&self, show: bool) {
        {
            let mut extras = self.app.local_extras();
            extras.spotlight.show_catalog_rows = show;
            extras.save(self.app.data_dir());
        }
        self.refresh_spotlight();
    }

    pub(super) fn open_spotlight_picker(&self) {
        if let Some(window) = self.window.upgrade() {
            window
                .global::<ShellModalsUi>()
                .set_spotlight_picker_open(true);
        }
        self.app.viewport().request_active_images();
    }

    pub(super) fn toggle_spotlight_catalog(&self, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let sp = self.app.settings().view().spotlight.clone();
        let Some(target) = sp.catalogs.get(index) else {
            return;
        };
        let pick = SpotlightCatalogPick {
            transport_url: target.transport_url.clone(),
            r#type: target.kind.clone(),
            id: target.id.clone(),
        };
        // Start from current effective picks so unchecking one of the defaults behaves naturally.
        let mut next: Vec<SpotlightCatalogPick> = if sp.is_default {
            sp.catalogs
                .iter()
                .filter(|c| c.selected_rank > 0)
                .map(|c| SpotlightCatalogPick {
                    transport_url: c.transport_url.clone(),
                    r#type: c.kind.clone(),
                    id: c.id.clone(),
                })
                .collect()
        } else {
            self.app.local_extras().spotlight.catalogs.clone()
        };

        if let Some(pos) = next.iter().position(|p| p == &pick) {
            if next.len() <= 1 {
                self.say("Keep at least one catalog");
                return;
            }
            next.remove(pos);
        } else {
            if next.len() >= MAX_CATALOGS {
                self.say("Remove one catalog to add another");
                return;
            }
            next.push(pick);
        }

        {
            let mut extras = self.app.local_extras();
            extras.spotlight.catalogs = next;
            extras.save(self.app.data_dir());
        }
        self.refresh_spotlight();
    }

    pub(super) fn reset_spotlight_catalogs(&self) {
        {
            let mut extras = self.app.local_extras();
            extras.spotlight.catalogs.clear();
            extras.save(self.app.data_dir());
        }
        self.refresh_spotlight();
    }
}

pub(super) fn wire(window: &MainWindow, wiring: &Wiring) {
    let board = window.global::<BoardUi>();
    let w = wiring.clone();
    board.on_spotlight_library(move |id, kind, in_lib| {
        w.spotlight_toggle_library(id.to_string(), kind.to_string(), in_lib);
    });

    let set = window.global::<SettingsUi>();
    let w = wiring.clone();
    set.on_set_spotlight_enabled(move |v| w.set_spotlight_enabled(v));
    let w = wiring.clone();
    set.on_set_spotlight_show_rows(move |v| w.set_spotlight_show_rows(v));
    let w = wiring.clone();
    set.on_open_spotlight_picker(move || w.open_spotlight_picker());
    let w = wiring.clone();
    set.on_toggle_spotlight_catalog(move |i| w.toggle_spotlight_catalog(i));
    let w = wiring.clone();
    set.on_reset_spotlight_catalogs(move || w.reset_spotlight_catalogs());
}
