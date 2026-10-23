//! The runtime's events, turned into three things: repaint the window, ask for any new images,
//! and keep the sign-in screen's state honest.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use slint::Weak;
use stremio_core::runtime::msg::Event;
use stremio_core::runtime::RuntimeEvent;
use stremio_lightning_env::AppEnv;

use super::{AppModel, AppRuntime, Snapshot};
use crate::plumbing::images::{backdrop_key, Images};
use crate::screens::{auth, discover, library, player, settings};
use crate::shell;
use crate::MainWindow;

/// Everything an event needs in order to repaint. Cloned once per event, because the stream
/// outlives any one of them.
#[derive(Clone)]
pub(crate) struct Context {
    pub(crate) weak: Weak<MainWindow>,
    pub(crate) runtime: Arc<AppRuntime>,
    pub(crate) images: Images,
    pub(crate) latest: Arc<Mutex<Snapshot>>,
    /// The Discover card whose preview panel is open.
    pub(crate) selected: Arc<AtomicUsize>,
    /// The Discover cards on screen, as the first one and how many: the only cards whose posters
    /// are asked for. The grid reports this whenever it is scrolled.
    pub(crate) visible: Arc<Mutex<(usize, usize)>>,
    /// The Season the details screen's episode picker is showing, if it has been touched.
    pub(crate) season: Arc<AtomicUsize>,
    /// The Season the player's side drawer is showing, if it has been touched.
    pub(crate) player_season: Arc<AtomicUsize>,
    /// The Video id whose Streams the details screen is showing, if one was picked.
    pub(crate) picked: Arc<Mutex<Option<String>>>,
    /// What the details screen's search box holds.
    pub(crate) video_query: Arc<Mutex<String>>,
    /// The Season and Episode the no-Streams message's steppers were moved to.
    pub(crate) moved: Arc<Mutex<Option<(u32, u32)>>>,
    /// What mpv said when it could not play the stream it was given. It belongs to the picture,
    /// not to the model, but the snapshot is what the window shows, so it is folded in here.
    pub(crate) playback_error: Arc<Mutex<String>>,
    pub(crate) local_extras: Arc<Mutex<settings::LocalExtras>>,
    pub(crate) auth: auth::Generation,
    /// Repaint now, with whatever the cache holds. Used when an image arrives.
    pub(crate) repaint: Arc<dyn Fn() + Send + Sync>,
}

impl Context {
    fn model(&self) -> Option<std::sync::RwLockReadGuard<'_, AppModel>> {
        self.runtime.model().ok()
    }
    fn extras(&self) -> settings::LocalExtras {
        self.local_extras.lock().unwrap().clone()
    }
}

/// Runs until the runtime stops. One event at a time, in order.
pub(crate) async fn pump(
    events: futures::channel::mpsc::Receiver<RuntimeEvent<AppEnv, AppModel>>,
    context: Context,
) {
    events
        .for_each(move |event| {
            let context = context.clone();
            async move { handle(event, &context) }
        })
        .await;
}

fn handle(event: RuntimeEvent<AppEnv, AppModel>, context: &Context) {
    match event {
        RuntimeEvent::CoreEvent(Event::Error { error, source }) => {
            // Any error means the attempt has finished, so always stop the spinner; only
            // authentication errors belong in the sign-in banner.
            let message = matches!(*source, Event::UserAuthenticated { .. })
                .then(|| auth::error_message(&error));
            let weak = context.weak.clone();
            let _ = weak.upgrade_in_event_loop(move |window| {
                auth::clear_progress(&window);
                if let Some(message) = message {
                    window.set_auth_error(message.into());
                }
            });
            return;
        }
        RuntimeEvent::CoreEvent(Event::UserAuthenticated { .. }) => {
            close_sign_in(context);
            return;
        }
        RuntimeEvent::NewState(..) => {}
        _ => return,
    }

    let Some(model) = context.model() else { return };
    // Whatever signed in, no other browser handshake is still wanted.
    if model.ctx.profile.auth.is_some() {
        close_sign_in(context);
    }
    // A library sync empties the bucket before it fills it again. Publishing that instant would
    // blank every watched mark in the drawer for a frame, which reads as the app losing them.
    let library_blank = model.ctx.library.items.is_empty() && model.ctx.profile.auth.is_some();
    let mut snapshot = snapshot(&model, context);
    drop(model);

    if library_blank {
        let previous = context.latest.lock().unwrap().player.episodes.clone();
        if previous.iter().any(|episode| episode.watched) {
            snapshot.player.episodes = previous;
        }
    }

    *context.latest.lock().unwrap() = snapshot.clone();
    request_images(&snapshot, context);
    shell::show(&context.weak, snapshot, &context.images);
}

