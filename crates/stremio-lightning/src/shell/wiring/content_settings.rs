//! Settings -> Content callbacks: Organize content (drag/toggle/rename) and Filter streams presets.

use std::cell::RefCell;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use stremio_core::models::catalog_with_filters::Selected as CatalogSelected;
use stremio_core::runtime::msg::{Action, ActionLoad};
use stremio_core::runtime::RuntimeAction;

use super::Wiring;
use crate::app::AppModelField;
use crate::screens::discover;
use crate::screens::settings::content_filter;
use crate::shell::shortcuts;
use crate::{MainWindow, Section, SettingsUi};

const BUILTIN_INCLUDE: &[&str] = &["4k", "1080p", "bluray", "720p"];
const BUILTIN_EXCLUDE: &[&str] = &["cam", "hdts", "telesync", "screener"];

#[derive(Default)]
struct PresetDraft {
    include: Vec<String>,
    exclude: Vec<String>,
}

thread_local! {
    static DRAFT: RefCell<PresetDraft> = RefCell::new(PresetDraft::default());
}

fn sync_draft_ui(window: &MainWindow) {
    let set = window.global::<SettingsUi>();
    DRAFT.with(|cell| {
        let d = cell.borrow();
        let has_inc = |t: &str| d.include.iter().any(|x| x.eq_ignore_ascii_case(t));
        let has_exc = |t: &str| d.exclude.iter().any(|x| x.eq_ignore_ascii_case(t));
        set.set_draft_inc_4k(has_inc("4k"));
        set.set_draft_inc_1080p(has_inc("1080p"));
        set.set_draft_inc_bluray(has_inc("bluray"));
        set.set_draft_inc_720p(has_inc("720p"));
        set.set_draft_exc_cam(has_exc("cam"));
        set.set_draft_exc_hdts(has_exc("hdts"));
        set.set_draft_exc_telesync(has_exc("telesync"));
        set.set_draft_exc_screener(has_exc("screener"));

        let custom_inc: Vec<SharedString> = d
            .include
            .iter()
            .filter(|t| !BUILTIN_INCLUDE.iter().any(|b| b.eq_ignore_ascii_case(t)))
            .map(|s| s.clone().into())
            .collect();
        let custom_exc: Vec<SharedString> = d
            .exclude
            .iter()
            .filter(|t| !BUILTIN_EXCLUDE.iter().any(|b| b.eq_ignore_ascii_case(t)))
            .map(|s| s.clone().into())
            .collect();
        set.set_draft_custom_include(ModelRc::new(VecModel::from(custom_inc)));
        set.set_draft_custom_exclude(ModelRc::new(VecModel::from(custom_exc)));
        set.set_draft_has_tags(!d.include.is_empty() || !d.exclude.is_empty());
    });
}

impl Wiring {
    fn dispatch_discover(&self, request: stremio_core::types::addon::ResourceRequest) {
        self.app.runtime().dispatch(RuntimeAction {
            field: Some(AppModelField::Discover),
            action: Action::Load(ActionLoad::CatalogWithFilters(Some(CatalogSelected {
                request,
            }))),
        });
    }

    pub(super) fn ensure_visible_discover_catalog(&self) {
        let content = self.app.local_extras().content.clone();
        if let Some(request) = self
            .app
            .runtime()
            .model()
            .ok()
            .and_then(|m| discover::fallback_request_if_hidden(&m, &content))
        {
            self.dispatch_discover(request);
        }
    }

    fn refresh_content(&self, affects_catalogs: bool, affects_streams: bool) {
        if affects_catalogs {
            self.ensure_visible_discover_catalog();
            self.app.board().refresh();
            self.app.discover().refresh();
        }
        if affects_streams {
            self.app.details().refresh();
        }
        self.app.settings().refresh();
        if affects_catalogs {
            self.app.viewport().request_active_images();
        }
    }

    pub(super) fn content_move_item(&self, tab: String, from: i32, to: i32) {
        let (Ok(from), Ok(to)) = (usize::try_from(from), usize::try_from(to)) else {
            return;
        };
        let view = self.app.settings().view();
        let slice = match tab.as_str() {
            "board" => &view.content.board_items,
            "discover" => &view.content.discover_items,
            "search" => &view.content.search_items,
            "streams" => &view.content.stream_items,
            _ => return,
        };
        let ids: Vec<String> = slice.iter().map(|i| i.id.clone()).collect();
        let moved = {
            let mut extras = self.app.local_extras();
            let changed = content_filter::move_item(&mut extras.content, &tab, &ids, from, to);
            if changed {
                extras.save(self.app.data_dir());
            }
            changed
        };
        if moved {
            if tab == "discover" {
                let content = self.app.local_extras().content.clone();
                if let Some(request) = self
                    .app
                    .runtime()
                    .model()
                    .ok()
                    .and_then(|m| discover::active_type_first_visible(&m, &content))
                {
                    self.dispatch_discover(request);
                }
            }
            let catalogs = tab == "board" || tab == "discover";
            self.refresh_content(catalogs, tab == "streams");
        }
    }

