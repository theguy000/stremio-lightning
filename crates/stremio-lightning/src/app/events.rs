//! The runtime's events, turned into three things: repaint the window, ask for any new images,
//! and keep the sign-in screen's state honest.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use slint::Weak;
use stremio_core::runtime::msg::{Action, ActionCatalogsWithExtra, Event};
use stremio_core::runtime::RuntimeAction;
use stremio_core::runtime::RuntimeEvent;
use stremio_lightning_env::AppEnv;

use super::{AppModel, AppRuntime, Snapshot};
use crate::plumbing::images::{backdrop_key, Images};
use crate::screens::{auth, discover, library, player, settings};
use crate::shell;
use crate::MainWindow;

/// The addons the Board's catalog range was last asked for, and how many catalogs that list had.
/// Both halves matter: installing an addon adds ids, and the catalog list itself arrives a beat
/// after startup, so the count grows on its own.
pub(crate) type BoardCatalogRange = (Vec<String>, usize);

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
    /// Where the app keeps its own files. The Mods screen reads its plugins and themes folders
    /// from here, so a snapshot names the files the user would find there.
    pub(crate) data_dir: std::path::PathBuf,
    /// The page of the Mods menu on screen. The window picks it, so every snapshot the core
    /// produces carries the page the user is looking at rather than the first one.
    pub(crate) mods_page: Arc<Mutex<String>>,
    /// The addons and catalog count the Board's range was last asked for, so that installing
    /// an addon asks for its catalogs and a growing list asks for the rows beyond the old one.
    pub(crate) board_catalogs: Arc<Mutex<Option<BoardCatalogRange>>>,
    /// The catalogs the plugins were last told arrived, by id and how many items each had. A
    /// catalog is told once per arrival; a reload with the same count is still a second arrival.
    pub(crate) said_catalogs: Arc<Mutex<std::collections::HashMap<String, usize>>>,
    pub(crate) auth: auth::Generation,
    /// The plugins the app is running: the Mods pages say what each one is doing, and every
    /// snapshot is drawn from what it holds now.
    pub(crate) plugins: Arc<crate::app::plugins::Runtime>,
    /// The plugin the Plugin settings page is showing, and what it holds for it.
    pub(crate) mods_plugin: Arc<Mutex<String>>,
    pub(crate) mods_edits: Arc<Mutex<crate::screens::mods::settings::Edits>>,
    /// The registry the Marketplace page last read, and what the update check made of the
    /// installed files. Every snapshot carries them, so a snapshot drawn for a core event shows
    /// the Marketplace with its cards rather than the page it drew before the read landed.
    pub(crate) market_listing:
        Arc<Mutex<Option<crate::screens::mods::marketplace::Listing>>>,
    pub(crate) market_updates: Arc<Mutex<crate::screens::mods::marketplace::Updates>>,
    /// What the Marketplace page's search box holds, and the lines under its list.
    pub(crate) market_search: Arc<Mutex<String>>,
    pub(crate) market_summary: Arc<Mutex<Vec<crate::screens::mods::Notice>>>,
    pub(crate) market_installing: Arc<Mutex<String>>,
    /// Repaint now, with whatever the cache holds. Used when an image arrives.
    pub(crate) repaint: Arc<dyn Fn() + Send + Sync>,
    /// What a plugin's ask means for the window, built from the wiring's `Send` parts: the theme
    /// rules it asked for are installed, and the Mods pages are drawn again. A line is already in
    /// the runtime's own record, which is what every card and the Logs page read.
    pub(crate) put_plugins: Arc<dyn Fn(Vec<crate::app::plugins::Asked>) + Send + Sync>,
}

impl Context {
    /// Lets every plugin run the timers that have come due and puts into effect what they asked
    /// for, on this task rather than the window's thread: a plugin call can wait out the
    /// interpreter's whole answer window, and the window does not stop drawing for it.
    ///
    /// This is also the one place a plugin's asks are drained when nothing in the app caused them.
    /// A plugin sets its timers from inside its own callbacks, and a line it logs from one has the
    /// timer's thread as its only reporter; nothing would bring that line to its card if the app
    /// only drained after an event of its own.
    fn tick_plugins(&self) {
        let mut asked = self.plugins.take();
        if self.plugins.holds_timers() {
            asked.extend(self.plugins.advance());
        }
        if !asked.is_empty() {
            (self.put_plugins)(asked);
        }
    }

    fn model(&self) -> Option<std::sync::RwLockReadGuard<'_, AppModel>> {
        self.runtime.model().ok()
    }
    fn extras(&self) -> settings::LocalExtras {
        self.local_extras.lock().unwrap().clone()
    }
}

/// How often the pump asks the plugins whether a timer has come due. The app has no event to
/// wait on for that one: a plugin's timer fires inside a call into it, and no part of the core
/// knows there is a timer. A fifth of a second is under what a person notices in a `setTimeout`
/// and over what the check costs when no plugin has a timer waiting.
pub(crate) const PLUGIN_TICK: std::time::Duration = std::time::Duration::from_millis(200);

/// Runs until the runtime stops. One event at a time, in order, with the plugins looked in on
/// between them.
pub(crate) async fn pump(
    mut events: futures::channel::mpsc::Receiver<RuntimeEvent<AppEnv, AppModel>>,
    context: Context,
) {
    let mut tick = tokio::time::interval(PLUGIN_TICK);
    // A tick that arrives while the last one is still running is dropped rather than queued: the
    // only thing a late tick could do is look at the same timers again.
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            event = events.next() => {
                let Some(event) = event else { return };
                // Plugins first, and before the model is read: a plugin can have logged a line
                // from a timer since the last event, and it belongs in the snapshot below.
                context.tick_plugins();
                handle(event, &context);
            }
            _ = tick.tick() => context.tick_plugins(),
        }
    }
}

