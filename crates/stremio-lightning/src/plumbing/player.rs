//! Video playback. libmpv draws every frame into Slint's own OpenGL context just before Slint
//! draws the window's items on top, so the player screen only has to leave the video area empty.
//!
//! Everything that talks to mpv lives here. The screen never learns mpv's property names: it
//! reads a [`Status`] and asks for one of the verbs on [`VideoPlayer`]. The status is pushed to a
//! listener the app installs, so the window and the core hear about playback in the same terms.

use std::cell::{Cell, RefCell};
use std::ffi::{c_void, CStr, CString};
use std::rc::Rc;
use std::time::Duration;

use libmpv2::events::Event;
use libmpv2::render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType};
use libmpv2::{mpv_end_file_reason, Mpv};
use slint::{ComponentHandle, GraphicsAPI, PhysicalSize, RenderingState, Timer, TimerMode, Weak};
use stremio_core::types::player::VideoScale;

use crate::MainWindow;

const GL_FRAMEBUFFER_BINDING: u32 = 0x8CA6;
const GL_COLOR_BUFFER_BIT: u32 = 0x4000;
const GL_SCISSOR_TEST: u32 = 0x0C11;
/// How often mpv's properties are read and its events drained. Web has `timeupdate` at about
/// 4 Hz, so the seek bar moves with the same grain here.
const TICK: Duration = Duration::from_millis(200);

type GlLookup<'a> = &'a dyn Fn(&CStr) -> *const c_void;
type GlGetIntegerv = unsafe extern "system" fn(u32, *mut i32);
type GlClear = unsafe extern "system" fn(u32);
type GlClearColor = unsafe extern "system" fn(f32, f32, f32, f32);
type GlDisable = unsafe extern "system" fn(u32);

/// One audio or subtitle track mpv can be switched to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Track {
    /// mpv's own track id, as a string because that is how its commands take it.
    pub id: String,
    pub title: String,
    pub language: String,
    pub selected: bool,
    /// A file added beside the video (the stream's or an addon's subtitle), not one inside it.
    pub external: bool,
}

/// What mpv is doing, in the app's words. Every field is `None` until playback starts, so the
/// player screen can tell "not playing yet" from "paused".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Status {
    /// A file has finished loading: the first frame is there to be drawn.
    pub loaded: bool,
    pub time: Option<f64>,
    pub duration: Option<f64>,
    pub paused: Option<bool>,
    pub volume: Option<f64>,
    pub muted: Option<bool>,
    pub speed: Option<f64>,
    /// mpv is starved of data: the buffering ring.
    pub buffering: bool,
    /// The furthest point mpv has cached, in seconds.
    pub buffered: Option<f64>,
    pub audio: Vec<Track>,
    pub subtitles: Vec<Track>,
    /// Subtitle size as a multiplier of mpv's own default.
    pub subtitles_scale: f64,
    /// Subtitle timing in seconds; positive shows subtitles later.
    pub subtitles_delay: f64,
    /// How far down the screen the subtitles sit, in percent -- mpv's `sub-pos`.
    pub subtitles_position: f64,
    /// Why the last load failed, when it did. Cleared by the next load.
    pub error: String,
    pub width: u32,
    pub height: u32,
    /// The file reached its end and playback stopped there.
    pub ended: bool,
}

impl Status {
    /// Whether anything is worth pushing to the listener: the tick only speaks when a value
    /// moved, so a paused player costs no repaints.
    fn same_as(&self, other: &Self) -> bool {
        self.loaded == other.loaded
            && self.time == other.time
            && self.duration == other.duration
            && self.paused == other.paused
            && self.volume == other.volume
            && self.muted == other.muted
            && self.speed == other.speed
            && self.buffering == other.buffering
            && self.buffered == other.buffered
            && self.audio == other.audio
            && self.subtitles == other.subtitles
            && self.subtitles_scale == other.subtitles_scale
            && self.subtitles_delay == other.subtitles_delay
            && self.error == other.error
            && self.width == other.width
            && self.height == other.height
            && self.ended == other.ended
    }
}