/// Rebuilds everything the window shows from the model.
fn snapshot(model: &AppModel, context: &Context) -> Snapshot {
    let (status, rows) = crate::screens::board::data(model);
    let mut player = player::data(
        model,
        player::chosen_season(context.player_season.load(Ordering::Relaxed)),
    );
    // A title whose address could not be resolved says so first; when the model has nothing to
    // say, mpv's own verdict on the address it was given stands.
    if player.error.is_empty() {
        player.error = context.playback_error.lock().unwrap().clone();
    }
    Snapshot {
        status,
        rows,
        discover: discover::data(model),
        library: library::data(model),
        addons: crate::screens::addons::data(model),
        details: crate::screens::details::data(
            model,
            crate::screens::details::chosen_season(context.season.load(Ordering::Relaxed)),
            context.picked.lock().unwrap().as_deref(),
            *context.moved.lock().unwrap(),
            &context.video_query.lock().unwrap(),
        ),
        settings: settings::data(model, &context.extras()),
        player,
    }
}

/// Asks for every image the new snapshot needs, and says which of them the window is showing, so
/// the cache keeps those. The cache ignores keys it has already asked for.
fn request_images(snapshot: &Snapshot, context: &Context) {
    let visible = *context.visible.lock().unwrap();
    let selected = context.selected.load(Ordering::Relaxed);
    // Said before the first request goes out, so the first picture to land is already protected
    // from the ones queued behind it.
    context
        .images
        .set_wanted(shown_images(snapshot, visible, selected));
    for url in snapshot_images(snapshot, visible, selected) {
        let repaint = context.repaint.clone();
        context.images.request(url, move || repaint());
    }
}

/// Every image a snapshot wants on screen, or is about to want: the cards it shows, the Season of
/// Episodes it lists, the drawer's Video pictures and the player's own logo and backdrop.
///
/// This is the one list, so a repaint that never touches the core (a Season switch in the details
/// picker is exactly that) can still ask for the pictures its rows need. Waiting for the next core
/// event instead is what made a later Season's thumbnails crawl in.
pub(crate) fn snapshot_images(
    snapshot: &Snapshot,
    visible: (usize, usize),
    selected: usize,
) -> impl Iterator<Item = String> + '_ {
    board_images(snapshot)
        // Only the cards on screen: the rest of the catalog is asked for as it is scrolled to.
        .chain(discover_images(snapshot, visible, selected))
        .chain(library_images(snapshot))
        .chain(details_images(snapshot))
        .chain(addon_images(snapshot))
        .chain(player_images(snapshot))
        // Every Episode of every Season, so picking another Season in the drawer paints at once
        // instead of leaving its rows as empty rectangles while the pictures arrive.
        .chain(snapshot.player.every_thumbnail.iter().cloned())
}

/// The pictures the window is showing, which are the ones the cache may not let go of: the same
/// list as `snapshot_images` minus the pictures asked for ahead of being seen -- the rest of a
/// Discover catalog, and the thumbnails of Seasons the drawer is not showing.
///
/// That difference is the whole point of it. Asking for a picture early costs a download; holding
/// one costs a poster's worth of memory, and holding every picture ever asked for is what filled
/// the cache with posters nobody was looking at and let go of the ones that were.
pub(crate) fn shown_images(
    snapshot: &Snapshot,
    visible: (usize, usize),
    selected: usize,
) -> impl Iterator<Item = String> + '_ {
    board_images(snapshot)
        .chain(discover_images(snapshot, visible, selected))
        .chain(library_images(snapshot))
        .chain(details_images(snapshot))
        .chain(addon_images(snapshot))
        .chain(player_images(snapshot))
}

