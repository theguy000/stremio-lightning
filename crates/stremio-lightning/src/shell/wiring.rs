//! The wiring: every window callback in one place, each one naming the action it performs.
//!
//! Nothing here decides anything. A callback works out what was clicked, hands it to a screen's
//! action, and puts the result where the window can see it.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, ModelRc, Timer, VecModel, Weak};
use stremio_core::models::catalog_with_filters::Selected as CatalogSelected;
use stremio_core::models::common::Loadable;
use stremio_core::models::library_with_filters::Selected as LibrarySelected;
use stremio_core::runtime::msg::{
    Action, ActionCatalogWithFilters, ActionCtx, ActionLibraryWithFilters, ActionLoad,
};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::profile::GDPRConsent;

use crate::app::events::BoardCatalogRange;
use crate::app::{AppModel, AppModelField, AppRuntime, Snapshot};
use crate::plumbing::clipboard;
use crate::plumbing::images::Images;
use crate::plumbing::links;
use crate::plumbing::player::VideoPlayer;
use crate::plumbing::plugin::Event;
use crate::plumbing::skip;
use crate::plumbing::stream_server::{self, Supervisor};
use crate::plumbing::theme;
use crate::screens::player::{auto_subtitle, SubtitleChoice, SubtitleTrackFacts};
use crate::screens::{addons, auth, calendar, details, discover, mods, player, settings};
use crate::shell;
use crate::MainWindow;

/// How long the Discover grid has to keep still before the posters for what is on screen are
/// fetched. Long enough that a scroll in progress never triggers it, short enough that the
/// screenful is asked for the moment the scrolling stops.
const SETTLE_MS: u64 = 150;

/// How fast the grid has to be moving before its posters wait for it to stop, in pixels a second.
/// A wheel turned at ten notches a second is about a thousand; a flick of a touchpad is three times
/// that, and at that speed a row of cards is on screen for a tenth of a second and nobody has read
/// it. Below this the screenful is fetched as it arrives, as a browser's `loading="lazy"` does.
const FLYING: f32 = 1800.0;

/// The grid's last report: where it is scrolled to, how tall it is, how far apart its rows are, how
/// many columns it has, and when it said so -- the last two numbers are what tells a flick from a
/// scroll. Kept for `fill_the_slack`, which runs after the scrolling has stopped.
static GRID: Mutex<Option<GridReport>> = Mutex::new(None);

#[derive(Clone, Copy)]
struct GridReport {
    scroll: f32,
    height: f32,
    pitch: f32,
    columns: i32,
    at: std::time::Instant,
}

/// Says which pictures the window is showing, so the cache keeps those while it makes room for
/// others. A free function rather than a method: nothing of the window's own is needed, and the
/// events that ask for pictures run it from the thread they arrive on.
fn remember_wanted(
    images: &Images,
    latest: &Mutex<Snapshot>,
    visible: &Mutex<(usize, usize)>,
    selected: usize,
) {
    let range = drawn_range(visible);
    let snapshot = latest.lock().unwrap();
    images.set_wanted(crate::app::events::shown_images(&snapshot, range, selected));
}

/// The cards the Discover grid is drawing, as the last report it sent describes them. It draws the
/// rows on screen and one either side of them, so a poster is worth keeping while its card is still
/// drawn rather than only while it is in the viewport. Before the first report, the screenful the
/// shell starts from.
fn drawn_range(visible: &Mutex<(usize, usize)>) -> (usize, usize) {
    // Read out and the lock let go of, so the two are never held at the same time.
    let grid = *GRID.lock().unwrap();
    match grid {
        Some(grid) => discover::settled_range(grid.scroll, grid.height, grid.pitch, grid.columns),
        None => *visible.lock().unwrap(),
    }
}

/// The plugin runtime and the few things a look at it needs, in a shape that can leave the
/// window's thread. A call into a plugin can wait out the interpreter's answer window, so the
/// event task and the server's task make those calls and hand the result back to the window;
/// this is what they hold instead of the wiring, which holds `Rc`s and cannot go with them.
#[derive(Clone)]
pub(crate) struct Plugins {
    pub(crate) runtime: Arc<crate::app::plugins::Runtime>,
    pub(crate) latest: Arc<Mutex<Snapshot>>,
    pub(crate) window: Weak<MainWindow>,
    pub(crate) images: Images,
    pub(crate) data_dir: std::path::PathBuf,
    /// The app's log records: every line the runtime keeps is already in them, and the Mods
    /// screen reads them for the Logs page.
    pub(crate) logs: Arc<crate::plumbing::logs::Logs>,
    /// Where the diagnostics file was written last, which the About page shows until the app
    /// closes. A plugin's line redraws the section, and the path has to stay on the page.
    pub(crate) export: Arc<Mutex<Option<String>>>,
}

impl Plugins {
    /// Puts what the plugins asked for into effect: the rules a theme asked for, and a fresh
    /// Mods page for anything that logged. A line is already in the runtime's own record, which
    /// is what every card and the Logs page read, so there is nothing here to log twice.
    pub(crate) fn put(&self, asked: Vec<crate::app::plugins::Asked>) {
        let mut theme: Vec<serde_json::Value> = Vec::new();
        let mut changed = false;
        for ask in asked {
            match ask {
                crate::app::plugins::Asked::Log(line, stopped) => {
                    let _ = (line, stopped);
                    changed = true;
                }
                crate::app::plugins::Asked::Theme(more) => {
                    theme.extend(more);
                    changed = true;
                }
                crate::app::plugins::Asked::Changed => changed = true,
            }
        }
        if !changed {
            return;
        }
        if !theme.is_empty() {
            // A plugin's rules go over the enabled theme's, which is what the window showed before
            // it asked: installing the plugin's rules alone would take the theme off the window.
            let rules = theme.clone();
            let (data_dir, logs) = (self.data_dir.clone(), Arc::clone(&self.logs));
            let _ = self.window.upgrade_in_event_loop(move |window| {
                let theme_rules = theme::enabled_rules(&data_dir, &logs);
                let said = theme::install_over(&window, &theme_rules, &rules);
                for warning in &said {
                    logs.record(
                        crate::plumbing::logs::Source::Theme,
                        "",
                        crate::plumbing::logs::Level::Warn,
                        warning,
                    );
                }
            });
        }
        // The whole Mods screen, as a core event builds it: the Logs page's rows come from the
        // app's records, which a line a plugin logged from a timer has just changed. The page on
        // screen and the last notice are the ones the window holds, so a plugin's line never
        // costs the user their page.
        //
        // The Marketplace's registry and the check's answer come off the snapshot as well. They
        // are the shell's, not the folder's: a page built from the folder alone would take the
        // registry's cards off a page that is showing them.
        let mut snapshot = self.latest.lock().unwrap().clone();
        let page = crate::screens::mods::Page::from_id(&snapshot.mods.page);
        let notice = snapshot.mods.notice.clone();
        let market = snapshot.mods.market.clone();
        let updates = snapshot.mods.updates.clone();
        let search = snapshot.mods.market_search.clone();
        snapshot.mods = crate::screens::mods::data(&self.data_dir)
            .with_page(page)
            .refresh(&self.runtime)
            .with_updates(updates)
            .with_market_parts(None, &search, &market.summary, &market.installing)
            .with_logs(&self.logs, false)
            .with_about(
                &crate::screens::settings::LocalExtras::load(&self.data_dir),
                &self.runtime,
                self.export.lock().unwrap().as_ref(),
            )
            .with_notice(notice);
        // The cards themselves come from the registry the page was drawn from, read again
        // against the folder as it is now, so a plugin's line does not empty the list either.
        snapshot.mods.market.note = market.note;
        snapshot.mods.market.empty = market.empty;
        snapshot.mods.market.plugins = market.plugins;
        snapshot.mods.market.themes = market.themes;
        *self.latest.lock().unwrap() = snapshot.clone();
        crate::shell::show(&self.window, snapshot, &self.images);
    }
}

/// Every handle the callbacks need, in one value, so registering a callback is one line.
#[derive(Clone)]
pub(crate) struct Wiring {
    pub(crate) window: Weak<MainWindow>,
    pub(crate) runtime: Arc<AppRuntime>,
    pub(crate) video: VideoPlayer,
    pub(crate) images: Images,
    pub(crate) latest: Arc<Mutex<Snapshot>>,
    pub(crate) selected: Arc<AtomicUsize>,
    /// The Discover cards on screen, as the first one and how many. Only these have their posters
    /// asked for; the grid reports the range again whenever it is scrolled.
    pub(crate) visible: Arc<Mutex<(usize, usize)>>,
    /// The Season the details screen's episode picker is showing, if it has been touched.
    pub(crate) season: Arc<AtomicUsize>,
    /// The Video id whose Streams the details screen is showing, if one was picked.
    pub(crate) picked: Arc<Mutex<Option<String>>>,
    /// What the details screen's search box holds.
    pub(crate) video_query: Arc<Mutex<String>>,
    /// The Season and Episode the no-Streams message's steppers were moved to, while they name a
    /// Video other than the picked one.
    pub(crate) moved: Arc<Mutex<Option<(u32, u32)>>>,
    /// Counts the messages shown at the bottom of the window, so an older one cannot clear a
    /// newer one on its way out.
    pub(crate) said: Arc<AtomicU64>,
    pub(crate) local_extras: Arc<Mutex<settings::LocalExtras>>,
    pub(crate) data_dir: std::path::PathBuf,
    pub(crate) server: Arc<Supervisor>,
    pub(crate) auth: auth::Generation,
    pub(crate) handle: tokio::runtime::Handle,
    /// The Session's own player state: the Season the drawer shows, the stream mpv already has,
    /// and how far through it is.
    pub(crate) player_season: Arc<AtomicUsize>,
    pub(crate) player_loaded: Arc<Mutex<String>>,
    /// The stream mpv was last asked for and refused. Without it the sync timer reads a failed
    /// play as "not loaded yet" and reloads the same address five times a second, each load
    /// cancelling the one before it -- so a stream that needed a moment to open never got one.
    pub(crate) player_attempted: Arc<Mutex<Option<String>>>,
    pub(crate) playback_error: Arc<Mutex<String>>,
    pub(crate) playback: Arc<Mutex<Playback>>,
    pub(crate) player_sync: Arc<Timer>,
    /// Restarted by the Discover grid every time it moves; fires when it has stopped, which is when
    /// the row of cards either side of the screen is fetched.
    pub(crate) settle: Arc<Timer>,
    /// What the library looked like when the Calendar last loaded: how many items, and the
    /// newest of them. A visit with the same signature reuses the month already in the model.
    pub(crate) calendar_library: Arc<Mutex<Option<calendar::LibrarySignature>>>,
    /// The addons the Board's catalog range was last asked for. Installing or removing one has to
    /// ask again, or its catalogs stay on the list the core never fetched.
    pub(crate) board_catalogs: Arc<Mutex<Option<BoardCatalogRange>>>,
    /// What the tracks looked like the last time they were handed to the window. mpv reports the
    /// same list ten times a second, and rebuilding the model each time destroys the row under the
    /// pointer -- which is why a press on a subtitle row could do nothing at all.
    pub(crate) shown_tracks: Arc<Mutex<ShownTracks>>,
    /// The subtitle the viewer picked in this session, in the web's own terms: whether subtitles
    /// are on, which side the track came from, and in which language. It outranks the profile's
    /// preferred language until the player closes.
    pub(crate) subtitle_choice: Arc<Mutex<SubtitleChoice>>,
    /// Whether this stream has already been auto-selected for. Once per load, as the web does it
    /// (`appliedTrack`), so a track the viewer picks is never overruled a moment later.
    pub(crate) subtitle_auto: Arc<Mutex<bool>>,
    /// The subtitle language the menu's Variants column is showing. Pressing a language row only
    /// looks -- it never touches the picture -- so this is the one piece of the menu that is the
    /// window's own state and not mpv's. Empty means "whichever language is playing", so a fresh
    /// player starts out following the track auto-selection chose.
    pub(crate) browsed_language: Arc<Mutex<String>>,
    /// Which page of the Mods menu is on screen. The shell owns the choice, as it owns which
    /// section is showing; the Mods screen only describes each page's content.
    pub(crate) mods_page: Arc<Mutex<String>>,
    /// The Mods card whose Remove button was pressed once and waits for the second press, with
    /// the page and the button it was pressed on. Pressing anything else, changing page, or this
    /// timer firing clears it; the second press is the one that removes the file.
    pub(crate) mods_armed: Arc<Mutex<Option<ArmedMod>>>,
    pub(crate) mods_disarm: Arc<Timer>,
    /// The plugins the app is running. Every switch, save and Remove on the Mods screen goes
    /// through it, and so does everything a plugin asks the app for.
    pub(crate) plugins: Arc<crate::app::plugins::Runtime>,
    /// The plugin the Plugin settings page is showing, by file name. It opens on the first
    /// plugin installed, and says so when none is.
    pub(crate) mods_plugin: Arc<Mutex<String>>,
    /// What the settings page holds for the plugin on screen: its schema, the values its file
    /// holds, and anything typed since. The page is built from this rather than read again on
    /// every press, so a control cannot lose what was typed while another was pressed.
    pub(crate) mods_edits: Arc<Mutex<crate::screens::mods::settings::Edits>>,
    /// Delays drawing while a text box is typed in. The page writes typed text on every press,
    /// and redrawing it on every press would take the editor out from under the next letter.
    pub(crate) mods_typed: Arc<Timer>,
    /// The registry the Marketplace page last read, kept by the window: reading it again is a
    /// request, so the page opens on what was read until Check for updates asks again. `None`
    /// until the page has been opened once.
    pub(crate) market_listing: Arc<Mutex<Option<mods::marketplace::Listing>>>,
    /// A registry read that is in flight, so a page shown again never asks for a second.
    pub(crate) market_asked: Arc<Mutex<bool>>,
    /// What the Marketplace's search box holds. Filtering happens on the page's own thread; the
    /// box asks nothing of the registry.
    pub(crate) market_search: Arc<Mutex<String>>,
    /// What one check made of the installed files, for the two installed pages and for whether a
    /// Marketplace card offers Update.
    pub(crate) market_updates: Arc<Mutex<mods::marketplace::Updates>>,
    /// A check that is in flight, so Check for updates is one request however often it is pressed.
    pub(crate) market_check_asked: Arc<Mutex<bool>>,
    /// The lines the Marketplace page shows about its presses, newest last.
    pub(crate) market_summary: Arc<Mutex<Vec<mods::Notice>>>,
    /// The card being fetched right now, by its download, empty when none is.
    pub(crate) market_installing: Arc<Mutex<String>>,
    /// The app's log records, which the Logs page lists. Shared with the plugin runtime, which
    /// records every line its plugins log, and with the rest of the shell where an error is worth
    /// a record.
    pub(crate) logs: Arc<crate::plumbing::logs::Logs>,
    /// The Logs page's Clear button, pressed once and waiting for the second press: the window's
    /// own state, like an armed Remove, because the screen only describes what it shows.
    pub(crate) mods_clearing: Arc<Mutex<bool>>,
    /// Where the diagnostics file was written, shown on the About page until the app closes.
    pub(crate) mods_export: Arc<Mutex<Option<String>>>,
}

/// A Mods card that is one press away from its file being deleted, with the page it was on. A
/// second press on the same card, on the same page, is what deletes the file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ArmedMod {
    pub(crate) id: String,
    pub(crate) page: String,
}

/// How long typing has to pause before the page draws what was typed. A tenth of a second is
/// under what a person sees and over what the window takes to draw.
const TYPING_PAUSE_MS: u64 = 100;

/// How long the window's thread waits after the Marketplace's news before it draws it. A registry
/// read and a check land as two pieces of news within a few milliseconds of each other, and
/// drawing after each would draw the page twice with half of it out of date. One short wait lets
/// both land, and a worst case of a blocked thread, land before the page is worked out on it.
const MARKET_SETTLE_MS: u64 = 20;

thread_local! {
    /// The timer that starts the Marketplace's draw, made on the window's thread because Slint's
    /// timers are bound to the thread that made them.
    static TIMER: Timer = Timer::default();
}

/// The Marketplace draws that have landed and not been drawn yet, oldest first.
static PENDING: Mutex<Vec<MarketSettler>> = Mutex::new(Vec::new());