struct Gl {
    /// Each entry is `None` when the loader could not hand back that function. A GL loader is
    /// allowed to know nothing about the functions every context has, so a null pointer here is
    /// an ordinary answer and calling one would take the whole app down.
    clear: Option<GlClear>,
    clear_color: Option<GlClearColor>,
    disable: Option<GlDisable>,
    get_integerv: Option<GlGetIntegerv>,
}

/// A loader's answer, as a callable function, or `None` when the loader gave nothing back.
fn loaded<F>(pointer: *const c_void) -> Option<F> {
    (!pointer.is_null()).then(|| {
        // SAFETY: a non-null result of a GL loader for a name has that function's own signature.
        unsafe { std::mem::transmute_copy::<*const c_void, F>(&pointer) }
    })
}

impl Gl {
    fn new(lookup: GlLookup<'_>) -> Self {
        let load = |name: &str| gl_proc(&lookup, name);
        #[cfg(debug_assertions)]
        for name in ["glClear", "glClearColor", "glDisable", "glGetIntegerv"] {
            if load(name).is_null() {
                eprintln!("[player] the GL loader does not resolve {name}");
            }
        }
        Self {
            clear: loaded(load("glClear")),
            clear_color: loaded(load("glClearColor")),
            disable: loaded(load("glDisable")),
            get_integerv: loaded(load("glGetIntegerv")),
        }
    }

    /// Blacks the whole frame before mpv draws its own rectangle into it. mpv leaves the
    /// letterbox bars alone, so without this the last frame's bars would stay on screen -- and
    /// the window's own background is transparent while the player is open, so nothing else
    /// fills them either. When the loader knows nothing about clearing, the frame is drawn over
    /// whatever was there, which is only ever a stale frame.
    fn black(&self) {
        let (Some(disable), Some(clear_color), Some(clear)) =
            (self.disable, self.clear_color, self.clear)
        else {
            return;
        };
        // SAFETY: Slint's OpenGL context is current inside the rendering notifier, and the
        // scissor is only dropped for the clear, before any item of the scene is drawn.
        unsafe {
            disable(GL_SCISSOR_TEST);
            clear_color(0.0, 0.0, 0.0, 1.0);
            clear(GL_COLOR_BUFFER_BIT);
        }
    }

    fn framebuffer(&self) -> i32 {
        let mut framebuffer = 0;
        if let Some(get_integerv) = self.get_integerv {
            // SAFETY: Slint's OpenGL context is current inside the rendering notifier.
            unsafe { get_integerv(GL_FRAMEBUFFER_BINDING, &mut framebuffer) };
        }
        framebuffer
    }
}

fn gl_proc(lookup: &GlLookup<'_>, name: &str) -> *mut c_void {
    CString::new(name).map_or(std::ptr::null_mut(), |name| lookup(&name).cast_mut())
}

struct Player {
    render: RenderContext,
    mpv: Mpv,
    gl: Gl,
    /// Where a freshly loaded stream should jump to once it is really open. A seek sent before
    /// the file opens is refused by mpv, so the resume position is held here until `FileLoaded`.
    pending_resume: Option<f64>,
}

