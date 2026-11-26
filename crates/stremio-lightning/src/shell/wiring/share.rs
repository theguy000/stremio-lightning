//! Details social actions (rate, library, watched) and Share Modal callbacks.

use slint::ComponentHandle;

use super::Wiring;
use crate::plumbing::{clipboard, links};
use crate::screens::details;
use crate::{DetailsUi, MainWindow};

impl Wiring {
    pub(super) fn rate(&self, like: bool) {
        let current = self.app.details().view().rating.clone();
        details::rate(
            self.app.runtime(),
            details::next_rating(current.as_ref(), like),
        );
    }

    pub(super) fn toggle_library(&self) {
        details::toggle_library(self.app.runtime());
    }

    pub(super) fn toggle_watched(&self) {
        details::toggle_watched(self.app.runtime());
    }

    pub(super) fn share(&self) {
        let Ok(model) = self.app.runtime().model() else {
            return;
        };
        let Some(selected) = model.meta_details.selected.as_ref() else {
            return;
        };
        let Some(item) = details::meta_item(&model.meta_details) else {
            return;
        };
        let link = details::share_link(
            &item.preview.name,
            &selected.meta_path.r#type,
            &selected.meta_path.id,
        );
        drop(model);
        if let Some(window) = self.window.upgrade() {
            window.set_share_link(link.into());
            window.set_share_open(true);
        }
    }

    pub(super) fn close_share(&self) {
        if let Some(window) = self.window.upgrade() {
            window.set_share_open(false);
        }
    }

    pub(super) fn copy_share(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let link = window.get_share_link().to_string();
        let said = match clipboard::copy(&link) {
            Ok(()) => "Link copied".to_owned(),
            Err(error) => error,
        };
        self.say(&said);
    }

    pub(super) fn open_share_target(&self, network: &str) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let link = window.get_share_link().to_string();
        let target = match network {
            "facebook" => format!("https://www.facebook.com/sharer/sharer.php?u={link}"),
            "x" => format!("https://twitter.com/intent/tweet?text={link}"),
            "reddit" => format!("https://www.reddit.com/submit?url={link}"),
            _ => return,
        };
        links::open(&target);
    }
}

pub(super) fn wire(window: &MainWindow, wiring: &Wiring) {
    let ui = window.global::<DetailsUi>();
    let w = wiring.clone();
    ui.on_share(move || w.share());
    let w = wiring.clone();
    ui.on_rate_like(move || w.rate(true));
    let w = wiring.clone();
    ui.on_rate_love(move || w.rate(false));
    let w = wiring.clone();
    ui.on_toggle_library(move || w.toggle_library());
    let w = wiring.clone();
    ui.on_toggle_watched(move || w.toggle_watched());
    let w = wiring.clone();
    window.on_share_close(move || w.close_share());
    let w = wiring.clone();
    window.on_share_copy(move || w.copy_share());
    let w = wiring.clone();
    window.on_share_to(move |net| w.open_share_target(net.as_str()));
}
