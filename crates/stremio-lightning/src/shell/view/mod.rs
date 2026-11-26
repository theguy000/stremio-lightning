//! Turning an `App` snapshot into Slint window properties.
//!
//! This is the only place in the app that knows both a screen's data and the window it lands in.
//! It decides nothing: every value here was decided by the screen that produced it.

mod browse;
mod details_player;
mod mods;
mod mods_logs;
pub(crate) mod sync;

pub(crate) use details_player::{settings_view, tracks};
pub(crate) use mods_logs::{apply_logs_data, show_mods};

use slint::ComponentHandle;

use crate::app::{App, ScreenMask, Snapshot};
use crate::plumbing::images::Images;
use crate::{CalendarUi, DetailsUi, MainWindow, PlayerUi, SettingsUi};

/// Drains `app`'s pending dirty `ScreenMask` on the UI thread and projects only the changed
/// screen slices onto `window`, cloning only `Arc` handles from `Snapshot`.
pub(crate) fn apply_dirty(window: &MainWindow, app: &App) {
    let mask = app.take_dirty();
    if mask.is_empty() {
        return;
    }
    let snapshot = app.snapshot();
    apply_mask(window, &snapshot, mask, app.images());
}

pub(crate) fn apply_mask(
    window: &MainWindow,
    snapshot: &Snapshot,
    mask: ScreenMask,
    images: &Images,
) {
    sync::prune_slint_images(images);

    if mask.contains(ScreenMask::BOARD) {
        browse::apply_board(window, &snapshot.board, images);
    }
    if mask.contains(ScreenMask::DISCOVER) {
        browse::apply_discover(window, &snapshot.discover, images);
    }
    if mask.contains(ScreenMask::LIBRARY) {
        browse::apply_library(window, &snapshot.library, images);
    }
    if mask.contains(ScreenMask::ADDONS) {
        browse::apply_addons(window, &snapshot.addons, images);
    }
    if mask.contains(ScreenMask::CALENDAR) {
        window
            .global::<CalendarUi>()
            .set_data(browse::calendar_view(&snapshot.calendar, images));
    }
    if mask.contains(ScreenMask::SETTINGS) {
        window
            .global::<SettingsUi>()
            .set_data(settings_view(&snapshot.settings));
        window
            .global::<DetailsUi>()
            .set_wrap_streams(snapshot.settings.wrap_streams);
        let player_ui = window.global::<PlayerUi>();
        player_ui.set_seek_step(snapshot.settings.seek_time_secs);
        player_ui.set_seek_short_step(snapshot.settings.seek_short_time_secs);
    }
    if mask.contains(ScreenMask::MODS) {
        show_mods(window, &snapshot.mods);
    } else if mask.contains(ScreenMask::MODS_LOGS) {
        apply_logs_data(window, &snapshot.mods.logs);
    }
    if mask.contains(ScreenMask::PLAYER) {
        details_player::player_view(window, &snapshot.player, images);
    }
    if mask.contains(ScreenMask::DETAILS) {
        details_player::apply_details(
            window,
            &snapshot.details,
            snapshot.settings.wrap_streams,
            images,
        );
    }
}