    pub(super) fn content_toggle_item(&self, id: String, active: bool) {
        let changed = {
            let mut extras = self.app.local_extras();
            let ok = content_filter::set_item_active(&mut extras.content, &id, active);
            if ok {
                extras.save(self.app.data_dir());
            }
            ok
        };
        if changed {
            let catalogs = id.starts_with("board|") || id.starts_with("discover|");
            self.refresh_content(catalogs, id.starts_with("streams|"));
        }
    }

    pub(super) fn content_rename_item(&self, id: String, title: String) {
        {
            let mut extras = self.app.local_extras();
            let next = (!title.trim().is_empty()).then_some(title.as_str());
            content_filter::set_item_title(&mut extras.content, &id, next);
            extras.save(self.app.data_dir());
        }
        let catalogs = id.starts_with("board|") || id.starts_with("discover|");
        self.refresh_content(catalogs, id.starts_with("streams|"));
    }

    pub(super) fn content_draft_clear(&self) {
        DRAFT.with(|c| *c.borrow_mut() = PresetDraft::default());
        if let Some(w) = self.window.upgrade() {
            sync_draft_ui(&w);
        }
    }

    pub(super) fn content_draft_toggle_tag(&self, is_exclude: bool, tag: String) {
        let tag = tag.trim().to_owned();
        if tag.is_empty() {
            return;
        }
        DRAFT.with(|cell| {
            let mut d = cell.borrow_mut();
            let list = if is_exclude {
                &mut d.exclude
            } else {
                &mut d.include
            };
            if let Some(pos) = list.iter().position(|t| t.eq_ignore_ascii_case(&tag)) {
                list.remove(pos);
            } else {
                list.push(tag);
            }
        });
        if let Some(w) = self.window.upgrade() {
            sync_draft_ui(&w);
        }
    }

    pub(super) fn content_draft_add_tag(&self, is_exclude: bool, tag: String) -> bool {
        let tag = tag.trim().to_owned();
        if tag.is_empty() {
            return false;
        }
        DRAFT.with(|cell| {
            let mut d = cell.borrow_mut();
            let list = if is_exclude {
                &mut d.exclude
            } else {
                &mut d.include
            };
            if !list.iter().any(|t| t.eq_ignore_ascii_case(&tag)) {
                list.push(tag);
            }
        });
        if let Some(w) = self.window.upgrade() {
            sync_draft_ui(&w);
        }
        true
    }

    pub(super) fn content_create_preset(&self, name: String) -> bool {
        let (inc, exc) = DRAFT.with(|cell| {
            let d = cell.borrow();
            (d.include.clone(), d.exclude.clone())
        });
        let saved = {
            let mut extras = self.app.local_extras();
            let ok = content_filter::create_preset(&mut extras.content, &name, &inc, &exc);
            if ok {
                extras.save(self.app.data_dir());
            }
            ok
        };
        if saved {
            self.content_draft_clear();
            self.refresh_content(false, true);
        }
        saved
    }

    pub(super) fn content_delete_preset(&self, id: String) {
        {
            let mut extras = self.app.local_extras();
            content_filter::delete_preset(&mut extras.content, &id);
            extras.save(self.app.data_dir());
        }
        self.refresh_content(false, true);
    }

    pub(super) fn content_toggle_preset(&self, id: String) {
        {
            let mut extras = self.app.local_extras();
            content_filter::toggle_active_preset(&mut extras.content, &id);
            extras.save(self.app.data_dir());
        }
        self.refresh_content(false, true);
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
    set.on_content_move_item(move |tab, from, to| w.content_move_item(tab.to_string(), from, to));
    let w = wiring.clone();
    set.on_content_toggle_item(move |id, active| w.content_toggle_item(id.to_string(), active));
    let w = wiring.clone();
    set.on_content_rename_item(move |id, title| {
        w.content_rename_item(id.to_string(), title.to_string());
    });
    let w = wiring.clone();
    set.on_content_draft_clear(move || w.content_draft_clear());
    let w = wiring.clone();
    set.on_content_draft_toggle_tag(move |exc, tag| {
        w.content_draft_toggle_tag(exc, tag.to_string());
    });
    let w = wiring.clone();
    set.on_content_draft_add_tag(move |exc, tag| w.content_draft_add_tag(exc, tag.to_string()));
    let w = wiring.clone();
    set.on_content_create_preset(move |name| w.content_create_preset(name.to_string()));
    let w = wiring.clone();
    set.on_content_delete_preset(move |id| w.content_delete_preset(id.to_string()));
    let w = wiring.clone();
    set.on_content_toggle_preset(move |id| w.content_toggle_preset(id.to_string()));
}
