//! Slint OpenGL video adapter: lazily initializes `MpvSession` inside `BeforeRendering`, tears it
//! down on `stop()` / `RenderingTeardown`, and forwards `Status` changes to the shell listener.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use slint::{ComponentHandle, GraphicsAPI, RenderingState, Timer, TimerMode, Weak};
use stremio_core::types::player::VideoScale;

use crate::plumbing::logs::Logs;
use crate::plumbing::player::{
    player_unavailable, DesiredSettings, MpvSession, PlayerConfig, Status,
};
use crate::MainWindow;

const TICK: Duration = Duration::from_millis(200);
const UNAVAILABLE: &str = "video playback is not available in this window";

type StatusListener = Rc<dyn Fn(Status)>;

#[derive(Default)]
struct HostState {
    session: Option<MpvSession>,
    gl_available: bool,
    config: Option<PlayerConfig>,
    desired: DesiredSettings,
    pending_play: Option<(String, f64)>,
    pending_error: Option<String>,
    teardown_requested: bool,
    showing: bool,
    status: Status,
    listener: Option<StatusListener>,
}

impl HostState {
    fn on_before_rendering(&mut self, api: &GraphicsAPI<'_>, weak: &Weak<MainWindow>, logs: &Logs) {
        if self.teardown_requested && self.pending_play.is_none() && !self.showing {
            self.teardown_requested = false;
            drop(self.session.take());
            return;
        }
        self.teardown_requested = false;
        if let Some((url, resume_at)) = self.pending_play.take() {
            if let Some(error) = self.start_or_load(api, weak, logs, &url, resume_at) {
                self.pending_error = Some(format!("Can't play this stream: {error}"));
            }
        }
        if self.showing {
            if let (Some(s), Some(w)) = (self.session.as_ref(), weak.upgrade()) {
                let size = w.window().size();
                s.draw(size.width as i32, size.height as i32);
            }
        }
    }

    fn start_or_load(
        &mut self,
        api: &GraphicsAPI<'_>,
        weak: &Weak<MainWindow>,
        logs: &Logs,
        url: &str,
        resume_at: f64,
    ) -> Option<String> {
        if let Some(existing) = self.session.as_mut() {
            return existing.load_stream(url, resume_at).err();
        }
        let GraphicsAPI::NativeOpenGL { get_proc_address } = api else {
            return Some(UNAVAILABLE.to_owned());
        };
        let frame_weak = weak.clone();
        match MpvSession::open(get_proc_address, move || {
            let _ = frame_weak.upgrade_in_event_loop(|w| w.window().request_redraw());
        }) {
            Ok(mut created) => {
                created.apply_desired(self.config.as_ref(), self.desired);
                let err = created.load_stream(url, resume_at).err();
                self.session = Some(created);
                err
            }
            Err(error) => {
                player_unavailable(logs, &error);
                Some(error)
            }
        }
    }

    fn poll_tick(&mut self, logs: &Logs) -> Option<(Status, Option<StatusListener>, bool)> {
        if let Some(error) = self.pending_error.take() {
            self.status.loaded = false;
            self.status.error = error;
            return Some((self.status.clone(), self.listener.clone(), false));
        }
        let previous = self.status.clone();
        let next = self.session.as_mut()?.poll_status(&previous, logs)?;
        self.status = next.clone();
        Some((next, self.listener.clone(), self.showing))
    }
}

#[derive(Clone)]
pub(crate) struct VideoPlayer {
    weak: Weak<MainWindow>,
    state: Rc<RefCell<HostState>>,
    events: Rc<Timer>,
}

impl VideoPlayer {
    pub(crate) fn install(window: &MainWindow, logs: Arc<Logs>) -> Self {
        let state = Rc::new(RefCell::new(HostState::default()));
        let installed = window.window().set_rendering_notifier({
            let (state, logs, weak) = (state.clone(), logs.clone(), window.as_weak());
            move |rendering, api| {
                let mut host = state.borrow_mut();
                match rendering {
                    RenderingState::RenderingSetup => {
                        host.gl_available = matches!(api, GraphicsAPI::NativeOpenGL { .. });
                    }
                    RenderingState::BeforeRendering => host.on_before_rendering(api, &weak, &logs),
                    RenderingState::RenderingTeardown => {
                        host.gl_available = false;
                        drop(host.session.take());
                    }
                    _ => {}
                }
            }
        });
        if let Err(error) = installed {
            player_unavailable(&logs, &error.to_string());
        }

        let events = Timer::default();
        events.start(TimerMode::Repeated, TICK, {
            let (state, logs, weak) = (state.clone(), logs.clone(), window.as_weak());
            move || {
                let notify = state.borrow_mut().poll_tick(&logs);
                if let Some((next, listener, redraw)) = notify {
                    if redraw {
                        if let Some(w) = weak.upgrade() {
                            w.window().request_redraw();
                        }
                    }
                    if let Some(listener) = listener {
                        listener(next);
                    }
                }
            }
        });
        events.stop();
        Self {
            weak: window.as_weak(),
            state,
            events: Rc::new(events),
        }
    }