/// Whether a starter is posted, or the timer it armed is still to run. Guarded together with
/// `PENDING` by `ASKING`, so the two are always read and written as one fact.
static ASKING: Mutex<bool> = Mutex::new(false);

/// What a task that is not the window's thread holds so it can ask for the Marketplace to be
/// drawn once its news has landed. It holds only `Send` parts of the wiring, because the wiring
/// itself holds `Rc`s. The draw is asked for through a timer on the window's thread: that is
/// where the page is worked out and shown, so the snapshot and the screen never disagree.
///
/// Slint's `Timer` belongs to the thread that made it, so a task leaves itself in `PENDING` and
/// posts one starter; the starter runs on the window's thread and the timer it starts draws the
/// last of what was left.
/// Runs on the window's thread: takes what is waiting and arms the timer that draws it. Called
/// again by that timer if news landed while the page was being drawn.
fn ask() {
    let last = {
        // One lock for the pair, so a `settle` that lands here is either drawn by this take or
        // sees the flag clear and posts a starter of its own. Never both, never neither.
        let mut asking = ASKING.lock().unwrap();
        let mut pending = PENDING.lock().unwrap();
        let last = pending.last().cloned();
        pending.clear();
        if last.is_none() {
            *asking = false;
        }
        last
    };
    let Some(last) = last else {
        return;
    };
    TIMER.with(|timer| {
        timer.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_millis(MARKET_SETTLE_MS),
            move || {
                last.draw();
                // Drawn on the window's thread, so the next draw is armed from here: posting a
                // starter from this thread would wait for an event loop this callback holds.
                ask();
            },
        );
    });
}

#[derive(Clone)]
struct MarketSettler {
    window: Weak<MainWindow>,
    latest: Arc<Mutex<Snapshot>>,
    images: Images,
    mods_page: Arc<Mutex<String>>,
    listing: Arc<Mutex<Option<mods::marketplace::Listing>>>,
    search: Arc<Mutex<String>>,
    updates: Arc<Mutex<mods::marketplace::Updates>>,
    summary: Arc<Mutex<Vec<mods::Notice>>>,
    installing: Arc<Mutex<String>>,
}

impl MarketSettler {
    /// Asks the window to draw the Marketplace once the news that has just landed settles. The
    /// news is already in the shared state, so the timer callback reads it; this adds nothing.
    ///
    /// `ASKING` is what decides whether a starter is needed, not how many items are waiting: a
    /// second piece of news has to keep its own starter, and counting the queue dropped one when
    /// the first starter drained the first item before the second was pushed.
    fn settle(&self) {
        let window = self.window.clone();
        let mut asking = ASKING.lock().unwrap();
        PENDING.lock().unwrap().push(self.clone());
        if *asking {
            return;
        }
        *asking = true;
        drop(asking);
        // Posted after the flag is set: the starter is allowed to take it straight back if it
        // runs at once, which is what a starter posted to the thread that is already running does.
        let _ = window.upgrade_in_event_loop(|_| ask());
    }

    /// Works the page out from the state as it is now and shows it.
    fn draw(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let mut held = self.latest.lock().unwrap();
        let page = mods::Page::from_id(&self.mods_page.lock().unwrap());
        let mut data = mods::data(&held.mods.data_dir).with_page(page);
        data.updates = self.updates.lock().unwrap().clone();
        data = data.with_market_parts(
            self.listing.lock().unwrap().as_ref(),
            &self.search.lock().unwrap(),
            &self.summary.lock().unwrap(),
            &self.installing.lock().unwrap(),
        );
        data.notice = held.mods.notice.clone();
        held.mods = data;
        let snapshot = held.clone();
        drop(held);
        // One write of the window's snapshot, and the section drawn from that same snapshot, so
        // the page the window shows is the page the next event will work from.
        *self.latest.lock().unwrap() = snapshot.clone();
        shell::show(&self.window, snapshot.clone(), &self.images);
        shell::view::show_mods(&window, &snapshot.mods);
    }
}

/// Draws the Mods section after an install has landed: the folder read again, the line about the
/// install in the page's summary, and the card now saying Uninstall. Nothing is turned on.
fn after_install(latest: &Arc<Mutex<Snapshot>>, settle: &MarketSettler, notice: mods::Notice) {
    let mut held = latest.lock().unwrap();
    let data_dir = held.mods.data_dir.clone();
    // The folder is read again, which is what turns the card from Install to Uninstall. Only the
    // folder's lists are written here: the page, the registry and the check's answer are left as
    // the snapshot has them, so news that landed while the fetch ran is not lost.
    let data = mods::data(&data_dir);
    held.mods.plugins = data.plugins;
    held.mods.themes = data.themes;
    held.mods.notice = notice;
    drop(held);
    settle.settle();
}

/// How long an armed Remove stays armed. Long enough to press twice on purpose, short enough that
/// a card left alone never deletes anything on a later, unrelated press.
const DISARM_MS: u64 = 5000;

/// The track lists the window was last given, so the same list is not handed over again and again.
#[derive(Clone, Default, PartialEq)]
pub(crate) struct ShownTracks {
    audio: Vec<crate::plumbing::player::Track>,
    subtitles: Vec<crate::plumbing::player::Track>,
    /// The subtitle files the addons offered for this stream. They are rows in the menu before
    /// they are tracks in mpv: picking one is what adds the file.
    candidates: Vec<crate::screens::player::Subtitle>,
    languages: Vec<(String, String)>,
    active_language: String,
    /// Whose variants the next column lists: the language being browsed, or the playing one when
    /// nothing has been browsed yet.
    browsed_language: String,
}

/// Where the picture is, as the player last heard it. The core is told about the play head once
/// a second, not on every tick, so a long film is not a stream of actions.
#[derive(Clone)]
pub(crate) struct Playback {
    pub(crate) time: f64,
    pub(crate) duration: f64,
    pub(crate) sent_time: f64,
    pub(crate) paused: Option<bool>,
    pub(crate) scale: stremio_core::types::player::VideoScale,
    /// The Video the plugins were last told is playing, empty when they were told it stopped.
    /// mpv reports the same file forty times a second, and a plugin hears about it once.
    pub(crate) said: String,
    /// The skip segments of the Video on screen, and which Video they were fetched for: a Video
    /// that has none keeps its id here so nothing is asked for it twice.
    pub(crate) skip: Vec<skip::Segment>,
    pub(crate) skip_for: String,
    /// The segment holding the play head, which is what the button acts on.
    pub(crate) skip_now: Option<skip::Segment>,
    /// How many segments were last said on the Logs page, so a change is one line.
    pub(crate) skip_total: usize,
    /// The marks last handed to the seek bar, as fractions of the length. Kept so a bar that has
    /// not changed is not handed a fresh model forty times a second.
    pub(crate) skip_marks: Vec<(f32, f32)>,
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            time: 0.0,
            duration: 0.0,
            sent_time: 0.0,
            paused: None,
            scale: stremio_core::types::player::VideoScale::Contain,
            said: String::new(),
            skip: Vec::new(),
            skip_for: String::new(),
            skip_now: None,
            skip_total: 0,
            skip_marks: Vec::new(),
        }
    }
}

/// The data the action list needs from whichever card opened it.
struct ActionTarget {
    id: String,
    kind: String,
    title: String,
    watched: bool,
}

impl Wiring {
    /// Repaints with the snapshot the last event produced.
    fn repaint(&self) {
        let snapshot = self.latest.lock().unwrap().clone();
        shell::show(&self.window, snapshot, &self.images);
        self.remember_wanted();
    }

    /// Repaints with a snapshot the caller has just changed, and asks for every picture that
    /// snapshot shows. A Season switch in the details picker never goes near the core, so without
    /// this its thumbnails waited for the next core event -- which is why a later Season crawled
    /// in while the first one, requested during a real core update, arrived at once.
    fn repaint_with(&self, snapshot: Snapshot) {
        *self.latest.lock().unwrap() = snapshot.clone();
        // The Mods section is written before the rest of the window. `shell::show` posts its own
        // copy of the same snapshot, which repaints the Mods section from it when it lands; a
        // marketplace card pressed by then would be pressed on the copy this write had just
        // replaced.
        if let Some(window) = self.window.upgrade() {
            shell::view::show_mods(&window, &snapshot.mods);
        }
        shell::show(&self.window, snapshot.clone(), &self.images);
        self.remember_wanted();
        let visible = *self.visible.lock().unwrap();
        let selected = self.selected.load(Ordering::Relaxed);
        for url in crate::app::events::snapshot_images(&snapshot, visible, selected) {
            self.request_painting(url);
        }
    }

    /// Asks for a picture and repaints when it lands. Everything the window is waiting to show goes
    /// through here: a key asked for without this arrives in the cache and stays a grey rectangle,
    /// which is what a grid scrolled into view used to look like.
    ///
    /// It does not say what is on screen: every caller of this asked for the picture *after*
    /// saying so, which is why the key is already being kept when it lands.
    fn request_painting(&self, url: String) {
        let (latest, window, images) = (
            self.latest.clone(),
            self.window.clone(),
            self.images.clone(),
        );
        self.images.request(url, move || {
            let snapshot = latest.lock().unwrap().clone();
            let images = images.clone();
            let _ = window.upgrade_in_event_loop(move |window| {
                crate::shell::view::apply(&window, &snapshot, &images);
            });
        });
    }

    fn open_item(&self, id: String, kind: String, _title: String) {
        // Drop what the previous item left behind so it can't flash up before the new data.
        self.latest.lock().unwrap().details = details::Data::default();
        // A new title opens on the Season the core suggests, not the one last browsed, and with
        // no Episode picked.
        self.season.store(details::NO_SEASON, Ordering::Relaxed);
        *self.picked.lock().unwrap() = None;
        *self.moved.lock().unwrap() = None;
        *self.video_query.lock().unwrap() = String::new();
        if let Some(window) = self.window.upgrade() {
            window.set_details_has_preview(false);
            window.set_play_message("".into());
            window.set_streams(ModelRc::default());
            window.set_streams_loading(0);
            window.set_details_seasons(ModelRc::default());
            window.set_details_episodes(ModelRc::default());
            window.set_details_show_picker(false);
        }
        details::open(&self.runtime, &id, &kind);
    }

    /// Recomputes the details screen from the model and everything the user has touched there:
    /// the Season being browsed, the Video picked, the steppers' position, the search box.
    fn refresh_details(&self, model: &AppModel) {
        let picked = self.picked.lock().unwrap().clone();
        let moved = *self.moved.lock().unwrap();
        let chosen = details::chosen_season(self.season.load(Ordering::Relaxed));
        let query = self.video_query.lock().unwrap().clone();
        let mut snapshot = self.latest.lock().unwrap().clone();
        snapshot.details = details::data(model, chosen, picked.as_deref(), moved, &query);
        self.repaint_with(snapshot);
    }

    /// Browsing the episode picker: nothing is loaded, so this only remembers the Season and
    /// repaints the list.
    fn select_season(&self, index: i32) {
        let Some(season) = usize::try_from(index).ok().and_then(|index| {
            self.latest
                .lock()
                .unwrap()
                .details
                .seasons
                .get(index)
                .map(|season| season.number)
        }) else {
            return;
        };
        self.season.store(season as usize, Ordering::Relaxed);
        let Ok(model) = self.runtime.model() else {
            return;
        };
        self.refresh_details(&model);
    }

    /// The search box changed: refilter the rows, nothing to load.
    fn search_videos(&self, query: String) {
        *self.video_query.lock().unwrap() = query;
        let Ok(model) = self.runtime.model() else {
            return;
        };
        self.refresh_details(&model);
    }

    /// Picking an Episode: remember it, then ask the core for that Video's Streams.
    fn pick_video(&self, video_id: String) {
        *self.picked.lock().unwrap() = Some(video_id.clone());
        // The steppers start over from the Video that is now on screen.
        *self.moved.lock().unwrap() = None;
        details::pick(&self.runtime, &video_id);
    }

    /// One press of a stepper in the no-Streams message. It only moves the pair on screen;
    /// "Show Streams" is what asks the addons.
    fn step(&self, seasons: i32, episodes: i32) {
        let Ok(model) = self.runtime.model() else {
            return;
        };
        let Some(item) = details::meta_item(&model.meta_details) else {
            return;
        };
        let Some(picked) = self.picked.lock().unwrap().clone() else {
            return;
        };
        let moved = *self.moved.lock().unwrap();
        let Some(at) = details::shown_pair(&item.videos, &picked, moved) else {
            return;
        };
        *self.moved.lock().unwrap() = Some(details::step(&item.videos, at, seasons, episodes));
        self.refresh_details(&model);
    }

    /// Web: "Show Streams" — the Video the steppers name, loaded the way a row is.
    fn show_streams(&self) {
        let Ok(model) = self.runtime.model() else {
            return;
        };
        let Some(item) = details::meta_item(&model.meta_details) else {
            return;
        };
        let Some(picked) = self.picked.lock().unwrap().clone() else {
            return;
        };
        let moved = *self.moved.lock().unwrap();
        let Some(at) = details::shown_pair(&item.videos, &picked, moved) else {
            return;
        };
        let Some(video) = details::stepped_video(&item.videos, at).map(str::to_owned) else {
            return;
        };
        drop(model);
        self.pick_video(video);
    }

    /// Next Episode: whatever the core says follows the pick, selected the same way a row is.
    fn next_episode(&self) {
        let next = self.latest.lock().unwrap().details.next_episode.clone();
        if let Some(next) = next {
            self.pick_video(next);
        }
    }

    /// A rating button: the like or the love, whichever was pressed, toggled against what the
    /// core last said the title carries.
    fn rate(&self, like: bool) {
        let current = self.latest.lock().unwrap().details.rating.clone();
        details::rate(&self.runtime, details::next_rating(current.as_ref(), like));
    }

    /// The library pair's first cell: out of the library it adds the title, in it it removes it.
    fn toggle_library(&self) {
        details::toggle_library(&self.runtime);
    }

    /// The library pair's second cell: marks the title watched, or not.
    fn toggle_watched(&self) {
        details::toggle_watched(&self.runtime);
    }