/// The Board's rows: Continue Watching and one row per catalog, which is what the home screen
/// draws. Kept whatever screen is showing, because it is the one every visit back from Discover
/// lands on and it does not grow: eight catalogs of ten cards, plus the continued titles.
fn board_images(snapshot: &Snapshot) -> impl Iterator<Item = String> + '_ {
    snapshot
        .rows
        .iter()
        .flat_map(|row| row.cards.iter())
        .filter_map(|card| card.poster.clone())
}

/// A screenful of the Discover grid, and the preview panel's pictures for the card it has open.
fn discover_images(
    snapshot: &Snapshot,
    visible: (usize, usize),
    selected: usize,
) -> impl Iterator<Item = String> + '_ {
    discover::card_images(&snapshot.discover, visible)
        .chain(discover::preview_images(&snapshot.discover, selected))
}

/// The details screen: the panel's logo, the artwork behind it, the blurred copy the column paints
/// under itself, and the Episode picker's thumbnails.
///
/// Nothing while the player is open: it covers the details screen, so its pictures are neither
/// drawn nor worth holding, and the page artwork they include is the largest picture the app has.
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
        // Made from the artwork above, so it is never downloaded on its own.
        .chain(
            preview
                .and_then(|preview| preview.background.as_ref())
                .map(|key| backdrop_key(key)),
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

/// The addon cards' logos: one small picture each, and all of them at once on that screen.
fn addon_images(snapshot: &Snapshot) -> impl Iterator<Item = String> + '_ {
    snapshot
        .addons
        .cards
        .iter()
        .filter_map(|card| card.logo.clone())
}

/// The Library grid's posters. The catalog is already page-limited by the core (100 a page), so
/// this is at most two or three screenfuls; the web's browser fetches the rest lazily.
fn library_images(snapshot: &Snapshot) -> impl Iterator<Item = String> + '_ {
    library::card_images(&snapshot.library)
}

/// The player: its logo, the artwork behind it and the Episode drawer's thumbnails. Empty until a
/// Stream is opened, which is what `player::data` hands back when nothing is picked.
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

/// The sign-in screen is done, whatever finished it.
fn close_sign_in(context: &Context) {
    context.auth.cancel();
    let _ = context.weak.upgrade_in_event_loop(|window| {
        auth::clear_progress(&window);
        window.set_auth_error("".into());
        window.set_auth_open(false);
    });
}

#[cfg(test)]
mod tests {
    use crate::app::Snapshot;
    use crate::screens::{board, details, discover, player, Card, Preview};

    use super::{shown_images, snapshot_images};

    fn card(poster: &str) -> Card {
        Card {
            id: poster.to_owned(),
            kind: "movie".to_owned(),
            title: poster.to_owned(),
            poster: Some(poster.to_owned()),
            progress: 0.0,
        }
    }

    fn preview(poster: &str) -> Preview {
        Preview {
            logo: Some(format!("{poster}#logo")),
            background: Some(format!("{poster}#blur")),
            ..Preview::default()
        }
    }

    fn posters(count: usize) -> Vec<Card> {
        (1..=count)
            .map(|index| card(&format!("https://example.com/card-{index}")))
            .collect()
    }

    /// A window showing a two-card Board row and a three-card Discover catalog.
    fn snapshot() -> Snapshot {
        Snapshot {
            rows: vec![board::Row {
                title: "Popular - Movie".to_owned(),
                shape: "poster",
                cards: vec![
                    card("https://example.com/board-1"),
                    card("https://example.com/board-2"),
                ],
            }],
            discover: discover::Data {
                shape: "poster",
                cards: posters(3),
                inputs: Vec::new(),
                previews: (1..=3)
                    .map(|index| preview(&format!("https://example.com/card-{index}")))
                    .collect(),
            },
            ..Snapshot::default()
        }
    }

