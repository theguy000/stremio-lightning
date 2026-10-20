//! Video playback. libmpv draws every frame into Slint's own OpenGL context just before Slint
//! draws the window's items on top, so the player screen only has to leave the video area empty.

use std::cell::RefCell;
use std::ffi::{c_void, CStr, CString};
use std::rc::Rc;
use std::time::Duration;

use libmpv2::events::Event;
use libmpv2::render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType};
use libmpv2::{mpv_end_file_reason, Mpv};
use slint::{ComponentHandle, GraphicsAPI, PhysicalSize, RenderingState, Timer, TimerMode, Weak};

use crate::MainWindow;

const GL_FRAMEBUFFER_BINDING: u32 = 0x8CA6;
const EVENT_POLL: Duration = Duration::from_millis(200);

type GlLookup<'a> = &'a dyn Fn(&CStr) -> *const c_void;
type GlGetIntegerv = unsafe extern "system" fn(u32, *mut i32);

struct Player {
    render: RenderContext,
    mpv: Mpv,
    get_integerv: Option<GlGetIntegerv>,
}

fn gl_proc(lookup: &GlLookup<'_>, name: &str) -> *mut c_void {
    CString::new(name).map_or(std::ptr::null_mut(), |name| lookup(&name).cast_mut())
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
            Ok(())
        })
        .map_err(|error| format!("could not start mpv: {error}"))?;
        mpv.disable_deprecated_events().ok();

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

        let get_integerv = lookup(c"glGetIntegerv");
        Ok(Self {
            render,
            mpv,
            // SAFETY: a non-null result of the loader for `glGetIntegerv` has this signature.
            get_integerv: (!get_integerv.is_null()).then(|| unsafe {
                std::mem::transmute::<*const c_void, GlGetIntegerv>(get_integerv)
            }),
        })
    }

    /// Draws the current video frame into whatever framebuffer Slint is rendering to.
    fn draw(&self, size: PhysicalSize) {
        let mut framebuffer = 0;
        if let Some(get_integerv) = self.get_integerv {
            // SAFETY: Slint's OpenGL context is current inside the rendering notifier.
            unsafe { get_integerv(GL_FRAMEBUFFER_BINDING, &mut framebuffer) };
        }
        let (width, height) = (size.width as i32, size.height as i32);
        if let Err(error) = self.render.render::<()>(framebuffer, width, height, true) {
            eprintln!("mpv could not draw a frame: {error}");
        }
    }
}

#[derive(Clone)]
pub struct VideoPlayer {
    player: Rc<RefCell<Option<Player>>>,
    _events: Rc<Timer>,
}

impl VideoPlayer {
    /// Hooks mpv into the window's rendering. If the renderer has no OpenGL, playing says so.
    pub fn install(window: &MainWindow) -> Self {
        let player: Rc<RefCell<Option<Player>>> = Rc::default();

        let installed = window.window().set_rendering_notifier({
            let player = player.clone();
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
                        if window.get_playing() {
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

        let events = Timer::default();
        events.start(TimerMode::Repeated, EVENT_POLL, {
            let player = player.clone();
            let weak = window.as_weak();
            move || {
                let mut guard = player.borrow_mut();
                let Some(current) = guard.as_mut() else {
                    return;
                };
                while let Some(event) = current.mpv.wait_event(0.0) {
                    let Ok(Event::EndFile(reason)) = event else {
                        continue;
                    };
                    let failed = reason == mpv_end_file_reason::Error;
                    if failed || reason == mpv_end_file_reason::Eof {
                        if let Some(window) = weak.upgrade() {
                            window.set_playing(false);
                            if failed {
                                window.set_play_message(
                                    "The player could not play this stream.".into(),
                                );
                            }
                        }
                    }
                }
            }
        });

        Self {
            player,
            _events: Rc::new(events),
        }
    }

    pub fn play(&self, url: &str) -> Result<(), String> {
        let guard = self.player.borrow();
        let player = guard
            .as_ref()
            .ok_or("video playback is not available in this window")?;
        player
            .mpv
            .command("loadfile", &[url, "replace"])
            .map_err(|error| error.to_string())
    }

    pub fn stop(&self) {
        if let Some(player) = self.player.borrow().as_ref() {
            let _ = player.mpv.command("stop", &[]);
        }
    }

    pub fn apply_settings(&self, settings: &crate::settings::PlayerConfig) {
        if let Some(player) = self.player.borrow().as_ref() {
            let hwdec = if settings.hardware_decoding {
                "auto-safe"
            } else {
                "no"
            };
            let _ = player.mpv.set_property("hwdec", hwdec);
            let _ = player
                .mpv
                .set_property("sub-scale", f64::from(settings.subtitles_size) / 100.0);
            if !settings.subtitles_language.is_empty() && settings.subtitles_language != "none" {
                let _ = player
                    .mpv
                    .set_property("slang", settings.subtitles_language.as_str());
                let _ = player.mpv.set_property("sub-visibility", "yes");
            } else if settings.subtitles_language == "none" {
                let _ = player.mpv.set_property("sub-visibility", "no");
            }
            if !settings.audio_language.is_empty() {
                let _ = player
                    .mpv
                    .set_property("alang", settings.audio_language.as_str());
            }
            if !settings.subtitles_text_color.is_empty() {
                let _ = player
                    .mpv
                    .set_property("sub-color", settings.subtitles_text_color.as_str());
            }
            if !settings.subtitles_background_color.is_empty() {
                let _ = player.mpv.set_property(
                    "sub-back-color",
                    settings.subtitles_background_color.as_str(),
                );
            }
            if !settings.subtitles_outline_color.is_empty() {
                let _ = player.mpv.set_property(
                    "sub-border-color",
                    settings.subtitles_outline_color.as_str(),
                );
            }
        }
    }
}
