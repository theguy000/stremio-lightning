//! The wiring: every window callback organized by screen, each naming the action it performs.
//!
//! Nothing here decides anything. A callback works out what was clicked, hands it to a screen's
//! action or `Arc<App>` handle, and projects the result onto the window.

mod addons;
mod browse;
mod details;
mod logs_about;
mod mods;
mod player;
mod player_skip;
mod player_skip_panel;
mod player_tracks;
mod settings_auth;
mod share;
mod viewport;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use slint::{ComponentHandle, Timer, Weak};

use crate::app::{ActiveScreen, App, ScreenMask};
use crate::plumbing::skip;
use crate::plumbing::stream_server::Supervisor;
use crate::screens::player::skip::SkipSession;
use crate::screens::player::SubtitleChoice;
use crate::shell::video_host::VideoPlayer;
use crate::{MainWindow, PlayerUi};

pub(crate) use player::Playback;
pub(crate) use player_tracks::ShownTracks;

pub(super) fn active_screen_of(window: &MainWindow) -> ActiveScreen {
    if window.get_playing() {
        return ActiveScreen::Player;
    }
    if window.get_details_open() {
        return ActiveScreen::Details;
    }
    match window.get_section() {
        crate::Section::Board => ActiveScreen::Board,
        crate::Section::Discover => ActiveScreen::Discover,
        crate::Section::Library => ActiveScreen::Library,
        crate::Section::Calendar => ActiveScreen::Calendar,
        crate::Section::Addons => ActiveScreen::Addons,
        crate::Section::Settings => ActiveScreen::Settings,
        crate::Section::Mods => ActiveScreen::Mods,
    }
}

/// Desktop shell callback adapter attached to `Arc<App>`.
///
/// Holds only shell/UI-local state (mpv player session, AniSkip session, one-shot UI timers,
/// toast counter); all authoritative app state lives in `app`.
#[derive(Clone)]
pub(crate) struct Wiring {
    pub(crate) window: Weak<MainWindow>,
    pub(crate) app: Arc<App>,
    pub(crate) video: VideoPlayer,
    pub(crate) server: Arc<Supervisor>,
    pub(crate) handle: tokio::runtime::Handle,
    pub(crate) said: Arc<AtomicU64>,
    pub(crate) player_loaded: Arc<Mutex<String>>,
    pub(crate) player_attempted: Arc<Mutex<Option<String>>>,
    pub(crate) playback: Arc<Mutex<Playback>>,
    pub(crate) settle: Arc<Timer>,
    pub(crate) shown_tracks: Arc<Mutex<ShownTracks>>,
    pub(crate) subtitle_choice: Arc<Mutex<SubtitleChoice>>,
    pub(crate) subtitle_auto: Arc<Mutex<bool>>,
    pub(crate) browsed_language: Arc<Mutex<String>>,
    pub(crate) skip_client: Arc<skip::SkipClient>,
    pub(crate) skip: Arc<Mutex<SkipSession>>,
    pub(crate) skip_submit: Arc<Mutex<bool>>,
    pub(crate) mods_disarm: Arc<Timer>,
}

impl Wiring {
    pub(crate) fn new(
        window: Weak<MainWindow>,
        app: Arc<App>,
        video: VideoPlayer,
        server: Arc<Supervisor>,
        handle: tokio::runtime::Handle,
    ) -> Self {
        Self {
            window,
            app,
            video,
            server,
            handle,
            said: Default::default(),
            player_loaded: Default::default(),
            player_attempted: Default::default(),
            playback: Default::default(),
            settle: Default::default(),
            shown_tracks: Default::default(),
            subtitle_choice: Default::default(),
            subtitle_auto: Default::default(),
            browsed_language: Default::default(),
            skip_client: Arc::new(skip::SkipClient::new()),
            skip: Default::default(),
            skip_submit: Default::default(),
            mods_disarm: Default::default(),
        }
    }

    fn player_select_episode(&self, video_id: String) {
        let _ = crate::screens::player::load_video(self.app.runtime(), &video_id);
        self.app
            .player()
            .set_season(crate::screens::player::FOLLOW_PLAYING_SEASON);
    }