impl Player {
    /// Must run while Slint's OpenGL context is current, which it is in `RenderingSetup`.
    fn new(lookup: GlLookup<'_>, window: Weak<MainWindow>) -> Result<Self, String> {
        let mut mpv = Mpv::with_initializer(|init| {
            init.set_property("vo", "libmpv")?;
            init.set_property("video-timing-offset", "0")?;
            init.set_property("terminal", "yes")?;
            init.set_property("cache", "yes")?;
            // On Slint's OpenGL ES context mpv's default rgba16f intermediate framebuffer drew a
            // fixed dotted pattern over the picture (RTX 3070, Windows). rgba8 does not.
            init.set_property("fbo-format", "rgba8")?;
            init.set_property("hwdec", "auto-safe")?;
            // The window is the player: mpv draws no on-screen controls of its own and takes no
            // keys, because the screen draws and binds all of them.
            init.set_property("osc", "no")?;
            init.set_property("input-default-bindings", "no")?;
            init.set_property("input-vo-keyboard", "no")?;
            // Ended files quit by themselves so the screen can move to the next Video.
            init.set_property("keep-open", "no")?;
            Ok(())
        })
        .map_err(|error| format!("could not start mpv: {error}"))?;
        mpv.disable_deprecated_events().ok();
        // Warnings and errors from mpv itself, so a stream that will not open can say why.
        // `terminal` alone never reached this process's stderr, so mpv keeps its own file beside
        // the app's log.
        mpv.set_property("terminal", "yes").ok();
        mpv.set_property("msg-level", "all=warn").ok();
        if let Some(dir) = std::env::temp_dir().to_str() {
            mpv.set_property("log-file", format!("{dir}\\stremio-lightning-mpv.log"))
                .ok();
        }

        // SAFETY: `mpv.ctx` is the valid mpv handle owned by `mpv`, which outlives this call.
        let handle = unsafe { mpv.ctx.as_mut() };
        let mut render = RenderContext::new(
            handle,
            [
                RenderParam::ApiType(RenderParamApiType::OpenGl),
                RenderParam::InitParams(OpenGLInitParams {
                    get_proc_address: gl_proc,
                    ctx: lookup,
                }),
                RenderParam::BlockForTargetTime(false),
            ],
        )
        .map_err(|error| format!("could not create the mpv render context: {error}"))?;

        render.set_update_callback(move || {
            let _ = window.upgrade_in_event_loop(|window| window.window().request_redraw());
        });

        Ok(Self {
            render,
            mpv,
            gl: Gl::new(lookup),
            pending_resume: None,
        })
    }

    /// Draws the current video frame into whatever framebuffer Slint is rendering to.
    fn draw(&self, size: PhysicalSize) {
        self.gl.black();
        let framebuffer = self.gl.framebuffer();
        let (width, height) = (size.width as i32, size.height as i32);
        if let Err(error) = self.render.render::<()>(framebuffer, width, height, true) {
            eprintln!("mpv could not draw a frame: {error}");
        }
    }

    fn number(&self, name: &str) -> Option<f64> {
        self.mpv.get_property::<f64>(name).ok()
    }

    fn flag(&self, name: &str) -> Option<bool> {
        self.mpv.get_property::<bool>(name).ok()
    }

    /// mpv's `track-list`, reassembled from the properties it publishes one track at a time.
    fn tracks(&self, kind: &str) -> Vec<Track> {
        let Ok(count) = self.mpv.get_property::<i64>("track-list/count") else {
            return Vec::new();
        };
        let mut tracks = Vec::new();
        for index in 0..count.max(0) {
            let at = |field: &str| format!("track-list/{index}/{field}");
            let Ok(track_kind) = self.mpv.get_property::<String>(&at("type")) else {
                continue;
            };
            if track_kind != kind {
                continue;
            }
            let mut title = self
                .mpv
                .get_property::<String>(&at("title"))
                .unwrap_or_default();
            let language = self
                .mpv
                .get_property::<String>(&at("lang"))
                .unwrap_or_default();
            if title.is_empty() {
                title = language.clone();
            }
            tracks.push(Track {
                id: self
                    .mpv
                    .get_property::<i64>(&at("id"))
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
                title,
                language,
                selected: self.flag(&at("selected")).unwrap_or(false),
                external: self.flag(&at("external")).unwrap_or(false),
            });
        }
        tracks
    }