    /// The share button: works out the link web would offer and opens the dialog that offers it.
    fn share(&self) {
        let Ok(model) = self.runtime.model() else {
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
        let Some(window) = self.window.upgrade() else {
            return;
        };
        window.set_share_link(link.into());
        window.set_share_open(true);
    }

    /// The dialog's way out, by its close button or a click on the cover.
    fn close_share(&self) {
        if let Some(window) = self.window.upgrade() {
            window.set_share_open(false);
        }
    }

    /// The dialog's Copy button. The dialog stays open, as it does on the site, and the window
    /// says what happened at the bottom.
    fn copy_share(&self) {
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

    /// One of the dialog's networks: hand the link to the site that shares it.
    fn open_share_target(&self, network: &str) {
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
        crate::plumbing::links::open(&target);
    }

    /// Puts one line at the bottom of the window for a moment. The count makes the last message
    /// win: a copy made while an earlier one is still fading does not get cleared early.
    fn say(&self, text: &str) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        window.set_toast(text.into());
        let generation = self.said.fetch_add(1, Ordering::Relaxed) + 1;
        let said = self.said.clone();
        let window = self.window.clone();
        self.handle.spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(2200)).await;
            if said.load(Ordering::Relaxed) == generation {
                let _ = window.upgrade_in_event_loop(|window| window.set_toast("".into()));
            }
        });
    }

    /// Back to the Episode Picker: the pick is gone, and the core drops the Streams with it.
    fn back_to_episodes(&self) {
        *self.picked.lock().unwrap() = None;
        *self.moved.lock().unwrap() = None;
        details::back_to_episodes(&self.runtime);
    }

    /// The Streams list's cut/wrap toggle. The choice is this shell's own, so it goes to
    /// `settings.json` beside the profile rather than through the core, and the screen
    /// only has to redraw: no request is sent for it.
    fn toggle_stream_wrap(&self) {
        let wrap = {
            let mut extras = self.local_extras.lock().unwrap();
            extras.wrap_streams = !extras.wrap_streams;
            extras.save(&self.data_dir);
            extras.wrap_streams
        };
        let mut snapshot = self.latest.lock().unwrap().clone();
        snapshot.settings.wrap_streams = wrap;
        self.repaint_with(snapshot);
    }

    /// A Stream was clicked: the player screen opens on it, and the core loads the rest.
    fn play_stream(&self, index: i32) {
        let index = usize::try_from(index).unwrap_or(usize::MAX);
        let opened = player::open(&self.runtime, index);
        let message = match opened {
            Ok(()) => String::new(),
            Err(error) => format!("Can't play this stream: {error}"),
        };
        if let Some(window) = self.window.upgrade() {
            window.set_play_message(message.into());
        }
    }

    // ---- the player screen ----

    /// The picture's state, as mpv reports it. One push per change, so a paused player costs
    /// nothing: the window, then the core, and then whatever the end of a stream means.
    pub(crate) fn playback_changed(&self, status: crate::plumbing::player::Status) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        window.set_player_loaded(status.loaded);
        window.set_player_buffering(status.buffering);
        window.set_player_time(status.time.unwrap_or_default() as f32);
        window.set_player_duration(status.duration.unwrap_or_default() as f32);
        window.set_player_buffered(status.buffered.unwrap_or_default() as f32);
        window.set_player_paused(status.paused.unwrap_or(true));
        window.set_player_seekable(
            status.duration.is_some_and(|duration| duration > 0.0)
                && status.time.is_some()
                && status.paused.is_some(),
        );
        window.set_player_volume(status.volume.unwrap_or_default() as f32);
        window.set_player_muted(status.muted.unwrap_or(false));
        window.set_player_speed(status.speed.unwrap_or(1.0) as f32);
        // The tracks only reach the window when they change. mpv repeats the same list ten times a
        // second, and handing the window a fresh model each time rebuilds the rows -- which threw
        // away the row under the pointer mid-press, so a click on a subtitle did nothing, and made
        // the selected dot flicker.
        let candidates = self.latest.lock().unwrap().player.subtitles.clone();
        let active_language = status
            .subtitles
            .iter()
            .find(|track| track.selected)
            .map(|track| track.language.clone())
            .unwrap_or_default();
        let shown = ShownTracks {
            audio: status.audio.clone(),
            subtitles: status.subtitles.clone(),
            candidates: candidates.clone(),
            languages: crate::screens::player::subtitle_languages(
                &status.subtitles,
                &candidates,
                &[
                    self.subtitles_language().as_str(),
                    self.interface_language().as_str(),
                ],
            ),
            active_language: active_language.clone(),
            browsed_language: browsed_language(
                self.browsed_language.lock().unwrap().as_str(),
                &active_language,
            ),
        };
        let changed = {
            let mut last = self.shown_tracks.lock().unwrap();
            if *last == shown {
                None
            } else {
                *last = shown.clone();
                Some(shown)
            }
        };
        if let Some(shown) = changed {
            self.emit_tracks(&shown);
        }
        window.set_player_subtitles_enabled(status.subtitles.iter().any(|track| track.selected));
        window.set_player_subtitles_size((status.subtitles_scale * 100.0) as f32);
        self.tell_playback(status.loaded);
        window.set_player_subtitles_offset((status.subtitles_delay * 1000.0) as f32);
        window.set_player_subtitles_position(status.subtitles_position as f32);

        // Auto-selection, as stremio-web does it, and once per stream: when the file has opened
        // and the viewer has not said otherwise for this player.
        if status.loaded && !status.subtitles.is_empty() {
            let applied = {
                let mut applied = self.subtitle_auto.lock().unwrap();
                if *applied {
                    true
                } else {
                    *applied = true;
                    false
                }
            };
            if !applied {
                let facts: Vec<SubtitleTrackFacts> = status
                    .subtitles
                    .iter()
                    .map(|track| SubtitleTrackFacts {
                        id: track.id.clone(),
                        language: track.language.clone(),
                        external: track.external,
                    })
                    .collect();
                let choice = self.subtitle_choice.lock().unwrap().clone();
                let preferred = self.subtitles_language();
                let wanted = auto_subtitle(&facts, &choice, &preferred);
                match wanted {
                    Some(id) => {
                        let _ = self.video.select_subtitles(Some(&id));
                    }
                    None => {
                        let _ = self.video.select_subtitles(None);
                    }
                }
            }
        }

        if status.error.is_empty() {
            // A file that is open has no error, whatever the one before it said.
            if !self.playback_error.lock().unwrap().is_empty() {
                *self.playback_error.lock().unwrap() = String::new();
                window.set_player_error("".into());
            }
        } else if *self.playback_error.lock().unwrap() != status.error {
            *self.playback_error.lock().unwrap() = status.error.clone();
            window.set_player_error(status.error.clone().into());
            // What mpv said about the address it was given is a record when it changes: the same
            // sentence reaches here many times a second while the picture holds.
            self.logs.record(
                crate::plumbing::logs::Source::App,
                "Player",
                crate::plumbing::logs::Level::Error,
                &status.error,
            );
        }

        let (time, duration, sent, was_paused) = {
            let mut playback = self.playback.lock().unwrap();
            let was_paused = playback.paused;
            playback.time = status.time.unwrap_or_default();
            playback.duration = status.duration.unwrap_or_default();
            playback.paused = status.paused;
            (
                playback.time,
                playback.duration,
                playback.sent_time,
                was_paused,
            )
        };
        // The core keeps the resume point: once a second is enough for a library item whose
        // updates are pushed to the server every three minutes anyway.
        if status.loaded && (time - sent).abs() >= 1.0 {
            self.playback.lock().unwrap().sent_time = time;
            player::time_changed(&self.runtime, time, duration, "mpv".to_owned());
        }
        // The skip button. The segments are fetched once per Video, from the model rather than
        // the last snapshot: the core knows the Video the moment it is picked, and the snapshot
        // only after a repaint. Every status then decides whether the play head is inside one.
        // Each step says what it did on the Logs page, so a button that never appears can say why.
        let skip_wanted = self.local_extras.lock().unwrap().skip_intros;
        if !skip_wanted {
            let mut playback = self.playback.lock().unwrap();
            if !playback.skip.is_empty() {
                self.logs.record(
                    crate::plumbing::logs::Source::App,
                    "Skip",
                    crate::plumbing::logs::Level::Info,
                    "Skip intros is switched off; the segments are dropped.",
                );
            }
            playback.skip.clear();
            playback.skip_for.clear();
        } else if status.loaded && duration > 0.0 {
            if let Some((video, meta_id, season, episode)) = self.skip_target() {
                let ask = {
                    let mut playback = self.playback.lock().unwrap();
                    if playback.skip_for == video {
                        false
                    } else {
                        playback.skip.clear();
                        playback.skip_for = video.clone();
                        true
                    }
                };
                if ask {
                    self.logs.record(
                        crate::plumbing::logs::Source::App,
                        "Skip",
                        crate::plumbing::logs::Level::Info,
                        format!(
                            "Asking AniSkip for {meta_id} season {season} episode {episode} ({duration:.0}s)"
                        ),
                    );
                    let playback = Arc::clone(&self.playback);
                    let logs = Arc::clone(&self.logs);
                    let for_video = video;
                    self.handle.spawn(async move {
                        let found = skip::segments(&meta_id, season, episode, duration).await;
                        let listing = found
                            .iter()
                            .map(|segment| {
                                format!(
                                    "{} {:.1}-{:.1}",
                                    segment.kind.label(),
                                    segment.start,
                                    segment.end
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(", ");
                        logs.record(
                            crate::plumbing::logs::Source::App,
                            "Skip",
                            crate::plumbing::logs::Level::Info,
                            match found.len() {
                                0 => format!(
                                    "AniSkip has no skip times for {meta_id} season {season} episode {episode}"
                                ),
                                count => format!(
                                    "AniSkip answered {count} segment(s) for {meta_id} season {season} episode {episode}: {listing}"
                                ),
                            },
                        );
                        if let Ok(mut playback) = playback.lock() {
                            // The player may have moved on while the source was answering, and
                            // its segments are not this Video's.
                            let same = playback.skip_for == for_video;
                            if same {
                                playback.skip = found.clone();
                            }
                            logs.record(
                                crate::plumbing::logs::Source::App,
                                "Skip",
                                crate::plumbing::logs::Level::Debug,
                                format!(
                                    "Kept {} segment(s) for {for_video}: player is on {:?} ({})",
                                    found.len(),
                                    playback.skip_for,
                                    if same { "same Video" } else { "moved on" }
                                ),
                            );
                        }
                    });
                }
            }
        }

        {
            let mut playback = self.playback.lock().unwrap();
            let had = playback.skip.len();
            if had != playback.skip_total {
                playback.skip_total = had;
                self.logs.record(
                    crate::plumbing::logs::Source::App,
                    "Skip",
                    crate::plumbing::logs::Level::Debug,
                    format!("The player now holds {had} segment(s) for {:?}", playback.skip_for),
                );
            }
            let before = playback.skip_now.map(|segment| segment.kind);
            playback.skip_now = if skip_wanted {
                playback
                    .skip
                    .iter()
                    .copied()
                    .find(|segment| segment.holds(time))
            } else {
                None
            };
            if before != playback.skip_now.map(|segment| segment.kind) {
                match playback.skip_now {
                    Some(segment) => self.logs.record(
                        crate::plumbing::logs::Source::App,
                        "Skip",
                        crate::plumbing::logs::Level::Info,
                        format!("Offering {} at {time:.0}s", segment.kind.label()),
                    ),
                    None => self.logs.record(
                        crate::plumbing::logs::Source::App,
                        "Skip",
                        crate::plumbing::logs::Level::Debug,
                        format!("The skip button is away at {time:.0}s"),
                    ),
                };
            }
            window.set_player_skip_visible(playback.skip_now.is_some());
            // The bar's own record of the segments: fractions of the length, so it marks an
            // Opening before the button offers it. The marks belong to the Video they were fetched
            // for, which is the one in the player only while `skip_for` says so.
            let same_video = self
                .skip_target()
                .is_some_and(|(video, ..)| video == playback.skip_for);
            let marks: Vec<(f32, f32)> = if skip_wanted && same_video && duration > 0.0 {
                playback
                    .skip
                    .iter()
                    .filter_map(|segment| {
                        let start = (segment.start / duration).clamp(0.0, 1.0) as f32;
                        let end = (segment.end / duration).clamp(0.0, 1.0) as f32;
                        (end > start).then_some((start, end))
                    })
                    .collect()
            } else {
                Vec::new()
            };
            if marks != playback.skip_marks {
                playback.skip_marks = marks.clone();
                window.set_player_skip_marks(ModelRc::new(VecModel::from(
                    marks
                        .iter()
                        .map(|(start, end)| crate::SkipMark {
                            start: *start,
                            end: *end,
                        })
                        .collect::<Vec<_>>(),
                )));
            }
        }

        if status.paused.is_some() && status.paused != was_paused {
            player::paused_changed(&self.runtime, status.paused.unwrap_or(false));
        }
        if status.ended {
            self.ended();
        }
    }

    /// Tells the plugins what the player is doing, when it has changed: a file that has loaded
    /// for the Video on screen started playing, and one that came off (or loaded with nothing on
    /// screen) stopped. mpv says the same thing many times a second, so the Video id the plugins
    /// last heard is kept here and a plugin hears each change once.
    fn tell_playback(&self, loaded: bool) {
        let video = if loaded {
            self.latest.lock().unwrap().player.video_id.clone()
        } else {
            String::new()
        };
        let event = {
            let mut playback = self.playback.lock().unwrap();
            if playback.said == video {
                return;
            }
            playback.said = video.clone();
            match video.is_empty() {
                true => Event::PlaybackStopped,
                false => Event::PlaybackStarted { video_id: video },
            }
        };
        self.drain(self.plugins.deliver(event));
    }

    /// The stream reached its end. Binge watching carries on to the next Video; anything else
    /// goes back to where the player was opened from, as the web's back navigation does.
    fn ended(&self) {
        player::ended(&self.runtime);
        let binge = self
            .runtime
            .model()
            .map(|model| {
                model.ctx.profile.settings.binge_watching && model.player.next_video.is_some()
            })
            .unwrap_or(false);
        if binge && player::next(&self.runtime).is_ok() {
            return;
        }
        self.player_back();
    }

    fn player_back(&self) {
        *self.browsed_language.lock().unwrap() = String::new();
        self.video.stop();
        self.video.show(false);
        *self.player_loaded.lock().unwrap() = String::new();
        *self.playback_error.lock().unwrap() = String::new();
        let was_playing = {
            let mut playback = self.playback.lock().unwrap();
            let was = !playback.said.is_empty();
            *playback = Playback::default();
            was
        };
        if was_playing {
            self.drain(self.plugins.deliver(Event::PlaybackStopped));
        }
        self.player_season
            .store(player::NO_SEASON, Ordering::Relaxed);
        player::unload(&self.runtime);
    }

    fn player_play_pause(&self) {
        let paused = self.video.status().paused.unwrap_or(false);
        let _ = self.video.set_paused(!paused);
    }

    fn player_play_next(&self) {
        let _ = player::next(&self.runtime);
    }

    /// The Video on screen and what the skip source needs to name its Episode: the show's own id
    /// (not the Video's), the season and the episode. Anything that is not an Episode has none.
    fn skip_target(&self) -> Option<(String, String, u32, u32)> {
        let model = self.runtime.model().ok()?;
        let video = model
            .player
            .selected
            .as_ref()?
            .stream_request
            .as_ref()?
            .path
            .id
            .clone();
        let series = model.player.series_info.as_ref()?;
        let Loadable::Ready(meta) = model.player.meta_item.as_ref()?.content.as_ref()? else {
            return None;
        };
        Some((
            video,
            meta.preview.id.to_string(),
            series.season,
            series.episode,
        ))
    }

    /// The skip button: the picture goes to the end of the segment the play head is inside,
    /// a breath past it so the segment is behind the play head rather than on its edge.
    fn player_skip(&self) {
        let (end, time) = {
            let playback = self.playback.lock().unwrap();
            (playback.skip_now.map(|segment| segment.end), playback.time)
        };
        if let Some(end) = end.filter(|end| *end > time) {
            self.player_seek((end + 0.5) as f32);
        }
    }

    fn player_seek(&self, time: f32) {
        let time = f64::from(time);
        let _ = self.video.seek(time);
        let duration = self.playback.lock().unwrap().duration;
        self.playback.lock().unwrap().sent_time = time;
        player::seek(&self.runtime, time, duration, "mpv".to_owned());
    }

    /// The arrow keys: ten seconds, or three with Shift, as the profile's own settings say.
    fn player_seek_by(&self, offset: f32) {
        let status = self.video.status();
        let Some(time) = status.time else { return };
        let duration = status.duration.unwrap_or_default();
        let target = (time + f64::from(offset)).clamp(0.0, duration.max(0.0));
        self.player_seek(target as f32);
    }

    fn player_set_volume(&self, volume: f32) {
        let _ = self.video.set_volume(f64::from(volume).clamp(0.0, 200.0));
    }

    fn player_toggle_muted(&self) {
        let muted = self.video.status().muted.unwrap_or(false);
        let _ = self.video.set_muted(!muted);
    }

    fn player_set_speed(&self, speed: f32) {
        let _ = self.video.set_speed(f64::from(speed).clamp(0.1, 4.0));
    }

    /// The scale button and the V key: Fit, then Crop, then Stretch, and round again.
    fn player_cycle_scale(&self) {
        use stremio_core::types::player::VideoScale;
        let next = match self.playback.lock().unwrap().scale {
            VideoScale::Contain => VideoScale::Cover,
            VideoScale::Cover => VideoScale::Fill,
            VideoScale::Fill => VideoScale::Contain,
        };
        self.playback.lock().unwrap().scale = next;
        let _ = self.video.set_scale(next);
        player::scale_changed(&self.runtime, next);
        if let Some(window) = self.window.upgrade() {
            window.set_player_scale(
                match next {
                    VideoScale::Contain => "contain",
                    VideoScale::Cover => "cover",
                    VideoScale::Fill => "fill",
                }
                .into(),
            );
        }
    }

    fn player_select_audio(&self, id: String) {
        let _ = self.video.select_audio(&id);
    }

    /// The track lists of the menu, handed to the window in one piece: what mpv has, and which
    /// language's variants the next column lists.
    fn emit_tracks(&self, shown: &ShownTracks) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        window.set_player_audio_tracks(shell::view::tracks(&shown.audio));
        window.set_player_subtitle_tracks(shell::view::tracks(&shown.subtitles));
        // Each language under its own name -- `languages.label` in the web -- with the code the
        // filtering and the addon requests work in kept beside it.
        window.set_player_subtitle_languages(ModelRc::new(VecModel::from(
            shown
                .languages
                .iter()
                .map(|(code, label)| crate::LanguageOption {
                    code: code.as_str().into(),
                    label: label.as_str().into(),
                    selected: crate::screens::player::same_language(code, &shown.active_language),
                })
                .collect::<Vec<_>>(),
        )));
        window.set_player_subtitle_language(shown.active_language.as_str().into());
        window.set_player_subtitle_variants(ModelRc::new(VecModel::from(self.variants(shown))));
    }

    /// The Variants column after a language row was pressed: the same lists re-cut for the language
    /// being browsed. Nothing is asked of mpv -- the picture keeps the track it has -- and nothing
    /// else in the menu is handed over again, so the row just pressed is not rebuilt under the
    /// pointer.
    fn browse_language(&self, language: String) {
        *self.browsed_language.lock().unwrap() = language;
        let last = self.shown_tracks.lock().unwrap().clone();
        let shown = ShownTracks {
            browsed_language: browsed_language(
                self.browsed_language.lock().unwrap().as_str(),
                &last.active_language,
            ),
            ..last
        };
        *self.shown_tracks.lock().unwrap() = shown.clone();
        let Some(window) = self.window.upgrade() else {
            return;
        };
        window.set_player_subtitle_variants(ModelRc::new(VecModel::from(self.variants(&shown))));
    }

    /// The subtitle tracks of the language being browsed, for the Variants column. Filtering here
    /// rather than in the window keeps a hidden row from leaving its gap behind: a row that is
    /// merely invisible still holds its place in a layout.
    fn variants(&self, shown: &ShownTracks) -> Vec<crate::PlayerTrack> {
        crate::screens::player::subtitle_rows(
            &shown.subtitles,
            &shown.candidates,
            &shown.browsed_language,
        )
    }

    /// The profile's preferred subtitle language, as the settings screen writes it.
    fn subtitles_language(&self) -> String {
        let Ok(model) = self.runtime.model() else {
            return String::new();
        };
        model
            .ctx
            .profile
            .settings
            .subtitles_language
            .clone()
            .unwrap_or_default()
    }

    /// The interface's language, which the web ranks just under the viewer's own when it orders
    /// the Languages column.
    fn interface_language(&self) -> String {
        let Ok(model) = self.runtime.model() else {
            return String::new();
        };
        model.ctx.profile.settings.interface_language.clone()
    }

    /// A language row in the Subtitles menu. Pressing one only brings that language's variants into
    /// the next column: the picture keeps the track it is playing, and it is a variant row that
    /// switches. (The web plays the language's best track here, `subtitlesLanguageOnClick`; this is
    /// the one place the shell is deliberately quieter than it.) OFF is not a language -- it is how
    /// the subtitles are turned off, so it still clears them.
    fn player_choose_language(&self, language: String) {
        if language.is_empty() {
            let choice = self.subtitle_choice.lock().unwrap().clone();
            *self.subtitle_choice.lock().unwrap() = choice.off();
            let _ = self.video.select_subtitles(None);
        }
        self.browse_language(language);
    }

    fn player_choose_subtitles(&self, id: String) {
        // What the row the viewer pressed actually is: a track mpv already has, or a subtitle file
        // an addon (or the stream itself) offered, which has to be added beside the video first.
        let candidate = self
            .latest
            .lock()
            .unwrap()
            .player
            .subtitles
            .iter()
            .find(|subtitle| subtitle.id == id)
            .cloned();
        if let Some(candidate) = &candidate {
            *self.subtitle_choice.lock().unwrap() =
                SubtitleChoice::picked(&candidate.language, true);
            let _ = self.video.add_subtitles(
                &candidate.url,
                &crate::screens::player::variant_label(candidate),
                &candidate.language,
            );
            return;
        }
        let track = self
            .shown_tracks
            .lock()
            .unwrap()
            .subtitles
            .iter()
            .find(|track| track.id == id)
            .cloned();
        if let Some(track) = &track {
            *self.subtitle_choice.lock().unwrap() =
                SubtitleChoice::picked(&track.language, track.external);
        }
        let _ = self.video.select_subtitles(Some(&id));
    }

    fn player_toggle_subtitles(&self) {
        let showing = self
            .video
            .status()
            .subtitles
            .iter()
            .any(|track| track.selected);
        if showing {
            let choice = self.subtitle_choice.lock().unwrap().clone();
            *self.subtitle_choice.lock().unwrap() = choice.off();
            let _ = self.video.select_subtitles(None);
        } else {
            // Back on where they were: the language and the side the viewer last had.
            let choice = self.subtitle_choice.lock().unwrap().clone();
            let tracks = self.shown_tracks.lock().unwrap().subtitles.clone();
            let facts: Vec<SubtitleTrackFacts> = tracks
                .iter()
                .map(|track| SubtitleTrackFacts {
                    id: track.id.clone(),
                    language: track.language.clone(),
                    external: track.external,
                })
                .collect();
            let preferred = self.subtitles_language();
            let wanted = auto_subtitle(
                &facts,
                &SubtitleChoice {
                    chosen: true,
                    enabled: true,
                    ..choice
                },
                &preferred,
            );
            let _ = self.video.select_subtitles(wanted.as_deref());
        }
    }

    /// The subtitles steppers: size in tenths, timing in hundredths of a second, as the web's
    /// own steppers move them.
    fn player_step_subtitles_size(&self, step: i32) {
        let current = self.video.status().subtitles_scale;
        let next = (current + f64::from(step) * 0.1).clamp(0.2, 3.0);
        let _ = self.video.set_subtitles_scale(next);
    }

    fn player_step_subtitles_position(&self, step: i32) {
        let current = self.video.status().subtitles_position;
        let next = (current + f64::from(step)).clamp(0.0, 100.0);
        let _ = self.video.set_subtitles_position(next);
    }

    fn player_step_subtitles_offset(&self, step: i32) {
        let current = self.video.status().subtitles_delay;
        let next = (current + f64::from(step) * 0.1).clamp(-60.0, 60.0);
        let _ = self.video.set_subtitles_delay(next);
    }

    /// An Episode in the drawer: the player reloads on it, as picking a row does.
    fn player_select_episode(&self, video_id: String) {
        let _ = player::load_video(&self.runtime, &video_id);
        self.player_season
            .store(player::NO_SEASON, Ordering::Relaxed);
    }

    /// A season in the drawer. Nothing is loaded: the list is rebuilt from the same title.
    fn player_select_season(&self, index: i32) {
        let Ok(index) = usize::try_from(index) else {
            return;
        };
        self.player_season.store(index, Ordering::Relaxed);
        if let Ok(model) = self.runtime.model() {
            let mut snapshot = self.latest.lock().unwrap().clone();
            snapshot.player = player::data(&model, Some(index));
            drop(model);
            *self.latest.lock().unwrap() = snapshot.clone();
            shell::show(&self.window, snapshot, &self.images);
        }
    }

    /// One row of the player's options menu.
    fn player_choose_option(&self, action: String) {
        let Ok(model) = self.runtime.model() else {
            return;
        };
        let target = match action.as_str() {
            "copy-stream" => player::stream_link(&model.player),
            "copy-magnet" => player::magnet_link(&model.player),
            "download" => player::download_link(&model.player),
            // The web offers whichever subtitle is showing; this shell offers the first the
            // stream or its addons listed, which is the one the menu lists first.
            "download-subtitles" => self
                .latest
                .lock()
                .unwrap()
                .player
                .subtitles
                .first()
                .map(|subtitle| subtitle.url.clone()),
            _ => None,
        };
        drop(model);
        match action.as_str() {
            "download" | "download-subtitles" => match target {
                Some(url) => {
                    crate::plumbing::links::open(&url);
                }
                None => self.say("Nothing to open"),
            },
            _ => match target {
                Some(url) => {
                    let said = match clipboard::copy(&url) {
                        Ok(()) => "Copied".to_owned(),
                        Err(error) => error,
                    };
                    self.say(&said);
                }
                None => self.say("Nothing to copy"),
            },
        }
    }

    fn select_input_option(&self, input: i32, option: i32) {
        let request = {
            let snapshot = self.latest.lock().unwrap();
            usize::try_from(input)
                .ok()
                .and_then(|input| snapshot.discover.inputs.get(input))
                .and_then(|input| {
                    usize::try_from(option)
                        .ok()
                        .and_then(|option| input.requests.get(option))
                })
                .cloned()
        };
        if let Some(request) = request {
            self.selected.store(0, Ordering::Relaxed);
            self.runtime.dispatch(RuntimeAction {
                field: None,
                action: Action::Load(ActionLoad::CatalogWithFilters(Some(CatalogSelected {
                    request,
                }))),
            });
        }
    }

    fn select_library_type(&self, option: i32) {
        let request = {
            let snapshot = self.latest.lock().unwrap();
            usize::try_from(option)
                .ok()
                .and_then(|option| snapshot.library.type_requests.get(option))
                .cloned()
        };
        if let Some(request) = request {
            self.runtime.dispatch(RuntimeAction {
                field: None,
                action: Action::Load(ActionLoad::LibraryWithFilters(LibrarySelected { request })),
            });
        }
    }

    fn select_library_sort(&self, option: i32) {
        let request = {
            let snapshot = self.latest.lock().unwrap();
            usize::try_from(option)
                .ok()
                .and_then(|option| snapshot.library.sorts.get(option))
                .map(|sort| sort.request.clone())
        };
        if let Some(request) = request {
            self.runtime.dispatch(RuntimeAction {
                field: None,
                action: Action::Load(ActionLoad::LibraryWithFilters(LibrarySelected { request })),
            });
        }
    }

    fn load_more_library(&self) {
        self.runtime.dispatch(RuntimeAction {
            field: None,
            action: Action::LibraryWithFilters(ActionLibraryWithFilters::LoadNextPage),
        });
    }

    /// The card behind an id, for the action list the Library grid and the Board's Continue
    /// Watching row both open. Both are library items, so both offer the same four options.
    fn action_target(&self, id: &str) -> Option<ActionTarget> {
        let snapshot = self.latest.lock().unwrap();
        let library_card = snapshot.library.cards.iter().find(|card| card.id == id);
        if let Some(card) = library_card {
            return Some(ActionTarget {
                id: card.id.clone(),
                kind: card.kind.clone(),
                title: card.title.clone(),
                watched: card.watched,
            });
        }
        // The Board draws before the Library has ever been opened, so a card acted on from there
        // is answered by its own row.
        snapshot
            .rows
            .iter()
            .filter(|row| row.menu)
            .flat_map(|row| row.cards.iter())
            .find(|card| card.id == id)
            .map(|card| ActionTarget {
                id: card.id.clone(),
                kind: card.kind.clone(),
                title: card.title.clone(),
                watched: card.watched,
            })
    }

    /// One item of a card's action list, or the play button on the poster: the web's Play, Details,
    /// Dismiss, Mark watched/unwatched and Remove, each doing what `LibItem.js` dispatches.
    fn item_action(&self, id: String, action: String) {
        let Some(card) = self.action_target(&id) else {
            return;
        };
        match action.as_str() {
            "play" => {
                let opened = player::open_for(&self.runtime, &id);
                let message = match opened {
                    Ok(()) => String::new(),
                    Err(error) => format!("Can't play this title: {error}"),
                };
                if let Some(window) = self.window.upgrade() {
                    window.set_play_message(message.into());
                }
            }
            "details" => self.open_item(card.id, card.kind, card.title),
            "watched" => self.runtime.dispatch(RuntimeAction {
                field: None,
                action: Action::Ctx(ActionCtx::LibraryItemMarkAsWatched {
                    id,
                    is_watched: !card.watched,
                }),
            }),
            "dismiss" => {
                self.runtime.dispatch(RuntimeAction {
                    field: None,
                    action: Action::Ctx(ActionCtx::RewindLibraryItem(id.clone())),
                });
                self.runtime.dispatch(RuntimeAction {
                    field: None,
                    action: Action::Ctx(ActionCtx::DismissNotificationItem(id)),
                });
            }
            "remove" => self.runtime.dispatch(RuntimeAction {
                field: None,
                action: Action::Ctx(ActionCtx::RemoveFromLibrary(id)),
            }),
            _ => {}
        }
    }

    /// Web: `useCalendar` loads the month when the route mounts, and it does so every time --
    /// the library it reads may have grown since the last visit, and a Calendar built before a
    /// title arrived would never show that title's Episodes. Asking every addon again on a visit
    /// where nothing has changed would be wasted work, so the month is re-loaded only when the
    /// library is not the one the current month was built from.
    fn calendar_shown(&self) {
        let Ok(model) = self.runtime.model() else {
            return;
        };
        let signature = calendar::library_signature(&model);
        let loaded = *self.calendar_library.lock().unwrap();
        let stale = calendar::needs_load(loaded, signature, model.calendar.meta_items.is_empty());
        let selected = model.calendar.selected.clone();
        drop(model);
        if !stale {
            return;
        }
        *self.calendar_library.lock().unwrap() = Some(signature);
        calendar::load(&self.runtime, selected.map(|held| (held.year, held.month)));
    }

    /// One press of the selector's arrows.
    fn calendar_step(&self, forward: bool) {
        calendar::step(&self.runtime, forward);
        if let Some(window) = self.window.upgrade() {
            window.set_calendar_selected_day(-1);
        }
    }

    /// A poster in the month grid or a row in the list: the title's details, on that Episode's
    /// Streams, which is where the web's calendar links lead.
    fn calendar_open(&self, meta_id: String, kind: String, video_id: String) {
        self.open_item(meta_id, kind, String::new());
        *self.picked.lock().unwrap() = Some(video_id.clone());
        details::pick(&self.runtime, &video_id);
    }

    fn select_preview(&self, index: i32) {
        let index = usize::try_from(index).unwrap_or(0);
        self.selected.store(index, Ordering::Relaxed);
        // The panel's pictures are on screen now, and the ones it had are not.
        self.remember_wanted();
        let snapshot = self.latest.lock().unwrap().clone();
        // Paint the panel now: its title, rating, genres and the rest are already in this snapshot,
        // and the pictures are baked into what is painted. Waiting for a picture to arrive before
        // repainting was the whole of why a click could open a blank panel -- a card whose pictures
        // had already been fetched on hover never repainted at all, because the request for them is
        // deduped and nothing ever fired, and a card whose pictures had not been fetched opened
        // blank until the last of them landed.
        shell::show(&self.window, snapshot.clone(), &self.images);
        for url in discover::preview_images(&snapshot.discover, index) {
            self.request_painting(url);
        }
    }

    /// The pointer came to rest on a card. Its pictures are asked for now, so the panel is already
    /// complete when the card is clicked: web gets that for free, because the panel's backdrop is
    /// the same URL the card's own poster has already downloaded, and the logo is small enough to
    /// be there by the time a click lands. Asking on hover is where we can get the same head start.
    fn hover_card(&self, index: i32) {
        let index = usize::try_from(index).unwrap_or(0);
        // The URLs are taken out under the lock rather than by cloning the snapshot: hovering fires
        // as cards move under a still pointer, and a clone of the snapshot is every card and every
        // preview in it, not the two pictures this wants.
        let urls = {
            let snapshot = self.latest.lock().unwrap();
            discover::preview_images(&snapshot.discover, index)
        };
        // With the repaint attached, so the pictures are in the painted panel by the time the click
        // comes: asking for them here and repainting only on the click would leave the click's own
        // request deduped, with nothing left to paint the panel when the pictures landed.
        for url in urls {
            self.request_painting(url);
        }
    }

    /// Says which pictures are on screen, so the cache keeps those while it makes room for others.
    /// Without this it is free to let go of a poster that is being looked at, and with a limit on
    /// it that is exactly what happened: a grid of blank cards after some scrolling -- and, since
    /// only the Discover grid ever said so, a board of them after a visit to Discover.
    fn remember_wanted(&self) {
        remember_wanted(
            &self.images,
            &self.latest,
            &self.visible,
            self.selected.load(Ordering::Relaxed),
        );
    }

    /// The grid moved. Its cards' posters are asked for as they come on screen -- a page of Discover
    /// can be a hundred cards, and fetching all of them at once is what web's `loading="lazy"` never
    /// does. The range is kept, so a repaint for any other reason asks for the same cards.
    fn visible_range(&self, scroll: f32, height: f32, pitch: f32, columns: i32) {
        let now = std::time::Instant::now();
        let previous = *GRID.lock().unwrap();
        *GRID.lock().unwrap() = Some(GridReport {
            scroll,
            height,
            pitch,
            columns,
            at: now,
        });
        // How fast the grid is going, from the last report: while it is flying past, a row of cards
        // is on screen for a fraction of a second and asking for its posters is work nobody sees.
        let flying = previous.is_some_and(|before| {
            let seconds = now.duration_since(before.at).as_secs_f32();
            seconds > 0.0 && seconds < 0.25 && (scroll - before.scroll).abs() / seconds > FLYING
        });
        let range = discover::visible_range(scroll, height, pitch, columns);
        // What the cache may keep is what the grid draws, not only what is strictly on screen: the
        // row either side is drawn too, and a poster let go of there comes back blank.
        self.remember_wanted();
        if *self.visible.lock().unwrap() != range {
            *self.visible.lock().unwrap() = range;
            if crate::plumbing::images::tracing() {
                eprintln!(
                    "[grid] scrolled {scroll:.0}px of {height:.0}px, rows {pitch:.0}px x {columns}: \
                     cards {}..{}{}",
                    range.0,
                    range.0 + range.1,
                    if flying { " (flying: posters wait)" } else { "" }
                );
            }
            if !flying {
                let urls: Vec<String> = {
                    let snapshot = self.latest.lock().unwrap();
                    discover::card_images(&snapshot.discover, range).collect()
                };
                for url in urls {
                    self.request_painting(url);
                }
            }
        }
        // Whether or not the posters were asked for now, the grid being still is what says the
        // screenful is worth filling: it restarts on every report, so it only fires once the
        // scrolling has stopped.
        let wiring = self.clone();
        self.settle.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_millis(SETTLE_MS),
            move || wiring.fill_the_slack(),
        );
    }

    /// The grid has stopped moving: fetch what is on screen and the row either side, so a screenful
    /// that was flown past is filled in and the next scroll finds its posters already asked for.
    /// Cards already asked for are dropped by the image cache, so a scroll that stopped where it was
    /// costs nothing.
    fn fill_the_slack(&self) {
        let Some(grid) = *GRID.lock().unwrap() else {
            return;
        };
        let range = discover::settled_range(grid.scroll, grid.height, grid.pitch, grid.columns);
        self.remember_wanted();
        let urls: Vec<String> = {
            let snapshot = self.latest.lock().unwrap();
            discover::card_images(&snapshot.discover, range).collect()
        };
        for url in urls {
            self.request_painting(url);
        }
        // Only now that the grid is still is the next page worth asking for: asked for on every
        // notch of a flick it pulls a page of cards, and their posters, into a scroll that is
        // already past them.
        if let Some(window) = self.window.upgrade() {
            window.set_grid_settle_count(window.get_grid_settle_count() + 1);
        }
    }

    /// A press in one of the Addons screen's two menus: the catalog menu ("Installed", "Official",
    /// "Community", "Podcasts") or the type menu under it. Each option carries the load it asks for,
    /// because the two lists on this screen come from two different models.
    fn select_addon_input_option(&self, input: i32, option: i32) {
        let choice = usize::try_from(input).ok().and_then(|input| {
            let snapshot = self.latest.lock().unwrap();
            snapshot.addons.choices.get(input).and_then(|choices| {
                usize::try_from(option)
                    .ok()
                    .and_then(|option| choices.get(option).cloned())
            })
        });
        if let Some(choice) = choice {
            addons::select(&self.runtime, &choice);
        }
    }

    /// A marketplace card's Install button: the descriptor the catalog answered with goes to the
    /// core's own install action, the same one the pasted-URL dialog uses.
    fn install_market_addon(&self, transport_url: &str) {
        // The card the press came from, off the same list the user is looking at: the live model
        // may be reloading the catalog at this instant, and the web's card carries its own
        // descriptor for exactly that reason.
        let card = self
            .latest
            .lock()
            .unwrap()
            .addons
            .cards
            .iter()
            .find(|card| card.transport_url == transport_url)
            .cloned();
        if let Some(card) = card {
            addons::install_card(&self.runtime, &card);
        }
    }

    // ---- Mods ----------------------------------------------------------------

    /// The page the Mods menu is on, and the menu with it. Changing page takes back an armed
    /// Remove: the card it was on is no longer on screen, so the next press cannot be its second.
    /// The folders are read again, so a file dropped in since the last core event has its card
    /// by the time the page finishes drawing.
    fn select_mods_page(&self, id: String) {
        let page = mods::Page::from_id(&id);
        self.disarm_mods();
        *self.mods_page.lock().unwrap() = page.id().to_owned();
        if page == mods::Page::Marketplace {
            // The registry is read the one time the page is opened, and the update check runs
            // with it, off this thread: what the page draws now is the folder as it is.
            self.market_shown();
        }
        // The page the window holds now, before the folders are read: the menu's own selection
        // follows the press without waiting for a snapshot, and the read below cannot lose it.
        if let Some(window) = self.window.upgrade() {
            window.set_mods_page(page.id().into());
        }
        // The screen's own data with the plugins folded in, as on every later event: the Plugin
        // settings page reads the folder here too, so opening it is not a page with nothing on it.
        let data = self.plugin_data(page);
        let mut snapshot = self.latest.lock().unwrap().clone();
        snapshot.mods = data.with_notice(snapshot.mods.notice.clone());
        // The section's own properties are set here, not only posted: the press is on the
        // window's thread already, and the page has to be the one picked by the time this returns.
        *self.latest.lock().unwrap() = snapshot.clone();
        self.show_mods_now();
        self.repaint_with(snapshot);
    }

    /// The kind of mod the page on screen lists, if it lists any.
    fn mods_page_kind(&self) -> Option<mods::catalogue::Kind> {
        mods::Page::from_id(&self.mods_page.lock().unwrap()).kind()
    }

    /// A press on a plugin's or a theme's switch: write the choice down, save it, and for a theme
    /// draw the window with the one it now names. `filename` is the file the card names, so the
    /// list of the page on screen is the only one that could hold it.
    fn toggle_mod(&self, filename: String, enabled: bool) {
        // A switch is not a Remove, so its press takes back any armed Remove.
        self.disarm_mods();
        let Some(kind) = self.mods_page_kind() else {
            return;
        };
        let notice = {
            let mut extras = self.local_extras.lock().unwrap();
            let notice = match kind {
                mods::catalogue::Kind::Plugin => {
                    mods::plugins::set_enabled(&mut extras, &filename, enabled)
                }
                mods::catalogue::Kind::Theme => {
                    mods::themes::set_enabled(&mut extras, &filename, enabled)
                }
            };
            extras.save(&self.data_dir);
            notice
        };
        // What the switch did is a record on the Logs page as well as the notice: a plugin that
        // starts and then stops says why, and this says where it was started from.
        self.recorded(kind, &filename, notice.as_ref());
        if kind == mods::catalogue::Kind::Theme {
            self.apply_themes();
        }
        if kind == mods::catalogue::Kind::Plugin {
            // The switch is the start and the stop: a plugin that is on runs, and one that is off
            // has no interpreter at all.
            let asked = if enabled {
                self.plugins.start(&filename)
            } else {
                self.plugins.stop(&filename)
            };
            self.drain(asked);
        }
        self.after_mod_action(notice);
    }

    /// Starts every plugin `settings.json` names. Called once at boot, after the window is up.
    pub(crate) fn start_enabled_plugins(&self) {
        let enabled = self.local_extras.lock().unwrap().enabled_plugins.clone();
        // The Plugin settings page opens on the plugin picked, which is the first one in the list
        // when the app has just started. The pick is remembered here; whether the plugin is
        // switched on is read from the folder with every snapshot.
        let page = self.plugin_data(mods::Page::Plugins);
        *self.mods_plugin.lock().unwrap() = page.plugin;
        let asked = self.plugins.start_enabled(&enabled);
        self.drain(asked);
        // The report is repainted so the first frame after boot already says which plugins run.
        let page = mods::Page::from_id(&self.mods_page.lock().unwrap());
        let data = self.plugin_data(page);
        let mut snapshot = self.latest.lock().unwrap().clone();
        snapshot.mods = data;
        self.repaint_with(snapshot);
    }

    /// What the event task does with a plugin's asks, built from the wiring's `Send` parts only:
    /// the event task is not the window's thread, and the wiring holds `Rc`s. This is where a
    /// line a plugin logged from a timer, and rules it asked for there, reach the window.
    pub(crate) fn puts_plugins(
        plugins: &Arc<crate::app::plugins::Runtime>,
        latest: &Arc<Mutex<Snapshot>>,
        window: &Weak<MainWindow>,
        images: &Images,
        data_dir: &std::path::Path,
        export: &Arc<Mutex<Option<String>>>,
    ) -> Arc<dyn Fn(Vec<crate::app::plugins::Asked>) + Send + Sync> {
        let plugins = Plugins {
            runtime: plugins.clone(),
            logs: Arc::clone(plugins.logs()),
            export: Arc::clone(export),
            latest: latest.clone(),
            window: window.clone(),
            images: images.clone(),
            data_dir: data_dir.to_path_buf(),
        };
        Arc::new(move |asked| plugins.put(asked))
    }

    /// What the event task and the server's task do with a plugin's asks. Built from the wiring's
    /// `Send` parts alone: those tasks are not on the window's thread, and the wiring holds `Rc`s.
    pub(crate) fn put_plugins(&self) -> Arc<dyn Fn(Vec<crate::app::plugins::Asked>) + Send + Sync> {
        Wiring::puts_plugins(
            &self.plugins,
            &self.latest,
            &self.window,
            &self.images,
            &self.data_dir,
            &self.mods_export,
        )
    }

    /// Puts what the plugins asked for into effect. Their lines are already in the runtime, which
    /// is what every card and the Plugin settings page read, so the one thing left to draw is the
    /// rules a plugin asked for. Returns whether anything changed what the Mods pages show.
    fn drain(&self, asked: Vec<crate::app::plugins::Asked>) -> bool {
        let mut changed = false;
        let mut rules: Vec<serde_json::Value> = Vec::new();
        for ask in asked {
            match ask {
                crate::app::plugins::Asked::Log(line, stopped) => {
                    changed = true;
                    self.say_plugin_line(&line, stopped);
                }
                crate::app::plugins::Asked::Theme(more) => {
                    rules.extend(more);
                    changed = true;
                }
                crate::app::plugins::Asked::Changed => changed = true,
            }
        }
        if !rules.is_empty() {
            self.install_plugin_theme(&rules);
        }
        changed
    }

    /// Says one plugin line where a person can see it. A line that stopped the plugin is the
    /// plugin's reason, and reasons belong on the card and on the settings page, which are drawn
    /// from the runtime; this puts the same words in the window's notice so they are seen even
    /// when the user is on another page when the plugin fails.
    fn say_plugin_line(&self, line: &crate::plumbing::plugin::LogLine, stopped: bool) {
        if !stopped {
            return;
        }
        let notice = mods::Notice::failed(format!("A plugin stopped: {}", line.text));
        let mut snapshot = self.latest.lock().unwrap().clone();
        snapshot.mods.notice = notice.clone();
        *self.latest.lock().unwrap() = snapshot.clone();
        let _ = self.window.upgrade_in_event_loop(move |window| {
            let _ = &window;
        });
        self.repaint_with(snapshot);
    }

    /// Draws the window with the rules a plugin asked for, and reports what this app could not
    /// understand back into the plugin's own state, where its card shows it.
    fn install_plugin_theme(&self, rules: &[serde_json::Value]) {
        let rules = rules.to_vec();
        let _ = self.window.upgrade_in_event_loop(move |window| {
            let warnings = theme::install(&window, &rules);
            if !warnings.is_empty() {
                // Nothing to print to: the app has no console a person can read, so a rule this
                // app cannot draw is left out. The plugin's own log is where it said it wanted it.
            }
        });
    }

    /// A press in the Plugin settings page's left list: the page is read for the plugin picked,
    /// and anything typed for the one before is thrown away, because it is no longer on screen.
    fn pick_plugin(&self, filename: String) {
        {
            let mut edits = self.mods_edits.lock().unwrap();
            *edits = if filename.is_empty() {
                mods::settings::Edits::default()
            } else {
                mods::settings::edits(&self.data_dir, &filename)
            };
        }
        *self.mods_plugin.lock().unwrap() = filename;
        self.after_mod_action(None);
    }

    /// A press in one control: the value goes into the page, not the file. Each kind of control
    /// reports the kind of value it holds, so a toggle cannot hand the page a string. A text box
    /// waits for a pause before the page is drawn again, so the letters after the first are typed
    /// into the editor the frame did not replace; every other control is a single press and is
    /// drawn straight away.
    fn edit_plugin_setting(&self, key: String, value: serde_json::Value, text: bool) {
        {
            let mut edits = self.mods_edits.lock().unwrap();
            mods::settings::set_value(&mut edits, &key, &value);
        }
        if text {
            self.draw_typing();
        } else {
            self.after_mod_action(None);
        }
    }

    /// Something was typed: the page is drawn once typing pauses. Every press counts, and the
    /// last count to be seen after the pause is the one drawn, so a key typed while the timer ran
    /// is never lost.
    fn draw_typing(&self) {
        if self.mods_typed.running() {
            return;
        }
        let (window, latest, images) = (
            self.window.clone(),
            self.latest.clone(),
            self.images.clone(),
        );
        self.mods_typed.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_millis(TYPING_PAUSE_MS),
            move || {
                // Drawn without a notice: typing is not an action worth one, and the notice is
                // what a saved page says.
                let snapshot = latest.lock().unwrap().clone();
                shell::show(&window, snapshot, &images);
            },
        );
    }

    /// The Revert press: every value goes back to what the plugin's file holds.
    fn revert_plugin_settings(&self) {
        let mut edits = self.mods_edits.lock().unwrap();
        mods::settings::revert(&mut edits);
        drop(edits);
        self.after_mod_action(Some(mods::Notice::said(
            "Reverted to the saved values. Nothing was written.",
        )));
    }

    // ---- The Logs page -------------------------------------------------------

    /// A press on one part of the Logs page's filter. The filter belongs to the recorder, which is
    /// where a line a plugin logs from a timer finds it too: a page redrawn by a core event draws
    /// the same filtered list. Pressing a control takes back an armed Clear.
    fn filter_logs(&self, part: &str, value: String) {
        *self.mods_clearing.lock().unwrap() = false;
        match part {
            "source" => self.logs.set_source(&value),
            "level" => self.logs.set_level(&value),
            _ => self.logs.set_search(&value),
        }
        self.repaint_mods_now();
    }

    /// Draws the Mods section with the page the window holds, and the last notice: the answer to a
    /// press that changed something the screen shows and nothing on disk.
    fn repaint_mods_now(&self) {
        let mut snapshot = self.latest.lock().unwrap().clone();
        snapshot.mods = self.page_data(snapshot.mods.to_page());
        self.repaint_with(snapshot);
    }

    /// The page on screen, as a press leaves it, built before the window is touched: the folder is
    /// read and every lock the screen takes is let go. A press is on the window's thread, and a
    /// lock still held here when the window draws can stop the whole window, not just this page.
    fn page_data(&self, page: mods::Page) -> mods::Data {
        let mut data = self.plugin_data(page);
        data.notice = self.latest.lock().unwrap().mods.notice.clone();
        data
    }

    /// A press on a record: its full text is on screen, or taken back when it is the one open. A
    /// record the list no longer holds opens nothing.
    fn open_log_record(&self, id: u32) {
        let mut data = self.plugin_data(mods::Page::Logs);
        data.press_record(&self.logs, u64::from(id));
        let mut snapshot = self.latest.lock().unwrap().clone();
        snapshot.mods = data.with_notice(snapshot.mods.notice.clone());
        self.finish_log_press(snapshot);
    }

    /// The Clear button. The first press arms it and removes nothing; the second empties the
    /// list, which deletes no file and stops nothing being recorded afterwards. The arm is set
    /// before the page is read, so the page and the window agree on what the button says.
    fn clear_logs(&self) {
        // The arm is read and set in one hold: the page below reads it, and two guards on the
        // same line would never let the second one through.
        let armed = {
            let mut armed = self.mods_clearing.lock().unwrap();
            let was_armed = *armed;
            if was_armed {
                self.logs.clear();
            }
            *armed = !was_armed;
            *armed
        };
        let mut snapshot = self.latest.lock().unwrap().clone();
        snapshot.mods = self.page_data(mods::Page::Logs);
        debug_assert_eq!(snapshot.mods.log_clear_armed, armed);
        self.finish_log_press(snapshot);
    }

    /// Draws the Mods section from `snapshot` on the window's thread, and posts it like every other
    /// repaint: the section is drawn now, and the whole window follows from the same snapshot.
    fn finish_log_press(&self, snapshot: Snapshot) {
        *self.latest.lock().unwrap() = snapshot.clone();
        if let Some(window) = self.window.upgrade() {
            shell::view::show_mods(&window, &snapshot.mods);
        }
        self.repaint_with(snapshot);
    }

    // ---- The About page ------------------------------------------------------

    /// The About page's Refresh press: the two folders and what the plugins are doing are read
    /// again, which is what the counts are, and the release feed is asked whether there is a newer
    /// version. The counts are drawn here at once; the feed's answer lands as an app record, which
    /// is where a check that could not be made says so.
    fn refresh_about(&self) {
        let mut snapshot = self.latest.lock().unwrap().clone();
        snapshot.mods = self.plugin_data(mods::Page::About);
        snapshot.mods.notice = mods::Notice::said("Read the plugin and theme folders again.");
        self.repaint_with(snapshot);
        let logs = self.logs.clone();
        self.handle.spawn(async move {
            let (level, text) =
                match stremio_lightning_core::app_update::check_app_update(mods::about::VERSION)
                    .await
                {
                    Ok(info) => match &info.new_version {
                        Some(newer) => (
                            crate::plumbing::logs::Level::Info,
                            format!("A newer version is available: {newer}."),
                        ),
                        None => (
                            crate::plumbing::logs::Level::Info,
                            format!(
                                "Version {} is the newest released version.",
                                info.current_version
                            ),
                        ),
                    },
                    Err(error) => (crate::plumbing::logs::Level::Warn, error),
                };
            logs.record(crate::plumbing::logs::Source::App, "About", level, &text);
        });
    }

    /// The Export diagnostics press: one text file in the app data folder's `diagnostics/`, and
    /// the path it went to stays on the page. Nothing is uploaded.
    fn export_diagnostics(&self) {
        let extras = self.local_extras.lock().unwrap().clone();
        let sources = mods::about::Sources {
            data_dir: &self.data_dir,
            extras: &extras,
            plugins: &self.plugins,
        };
        let about = mods::about::data(&sources);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_secs())
            .unwrap_or_default();
        let result = mods::about::export(&about, &self.logs, &self.data_dir, now);
        let notice = match result {
            Ok((path, _)) => {
                *self.mods_export.lock().unwrap() = Some(path.display().to_string());
                self.logs.record(
                    crate::plumbing::logs::Source::App,
                    "About",
                    crate::plumbing::logs::Level::Info,
                    format!("The diagnostics file was written to {}.", path.display()),
                );
                mods::Notice::said(format!("Wrote the diagnostics file to {}.", path.display()))
            }
            Err(error) => {
                self.logs.record(
                    crate::plumbing::logs::Source::App,
                    "About",
                    crate::plumbing::logs::Level::Error,
                    &error,
                );
                mods::Notice::failed(error)
            }
        };
        self.after_mod_action(Some(notice));
    }

    /// The About page's Copy press: the version and the build go to the clipboard, one line each,
    /// which is what a bug report asks for.
    fn copy_version(&self) {
        let text = format!(
            "Stremio Lightning {} ({}) on {} {}",
            mods::about::VERSION,
            mods::about::BUILD,
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        let notice = match clipboard::copy(&text) {
            Ok(()) => mods::Notice::said(format!("Copied: {text}")),
            Err(error) => mods::Notice::failed(format!("Nothing was copied: {error}")),
        };
        self.after_mod_action(Some(notice));
    }

    /// The Save press: every changed value is written to the plugin's settings file, and the
    /// plugin's own `onSettingsSaved` callbacks are handed the file. Nothing is written when
    /// nothing changed, so a press on a page nobody touched leaves the file alone.
    fn save_plugin_settings(&self) {
        let (filename, saved) = {
            let mut edits = self.mods_edits.lock().unwrap();
            if edits.filename.is_empty() {
                return;
            }
            let filename = edits.filename.clone();
            if !edits.dirty() {
                drop(edits);
                self.after_mod_action(Some(mods::Notice::said("Nothing to save.")));
                return;
            }
            let saved = mods::settings::save(&self.data_dir, &mut edits);
            (filename, saved)
        };
        if !saved.written {
            self.after_mod_action(Some(mods::Notice::failed(format!(
                "{filename}: {}",
                saved.error
            ))));
            return;
        }
        self.drain(self.plugins.settings_saved(&filename, saved.values));
        self.after_mod_action(Some(mods::Notice::said(format!("Saved {filename}."))));
    }

    /// The Mods screen for `page`, with what the plugin runtime holds folded in, and the plugin
    /// the Plugin settings page has picked. The picked plugin is what the user last chose, so the
    /// page it opens on is read from the runtime rather than from the folder every time.
    fn plugin_data(&self, page: mods::Page) -> mods::Data {
        // The lock is taken before the screen is built: a guard alive across the `with_plugins`
        // call would sit on the same line as the screen's own fields.
        let pick = self.mods_plugin.lock().unwrap().clone();
        let screen = mods::data(&self.data_dir)
            .with_page(page)
            .with_plugins(&self.plugins);
        let pick = if pick.is_empty() {
            screen.plugin.clone()
        } else {
            pick
        };
        // The rest of the section: the Marketplace's registry and check, the Logs page's rows, the
        // About page's counts, and the Clear button's arm. Every press on the section builds its
        // page through here, so no page is ever drawn from a stale list.
        self.with_market_state(
            screen
                .with_edits(pick, self.mods_edits.lock().unwrap().clone())
                .with_logs(&self.logs, *self.mods_clearing.lock().unwrap())
                .with_about(
                    &self.local_extras.lock().unwrap().clone(),
                    &self.plugins,
                    self.mods_export.lock().unwrap().as_ref(),
                ),
        )
    }

    /// The Remove button of a mod card. Its first press arms the card and deletes nothing; its
    /// second press, on the same card, deletes the file. A press on any other card's button moves
    /// the arm to that card rather than confirming the first.
    fn remove_mod(&self, id: String) {
        let Some(kind) = self.mods_page_kind() else {
            return;
        };
        let page = self.mods_page.lock().unwrap().clone();
        let pressed = ArmedMod {
            id: id.clone(),
            page,
        };
        if self.armed_mod() != Some(pressed.clone()) {
            shell::view::arm(Some(&id));
            *self.mods_armed.lock().unwrap() = Some(pressed);
            self.restart_disarm();
            self.show_mods_now();
            return;
        }
        self.disarm_mods();
        let notice = {
            let mut extras = self.local_extras.lock().unwrap();
            let notice = match kind {
                mods::catalogue::Kind::Plugin => {
                    mods::plugins::remove(&self.data_dir, &mut extras, &id)
                }
                mods::catalogue::Kind::Theme => {
                    mods::themes::remove(&self.data_dir, &mut extras, &id)
                }
            };
            extras.save(&self.data_dir);
            notice
        };
        if kind == mods::catalogue::Kind::Theme {
            // The theme that was removed may have been the one drawn with, so the window goes back
            // to its own colours the moment its file is gone.
            self.apply_themes();
        }
        self.recorded(kind, &id, Some(&notice));
        self.after_mod_action(Some(notice));
    }

    /// A press anywhere else on a Mods card: the armed card on it is not the one pressed, so the
    /// arm goes away.
    fn dismiss_mods_remove(&self) {
        if self.disarm_mods() {
            self.show_mods_now();
        }
    }

    /// Draws the Mods section from the last snapshot on the window's thread, without waiting for
    /// the event loop. A press on the section is on this thread already, and the card's next frame
    /// should be the answer to it.
    fn show_mods_now(&self) {
        if let Some(window) = self.window.upgrade() {
            let data = self.latest.lock().unwrap().mods.clone();
            shell::view::show_mods(&window, &data);
        }
    }

    /// Takes the arm back and returns whether there was one to take back.
    fn disarm_mods(&self) -> bool {
        let had = self.mods_armed.lock().unwrap().take().is_some();
        shell::view::arm(None);
        if had {
            self.mods_disarm.stop();
        }
        had
    }

    fn armed_mod(&self) -> Option<ArmedMod> {
        self.mods_armed.lock().unwrap().clone()
    }

    /// Starts the five seconds an armed card waits before it is no longer armed.
    fn restart_disarm(&self) {
        let (armed, window, latest, images) = (
            self.mods_armed.clone(),
            self.window.clone(),
            self.latest.clone(),
            self.images.clone(),
        );
        self.mods_disarm.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_millis(DISARM_MS),
            move || {
                let still_armed = armed
                    .lock()
                    .map(|mut slot| slot.take().is_some())
                    .unwrap_or(false);
                if !still_armed {
                    return;
                }
                // The outline on the card is part of what the window draws, so the arm going away
                // is a repaint, not just a change of state the next press would reveal.
                shell::view::arm(None);
                let snapshot = latest.lock().unwrap().clone();
                shell::show(&window, snapshot, &images);
            },
        );
    }

    /// Records what a press on a Mods card did, when the press had something to say: a switch that
    /// changed a list, or a removal. A press that changed nothing records nothing. A plugin's
    /// record is a plugin one, and a theme's is a theme one, so the Logs page's filter finds them
    /// beside the lines that came from the file itself.
    fn recorded(&self, kind: mods::catalogue::Kind, filename: &str, notice: Option<&mods::Notice>) {
        let Some(notice) = notice else {
            return;
        };
        let source = match kind {
            mods::catalogue::Kind::Plugin => crate::plumbing::logs::Source::Plugin,
            mods::catalogue::Kind::Theme => crate::plumbing::logs::Source::Theme,
        };
        let level = if notice.failed {
            crate::plumbing::logs::Level::Error
        } else {
            crate::plumbing::logs::Level::Info
        };
        self.logs
            .record(source, filename, level, notice.text.clone());
    }

    /// The Mods screen for `page` with what the Marketplace has read folded in: the registry, the
    /// search box's text, and the check's answer. Used wherever a screen is built off the snapshot
    /// rather than off the folder alone.
    fn with_market_state(&self, data: mods::Data) -> mods::Data {
        let updates = self.market_updates.lock().unwrap().clone();
        data.with_updates(updates).with_market_parts(
            self.market_listing.lock().unwrap().as_ref(),
            &self.market_search.lock().unwrap(),
            &self.market_summary.lock().unwrap(),
            &self.market_installing.lock().unwrap(),
        )
    }

    /// The Mods screen after an action that changed the folders or `settings.json`: the lists are
    /// read again, and the action's own line is what the notice says.
    fn after_mod_action(&self, notice: Option<mods::Notice>) {
        let page = mods::Page::from_id(&self.mods_page.lock().unwrap());
        let mut data = self.plugin_data(page);
        if let Some(notice) = notice {
            data.notice = notice;
        } else {
            data.notice = self.latest.lock().unwrap().mods.notice.clone();
        }
        // A press on one page leaves the Marketplace page as it was: its lines about the presses
        // made there, and the card that is still being fetched.
        if page != mods::Page::Marketplace {
            data.market = self.latest.lock().unwrap().mods.market.clone();
        }
        let mut snapshot = self.latest.lock().unwrap().clone();
        snapshot.mods = data;
        self.repaint_with(snapshot);
    }

    // ---- Marketplace and updates ---------------------------------------------

    /// Reads the registry off the window's thread, once per press of Check for updates. Opening
    /// the page reads it too, through `market_shown`.
    fn read_registry(&self) {
        {
            let mut asked = self.market_asked.lock().unwrap();
            if *asked {
                return;
            }
            *asked = true;
        }
        let (listing, settle, task) = (
            self.market_listing.clone(),
            self.market_settler(),
            self.handle.clone(),
        );
        task.spawn(async move {
            let read = mods::marketplace::fetch().await;
            *listing.lock().unwrap() = Some(match read {
                Ok(registry) => mods::marketplace::Listing::from_registry(registry),
                Err(error) => {
                    mods::marketplace::Listing::failed(mods::marketplace::registry_failed(&error))
                }
            });
            // Drawn only now, so the page never shows the registry it is still waiting for.
            settle.settle();
        });
    }

    /// The page is on screen: the registry is read if it has not been, and the update check runs
    /// once, so both installed pages say where they stand without a press.
    fn market_shown(&self) {
        if self.market_listing.lock().unwrap().is_none() {
            self.read_registry();
        }
        self.check_updates(false);
    }

    /// Draws the Mods section again from the folder, the registry read so far and the check's
    /// answer, and the search box's text.
    ///
    /// The page is worked out while the window's snapshot is held, and written back to that same
    /// snapshot. A registry read landing between a copy and the write would otherwise be dropped
    /// by it, and the page would keep the empty list it drew first.
    fn repaint_market(&self) {
        let mut held = self.latest.lock().unwrap();
        let page = mods::Page::from_id(&self.mods_page.lock().unwrap());
        let mut data = self.with_market_state(self.plugin_data(page));
        data.notice = held.mods.notice.clone();
        held.mods = data;
        let snapshot = held.clone();
        drop(held);
        self.repaint_with(snapshot);
    }

    /// The parts the Marketplace's tasks need to ask for a draw.
    fn market_settler(&self) -> MarketSettler {
        MarketSettler {
            window: self.window.clone(),
            latest: self.latest.clone(),
            images: self.images.clone(),
            mods_page: self.mods_page.clone(),
            listing: self.market_listing.clone(),
            search: self.market_search.clone(),
            updates: self.market_updates.clone(),
            summary: self.market_summary.clone(),
            installing: self.market_installing.clone(),
        }
    }

    /// A press in the Marketplace's search box. The list is filtered from what the window already
    /// read, so typing asks the registry nothing.
    fn search_market(&self, text: String) {
        *self.market_search.lock().unwrap() = text;
        self.repaint_market();
    }

    /// A press on a Marketplace card's one button. An installed card's Uninstall arms on the first
    /// press and removes on the second; every other press fetches the entry and installs it.
    fn install_market(&self, id: String) {
        if !self.market_installing.lock().unwrap().is_empty() {
            // One fetch at a time: a second press while one is in flight is not a second install.
            return;
        }
        let Some((kind, entry)) = self.market_entry(&id) else {
            return;
        };
        let data_dir = self.data_dir.clone();
        let installed = mods::marketplace::installed_file(&data_dir, kind, &entry);
        match installed {
            Some(filename) => {
                if shell::view::armed_market().as_deref() != Some(id.as_str()) {
                    // The first press asks for the second. Nothing is deleted here.
                    shell::view::arm_market(Some(&id));
                    self.restart_disarm_market();
                    self.repaint_market();
                    return;
                }
                shell::view::arm_market(None);
                self.mods_disarm.stop();
                let (row, notice) = mods::marketplace::uninstall(&data_dir, kind, &filename);
                self.push_market_line(notice.clone());
                // The folder is read again by `repaint_market`; the notice is the answer to this
                // press and stays on screen with it. The snapshot is edited where it is, not
                // copied and written back, so a draw that landed in between is not undone.
                let mut held = self.latest.lock().unwrap();
                held.mods.notice = notice;
                held.mods.plugins = row.plugins;
                held.mods.themes = row.themes;
                drop(held);
                self.repaint_market();
            }
            None => self.start_install(id, kind, entry),
        }
    }

    /// Fetches `entry` and writes it into its folder, with the card saying "Installing…" until the
    /// answer lands. Nothing is turned on: a new mod is off until its switch is pressed.
    fn start_install(
        &self,
        id: String,
        kind: mods::catalogue::Kind,
        entry: stremio_lightning_core::mods::RegistryEntry,
    ) {
        shell::view::arm_market(None);
        shell::view::set_installing(Some(&id));
        *self.market_installing.lock().unwrap() = id.clone();
        // The card is drawn waiting before the fetch starts, so the press has an answer at once.
        self.repaint_market();
        let (data_dir, task) = (self.data_dir.clone(), self.handle.clone());
        let (latest, settle) = (self.latest.clone(), self.market_settler());
        let summary = self.market_summary.clone();
        let installing = self.market_installing.clone();
        task.spawn(async move {
            let outcome = mods::marketplace::install(&data_dir, kind, &entry).await;
            let notice = match outcome {
                Ok(filename) => mods::Notice::said(format!(
                    "Installed {filename}. It is off until you turn it on in {}.",
                    match kind {
                        mods::catalogue::Kind::Plugin => "Plugins",
                        mods::catalogue::Kind::Theme => "Themes",
                    }
                )),
                Err(error) => {
                    mods::Notice::failed(mods::marketplace::download_failed(&entry.name, &error))
                }
            };
            installing.lock().unwrap().clear();
            shell::view::set_installing(None);
            summary.lock().unwrap().push(notice.clone());
            // The listing is unchanged, so the page is built from the folder it now has.
            after_install(&latest, &settle, notice);
        });
    }

    /// The registry entry a Marketplace press names, and the page it belongs to. A card's id is
    /// the entry's download, which is what the press carries.
    fn market_entry(
        &self,
        id: &str,
    ) -> Option<(
        mods::catalogue::Kind,
        stremio_lightning_core::mods::RegistryEntry,
    )> {
        let listing = self.market_listing.lock().unwrap().clone()?;
        for kind in [mods::catalogue::Kind::Plugin, mods::catalogue::Kind::Theme] {
            if let Some(entry) = listing
                .entries(kind)
                .iter()
                .find(|entry| entry.download == id)
            {
                return Some((kind, entry.clone()));
            }
        }
        None
    }

    /// One more line in the Marketplace page's summary.
    fn push_market_line(&self, notice: mods::Notice) {
        self.market_summary.lock().unwrap().push(notice);
    }

    /// Starts the five seconds a Marketplace card's Uninstall stays armed.
    fn restart_disarm_market(&self) {
        let (latest, window, images) = (
            self.latest.clone(),
            self.window.clone(),
            self.images.clone(),
        );
        self.mods_disarm.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_millis(DISARM_MS),
            move || {
                if shell::view::armed_market().is_none() {
                    return;
                }
                shell::view::arm_market(None);
                let snapshot = latest.lock().unwrap().clone();
                shell::show(&window, snapshot, &images);
            },
        );
    }

    /// A press anywhere else on a Marketplace card takes its Uninstall back.
    fn dismiss_market(&self) {
        if shell::view::armed_market().is_some() {
            shell::view::arm_market(None);
            self.mods_disarm.stop();
            self.repaint_market();
        }
    }

    /// Check for updates, off the window's thread. A check already running is not asked for
    /// again. The registry is read with it when `registry` is true, which is what Check for
    /// updates means: both the list and the installed mods are looked at again.
    fn check_updates(&self, registry: bool) {
        {
            let mut asked = self.market_check_asked.lock().unwrap();
            if *asked {
                return;
            }
            *asked = true;
        }
        if registry {
            self.read_registry();
        }
        shell::view::set_checking(true);
        let (updates, data_dir, task) = (
            self.market_updates.clone(),
            self.data_dir.clone(),
            self.handle.clone(),
        );
        let (latest, settle) = (self.latest.clone(), self.market_settler());
        task.spawn(async move {
            let answer = mods::marketplace::Updates::check(&data_dir).await;
            match answer {
                Ok(now) => *updates.lock().unwrap() = now,
                Err(error) => {
                    // A folder that cannot be read is one line on the Marketplace page, where the
                    // press was made, and the check is not asked for again until the next press.
                    latest.lock().unwrap().mods.market.trouble = error;
                }
            }
            shell::view::set_checking(false);
            settle.settle();
        });
        self.repaint_market();
    }

    /// Update all on the page on screen, or one card's Update, which names its file.
    fn update_mods(&self, filename: Option<String>) {
        let page = mods::Page::from_id(&self.mods_page.lock().unwrap());
        let Some(kind) = page.kind() else {
            return;
        };
        let updates = self.market_updates.lock().unwrap().clone();
        let cards = mods::marketplace::with_updates(&self.data_dir, kind, &updates);
        let (mut wanted, refused) = mods::marketplace::wanted(&cards, &updates, kind);
        if let Some(filename) = &filename {
            wanted.retain(|want| &want.filename == filename);
        }
        if wanted.is_empty() {
            let notice = match refused.into_iter().next() {
                Some(refused) => refused,
                None => mods::Notice::said("Nothing needed an update."),
            };
            // The line stays on the Marketplace page after the card that was pressed is gone from
            // it, beside the page's own copy of it.
            self.push_market_line(notice.clone());
            self.after_mod_action(Some(notice));
            return;
        }
        let (data_dir, task) = (self.data_dir.clone(), self.handle.clone());
        let (latest, settle) = (self.latest.clone(), self.market_settler());
        let (summary, updates) = (self.market_summary.clone(), self.market_updates.clone());
        let pressed = wanted.len();
        task.spawn(async move {
            let mut results = Vec::new();
            for want in wanted {
                let outcome =
                    mods::marketplace::update_file(&data_dir, kind, &want.filename, &want.url)
                        .await;
                results.push(
                    outcome
                        .map_err(|error| mods::marketplace::update_failed(&want.filename, &error)),
                );
            }
            let mut notice = mods::marketplace::updated(pressed, results);
            for refused in refused {
                notice.text = format!("{} {}", notice.text, refused.text);
                notice.failed |= refused.failed;
            }
            // The check is asked again so the cards say the new versions are the current ones.
            *updates.lock().unwrap() = mods::marketplace::Updates::default();
            summary.lock().unwrap().push(notice.clone());
            latest.lock().unwrap().mods.notice = notice;
            settle.settle();
        });
    }

    /// Draws the window with the themes `settings.json` now names, and records what each one says
    /// that the app cannot use, which is what puts a theme's warnings on the Logs page too.
    fn apply_themes(&self) {
        let data_dir = self.data_dir.clone();
        let logs = self.logs.clone();
        let _ = self.window.upgrade_in_event_loop(move |window| {
            theme::apply_and_record(&window, &data_dir, &logs);
        });
    }

    fn setting_changed(&self, updated: Option<stremio_core::types::profile::Settings>) {
        if let Some(updated) = updated {
            settings::dispatch_settings(&self.runtime, updated);
        }
        let Ok(model) = self.runtime.model() else {
            return;
        };
        let data = settings::data(&model, &self.local_extras.lock().unwrap());
        self.video.apply_settings(&data.player_config);
        let mut snapshot = self.latest.lock().unwrap().clone();
        snapshot.settings = data;
        self.repaint_with(snapshot);
    }

    fn select_setting_option(&self, key: String, option: i32) {
        let Ok(index) = usize::try_from(option) else {
            return;
        };
        let updated = self.with_current_settings(|current, extras| {
            settings::apply_select_option(current, extras, &key, index)
        });
        self.setting_changed(updated);
    }

    fn toggle_setting_option(&self, key: String, enabled: bool) {
        let updated = self.with_current_settings(|current, extras| {
            settings::apply_toggle_option(current, extras, &key, enabled)
        });
        self.setting_changed(updated);
    }

    /// Runs a settings change against the profile's current settings and this shell's extras,
    /// saving the extras whether or not the profile understood the key.
    fn with_current_settings(
        &self,
        change: impl FnOnce(
            &stremio_core::types::profile::Settings,
            &mut settings::LocalExtras,
        ) -> Option<stremio_core::types::profile::Settings>,
    ) -> Option<stremio_core::types::profile::Settings> {
        let current = self.runtime.model().ok()?.ctx.profile.settings.clone();
        let mut extras = self.local_extras.lock().unwrap();
        let updated = change(&current, &mut extras);
        extras.save(&self.data_dir);
        updated
    }

    fn reload_streaming_server(&self) {
        let _ = self.window.upgrade_in_event_loop(|window| {
            window.set_server_status("Stream server: restarting...".into())
        });
        let _ = self.server.stop();
        let started = self.server.start();
        self.report_server_state(started);
        settings::reload_server(&self.runtime);
    }

    /// Waits for the server to answer and shows which of the possible things happened.
    pub(crate) fn report_server_state(&self, started: Result<(), String>) {
        // Only the window handle, the server, the plugin runtime and the line's own reporter
        // cross into the task: `VideoPlayer` holds `Rc`s and is not `Send`.
        let (window, server) = (self.window.clone(), self.server.clone());
        let (plugins, put) = (self.plugins.clone(), self.put_plugins());
        let logs = self.logs.clone();
        self.handle.spawn(async move {
            let state = stream_server::wait_until_answering(&server, started).await;
            let text = settings::server_status(&state);
            let running = matches!(state, crate::plumbing::stream_server::State::Ready);
            // What the server became is a record like any other. A server that answers is news; a
            // server that never did, or one that could not be started, is worth a warning.
            let level = match state {
                crate::plumbing::stream_server::State::Ready => crate::plumbing::logs::Level::Info,
                crate::plumbing::stream_server::State::PortTaken => {
                    crate::plumbing::logs::Level::Debug
                }
                _ => crate::plumbing::logs::Level::Warn,
            };
            logs.record(
                crate::plumbing::logs::Source::Server,
                stream_server::ADDRESS,
                level,
                &text,
            );
            // The plugins hear the server's own verdict, not the boot's hope: this task waits
            // until the server answers or gives up, which is when the fact is known. What they
            // ask for is drained here, so a line from their callback is on the Plugins page even
            // if no core event ever follows.
            let asked = plugins.deliver(Event::ServerRunning { running });
            put(asked);
            let _ =
                window.upgrade_in_event_loop(move |window| window.set_server_status(text.into()));
        });
    }

    fn start_browser_login(&self, provider: auth::Provider) {
        auth::start_browser_login(
            provider,
            &self.window,
            &self.runtime,
            &self.auth,
            &self.handle,
        );
    }

    /// Leaving the sign-in screen: drop the attempt in flight, and drop its spinner with it, or
    /// the overlay would reopen with its buttons still waiting for a sign-in that is gone.
    fn dismiss_sign_in(&self) {
        self.auth.cancel();
        let _ = self.window.upgrade_in_event_loop(|window| {
            auth::clear_progress(&window);
            window.set_auth_open(false);
            window.set_auth_error("".into());
        });
    }

    fn forgot_password(&self, email: String) {
        let email = email.trim();
        if email.is_empty() {
            links::open("reset-password");
        } else {
            let encoded =
                url::form_urlencoded::byte_serialize(email.as_bytes()).collect::<String>();
            links::open(&format!(
                "https://www.strem.io/reset-password/?email={encoded}"
            ));
        }
    }
}

