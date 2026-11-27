//! Settings -> Content callbacks: catalog reordering/visibility, stream addons, and filter presets.

use slint::ComponentHandle;
use stremio_core::models::catalog_with_filters::Selected as CatalogSelected;
use stremio_core::runtime::msg::{Action, ActionLoad};
use stremio_core::runtime::RuntimeAction;

use super::Wiring;
use crate::app::AppModelField;
use crate::screens::discover;
use crate::screens::settings::content_filter;
use crate::shell::shortcuts;
use crate::{MainWindow, Section, SettingsUi};

impl Wiring {
    pub(super) fn ensure_visible_discover_catalog(&self) {
        let fallback = {
            let content = self.app.local_extras().content.clone();
            self.app
                .runtime()
                .model()
                .ok()
                .and_then(|model| discover::fallback_request_if_hidden(&model, &content))
        };
        if let Some(request) = fallback {
            self.app.runtime().dispatch(RuntimeAction {
                field: Some(AppModelField::Discover),
                action: Action::Load(ActionLoad::CatalogWithFilters(Some(CatalogSelected {
                    request,
                }))),
            });
        }
    }

    fn refresh_content_catalogs(&self) {
        self.ensure_visible_discover_catalog();
        self.app.board().refresh();
        self.app.discover().refresh();
        self.app.settings().refresh();
        self.app.viewport().request_active_images();
    }

    fn refresh_content_streams(&self) {
        self.app.details().refresh();
        self.app.settings().refresh();
    }

    pub(super) fn content_move_catalog(&self, index: i32, delta: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        let keys: Vec<_> = self
            .app
            .settings()
            .view()
            .content
            .catalogs
            .iter()
            .map(|c| c.key.clone())
            .collect();
        let moved = {
            let mut extras = self.app.local_extras();
            let changed = content_filter::move_catalog(&mut extras.content, &keys, index, delta);
            if changed {
                extras.save(self.app.data_dir());
            }
            changed
        };
        if moved {
            self.refresh_content_catalogs();
        }
    }

    pub(super) fn content_toggle_board(&self, index: i32) {
        let Some(key) = usize::try_from(index).ok().and_then(|i| {
            self.app
                .settings()
                .view()
                .content
                .catalogs
                .get(i)
                .map(|c| c.key.clone())
        }) else {
            return;
        };
        {
            let mut extras = self.app.local_extras();
            content_filter::toggle_board_catalog(&mut extras.content, key);
            extras.save(self.app.data_dir());
        }
        self.refresh_content_catalogs();
    }

    pub(super) fn content_toggle_discover(&self, index: i32) {
        let Some(key) = usize::try_from(index).ok().and_then(|i| {
            self.app
                .settings()
                .view()
                .content
                .catalogs
                .get(i)
                .map(|c| c.key.clone())
        }) else {
            return;
        };
        {
            let mut extras = self.app.local_extras();
            content_filter::toggle_discover_catalog(&mut extras.content, key);
            extras.save(self.app.data_dir());
        }
        self.refresh_content_catalogs();
    }

    pub(super) fn content_reset_catalogs(&self) {
        {
            let mut extras = self.app.local_extras();
            content_filter::reset_catalogs(&mut extras.content);
            extras.save(self.app.data_dir());
        }
        self.refresh_content_catalogs();
    }

    pub(super) fn content_toggle_stream_addon(&self, url: String, enabled: bool) {
        {
            let mut extras = self.app.local_extras();
            content_filter::toggle_stream_addon(&mut extras.content, &url, enabled);
            extras.save(self.app.data_dir());
        }
        self.refresh_content_streams();
    }

    pub(super) fn content_save_preset(
        &self,
        id: String,
        name: String,
        include_csv: String,
        exclude_csv: String,
    ) -> bool {
        let saved = {
            let mut extras = self.app.local_extras();
            let ok = content_filter::upsert_preset(
                &mut extras.content,
                &id,
                &name,
                &include_csv,
                &exclude_csv,
            );
            if ok {
                extras.save(self.app.data_dir());
            }
            ok
        };
        if saved {
            self.refresh_content_streams();
        }
        saved
    }

    pub(super) fn content_delete_preset(&self, id: String) {
        {
            let mut extras = self.app.local_extras();
            content_filter::delete_preset(&mut extras.content, &id);
            extras.save(self.app.data_dir());
        }
        self.refresh_content_streams();
    }

    pub(super) fn content_select_preset(&self, id: String) {
        {
            let mut extras = self.app.local_extras();
            content_filter::set_active_preset(&mut extras.content, &id);
            extras.save(self.app.data_dir());
        }
        self.refresh_content_streams();
    }

    pub(super) fn open_content_settings_page(&self) {
        if let Some(window) = self.window.upgrade() {
            window.set_details_open(false);
            let set = window.global::<SettingsUi>();
            set.set_search_query("".into());
            set.set_hit_row("".into());
            set.set_active_page("content".into());
            window.set_settings_scroll(0.0);
            shortcuts::switch_section(&window, Section::Settings);
        }
    }
}

pub(super) fn wire(window: &MainWindow, wiring: &Wiring) {
    let set = window.global::<SettingsUi>();
    let w = wiring.clone();
    set.on_content_move_catalog(move |i, d| w.content_move_catalog(i, d));
    let w = wiring.clone();
    set.on_content_toggle_board(move |i| w.content_toggle_board(i));
    let w = wiring.clone();
    set.on_content_toggle_discover(move |i| w.content_toggle_discover(i));
    let w = wiring.clone();
    set.on_content_reset_catalogs(move || w.content_reset_catalogs());
    let w = wiring.clone();
    set.on_content_toggle_stream_addon(move |u, e| {
        w.content_toggle_stream_addon(u.to_string(), e);
    });
    let w = wiring.clone();
    set.on_content_save_preset(move |id, name, inc, exc| {
        w.content_save_preset(
            id.to_string(),
            name.to_string(),
            inc.to_string(),
            exc.to_string(),
        )
    });
    let w = wiring.clone();
    set.on_content_delete_preset(move |id| w.content_delete_preset(id.to_string()));
    let w = wiring.clone();
    set.on_content_select_preset(move |id| w.content_select_preset(id.to_string()));
}