    fn status(&self) -> Status {
        Status {
            loaded: false,
            time: self.number("time-pos"),
            duration: self.number("duration"),
            paused: self.flag("pause"),
            volume: self.number("volume"),
            muted: self.flag("mute"),
            speed: self.number("speed"),
            buffering: self.flag("paused-for-cache").unwrap_or(false)
                || self.flag("seeking").unwrap_or(false),
            buffered: self.number("demuxer-cache-time"),
            audio: self.tracks("audio"),
            subtitles: self.tracks("sub"),
            subtitles_scale: self.number("sub-scale").unwrap_or(1.0),
            subtitles_delay: self.number("sub-delay").unwrap_or(0.0),
            subtitles_position: self.number("sub-pos").unwrap_or(100.0),
            width: self.number("width").unwrap_or(0.0).max(0.0) as u32,
            height: self.number("height").unwrap_or(0.0).max(0.0) as u32,
            error: String::new(),
            ended: false,
        }
    }
}

/// The video scale the core remembers, as mpv's own aspect options. Fit letterboxes, Crop fills
/// the frame and cuts the edges off, Stretch distorts the picture to fill it.
pub(crate) fn apply_scale(player: &Mpv, scale: VideoScale) {
    let (keepaspect, panscan) = match scale {
        VideoScale::Contain => ("yes", "0.0"),
        VideoScale::Cover => ("yes", "1.0"),
        VideoScale::Fill => ("no", "0.0"),
    };
    let _ = player.set_property("keepaspect", keepaspect);
    let _ = player.set_property("panscan", panscan);
}

/// How the app hears about mpv: once per change, with everything the player screen draws.
type StatusListener = Rc<dyn Fn(Status)>;
type ListenerSlot = Rc<RefCell<Option<StatusListener>>>;

#[derive(Clone)]
pub(crate) struct VideoPlayer {
    player: Rc<RefCell<Option<Player>>>,
    status: Rc<RefCell<Status>>,
    listener: ListenerSlot,
    /// Whether the player screen is the thing on screen. Only then is a frame drawn, so the rest
    /// of the app never gets video over it.
    showing: Rc<Cell<bool>>,
    _events: Rc<Timer>,
}

impl VideoPlayer {
    /// Hooks mpv into the window's rendering. If the renderer has no OpenGL, playing says so.
    pub(crate) fn install(window: &MainWindow) -> Self {
        let player: Rc<RefCell<Option<Player>>> = Rc::default();
        let showing: Rc<Cell<bool>> = Rc::default();

        let installed = window.window().set_rendering_notifier({
            let player = player.clone();
            let showing = showing.clone();
            let weak = window.as_weak();
            move |state, api| match state {
                RenderingState::RenderingSetup => {
                    if let GraphicsAPI::NativeOpenGL { get_proc_address } = api {
                        match Player::new(get_proc_address, weak.clone()) {
                            Ok(created) => *player.borrow_mut() = Some(created),
                            Err(error) => eprintln!("video playback is unavailable: {error}"),
                        }
                    }
                }
                RenderingState::BeforeRendering => {
                    if let (Some(player), Some(window)) = (player.borrow().as_ref(), weak.upgrade())
                    {
                        if showing.get() {
                            player.draw(window.window().size());
                        }
                    }
                }
                // The render context frees GPU objects, so it has to go while the context is current.
                RenderingState::RenderingTeardown => drop(player.borrow_mut().take()),
                _ => {}
            }
        });
        if let Err(error) = installed {
            eprintln!("video playback is unavailable: {error}");
        }

        let status: Rc<RefCell<Status>> = Rc::default();
        let listener: ListenerSlot = Rc::default();
        let events = Timer::default();
        events.start(TimerMode::Repeated, TICK, {
            let player = player.clone();
            let status = status.clone();
            let listener = listener.clone();
            let showing = showing.clone();
            let weak = window.as_weak();
            move || {
                let mut guard = player.borrow_mut();
                let Some(current) = guard.as_mut() else {
                    return;
                };
                let previous = status.borrow().clone();
                let mut next = previous.clone();
                drain_events(current, &mut next);
                let measured = current.status();
                next.time = measured.time;
                next.duration = measured.duration;
                next.paused = measured.paused;
                next.volume = measured.volume;
                next.muted = measured.muted;
                next.speed = measured.speed;
                next.buffering = measured.buffering;
                next.buffered = measured.buffered;
                next.audio = measured.audio;
                next.subtitles = measured.subtitles;
                next.subtitles_scale = measured.subtitles_scale;
                next.subtitles_delay = measured.subtitles_delay;
                next.width = measured.width;
                next.height = measured.height;

                if previous.same_as(&next) {
                    return;
                }
                *status.borrow_mut() = next.clone();
                // The picture has to be asked for again: drawing is driven by this flag, and a
                // player sitting still would otherwise never be redrawn after a resize.
                if showing.get() {
                    if let Some(window) = weak.upgrade() {
                        window.window().request_redraw();
                    }
                }
                // The listener must not run while mpv is borrowed: it may ask the player to act.
                drop(guard);
                let heard = listener.borrow().clone();
                if let Some(listener) = heard {
                    listener(next);
                }
            }
        });

        Self {
            player,
            status,
            listener,
            showing,
            _events: Rc::new(events),
        }
    }

