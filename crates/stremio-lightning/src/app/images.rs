//! Active-screen image scoping: decides which image URLs belong to the current viewport
//! (protected from cache eviction) and which should be prefetched. Pure plain Rust.

use super::{ActiveScreen, Snapshot};
use crate::plumbing::images::backdrop_key;
use crate::screens::{board, discover, library};

/// Every image the active screen wants now or is about to want (such as the player drawer's other
/// Seasons). Inactive tabs are not prefetched ahead of time, keeping memory bounded to the active
/// screen.
pub(crate) fn snapshot_images(
    snapshot: &Snapshot,
    screen: ActiveScreen,
    board_view: board::Viewport,
    visible: (usize, usize),
    selected: usize,
) -> Vec<String> {
    let mut urls = shown_images(snapshot, screen, board_view, visible, selected);
    if snapshot.player.open || screen == ActiveScreen::Player {
        urls.extend(snapshot.player.every_thumbnail.iter().cloned());
    }
    urls
}

/// The pictures currently visible on the active screen, which the image cache protects from
/// eviction while the cache trims to its byte budget.
pub(crate) fn shown_images(
    snapshot: &Snapshot,
    screen: ActiveScreen,
    board_view: board::Viewport,
    visible: (usize, usize),
    selected: usize,
) -> Vec<String> {
    if snapshot.player.open || screen == ActiveScreen::Player {
        return player_images(snapshot).collect();
    }
    match screen {
        ActiveScreen::Board => board::visible_images_with_spotlight(
            &snapshot.board.rows,
            &snapshot.board.spotlight,
            board_view,
        ),
        ActiveScreen::Discover => discover_images(snapshot, visible, selected).collect(),
        ActiveScreen::Library => library_images(snapshot).collect(),
        ActiveScreen::Calendar => calendar_images(snapshot).collect(),
        ActiveScreen::Addons => addon_images(snapshot).collect(),
        ActiveScreen::Details => details_images(snapshot).collect(),
        ActiveScreen::Player => player_images(snapshot).collect(),
        ActiveScreen::Settings => {
            let sp = &snapshot.settings.spotlight;
            sp.preview_bg
                .iter()
                .chain(sp.preview_logo.iter())
                .chain(sp.catalogs.iter().filter_map(|c| c.preview_bg.as_ref()))
                .cloned()
                .collect()
        }
        ActiveScreen::Mods => Vec::new(),
    }
}

fn discover_images(
    snapshot: &Snapshot,
    visible: (usize, usize),
    selected: usize,
) -> impl Iterator<Item = String> + '_ {
    discover::card_images(&snapshot.discover, visible)
        .chain(discover::preview_images(&snapshot.discover, selected))
}

fn calendar_images(snapshot: &Snapshot) -> impl Iterator<Item = String> + '_ {
    snapshot
        .calendar
        .days
        .iter()
        .flat_map(|day| day.slots.iter())
        .filter_map(|slot| slot.thumbnail.clone().or_else(|| slot.poster.clone()))
}

fn details_images(snapshot: &Snapshot) -> impl Iterator<Item = String> + '_ {
    let preview = if snapshot.player.open {
        None
    } else {
        snapshot.details.preview.as_ref()
    };
    preview
        .and_then(|preview| preview.logo.clone())
        .into_iter()
        .chain(preview.and_then(|preview| preview.background.clone()))
        .chain(
            preview
                .and_then(|preview| preview.background.as_ref())
                .map(|key| backdrop_key(key)),
        )
        .chain(
            snapshot
                .details
                .picked_episode
                .as_ref()
                .filter(move |_| preview.is_some())
                .and_then(|episode| episode.thumbnail.clone()),
        )
        .chain(
            snapshot
                .details
                .episodes
                .iter()
                .filter(move |_| preview.is_some())
                .filter_map(|episode| episode.thumbnail.clone()),
        )
}

fn addon_images(snapshot: &Snapshot) -> impl Iterator<Item = String> + '_ {
    snapshot
        .addons
        .cards
        .iter()
        .filter_map(|card| card.logo.clone())
        .chain(
            snapshot
                .addons
                .pending
                .as_ref()
                .and_then(|pending| pending.logo.clone()),
        )
}

fn library_images(snapshot: &Snapshot) -> impl Iterator<Item = String> + '_ {
    library::card_images(&snapshot.library)
}

fn player_images(snapshot: &Snapshot) -> impl Iterator<Item = String> + '_ {
    snapshot
        .player
        .logo
        .clone()
        .into_iter()
        .chain(snapshot.player.background.clone())
        .chain(
            snapshot
                .player
                .episodes
                .iter()
                .filter_map(|episode| episode.thumbnail.clone()),
        )
}

#[cfg(test)]
#[path = "images_tests.rs"]
mod tests;