/// The catalogs whose first page has arrived since the last snapshot, each as an event a plugin
/// hears. A catalog is remembered by its id and item count: the same catalog reloaded to the same
/// count is a fresh arrival, and an id a plugin was told about once is not told again on every
/// later snapshot.
fn catalogs_arrived(model: &AppModel, context: &Context) -> Vec<crate::plumbing::plugin::Event> {
    use stremio_core::models::common::Loadable;
    let mut said = context.said_catalogs.lock().unwrap();
    let mut events = Vec::new();
    for catalog in &model.board.catalogs {
        let Some(page) = catalog.first() else {
            continue;
        };
        let Some(Loadable::Ready(items)) = &page.content else {
            // A catalog that is loading or failed has not arrived; the next ready one is news
            // even if it brings the same count as the last that did.
            said.remove(&page.request.path.id);
            continue;
        };
        let id = page.request.path.id.clone();
        let count = items.len();
        if said.get(&id) == Some(&count) {
            continue;
        }
        said.insert(id.clone(), count);
        events.push(crate::plumbing::plugin::Event::CatalogLoaded { id, count });
    }
    events
}

/// Asks the core for every catalog the account has, and asks again whenever the addons or the
/// catalog list change. See the call site for why this cannot live with the other wiring.
fn sync_board_catalogs(context: &Context) {
    let count = {
        let Some(model) = context.model() else {
            return;
        };
        let ids = model
            .ctx
            .profile
            .addons
            .iter()
            .map(|addon| addon.manifest.id.clone())
            .collect::<Vec<_>>();
        let count = model.board.catalogs.len();
        let signature = (ids, count);
        {
            let mut known = context.board_catalogs.lock().unwrap();
            if known.as_ref() == Some(&signature) {
                return;
            }
            *known = Some(signature);
        }
        count
    };
    if count == 0 {
        return;
    }
    context.runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::CatalogsWithExtra(ActionCatalogsWithExtra::LoadRange(0..count)),
    });
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

    // The core fetches only the catalogs it is told about, and the startup load names the first
    // eight. Asking again here -- outside the model borrow, because a dispatch takes it -- is what
    // puts an installed addon's catalogs on the Board without a restart.
    sync_board_catalogs(context);
    let Some(model) = context.model() else { return };
    // Whatever signed in, no other browser handshake is still wanted.
    if model.ctx.profile.auth.is_some() {
        close_sign_in(context);
    }
    // A library sync empties the bucket before it fills it again. Publishing that instant would
    // blank every watched mark in the drawer for a frame, which reads as the app losing them.
    let library_blank = model.ctx.library.items.is_empty() && model.ctx.profile.auth.is_some();
    let mut snapshot = snapshot(&model, context);
    // A catalog whose first page has arrived, with how many items it brought. Told before the
    // model is dropped: reading it takes the model, and a plugin call would take it again.
    let arrived = catalogs_arrived(&model, context);
    drop(model);
    if !arrived.is_empty() {
        let asked: Vec<crate::app::plugins::Asked> = arrived
            .into_iter()
            .flat_map(|event| context.plugins.deliver(event))
            .collect();
        if !asked.is_empty() {
            // The window is drawn from the snapshot below anyway; only the rules a plugin asked
            // for need the window's thread before it.
            for ask in asked {
                if let crate::app::plugins::Asked::Theme(rules) = ask {
                    let rules = rules.clone();
                    let weak = context.weak.clone();
                    let _ = weak.upgrade_in_event_loop(move |window| {
                        let _ = crate::plumbing::theme::install(&window, &rules);
                    });
                }
            }
        }
    }

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
        calendar: crate::screens::calendar::data(model),
        details: crate::screens::details::data(
            model,
            crate::screens::details::chosen_season(context.season.load(Ordering::Relaxed)),
            context.picked.lock().unwrap().as_deref(),
            *context.moved.lock().unwrap(),
            &context.video_query.lock().unwrap(),
        ),
        settings: settings::data(model, &context.extras()),
        mods: crate::screens::mods::data(&context.data_dir)
            .with_page(crate::screens::mods::Page::from_id(
                &context.mods_page.lock().unwrap(),
            ))
            .refresh(&context.plugins)
            .with_edits(
                context.mods_plugin.lock().unwrap().clone(),
                context.mods_edits.lock().unwrap().clone(),
            )
            // The line about the last action belongs to the section, not to the folders: it is read
            // back with the rest so a core event -- a library update, a catalog arriving -- does
            // not take the answer to a press off the screen before it was read.
            .with_notice(context.latest.lock().unwrap().mods.notice.clone())
            .with_updates(context.market_updates.lock().unwrap().clone())
            .with_market_parts(
                context.market_listing.lock().unwrap().as_ref(),
                &context.market_search.lock().unwrap(),
                &context.market_summary.lock().unwrap(),
                &context.market_installing.lock().unwrap(),
            ),
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
        .chain(calendar_images(snapshot))
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

/// The Calendar: the posters in the month grid and the Episode art in the list beside it.
fn calendar_images(snapshot: &Snapshot) -> impl Iterator<Item = String> + '_ {
    snapshot
        .calendar
        .days
        .iter()
        .flat_map(|day| day.slots.iter())
        .filter_map(|slot| slot.thumbnail.clone().or_else(|| slot.poster.clone()))
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
        // The logo of the addon being added, while its dialog is up.
        .chain(
            snapshot
                .addons
                .pending
                .as_ref()
                .and_then(|pending| pending.logo.clone()),
        )
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
            watched: false,
            playable: false,
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
                menu: false,
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