    /// Where playback news goes. One listener: the app's wiring.
    pub(crate) fn listen(&self, listener: impl Fn(Status) + 'static) {
        *self.listener.borrow_mut() = Some(Rc::new(listener));
    }

    /// Whether the player screen is on. Turning it off stops the picture without unloading the
    /// file, so coming back to the player resumes where it was.
    pub(crate) fn show(&self, showing: bool) {
        self.showing.set(showing);
    }

    pub(crate) fn status(&self) -> Status {
        self.status.borrow().clone()
    }

    fn with<R>(&self, act: impl FnOnce(&Mpv) -> Result<R, String>) -> Result<R, String> {
        let guard = self.player.borrow();
        let player = guard
            .as_ref()
            .ok_or("video playback is not available in this window")?;
        act(&player.mpv)
    }

    fn command(&self, name: &str, args: &[&str]) -> Result<(), String> {
        self.with(|mpv| mpv.command(name, args).map_err(describe))
    }

    /// Loads a stream and starts it. `resume_at` is where a half-watched Video left off.
    pub(crate) fn play(&self, url: &str, resume_at: f64) -> Result<(), String> {
        {
            let mut status = self.status.borrow_mut();
            *status = Status {
                volume: status.volume,
                muted: status.muted,
                speed: status.speed,
                ..Status::default()
            };
        }
        // The resume position travels *with* the load, as mpv's own `start` option, so the file
        // opens onto that second and the first frame is already the right one -- no seek, and no
        // flash of the opening shot. (Web: `initialTime` on the player, the same idea.)
        //
        // It is set on every load, `0` included, because `start` is a per-file option: left at a
        // previous value it would drag the *next* stream to the same second.
        //
        // Held as well, for the case where this mpv ignores the option: the held position is
        // applied when mpv reports `FileLoaded`, which is the first moment it accepts a seek. What
        // must never happen -- and did -- is a seek sent while the file is still opening: mpv
        // refuses those with MPV_ERROR_LOADING_FAILED ("the stream could not be opened", about a
        // stream that is fine), and the caller's retry loop then reloaded the address every 200ms,
        // each load cancelling the one before it.
        let start = if resume_at > 0.0 {
            format!("{resume_at:.3}")
        } else {
            "0".to_owned()
        };
        let _ = self.with(|mpv| mpv.set_property("start", start.as_str()).map_err(describe));
        if let Some(player) = self.player.borrow_mut().as_mut() {
            player.pending_resume = (resume_at > 0.0).then_some(resume_at);
        }
        self.command("loadfile", &[url, "replace"])?;
        // A stream always starts playing, whatever pause a previous one was left in. Not worth
        // failing the load over: at worst the first picture waits for a press.
        let _ = self.set_paused(false);
        Ok(())
    }

    pub(crate) fn stop(&self) {
        let _ = self.command("stop", &[]);
        *self.status.borrow_mut() = Status::default();
    }

