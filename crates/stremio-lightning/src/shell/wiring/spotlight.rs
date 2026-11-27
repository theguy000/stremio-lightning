//! Board Spotlight carousel & Settings Spotlight picker callbacks.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use slint::ComponentHandle;
use stremio_core::models::common::Loadable;
use stremio_core::runtime::msg::{Action, ActionCtx};
use stremio_core::runtime::RuntimeAction;

use super::Wiring;
use crate::plumbing::local_settings::SpotlightCatalogPick;
use crate::screens::board::spotlight::MAX_CATALOGS;
use crate::shell::{shortcuts, view};
use crate::{BoardUi, MainWindow, Section, SettingsUi, ShellModalsUi};

const NUDGE_DURATION: Duration = Duration::from_millis(2400);

impl Wiring {
    fn refresh_spotlight(&self) {
        self.app.board().refresh();
        self.app.settings().refresh();
        self.app.viewport().request_active_images();
    }

    fn set_spotlight_nudge(&self, nudge: &'static str, seq: &Arc<AtomicU64>) {
        let gen_id = seq.fetch_add(1, Ordering::Relaxed) + 1;
        if let Some(window) = self.window.upgrade() {
            window
                .global::<SettingsUi>()
                .set_spotlight_nudge(nudge.into());
        }
        let weak = self.window.clone();
        let seq_c = seq.clone();
        self.handle.spawn(async move {
            tokio::time::sleep(NUDGE_DURATION).await;
            if seq_c.load(Ordering::Relaxed) == gen_id {
                let _ = weak.upgrade_in_event_loop(|w| {
                    w.global::<SettingsUi>().set_spotlight_nudge("".into());
                });
            }
        });
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
            let set = window.global::<SettingsUi>();
            set.set_spotlight_search_query("".into());
            set.set_spotlight_nudge("".into());
            let sp = self.app.settings().view().spotlight.clone();
            view::spotlight::apply_spotlight_picker(&window, &sp, self.app.images());
            window
                .global::<ShellModalsUi>()
                .set_spotlight_picker_open(true);
        }
        self.app.viewport().request_active_images();
    }

    pub(super) fn spotlight_search_changed(&self) {
        if let Some(window) = self.window.upgrade() {
            let sp = self.app.settings().view().spotlight.clone();
            view::spotlight::apply_spotlight_picker(&window, &sp, self.app.images());
        }
    }

    pub(super) fn spotlight_go_addons(&self) {
        if let Some(window) = self.window.upgrade() {
            window
                .global::<ShellModalsUi>()
                .set_spotlight_picker_open(false);
            shortcuts::switch_section(&window, Section::Addons);
        }
    }

    pub(super) fn toggle_spotlight_catalog(&self, index: i32, nudge_seq: &Arc<AtomicU64>) {
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
        // Expand defaults in selected_rank order so unchecking one preserves ranking.
        let mut next: Vec<SpotlightCatalogPick> = if sp.is_default {
            let mut ranked: Vec<_> = sp.catalogs.iter().filter(|c| c.selected_rank > 0).collect();
            ranked.sort_by_key(|c| c.selected_rank);
            ranked
                .into_iter()
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
                self.set_spotlight_nudge("keep-one", nudge_seq);
                return;
            }
            next.remove(pos);
        } else {
            if next.len() >= MAX_CATALOGS {
                self.set_spotlight_nudge("max", nudge_seq);
                return;
            }
            next.push(pick);
        }

        if let Some(window) = self.window.upgrade() {
            window.global::<SettingsUi>().set_spotlight_nudge("".into());
        }
        {
            let mut extras = self.app.local_extras();
            extras.spotlight.catalogs = next;
            extras.save(self.app.data_dir());
        }
        self.refresh_spotlight();
    }

    pub(super) fn reset_spotlight_catalogs(&self) {
        if let Some(window) = self.window.upgrade() {
            window.global::<SettingsUi>().set_spotlight_nudge("".into());
        }
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
    let nudge_seq = Arc::new(AtomicU64::new(0));
    let w = wiring.clone();
    set.on_set_spotlight_enabled(move |v| w.set_spotlight_enabled(v));
    let w = wiring.clone();
    set.on_set_spotlight_show_rows(move |v| w.set_spotlight_show_rows(v));
    let w = wiring.clone();
    set.on_open_spotlight_picker(move || w.open_spotlight_picker());
    let w = wiring.clone();
    let seq = nudge_seq.clone();
    set.on_toggle_spotlight_catalog(move |i| w.toggle_spotlight_catalog(i, &seq));
    let w = wiring.clone();
    set.on_reset_spotlight_catalogs(move || w.reset_spotlight_catalogs());
    let w = wiring.clone();
    set.on_spotlight_search_changed(move |_| w.spotlight_search_changed());
    let w = wiring.clone();
    set.on_spotlight_go_addons(move || w.spotlight_go_addons());
}
