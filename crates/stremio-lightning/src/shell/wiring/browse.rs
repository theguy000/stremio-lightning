//! Board, Discover, Library, Calendar, and viewport scroll/settle callbacks.

use slint::ComponentHandle;
use stremio_core::models::catalog_with_filters::Selected as CatalogSelected;
use stremio_core::models::library_with_filters::Selected as LibrarySelected;
use stremio_core::runtime::msg::{
    Action, ActionCatalogWithFilters, ActionCtx, ActionLibraryWithFilters, ActionLoad,
};
use stremio_core::runtime::RuntimeAction;

use super::{active_screen_of, Wiring};
use crate::app::{ActiveScreen, AppModelField, ScreenMask};
use crate::plumbing::{external_player, links};
use crate::screens::{discover, player};
use crate::MainWindow;

struct ActionTarget {
    id: String,
    kind: String,
    title: String,
    watched: bool,
}

impl Wiring {
    pub(super) fn select_input_option(&self, input: i32, option: i32) {
        let discover = self.app.discover().view();
        let request = usize::try_from(input)
            .ok()
            .and_then(|input| discover.inputs.get(input))
            .and_then(|input| {
                usize::try_from(option)
                    .ok()
                    .and_then(|o| input.values.get(o))
            })
            .cloned();
        if let Some(request) = request {
            self.app.viewport().set_discover_selected(0);
            self.app.runtime().dispatch(RuntimeAction {
                field: Some(AppModelField::Discover),
                action: Action::Load(ActionLoad::CatalogWithFilters(Some(CatalogSelected {
                    request,
                }))),
            });
        }
    }

    pub(super) fn select_library_type(&self, option: i32) {
        let library = self.app.library().view();
        let request = usize::try_from(option)
            .ok()
            .and_then(|option| library.types.values.get(option))
            .cloned();
        if let Some(request) = request {
            self.app.runtime().dispatch(RuntimeAction {
                field: None,
                action: Action::Load(ActionLoad::LibraryWithFilters(LibrarySelected { request })),
            });
        }
    }

    pub(super) fn select_library_sort(&self, option: i32) {
        let library = self.app.library().view();
        let request = usize::try_from(option)
            .ok()
            .and_then(|option| library.sorts.get(option))
            .map(|sort| sort.request.clone());
        if let Some(request) = request {
            self.app.runtime().dispatch(RuntimeAction {
                field: None,
                action: Action::Load(ActionLoad::LibraryWithFilters(LibrarySelected { request })),
            });
        }
    }

    pub(super) fn load_more_library(&self) {
        self.app.runtime().dispatch(RuntimeAction {
            field: None,
            action: Action::LibraryWithFilters(ActionLibraryWithFilters::LoadNextPage),
        });
    }

    fn action_target(&self, id: &str) -> Option<ActionTarget> {
        let target = |c: &crate::screens::Card| ActionTarget {
            id: c.id.clone(),
            kind: c.kind.clone(),
            title: c.title.clone(),
            watched: c.watched,
        };
        let library = self.app.library().view();
        if let Some(card) = library.cards.iter().find(|c| c.id == id) {
            return Some(target(card));
        }
        let board = self.app.board().view();
        board
            .rows
            .iter()
            .filter(|r| r.menu)
            .flat_map(|r| r.cards.iter())
            .find(|c| c.id == id)
            .map(target)
    }