    pub(crate) fn set_paused(&self, paused: bool) -> Result<(), String> {
        self.with(|mpv| {
            mpv.set_property("pause", paused)
                .map_err(|e| e.to_string())
        })
    }

    /// Moves the play head. mpv seeks to an absolute second and lands exactly there.
    pub(crate) fn seek(&self, time: f64) -> Result<(), String> {
        let seconds = format!("{time:.3}");
        self.command("seek", &[seconds.as_str(), "absolute+exact"])
    }

    pub(crate) fn set_volume(&self, volume: f64) -> Result<(), String> {
        self.with(|mpv| {
            mpv.set_property("volume", volume)
                .map_err(|e| e.to_string())
        })
    }

    pub(crate) fn set_muted(&self, muted: bool) -> Result<(), String> {
        self.with(|mpv| mpv.set_property("mute", muted).map_err(|e| e.to_string()))
    }

    pub(crate) fn set_speed(&self, speed: f64) -> Result<(), String> {
        self.with(|mpv| mpv.set_property("speed", speed).map_err(|e| e.to_string()))
    }

    /// Switches to one of the file's own audio tracks, by mpv's track id.
    pub(crate) fn select_audio(&self, id: &str) -> Result<(), String> {
        self.with(|mpv| mpv.set_property("aid", id).map_err(|e| e.to_string()))
    }

    /// Switches to one of the file's own subtitle tracks, or turns subtitles off.
    pub(crate) fn select_subtitles(&self, id: Option<&str>) -> Result<(), String> {
        self.with(|mpv| {
            mpv.set_property("sid", id.unwrap_or("no"))
                .map_err(|e| e.to_string())
        })
    }

    /// Adds a subtitle file beside the video and selects it: what a stream's own subtitles and
    /// every addon's are, since neither lives inside the file.
    pub(crate) fn add_subtitles(&self, url: &str, language: &str) -> Result<(), String> {
        let arguments = ["file", url, "select"];
        let loaded = self.command("sub-add", &arguments);
        if loaded.is_ok() && !language.is_empty() {
            let _ = self.command("set", &["sub-lang", language]);
        }
        loaded
    }

    pub(crate) fn set_subtitles_scale(&self, scale: f64) -> Result<(), String> {
        self.with(|mpv| {
            mpv.set_property("sub-scale", scale)
                .map_err(|e| e.to_string())
        })
    }

    /// Where the subtitles sit vertically, in percent of the screen height (mpv's `sub-pos`).
    pub(crate) fn set_subtitles_position(&self, position: f64) -> Result<(), String> {
        self.with(|mpv| {
            mpv.set_property("sub-pos", position)
                .map_err(|e| e.to_string())
        })
    }

    pub(crate) fn set_subtitles_delay(&self, delay: f64) -> Result<(), String> {
        self.with(|mpv| {
            mpv.set_property("sub-delay", delay)
                .map_err(|e| e.to_string())
        })
    }

    pub(crate) fn set_scale(&self, scale: VideoScale) -> Result<(), String> {
        self.with(|mpv| {
            apply_scale(mpv, scale);
            Ok(())
        })
    }

    pub(crate) fn apply_settings(&self, settings: &crate::screens::settings::PlayerConfig) {
        let _: Result<(), String> = self.with(|mpv| {
            let hwdec = if settings.hardware_decoding {
                "auto-safe"
            } else {
                "no"
            };
            let _ = mpv.set_property("hwdec", hwdec);
            let size = f64::from(settings.subtitles_size).max(1.0) / 100.0;
            let _ = mpv.set_property("sub-scale", size);
            if !settings.subtitles_language.is_empty() && settings.subtitles_language != "none" {
                let _ = mpv.set_property("slang", settings.subtitles_language.as_str());
                let _ = mpv.set_property("sub-visibility", "yes");
            } else if settings.subtitles_language == "none" {
                let _ = mpv.set_property("sub-visibility", "no");
            }
            if !settings.audio_language.is_empty() {
                let _ = mpv.set_property("alang", settings.audio_language.as_str());
            }
            if !settings.subtitles_text_color.is_empty() {
                let _ = mpv.set_property("sub-color", settings.subtitles_text_color.as_str());
            }
            if !settings.subtitles_background_color.is_empty() {
                let _ = mpv.set_property(
                    "sub-back-color",
                    settings.subtitles_background_color.as_str(),
                );
            }
            if !settings.subtitles_outline_color.is_empty() {
                let _ = mpv.set_property(
                    "sub-border-color",
                    settings.subtitles_outline_color.as_str(),
                );
            }
            Ok(())
        });
    }
}