    pub(crate) fn listen(&self, listener: impl Fn(Status) + 'static) {
        self.state.borrow_mut().listener = Some(Rc::new(listener));
    }
    pub(crate) fn show(&self, showing: bool) {
        self.state.borrow_mut().showing = showing;
    }
    pub(crate) fn status(&self) -> Status {
        self.state.borrow().status.clone()
    }

    fn with_session<R>(
        &self,
        act: impl FnOnce(&MpvSession) -> Result<R, String>,
    ) -> Result<R, String> {
        act(self.state.borrow().session.as_ref().ok_or(UNAVAILABLE)?)
    }

    fn set_preinit(
        &self,
        save: impl FnOnce(&mut DesiredSettings),
        apply: impl FnOnce(&MpvSession) -> Result<(), String>,
    ) -> Result<(), String> {
        let mut host = self.state.borrow_mut();
        save(&mut host.desired);
        if host.session.is_none() && host.pending_play.is_some() {
            return Ok(());
        }
        apply(host.session.as_ref().ok_or(UNAVAILABLE)?)
    }

    pub(crate) fn play(&self, url: &str, resume_at: f64) -> Result<(), String> {
        let mut host = self.state.borrow_mut();
        if !host.gl_available && host.session.is_none() {
            return Err(UNAVAILABLE.to_owned());
        }
        host.status = Status {
            volume: host.status.volume.or(host.desired.volume),
            muted: host.status.muted.or(host.desired.muted),
            speed: host.status.speed.or(host.desired.speed),
            ..Status::default()
        };
        host.teardown_requested = false;
        host.pending_play = None;
        if !self.events.running() {
            self.events.restart();
        }
        if let Some(session) = host.session.as_mut() {
            return session.load_stream(url, resume_at);
        }
        host.pending_play = Some((url.to_owned(), resume_at));
        drop(host);
        if let Some(w) = self.weak.upgrade() {
            w.window().request_redraw();
        }
        Ok(())
    }

    pub(crate) fn stop(&self) {
        let mut host = self.state.borrow_mut();
        host.pending_play = None;
        host.showing = false;
        self.events.stop();
        let needs_teardown = host.session.as_ref().is_some_and(|s| {
            s.stop();
            true
        });
        host.teardown_requested = needs_teardown;
        host.status = Status::default();
        drop(host);
        if needs_teardown {
            if let Some(w) = self.weak.upgrade() {
                w.window().request_redraw();
            }
        }
    }

    pub(crate) fn set_paused(&self, paused: bool) -> Result<(), String> {
        self.with_session(|s| s.set_prop("pause", paused))
    }
    pub(crate) fn seek(&self, time: f64) -> Result<(), String> {
        let seconds = format!("{time:.3}");
        self.with_session(|s| s.command("seek", &[seconds.as_str(), "absolute+exact"]))
    }
    pub(crate) fn set_volume(&self, v: f64) -> Result<(), String> {
        self.set_preinit(|d| d.volume = Some(v), |s| s.set_prop("volume", v))
    }
    pub(crate) fn set_muted(&self, m: bool) -> Result<(), String> {
        self.set_preinit(|d| d.muted = Some(m), |s| s.set_prop("mute", m))
    }
    pub(crate) fn set_speed(&self, v: f64) -> Result<(), String> {
        self.set_preinit(|d| d.speed = Some(v), |s| s.set_prop("speed", v))
    }
    pub(crate) fn select_audio(&self, id: &str) -> Result<(), String> {
        self.with_session(|s| s.set_prop("aid", id))
    }
    pub(crate) fn select_subtitles(&self, id: Option<&str>) -> Result<(), String> {
        self.with_session(|s| s.set_prop("sid", id.unwrap_or("no")))
    }
    pub(crate) fn add_subtitles(&self, url: &str, title: &str, lang: &str) -> Result<(), String> {
        self.with_session(|s| s.add_subtitles(url, title, lang))
    }
    pub(crate) fn set_subtitles_scale(&self, v: f64) -> Result<(), String> {
        self.with_session(|s| s.set_prop("sub-scale", v))
    }
    pub(crate) fn set_subtitles_position(&self, v: f64) -> Result<(), String> {
        self.with_session(|s| s.set_prop("sub-pos", v))
    }
    pub(crate) fn set_subtitles_delay(&self, v: f64) -> Result<(), String> {
        self.with_session(|s| s.set_prop("sub-delay", v))
    }
    pub(crate) fn set_scale(&self, scale: VideoScale) -> Result<(), String> {
        self.set_preinit(
            |d| d.scale = scale,
            |s| {
                s.apply_scale(scale);
                Ok(())
            },
        )
    }
    pub(crate) fn apply_settings(&self, settings: &PlayerConfig) {
        let mut host = self.state.borrow_mut();
        host.config = Some(settings.clone());
        if let Some(session) = host.session.as_ref() {
            session.apply_config(settings);
        }
    }
}