/// Whose variants the Variants column lists: the language whose row was pressed, or, until one is,
/// the language that is playing.
fn browsed_language(pressed: &str, active: &str) -> String {
    if pressed.is_empty() {
        active.to_owned()
    } else {
        pressed.to_owned()
    }
}

/// Watches for the address the core resolved and hands it to mpv once. A timer rather than a
/// callback because the address arrives with a core event on another thread, and the player
/// cannot leave this one; a fifth of a second is well under the time anyone notices.
pub(crate) fn start_player_sync(wiring: &Wiring) {
    let (window, latest, video) = (
        wiring.window.clone(),
        wiring.latest.clone(),
        wiring.video.clone(),
    );
    let runtime = wiring.runtime.clone();
    let loaded = wiring.player_loaded.clone();
    let attempted = wiring.player_attempted.clone();
    let auto = wiring.subtitle_auto.clone();
    let browsed = wiring.browsed_language.clone();
    let playback = wiring.playback.clone();
    let error = wiring.playback_error.clone();
    wiring.player_sync.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(200),
        move || {
            let data = latest.lock().unwrap().player.clone();
            if !data.open {
                video.show(false);
                // Closed: the next Play is a fresh request, even for the stream that just failed.
                *attempted.lock().unwrap() = None;
                return;
            }
            if !crate::plumbing::player::should_open(
                &data.key,
                &loaded.lock().unwrap(),
                attempted.lock().unwrap().as_deref(),
            ) {
                return;
            }
            let Some(url) = data.url.clone() else {
                return;
            };
            match video.play(&url, data.resume_at) {
                Ok(()) => {
                    *loaded.lock().unwrap() = data.key.clone();
                    // A new stream: its subtitles are chosen afresh, and the menu starts out
                    // looking at whichever language is playing.
                    *auto.lock().unwrap() = false;
                    *browsed.lock().unwrap() = String::new();
                    *error.lock().unwrap() = String::new();
                    if let Ok(mut playing) = playback.lock() {
                        playing.sent_time = data.resume_at;
                        playing.paused = None;
                        playing.scale = match data.scale {
                            "cover" => stremio_core::types::player::VideoScale::Cover,
                            "fill" => stremio_core::types::player::VideoScale::Fill,
                            _ => stremio_core::types::player::VideoScale::Contain,
                        };
                        let _ = video.set_scale(playing.scale);
                    }
                    // What the stream says the file is: the core looks for subtitles that match
                    // this file by its size and name.
                    if let Ok(model) = runtime.model() {
                        let hints = model
                            .player
                            .selected
                            .as_ref()
                            .map(|selected| selected.stream.behavior_hints.clone())
                            .unwrap_or_default();
                        drop(model);
                        player::video_params_changed(
                            &runtime,
                            hints.video_size,
                            hints.filename.clone(),
                        );
                    }
                    video.show(true);
                    let _ = window.upgrade_in_event_loop(|window| {
                        window.set_player_error("".into());
                        window.set_player_loaded(false);
                    });
                }
                Err(message) => {
                    // One attempt per address. The stream may well have opened a moment later,
                    // but reloading it five times a second guarantees it never does.
                    *attempted.lock().unwrap() = Some(data.key.clone());
                    let _ = window.upgrade_in_event_loop(move |window| {
                        window.set_player_error(format!("Can't play this stream: {message}").into())
                    });
                }
            }
        },
    );
}