    pub(super) fn item_action(&self, id: String, action: String) {
        let Some(card) = self.action_target(&id) else {
            return;
        };
        match action.as_str() {
            "play" => {
                let message = match player::open_for(self.app.runtime(), &id) {
                    Ok(player::OpenResult::OpenBrowser(url)) => {
                        links::open(&url);
                        String::new()
                    }
                    Ok(player::OpenResult::ExternalPlayer(launch)) => {
                        match external_player::open(&launch) {
                            Ok(()) => {
                                self.say("Stream opened in external player");
                                String::new()
                            }
                            Err(error) => {
                                self.say(&error);
                                error
                            }
                        }
                    }
                    Ok(player::OpenResult::Loaded) => String::new(),
                    Err(error) => format!("Can't play this title: {error}"),
                };
                if let Some(window) = self.window.upgrade() {
                    window
                        .global::<crate::DetailsUi>()
                        .set_play_message(message.into());
                }
            }
            "details" => self.open_item(card.id, card.kind, card.title),
            "watched" => self.app.runtime().dispatch(RuntimeAction {
                field: None,
                action: Action::Ctx(ActionCtx::LibraryItemMarkAsWatched {
                    id,
                    is_watched: !card.watched,
                }),
            }),
            "dismiss" => {
                self.app.runtime().dispatch(RuntimeAction {
                    field: None,
                    action: Action::Ctx(ActionCtx::RewindLibraryItem(id.clone())),
                });
                self.app.runtime().dispatch(RuntimeAction {
                    field: None,
                    action: Action::Ctx(ActionCtx::DismissNotificationItem(id)),
                });
            }
            "remove" => self.app.runtime().dispatch(RuntimeAction {
                field: None,
                action: Action::Ctx(ActionCtx::RemoveFromLibrary(id)),
            }),
            _ => {}
        }
    }

    pub(super) fn select_preview(&self, index: i32) {
        let index = usize::try_from(index).unwrap_or(0);
        self.app.viewport().set_discover_selected(index);
        self.app.viewport().sync_images();
        let discover = self.app.discover().view();
        self.app.mark_dirty(ScreenMask::DISCOVER);
        for url in discover::preview_images(&discover, index) {
            self.request_painting(url);
        }
    }

    pub(super) fn hover_card(&self, index: i32) {
        let index = usize::try_from(index).unwrap_or(0);
        let discover = self.app.discover().view();
        for url in discover::preview_images(&discover, index) {
            self.request_painting(url);
        }
    }

    pub(super) fn section_changed(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let screen = active_screen_of(&window);
        if screen != ActiveScreen::Details {
            Self::clear_details_ui(&window, false);
        }
        self.app.viewport().set_active_screen(screen);
        self.app.mark_dirty(screen.mask());
    }
}

pub(super) fn wire(window: &MainWindow, wiring: &Wiring) {
    let disc = window.global::<crate::DiscoverUi>();
    let w = wiring.clone();
    disc.on_load_next_page(move || {
        w.app.runtime().dispatch(RuntimeAction {
            field: Some(AppModelField::Discover),
            action: Action::CatalogWithFilters(ActionCatalogWithFilters::LoadNextPage),
        });
    });
    let w = wiring.clone();
    disc.on_select_option(move |input, option| w.select_input_option(input, option));
    let w = wiring.clone();
    disc.on_select_item(move |index| w.select_preview(index));
    let w = wiring.clone();
    disc.on_hover_card(move |index| w.hover_card(index));
    let w = wiring.clone();
    disc.on_visible_range(move |s, h, p, c| w.visible_range(s, h, p, c));
    let lib = window.global::<crate::LibraryUi>();
    let w = wiring.clone();
    lib.on_select_type(move |option| w.select_library_type(option));
    let w = wiring.clone();
    lib.on_select_sort(move |option| w.select_library_sort(option));
    let w = wiring.clone();
    lib.on_load_more(move || w.load_more_library());
    let w = wiring.clone();
    lib.on_item_action(move |id, act| w.item_action(id.to_string(), act.to_string()));
    let board = window.global::<crate::BoardUi>();
    let w = wiring.clone();
    board.on_item_action(move |id, act| w.item_action(id.to_string(), act.to_string()));
    let w = wiring.clone();
    board.on_board_visible(move |s, h, cw, vw| w.board_visible(s, h, cw, vw));
    let w = wiring.clone();
    window.on_section_changed(move || w.section_changed());
}