/// mpv's own error, in words: the codes alone ("Raw(-12)") mean nothing to a person reading the
/// player's error line.
fn describe(error: libmpv2::Error) -> String {
    match error {
        libmpv2::Error::Raw(code) => match code {
            -4 => "this stream is not playable".to_owned(),
            -8 | -9 => "the player could not decode this stream".to_owned(),
            -12 => "the stream could not be opened".to_owned(),
            -13 => "the stream was not found".to_owned(),
            -16 => "there is no video output".to_owned(),
            -17 => "the player is not available".to_owned(),
            other => format!("the player reported error {other}"),
        },
        other => other.to_string(),
    }
}

/// Reads the events mpv has queued, folding the ones the screen cares about into `status`.
fn drain_events(player: &mut Player, status: &mut Status) {
    while let Some(event) = player.mpv.wait_event(0.0) {
        let Ok(event) = event else {
            continue;
        };
        match event {
            Event::StartFile => {
                status.loaded = false;
                status.ended = false;
            }
            Event::FileLoaded => {
                status.loaded = true;
                status.error.clear();
                // The tab is open now, so the resume seek can land: this is the first moment mpv
                // will take one.
                if let Some(at) = player.pending_resume.take() {
                    let seconds = format!("{at:.3}");
                    let _ = player.mpv.command("seek", &[&seconds, "absolute+exact"]);
                }
                // Autoplay, once the file is really open: a press that starts the load and a
                // release that lands on the new screen must not leave the picture paused.
                let _ = player.mpv.set_property("pause", false);
            }
            Event::LogMessage {
                prefix,
                level,
                text,
                ..
            } => {
                // mpv says why in its own log ("Failed to open ...", "HTTP error 403", "unsafe
                // TLS"). The screen has one line for the viewer; this one is for whoever has to
                // answer "why can't it play this stream".
                if matches!(level, "error" | "fatal" | "warn") {
                    eprintln!("[mpv {level}] {prefix}{text}");
                }
            }
            Event::EndFile(reason) => {
                status.loaded = false;
                if reason == mpv_end_file_reason::Error {
                    status.error = "The player could not play this stream.".to_owned();
                } else if reason == mpv_end_file_reason::Eof {
                    status.ended = true;
                }
            }
            _ => {}
        }
    }
}

/// Whether a stream still needs handing to mpv. A stream already open needs nothing done, and one
/// that has already been refused must not be loaded again: every reload cancels the attempt already
/// in flight, so retrying on a timer guarantees the failure it is reacting to.
pub(crate) fn should_open(key: &str, loaded: &str, attempted: Option<&str>) -> bool {
    key != loaded && Some(key) != attempted
}

#[cfg(test)]
mod tests {
    use super::should_open;

    #[test]
    fn a_stream_already_open_is_left_alone() {
        assert!(!should_open("a", "a", None));
    }

    #[test]
    fn a_stream_that_failed_is_not_retried() {
        // The timer fires five times a second; retrying here would reload the address and abort
        // the open that is still in flight.
        assert!(!should_open("a", "", Some("a")));
    }

    #[test]
    fn another_stream_is_opened() {
        assert!(should_open("b", "a", Some("a")));
    }

    #[test]
    fn closing_the_player_allows_the_same_stream_again() {
        // The shell clears what it attempted when the player closes, so pressing Play again is a
        // fresh request rather than a repeat of the failure.
        assert!(should_open("a", "", None));
    }
}