/// Registers every callback the window has. Each one is the same shape: work out what was
/// clicked, do it, and let the window catch up.
pub(crate) fn wire(window: &MainWindow, wiring: Wiring) {
    start_player_sync(&wiring);
    {
        // Web: `src/App/App.js` re-reads the account on mount and on every window focus, which
        // is how a mark made in the browser reaches another client. Slint's `Window` has no focus
        // flag, so the shell asks the platform, and only the rising edge pulls.
        let wiring = wiring.clone();
        let mut was_focused = false;
        window.on_check_focus(move || {
            let focused = window_is_in_front();
            if focused && !was_focused {
                crate::app::build::pull_from_api(&wiring.runtime);
            }
            was_focused = focused;
        });
    }
    {
        let wiring = wiring.clone();
        window.on_player_back(move || wiring.player_back());
    }
    {
        let wiring = wiring.clone();
        window.on_player_play_pause(move || wiring.player_play_pause());
    }
    {
        let wiring = wiring.clone();
        window.on_player_play_next(move || wiring.player_play_next());
    }
    {
        let wiring = wiring.clone();
        window.on_player_skip(move || wiring.player_skip());
    }
    {
        let wiring = wiring.clone();
        window.on_player_seek(move |time| wiring.player_seek(time));
    }
    {
        let wiring = wiring.clone();
        window.on_player_seek_by(move |offset| wiring.player_seek_by(offset));
    }
    {
        let wiring = wiring.clone();
        window.on_player_set_volume(move |volume| wiring.player_set_volume(volume));
    }
    {
        let wiring = wiring.clone();
        window.on_player_toggle_muted(move || wiring.player_toggle_muted());
    }
    {
        let wiring = wiring.clone();
        window.on_player_set_speed(move |speed| wiring.player_set_speed(speed));
    }
    {
        let wiring = wiring.clone();
        window.on_player_cycle_scale(move || wiring.player_cycle_scale());
    }
    {
        let wiring = wiring.clone();
        window.on_player_select_audio(move |id| wiring.player_select_audio(id.to_string()));
    }
    {
        let wiring = wiring.clone();
        window.on_player_choose_subtitles(move |id| wiring.player_choose_subtitles(id.to_string()));
    }
    {
        let wiring = wiring.clone();
        window.on_player_toggle_subtitles(move || wiring.player_toggle_subtitles());
    }
    {
        let wiring = wiring.clone();
        window.on_player_step_subtitles_size(move |step| wiring.player_step_subtitles_size(step));
    }
    {
        let wiring = wiring.clone();
        window
            .on_player_step_subtitles_offset(move |step| wiring.player_step_subtitles_offset(step));
    }
    {
        let wiring = wiring.clone();
        window.on_player_choose_language(move |language| {
            wiring.player_choose_language(language.to_string())
        });
    }
    {
        let wiring = wiring.clone();
        window.on_player_step_subtitles_position(move |step| {
            wiring.player_step_subtitles_position(step)
        });
    }
    {
        let wiring = wiring.clone();
        window.on_player_select_episode(move |id| wiring.player_select_episode(id.to_string()));
    }
    {
        let wiring = wiring.clone();
        window.on_player_select_season(move |index| wiring.player_select_season(index));
    }
    {
        let wiring = wiring.clone();
        window
            .on_player_choose_option(move |action| wiring.player_choose_option(action.to_string()));
    }
    {
        let wiring = wiring.clone();
        window.on_player_toggle_fullscreen(move || {
            let Some(window) = wiring.window.upgrade() else {
                return;
            };
            let wanted = !window.window().is_fullscreen();
            window.window().set_fullscreen(wanted);
            // The window is the only thing that knows whether it went: a compositor can refuse,
            // and the plugin hears what happened rather than what was asked for.
            let fullscreen = window.window().is_fullscreen();
            window.set_player_fullscreen(fullscreen);
            // Drained here rather than at the next repaint: a plugin that logs from this callback
            // or asks for a colour has its answer on screen before the press is over.
            let asked = wiring.plugins.deliver(Event::Fullscreen { on: fullscreen });
            wiring.drain(asked);
        });
    }
    {
        let wiring = wiring.clone();
        window.on_open_item(move |id, kind, title| {
            wiring.open_item(id.to_string(), kind.to_string(), title.to_string())
        });
    }
    {
        let wiring = wiring.clone();
        window.on_play_stream(move |index| wiring.play_stream(index));
    }
    {
        let wiring = wiring.clone();
        window.on_select_season(move |index| wiring.select_season(index));
    }
    {
        let wiring = wiring.clone();
        window.on_pick_video(move |video_id| wiring.pick_video(video_id.to_string()));
    }
    {
        let wiring = wiring.clone();
        window.on_search_videos(move |query| wiring.search_videos(query.to_string()));
    }
    {
        let wiring = wiring.clone();
        window.on_back_to_episodes(move || wiring.back_to_episodes());
    }
    {
        let wiring = wiring.clone();
        window.on_toggle_stream_wrap(move || wiring.toggle_stream_wrap());
    }
    {
        let wiring = wiring.clone();
        window.on_step_season(move |step| wiring.step(step, 0));
    }
    {
        let wiring = wiring.clone();
        window.on_step_episode(move |step| wiring.step(0, step));
    }
    {
        let wiring = wiring.clone();
        window.on_show_streams(move || wiring.show_streams());
    }
    {
        let wiring = wiring.clone();
        window.on_share(move || wiring.share());
    }
    {
        let wiring = wiring.clone();
        window.on_rate_like(move || wiring.rate(true));
    }
    {
        let wiring = wiring.clone();
        window.on_rate_love(move || wiring.rate(false));
    }
    {
        let wiring = wiring.clone();
        let _ = &wiring;
        window.on_details_toggle_library(move || wiring.toggle_library());
    }
    {
        let wiring = wiring.clone();
        let _ = &wiring;
        window.on_details_toggle_watched(move || wiring.toggle_watched());
    }
    {
        let wiring = wiring.clone();
        window.on_share_close(move || wiring.close_share());
    }
    {
        let wiring = wiring.clone();
        window.on_share_copy(move || wiring.copy_share());
    }
    {
        let wiring = wiring.clone();
        window.on_share_to(move |network| wiring.open_share_target(network.as_str()));
    }
    {
        let wiring = wiring.clone();
        window.on_next_episode(move || wiring.next_episode());
    }
    {
        let wiring = wiring.clone();
        window.on_load_next_page(move || {
            wiring.runtime.dispatch(RuntimeAction {
                field: None,
                action: Action::CatalogWithFilters(ActionCatalogWithFilters::LoadNextPage),
            });
        });
    }
    {
        let wiring = wiring.clone();
        window
            .on_select_input_option(move |input, option| wiring.select_input_option(input, option));
    }
    {
        let wiring = wiring.clone();
        window.on_select_preview(move |index| wiring.select_preview(index));
    }
    {
        let wiring = wiring.clone();
        window.on_hover_card(move |index| wiring.hover_card(index));
    }
    {
        let wiring = wiring.clone();
        window.on_select_library_type(move |option| wiring.select_library_type(option));
    }
    {
        let wiring = wiring.clone();
        window.on_select_library_sort(move |option| wiring.select_library_sort(option));
    }
    {
        let wiring = wiring.clone();
        window.on_load_more_library(move || wiring.load_more_library());
    }
    {
        let wiring = wiring.clone();
        window.on_library_item_action(move |id, action| {
            wiring.item_action(id.to_string(), action.to_string())
        });
    }
    {
        let wiring = wiring.clone();
        window.on_board_item_action(move |id, action| {
            wiring.item_action(id.to_string(), action.to_string())
        });
    }
    {
        let wiring = wiring.clone();
        window.on_visible_range(move |scroll, height, pitch, columns| {
            wiring.visible_range(scroll, height, pitch, columns)
        });
    }
    {
        let wiring = wiring.clone();
        window.on_select_addon_input_option(move |input, option| {
            wiring.select_addon_input_option(input, option)
        });
    }
    {
        let wiring = wiring.clone();
        window.on_search_addons(move |_| wiring.repaint());
    }
    {
        let wiring = wiring.clone();
        window.on_addon_add(move || {
            // The dialog is opened to read a new URL, so whatever addon it was last shown is
            // forgotten with it. Otherwise its one button still holds the old manifest as ready
            // and pressing it would install that addon instead of reading the URL just pasted.
            wiring.runtime.dispatch(RuntimeAction {
                field: Some(AppModelField::AddonDetails),
                action: Action::Unload,
            });
            if let Some(window) = wiring.window.upgrade() {
                window.set_addon_add_error("".into());
            }
        });
    }
    {
        let wiring = wiring.clone();
        window.on_addon_add_submit(move |text| {
            let error = addons::open_details(&wiring.runtime, &text)
                .err()
                .unwrap_or_default();
            if let Some(window) = wiring.window.upgrade() {
                window.set_addon_add_error(error.into());
            }
        });
    }
    {
        let wiring = wiring.clone();
        window.on_addon_add_install(move || addons::install(&wiring.runtime));
    }
    {
        let wiring = wiring.clone();
        window.on_addon_add_close(move || {
            if let Some(window) = wiring.window.upgrade() {
                window.set_addon_add_error("".into());
            }
        });
    }
    {
        let wiring = wiring.clone();
        window.on_addon_uninstall(move |url| addons::uninstall(&wiring.runtime, &url));
    }
    {
        let wiring = wiring.clone();
        window.on_addon_share(move |url| {
            if let Some(window) = wiring.window.upgrade() {
                window.set_share_link(url.clone());
                window.set_share_open(true);
            }
        });
    }
    {
        let wiring = wiring.clone();
        window.on_addon_open(move |url| {
            // A marketplace card has no configuration page to open yet: the web opens the manifest
            // dialog on it, which is where its Install button is.
            let installed = wiring
                .latest
                .lock()
                .unwrap()
                .addons
                .cards
                .iter()
                .any(|card| card.transport_url == url.as_str() && card.installed);
            if !installed {
                if addons::open_details(&wiring.runtime, &url).is_ok() {
                    if let Some(window) = wiring.window.upgrade() {
                        window.set_addon_add_open(true);
                        window.set_addon_add_url(url.clone());
                        window.set_addon_add_submitted(true);
                        window.set_addon_add_error("".into());
                    }
                }
                return;
            }
            addons::configure(&url);
        });
    }
    {
        let wiring = wiring.clone();
        window.on_addon_install_market(move |url| wiring.install_market_addon(&url));
    }
    // Web: the Configure pill on a card, and the icon one beside Uninstall, both open the addon's
    // own configuration page.
    window.on_addon_configure(move |url| addons::configure(&url));
    {
        let wiring = wiring.clone();
        window.on_select_setting_option(move |key, option| {
            wiring.select_setting_option(key.to_string(), option)
        });
    }
    {
        let wiring = wiring.clone();
        window.on_toggle_setting_option(move |key, enabled| {
            wiring.toggle_setting_option(key.to_string(), enabled)
        });
    }
    {
        let wiring = wiring.clone();
        window.on_calendar_shown(move || wiring.calendar_shown());
    }
    {
        let wiring = wiring.clone();
        window.on_calendar_prev_month(move || wiring.calendar_step(false));
    }
    {
        let wiring = wiring.clone();
        window.on_calendar_next_month(move || wiring.calendar_step(true));
    }
    {
        let wiring = wiring.clone();
        window.on_calendar_open_slot(move |meta_id, kind, video_id| {
            wiring.calendar_open(meta_id.to_string(), kind.to_string(), video_id.to_string())
        });
    }
    window.on_open_settings_link(move |kind| links::open(&kind));
    {
        let wiring = wiring.clone();
        window.on_logout_user(move || settings::logout(&wiring.runtime));
    }
    {
        let wiring = wiring.clone();
        window.on_reload_streaming_server(move || wiring.reload_streaming_server());
    }
    {
        let wiring = wiring.clone();
        window.on_select_mods_page(move |page| wiring.select_mods_page(page.to_string()));
    }
    {
        let wiring = wiring.clone();
        window.on_toggle_mods_mod(move |filename, enabled| {
            wiring.toggle_mod(filename.to_string(), enabled);
        });
    }
    {
        let wiring = wiring.clone();
        window.on_remove_mods_mod(move |filename| wiring.remove_mod(filename.to_string()));
    }
    {
        let wiring = wiring.clone();
        window.on_dismiss_mods_remove(move || wiring.dismiss_mods_remove());
    }
    {
        let wiring = wiring.clone();
        window.on_pick_mods_settings_plugin(move |filename| {
            wiring.pick_plugin(filename.to_string());
        });
    }
    {
        let wiring = wiring.clone();
        window.on_edit_mods_setting_toggle(move |key, on| {
            wiring.edit_plugin_setting(key.to_string(), serde_json::Value::Bool(on), false);
        });
    }
    {
        let wiring = wiring.clone();
        window.on_edit_mods_setting_text(move |key, text| {
            wiring.edit_plugin_setting(
                key.to_string(),
                serde_json::Value::String(text.to_string()),
                true,
            );
        });
    }
    {
        let wiring = wiring.clone();
        window.on_edit_mods_setting_number(move |key, number| {
            // A slider reports a float; a JSON number that will not take one is no press.
            let value = serde_json::Number::from_f64(f64::from(number))
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null);
            wiring.edit_plugin_setting(key.to_string(), value, false);
        });
    }
    {
        let wiring = wiring.clone();
        window.on_save_mods_settings(move || wiring.save_plugin_settings());
    }
    {
        let wiring = wiring.clone();
        window.on_revert_mods_settings(move || wiring.revert_plugin_settings());
    }
    {
        let wiring = wiring.clone();
        window.on_search_mods_market(move |text| wiring.search_market(text.to_string()));
    }
    {
        let wiring = wiring.clone();
        window.on_install_mods_market(move |id| wiring.install_market(id.to_string()));
    }
    {
        let wiring = wiring.clone();
        window.on_update_mods_mod(move |filename| {
            wiring.update_mods(Some(filename.to_string()));
        });
    }
    {
        let wiring = wiring.clone();
        window.on_update_all_mods_mods(move || wiring.update_mods(None));
    }
    {
        let wiring = wiring.clone();
        window.on_check_mods_updates(move || {
            *wiring.market_check_asked.lock().unwrap() = false;
            wiring.check_updates(true);
        });
    }
    {
        let wiring = wiring.clone();
        window.on_dismiss_mods_market(move || wiring.dismiss_market());
    }
    {
        let wiring = wiring.clone();
        window.on_filter_mods_logs(move |part, value| {
            wiring.filter_logs(&part, value.to_string());
        });
    }
    {
        let wiring = wiring.clone();
        window.on_search_mods_logs(move |text| {
            wiring.filter_logs("search", text.to_string());
        });
    }
    {
        let wiring = wiring.clone();
        window.on_open_mods_log(move |id| wiring.open_log_record(id.max(0) as u32));
    }
    {
        let wiring = wiring.clone();
        window.on_clear_mods_logs(move || wiring.clear_logs());
    }
    {
        let wiring = wiring.clone();
        window.on_refresh_mods_about(move || wiring.refresh_about());
    }
    {
        let wiring = wiring.clone();
        window.on_export_mods_diagnostics(move || wiring.export_diagnostics());
    }
    {
        let wiring = wiring.clone();
        window.on_copy_mods_version(move || wiring.copy_version());
    }
    {
        let wiring = wiring.clone();
        window.on_submit_auth_login(move |email, password| {
            let _ = wiring.window.upgrade_in_event_loop(|window| {
                window.set_auth_loading(true);
                window.set_auth_error("".into());
            });
            auth::login(
                &wiring.runtime,
                email.trim().to_string(),
                password.to_string(),
            );
        });
    }
    {
        let wiring = wiring.clone();
        window.on_submit_auth_register(move |email, password, tos, privacy, marketing| {
            let _ = wiring.window.upgrade_in_event_loop(|window| {
                window.set_auth_loading(true);
                window.set_auth_error("".into());
            });
            auth::register(
                &wiring.runtime,
                email.trim().to_string(),
                password.to_string(),
                GDPRConsent {
                    tos,
                    privacy,
                    marketing,
                    from: Some("stremio-lightning".to_string()),
                },
            );
        });
    }
    {
        let wiring = wiring.clone();
        window.on_auth_guest_login(move || wiring.dismiss_sign_in());
    }
    {
        let wiring = wiring.clone();
        window.on_auth_facebook_login(move || wiring.start_browser_login(auth::Provider::Facebook));
    }
    {
        let wiring = wiring.clone();
        window.on_auth_apple_login(move || wiring.start_browser_login(auth::Provider::Apple));
    }
    {
        let wiring = wiring.clone();
        window.on_auth_forgot_password(move |email| wiring.forgot_password(email.to_string()));
    }
    window.on_auth_dismissed(move || wiring.dismiss_sign_in());
}

/// Whether this process owns the window in front of the user. On Windows that is
/// `GetForegroundWindow` and the id of the process that made it; everywhere else there is no
/// cheap answer, so the app treats itself as in front and leaves the pulls to startup.
fn window_is_in_front() -> bool {
    #[cfg(windows)]
    {
        use windows::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId,
        };
        let front = unsafe { GetForegroundWindow() };
        if front.0.is_null() {
            return false;
        }
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(front, Some(&mut pid)) };
        pid == std::process::id()
    }
    #[cfg(not(windows))]
    {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::browsed_language;

    #[test]
    fn the_variants_column_starts_out_on_the_language_that_plays() {
        assert_eq!(browsed_language("", "eng"), "eng");
    }

    #[test]
    fn a_pressed_language_holds_the_column_there() {
        // Pressing a language row only looks at it; the track that plays is another matter.
        assert_eq!(browsed_language("spa", "eng"), "spa");
    }

    #[test]
    fn turning_subtitles_off_leaves_no_language_to_look_at() {
        assert_eq!(browsed_language("", ""), "");
    }
}
