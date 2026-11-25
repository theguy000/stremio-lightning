//! Addons Screen and Add-Addon Modal callbacks.

use slint::ComponentHandle;
use stremio_core::runtime::msg::Action;
use stremio_core::runtime::RuntimeAction;

use super::Wiring;
use crate::app::AppModelField;
use crate::plumbing::links;
use crate::screens::{addons, calendar, details};
use crate::{AddonsUi, CalendarUi, MainWindow};

impl Wiring {
    pub(super) fn select_addon_input_option(&self, input: i32, option: i32) {
        let addons_data = self.app.addons().view();
        let choice = usize::try_from(input)
            .ok()
            .and_then(|i| addons_data.inputs.get(i))
            .and_then(|menu| {
                usize::try_from(option)
                    .ok()
                    .and_then(|o| menu.values.get(o).cloned())
            });
        if let Some(choice) = choice {
            addons::select(self.app.runtime(), &choice);
        }
    }

    pub(super) fn calendar_shown(&self) {
        let Ok(model) = self.app.runtime().model() else {
            return;
        };
        let signature = calendar::library_signature(&model);
        let loaded = self.app.calendar().last_library();
        let stale = calendar::needs_load(loaded, signature, model.calendar.meta_items.is_empty());
        let selected = model.calendar.selected.clone();
        drop(model);
        if !stale {
            return;
        }
        self.app.calendar().set_last_library(Some(signature));
        calendar::load(
            self.app.runtime(),
            selected.map(|held| (held.year, held.month)),
        );
    }

    pub(super) fn calendar_step(&self, forward: bool) {
        calendar::step(self.app.runtime(), forward);
        if let Some(window) = self.window.upgrade() {
            window.global::<CalendarUi>().set_selected_day(-1);
        }
    }

    pub(super) fn calendar_open(&self, meta_id: String, kind: String, video_id: String) {
        self.open_item(meta_id, kind, String::new());
        self.app.details().set_picked(Some(video_id.clone()));
        details::pick(self.app.runtime(), &video_id);
    }

    pub(super) fn install_market_addon(&self, transport_url: &str) {
        let card = self
            .app
            .addons()
            .view()
            .cards
            .iter()
            .find(|card| card.transport_url == transport_url)
            .cloned();
        if let Some(card) = card {
            if let addons::InstallOutcome::OpenConfigure(url) =
                addons::install_card(self.app.runtime(), &card)
            {
                links::open(&url);
            }
        }
    }
}

pub(super) fn wire(window: &MainWindow, wiring: &Wiring) {
    let ui = window.global::<AddonsUi>();
    let w = wiring.clone();
    ui.on_select_option(move |input, option| w.select_addon_input_option(input, option));
    let w = wiring.clone();
    ui.on_search_changed(move |_| w.repaint());
    let w = wiring.clone();
    ui.on_add_addon(move || {
        w.app.runtime().dispatch(RuntimeAction {
            field: Some(AppModelField::AddonDetails),
            action: Action::Unload,
        });
        if let Some(window) = w.window.upgrade() {
            window.global::<AddonsUi>().set_add_error("".into());
        }
    });
    let w = wiring.clone();
    ui.on_add_submit(move |text| {
        let error = addons::open_details(w.app.runtime(), &text)
            .err()
            .unwrap_or_default();
        if let Some(window) = w.window.upgrade() {
            window.global::<AddonsUi>().set_add_error(error.into());
        }
    });
    let w = wiring.clone();
    ui.on_add_install(move || addons::install(w.app.runtime()));
    let w = wiring.clone();
    ui.on_add_close(move || {
        if let Some(window) = w.window.upgrade() {
            window.global::<AddonsUi>().set_add_error("".into());
        }
    });
    let w = wiring.clone();
    ui.on_uninstall(move |url| addons::uninstall(w.app.runtime(), &url));
    let w = wiring.clone();
    ui.on_share(move |url| {
        if let Some(window) = w.window.upgrade() {
            window.set_share_link(url.clone());
            window.set_share_open(true);
        }
    });
    let w = wiring.clone();
    ui.on_open(move |url| {
        let installed = w
            .app
            .addons()
            .view()
            .cards
            .iter()
            .any(|card| card.transport_url == url.as_str() && card.installed);
        if !installed {
            if addons::open_details(w.app.runtime(), &url).is_ok() {
                if let Some(window) = w.window.upgrade() {
                    let ui = window.global::<AddonsUi>();
                    ui.set_add_open(true);
                    ui.set_add_url(url.clone());
                    ui.set_add_submitted(true);
                    ui.set_add_error("".into());
                }
            }
            return;
        }
        links::open(&addons::configure_url(&url));
    });
    let w = wiring.clone();
    ui.on_install(move |url| w.install_market_addon(&url));
    ui.on_configure(move |url| links::open(&addons::configure_url(&url)));
    let cal = window.global::<CalendarUi>();
    let w = wiring.clone();
    cal.on_shown(move || w.calendar_shown());
    let w = wiring.clone();
    cal.on_prev_month(move || w.calendar_step(false));
    let w = wiring.clone();
    cal.on_next_month(move || w.calendar_step(true));
    let w = wiring.clone();
    cal.on_open_slot(move |meta_id, kind, video_id| {
        w.calendar_open(meta_id.to_string(), kind.to_string(), video_id.to_string())
    });
}
