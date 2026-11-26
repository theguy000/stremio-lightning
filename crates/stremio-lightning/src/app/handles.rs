//! Direct, zero-cost typed screen handles on `App`.
//!
//! Callers (both `shell/` and tests) read/write per-screen state through these handles instead
//! of holding 30 duplicated `Arc<Mutex<...>>` fields or touching `Snapshot` directly.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use super::store::AppStore;
use super::{ActiveScreen, BoardData, GridReport, ScreenMask};
use crate::screens::{addons, board, calendar, details, discover, library, player, settings};

pub struct ViewportHandle<'a>(pub(crate) &'a Arc<AppStore>);
impl<'a> ViewportHandle<'a> {
    pub fn active_screen(&self) -> ActiveScreen {
        *self.0.active_screen.lock().unwrap()
    }

    /// Switches the active screen, lazily materializing its slice if background core events
    /// arrived while it was inactive, and updates image cache pinning.
    pub fn set_active_screen(&self, screen: ActiveScreen) {
        *self.0.active_screen.lock().unwrap() = screen;
        self.0.materialize_if_stale(screen.mask());
        self.0.request_active_images();
    }

    pub fn board_viewport(&self) -> board::Viewport {
        *self.0.board_visible.lock().unwrap()
    }

    pub fn set_board_viewport(&self, next: board::Viewport) {
        *self.0.board_visible.lock().unwrap() = next;
    }

    pub fn discover_selected(&self) -> usize {
        self.0.discover_selected.load(Ordering::Relaxed)
    }

    pub fn set_discover_selected(&self, index: usize) {
        self.0.discover_selected.store(index, Ordering::Relaxed);
    }

    pub fn discover_visible(&self) -> (usize, usize) {
        *self.0.discover_visible.lock().unwrap()
    }

    pub fn set_discover_visible(&self, range: (usize, usize)) {
        *self.0.discover_visible.lock().unwrap() = range;
    }

    pub fn grid_report(&self) -> Option<GridReport> {
        *self.0.discover_grid.lock().unwrap()
    }

    pub fn set_grid_report(&self, report: GridReport) -> Option<GridReport> {
        self.0.discover_grid.lock().unwrap().replace(report)
    }

    pub fn drawn_range(&self) -> (usize, usize) {
        self.0.drawn_range()
    }

    pub fn sync_images(&self) -> bool {
        self.0.sync_images()
    }

    pub fn request_active_images(&self) {
        self.0.request_active_images();
    }

    pub fn request_image(&self, url: String) {
        self.0.request_image(url);
    }
}

pub struct BoardHandle<'a>(pub(crate) &'a Arc<AppStore>);
impl<'a> BoardHandle<'a> {
    pub fn view(&self) -> Arc<BoardData> {
        self.0.materialize_if_stale(ScreenMask::BOARD);
        self.0.snapshot.lock().unwrap().board.clone()
    }
}

pub struct DiscoverHandle<'a>(pub(crate) &'a Arc<AppStore>);
impl<'a> DiscoverHandle<'a> {
    pub fn view(&self) -> Arc<discover::Data> {
        self.0.materialize_if_stale(ScreenMask::DISCOVER);
        self.0.snapshot.lock().unwrap().discover.clone()
    }
}

pub struct LibraryHandle<'a>(pub(crate) &'a Arc<AppStore>);
impl<'a> LibraryHandle<'a> {
    pub fn view(&self) -> Arc<library::Data> {
        self.0.materialize_if_stale(ScreenMask::LIBRARY);
        self.0.snapshot.lock().unwrap().library.clone()
    }
}

pub struct AddonsHandle<'a>(pub(crate) &'a Arc<AppStore>);
impl<'a> AddonsHandle<'a> {
    pub fn view(&self) -> Arc<addons::Data> {
        self.0.materialize_if_stale(ScreenMask::ADDONS);
        self.0.snapshot.lock().unwrap().addons.clone()
    }
}