    fn listed(list: impl Iterator<Item = String>) -> Vec<String> {
        list.collect()
    }

    #[test]
    fn the_board_is_shown_whatever_screen_the_window_is_on() {
        // The board is what every visit back from Discover lands on, so it is held even while
        // the grid is the screen on the window -- which is when it used to be let go of.
        let snapshot = snapshot();
        let shown = listed(shown_images(&snapshot, (0, 2), 0));
        assert!(shown.contains(&"https://example.com/board-1".to_owned()));
        assert!(shown.contains(&"https://example.com/board-2".to_owned()));
    }

    #[test]
    fn the_screenful_on_display_is_shown_and_the_rest_of_the_catalog_is_not() {
        let snapshot = snapshot();
        let shown = listed(shown_images(&snapshot, (0, 2), 0));
        assert!(shown.contains(&"https://example.com/card-1".to_owned()));
        assert!(shown.contains(&"https://example.com/card-2".to_owned()));
        // The card scrolled out of the range is drawn blank and filled in when it comes back.
        assert!(!shown.contains(&"https://example.com/card-3".to_owned()));
        // The panel's pictures come with the card it has open, and only that one's.
        assert!(shown.contains(&"https://example.com/card-1#logo".to_owned()));
        assert!(shown.contains(&"https://example.com/card-1#blur".to_owned()));
        assert!(!shown.contains(&"https://example.com/card-2#logo".to_owned()));
    }

    #[test]
    fn what_is_asked_for_ahead_of_being_seen_is_asked_for_but_not_held() {
        let mut snapshot = snapshot();
        snapshot.player.every_thumbnail = vec!["https://example.com/season-4.jpg".to_owned()];
        assert!(listed(snapshot_images(&snapshot, (0, 3), 0))
            .contains(&"https://example.com/season-4.jpg".to_owned()));
        assert!(!listed(shown_images(&snapshot, (0, 3), 0))
            .contains(&"https://example.com/season-4.jpg".to_owned()));
    }

    #[test]
    fn the_details_column_is_held_until_the_player_covers_it() {
        let mut snapshot = snapshot();
        snapshot.details.preview = Some(Preview {
            logo: Some("https://example.com/art#logo".to_owned()),
            background: Some("https://example.com/art#page".to_owned()),
            ..Preview::default()
        });
        snapshot.details.episodes = vec![details::Episode {
            thumbnail: Some("https://example.com/episode.jpg".to_owned()),
            ..details::Episode::default()
        }];
        let shown = listed(shown_images(&snapshot, (0, 2), 0));
        assert!(shown.contains(&"https://example.com/art#logo".to_owned()));
        assert!(shown.contains(&"https://example.com/art#page".to_owned()));
        // The column's blurred backdrop is made from the page artwork, and is a key of its own.
        assert!(shown.contains(&"https://example.com/art#backdrop".to_owned()));
        assert!(shown.contains(&"https://example.com/episode.jpg".to_owned()));

        // A player covers the details screen, and brings the artwork behind itself instead.
        snapshot.player.open = true;
        snapshot.player.background = Some("https://example.com/wide#blur".to_owned());
        snapshot.player.episodes = vec![player::Episode {
            thumbnail: Some("https://example.com/drawer.jpg".to_owned()),
            ..player::Episode::default()
        }];
        let shown = listed(shown_images(&snapshot, (0, 2), 0));
        assert!(!shown.contains(&"https://example.com/episode.jpg".to_owned()));
        assert!(!shown.contains(&"https://example.com/art#page".to_owned()));
        assert!(shown.contains(&"https://example.com/wide#blur".to_owned()));
        assert!(shown.contains(&"https://example.com/drawer.jpg".to_owned()));
    }
}