    fn player_select_season(&self, index: i32) {
        if let Ok(index) = usize::try_from(index) {
            self.app.player().set_season(index);
            self.app.player().refresh();
        }
    }

    pub(super) fn repaint(&self) {
        self.app.mark_dirty(ScreenMask::ALL);
        self.app.viewport().sync_images();
    }

    pub(super) fn request_painting(&self, url: String) {
        self.app.viewport().request_image(url);
    }

    /// Shows a transient toast message at the bottom of the window.
    pub(super) fn say(&self, text: &str) {
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
}

fn wire_player(window: &MainWindow, wiring: &Wiring) {
    let ui = window.global::<PlayerUi>();
    let w = wiring.clone();
    ui.on_back(move || w.player_back());
    let w = wiring.clone();
    ui.on_play_pause(move || w.player_play_pause());
    let w = wiring.clone();
    ui.on_play_next(move || w.player_play_next());
    let w = wiring.clone();
    ui.on_skip(move || w.player_skip());
    let w = wiring.clone();
    ui.on_open_skip(move || w.open_skip());
    let w = wiring.clone();
    ui.on_pick_skip_kind(move |kind| w.pick_skip_kind(kind.to_string()));
    let w = wiring.clone();
    ui.on_submit_skip(move |k, s, e| w.submit_skip(k.to_string(), s.to_string(), e.to_string()));
    let w = wiring.clone();
    ui.on_vote_skip(move |id, up| w.vote_skip(id.to_string(), up));
    let w = wiring.clone();
    ui.on_seek(move |time| w.player_seek(time));
    let w = wiring.clone();
    ui.on_seek_by(move |offset| w.player_seek_by(offset));
    let w = wiring.clone();
    ui.on_set_volume(move |volume| w.player_set_volume(volume));
    let w = wiring.clone();
    ui.on_toggle_muted(move || w.player_toggle_muted());
    let w = wiring.clone();
    ui.on_set_speed(move |speed| w.player_set_speed(speed));
    let w = wiring.clone();
    ui.on_cycle_scale(move || w.player_cycle_scale());
    let w = wiring.clone();
    ui.on_select_audio(move |id| w.player_select_audio(id.to_string()));
    let w = wiring.clone();
    ui.on_choose_subtitles(move |id| w.player_choose_subtitles(id.to_string()));
    let w = wiring.clone();
    ui.on_toggle_subtitles(move || w.player_toggle_subtitles());
    let w = wiring.clone();
    ui.on_step_subtitles_size(move |step| w.player_step_subtitles_size(step));
    let w = wiring.clone();
    ui.on_step_subtitles_offset(move |step| w.player_step_subtitles_offset(step));
    let w = wiring.clone();
    ui.on_choose_language(move |lang| w.player_choose_language(lang.to_string()));
    let w = wiring.clone();
    ui.on_step_subtitles_position(move |step| w.player_step_subtitles_position(step));
    let w = wiring.clone();
    ui.on_select_episode(move |id| w.player_select_episode(id.to_string()));
    let w = wiring.clone();
    ui.on_select_season(move |index| w.player_select_season(index));
    let w = wiring.clone();
    ui.on_choose_option(move |action| w.player_choose_option(action.to_string()));
    let w = wiring.clone();
    ui.on_toggle_fullscreen(move || {
        let Some(window) = w.window.upgrade() else {
            return;
        };
        let wanted = !window.window().is_fullscreen();
        window.window().set_fullscreen(wanted);
        window
            .global::<PlayerUi>()
            .set_fullscreen(window.window().is_fullscreen());
    });
}

pub(crate) fn wire(window: &MainWindow, wiring: Wiring) {
    wire_player(window, &wiring);
    details::wire(window, &wiring);
    share::wire(window, &wiring);
    browse::wire(window, &wiring);
    addons::wire(window, &wiring);
    settings_auth::wire(window, &wiring);
    mods::wire(window, &wiring);
    logs_about::wire(window, &wiring);
}