pub struct CalendarHandle<'a>(pub(crate) &'a Arc<AppStore>);
impl<'a> CalendarHandle<'a> {
    pub fn view(&self) -> Arc<calendar::Data> {
        self.0.materialize_if_stale(ScreenMask::CALENDAR);
        self.0.snapshot.lock().unwrap().calendar.clone()
    }

    pub fn last_library(&self) -> Option<calendar::LibrarySignature> {
        *self.0.calendar_library.lock().unwrap()
    }

    pub fn set_last_library(&self, sig: Option<calendar::LibrarySignature>) {
        *self.0.calendar_library.lock().unwrap() = sig;
    }
}

pub struct DetailsHandle<'a>(pub(crate) &'a Arc<AppStore>);
impl<'a> DetailsHandle<'a> {
    pub fn view(&self) -> Arc<details::Data> {
        self.0.materialize_if_stale(ScreenMask::DETAILS);
        self.0.snapshot.lock().unwrap().details.clone()
    }

    pub fn reset_inputs(&self) {
        self.0
            .details_season
            .store(details::NO_SEASON, Ordering::Relaxed);
        *self.0.details_picked.lock().unwrap() = None;
        *self.0.details_moved.lock().unwrap() = None;
        self.0.details_video_query.lock().unwrap().clear();
    }

    pub fn clear_snapshot(&self) {
        self.0.snapshot.lock().unwrap().details = Arc::new(details::Data::default());
        self.0.mark_dirty(ScreenMask::DETAILS);
    }

    pub fn set_season(&self, season: usize) {
        self.0.details_season.store(season, Ordering::Relaxed);
    }

    pub fn picked(&self) -> Option<String> {
        self.0.details_picked.lock().unwrap().clone()
    }

    pub fn set_picked(&self, picked: Option<String>) {
        *self.0.details_picked.lock().unwrap() = picked;
    }

    pub fn moved(&self) -> Option<(u32, u32)> {
        *self.0.details_moved.lock().unwrap()
    }

    pub fn set_moved(&self, moved: Option<(u32, u32)>) {
        *self.0.details_moved.lock().unwrap() = moved;
    }

    pub fn set_video_query(&self, query: String) {
        *self.0.details_video_query.lock().unwrap() = query;
    }

    pub fn refresh(&self) -> Arc<details::Data> {
        self.0.materialize(ScreenMask::DETAILS);
        self.0.mark_dirty(ScreenMask::DETAILS);
        self.0.sync_images();
        self.0.snapshot.lock().unwrap().details.clone()
    }
}

pub struct PlayerHandle<'a>(pub(crate) &'a Arc<AppStore>);
impl<'a> PlayerHandle<'a> {
    pub fn view(&self) -> Arc<player::Data> {
        self.0.materialize_if_stale(ScreenMask::PLAYER);
        self.0.snapshot.lock().unwrap().player.clone()
    }

    pub fn set_season(&self, season: usize) {
        self.0.player_season.store(season, Ordering::Relaxed);
    }

    pub fn playback_error(&self) -> String {
        self.0.playback_error.lock().unwrap().clone()
    }

    pub fn set_playback_error(&self, error: String) {
        *self.0.playback_error.lock().unwrap() = error;
    }

    pub fn refresh(&self) -> Arc<player::Data> {
        self.0.materialize(ScreenMask::PLAYER);
        self.0.mark_dirty(ScreenMask::PLAYER);
        self.0.sync_images();
        self.0.snapshot.lock().unwrap().player.clone()
    }
}

pub struct SettingsHandle<'a>(pub(crate) &'a Arc<AppStore>);
impl<'a> SettingsHandle<'a> {
    pub fn view(&self) -> Arc<settings::Data> {
        self.0.materialize_if_stale(ScreenMask::SETTINGS);
        self.0.snapshot.lock().unwrap().settings.clone()
    }

    pub fn refresh(&self) -> Arc<settings::Data> {
        self.0.materialize(ScreenMask::SETTINGS);
        self.0.mark_dirty(ScreenMask::SETTINGS);
        self.0.snapshot.lock().unwrap().settings.clone()
    }
}
