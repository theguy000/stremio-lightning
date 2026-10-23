//! The wiring: every window callback in one place, each one naming the action it performs.
//!
//! Nothing here decides anything. A callback works out what was clicked, hands it to a screen's
//! action, and puts the result where the window can see it.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, ModelRc, Timer, Weak};
use stremio_core::models::catalog_with_filters::Selected as CatalogSelected;
use stremio_core::models::installed_addons_with_filters::Selected as InstalledSelected;
use stremio_core::runtime::msg::{Action, ActionCatalogWithFilters, ActionLoad};
use stremio_core::runtime::RuntimeAction;
use stremio_core::types::profile::GDPRConsent;

use crate::app::{AppModel, AppRuntime, Snapshot};
use crate::plumbing::clipboard;
use crate::plumbing::images::Images;
use crate::plumbing::links;
use crate::plumbing::player::VideoPlayer;
use crate::plumbing::stream_server::{self, Supervisor};
use crate::screens::{auth, details, discover, player, settings};
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
    pub(crate) playback_error: Arc<Mutex<String>>,
    pub(crate) playback: Arc<Mutex<Playback>>,
    pub(crate) player_sync: Arc<Timer>,
    /// Restarted by the Discover grid every time it moves; fires when it has stopped, which is when
    /// the row of cards either side of the screen is fetched.
    pub(crate) settle: Arc<Timer>,
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
}

impl Default for Playback {
    fn default() -> Self {
        Self {
            time: 0.0,
            duration: 0.0,
            sent_time: 0.0,
            paused: None,
            scale: stremio_core::types::player::VideoScale::Contain,
        }
    }
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
        window.set_player_audio_tracks(shell::view::tracks(&status.audio));
        window.set_player_subtitle_tracks(shell::view::tracks(&status.subtitles));
        window.set_player_subtitles_enabled(status.subtitles.iter().any(|track| track.selected));
        window.set_player_subtitles_size((status.subtitles_scale * 100.0) as f32);
        window.set_player_subtitles_offset((status.subtitles_delay * 1000.0) as f32);

        if status.error.is_empty() {
            // A file that is open has no error, whatever the one before it said.
            if !self.playback_error.lock().unwrap().is_empty() {
                *self.playback_error.lock().unwrap() = String::new();
                window.set_player_error("".into());
            }
        } else if *self.playback_error.lock().unwrap() != status.error {
            *self.playback_error.lock().unwrap() = status.error.clone();
            window.set_player_error(status.error.into());
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
        if status.paused.is_some() && status.paused != was_paused {
            player::paused_changed(&self.runtime, status.paused.unwrap_or(false));
        }
        if status.ended {
            self.ended();
        }
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
        self.video.stop();
        self.video.show(false);
        *self.player_loaded.lock().unwrap() = String::new();
        *self.playback_error.lock().unwrap() = String::new();
        *self.playback.lock().unwrap() = Playback::default();
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

    /// One of the subtitle rows. An id from the list is either an mpv track already inside the
    /// file or a subtitle file the stream or an addon offered, which has to be added first.
    fn player_choose_subtitles(&self, id: String) {
        let found = self
            .latest
            .lock()
            .unwrap()
            .player
            .subtitles
            .iter()
            .find(|subtitle| subtitle.id == id)
            .map(|subtitle| (subtitle.url.clone(), subtitle.title.clone()));
        match found {
            Some((url, language)) => {
                let _ = self.video.add_subtitles(&url, &language);
            }
            None => {
                let _ = self.video.select_subtitles(Some(&id));
            }
        }
    }

    fn player_toggle_subtitles(&self) {
        let showing = self
            .video
            .status()
            .subtitles
            .iter()
            .any(|track| track.selected);
        let _ = self
            .video
            .select_subtitles(if showing { None } else { Some("auto") });
    }

    /// The subtitles steppers: size in tenths, timing in hundredths of a second, as the web's
    /// own steppers move them.
    fn player_step_subtitles_size(&self, step: i32) {
        let current = self.video.status().subtitles_scale;
        let next = (current + f64::from(step) * 0.1).clamp(0.2, 3.0);
        let _ = self.video.set_subtitles_scale(next);
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

    fn select_addon_input_option(&self, input: i32, option: i32) {
        // Input 0 is the catalog menu, which has only "Installed".
        if input != 1 {
            return;
        }
        let request = usize::try_from(option).ok().and_then(|option| {
            self.latest
                .lock()
                .unwrap()
                .addons
                .type_requests
                .get(option)
                .cloned()
        });
        if let Some(request) = request {
            self.runtime.dispatch(RuntimeAction {
                field: None,
                action: Action::Load(ActionLoad::InstalledAddonsWithFilters(InstalledSelected {
                    request,
                })),
            });
        }
    }

    /// A setting changed: write it, tell the core, and let the player and the screen catch up.
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
        // Only the window handle and the server cross into the task: `VideoPlayer` holds `Rc`s
        // and is not `Send`.
        let (window, server) = (self.window.clone(), self.server.clone());
        self.handle.spawn(async move {
            let state = stream_server::wait_until_answering(&server, started).await;
            let text = settings::server_status(&state);
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
    let playback = wiring.playback.clone();
    let error = wiring.playback_error.clone();
    wiring.player_sync.start(
        slint::TimerMode::Repeated,
        std::time::Duration::from_millis(200),
        move || {
            let data = latest.lock().unwrap().player.clone();
            if !data.open {
                video.show(false);
                return;
            }
            if data.key == *loaded.lock().unwrap() {
                return;
            }
            let Some(url) = data.url.clone() else {
                return;
            };
            match video.play(&url, data.resume_at) {
                Ok(()) => {
                    *loaded.lock().unwrap() = data.key.clone();
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
            let _ = wiring.window.upgrade_in_event_loop(|window| {
                let fullscreen = !window.window().is_fullscreen();
                window.window().set_fullscreen(fullscreen);
                window.set_player_fullscreen(fullscreen);
            });
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
