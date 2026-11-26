//! Pure libmpv + OpenGL render-context session (zero Slint or window imports).

use libmpv2::events::Event;
use libmpv2::render::{OpenGLInitParams, RenderContext, RenderParam, RenderParamApiType};
use libmpv2::{mpv_end_file_reason, Mpv};
use stremio_core::types::player::VideoScale;

use super::gl::{gl_proc, Gl, GlLookup};
use super::{describe, sub_add_arguments, DesiredSettings, PlayerConfig, Status, Track};
use crate::plumbing::logs::{Level, Logs, Source};

pub(crate) struct MpvSession {
    render: RenderContext,
    mpv: Mpv,
    gl: Gl,
    pending_resume: Option<f64>,
}

impl MpvSession {
    /// Creates mpv and its OpenGL `RenderContext` while the host OpenGL context is current.
    pub(crate) fn open(
        lookup: GlLookup<'_>,
        on_frame: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, String> {
        let mut mpv = Mpv::with_initializer(|init| {
            init.set_property("vo", "libmpv")?;
            init.set_property("video-timing-offset", "0")?;
            init.set_property("terminal", "yes")?;
            init.set_property("cache", "yes")?;
            // rgba8 avoids the dotted pattern rgba16f produced on OpenGL ES (RTX 3070, Windows).
            init.set_property("fbo-format", "rgba8")?;
            init.set_property("hwdec", "auto-safe")?;
            init.set_property("osc", "no")?;
            init.set_property("input-default-bindings", "no")?;
            init.set_property("input-vo-keyboard", "no")?;
            init.set_property("keep-open", "no")?;
            Ok(())
        })
        .map_err(|error| format!("could not start mpv: {error}"))?;
        mpv.disable_deprecated_events().ok();
        mpv.set_property("terminal", "yes").ok();
        mpv.set_property("msg-level", "all=warn").ok();
        let log_path = std::env::temp_dir().join("stremio-lightning-mpv.log");
        if let Some(path) = log_path.to_str() {
            mpv.set_property("log-file", path).ok();
        }

        // SAFETY: `mpv.ctx` is the valid handle owned by `mpv`, which outlives `render`.
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
        render.set_update_callback(on_frame);

        Ok(Self {
            render,
            mpv,
            gl: Gl::new(lookup),
            pending_resume: None,
        })
    }

    pub(crate) fn draw(&self, width: i32, height: i32) {
        self.gl.black();
        let framebuffer = self.gl.framebuffer();
        if let Err(error) = self.render.render::<()>(framebuffer, width, height, true) {
            eprintln!("mpv could not draw a frame: {error}");
        }
    }

    pub(crate) fn load_stream(&mut self, url: &str, resume_at: f64) -> Result<(), String> {
        let start = if resume_at > 0.0 {
            format!("{resume_at:.3}")
        } else {
            "0".to_owned()
        };
        let _ = self.mpv.set_property("start", start.as_str());
        self.pending_resume = (resume_at > 0.0).then_some(resume_at);
        self.mpv
            .command("loadfile", &[url, "replace"])
            .map_err(describe)?;
        let _ = self.mpv.set_property("pause", false);
        Ok(())
    }

    pub(crate) fn stop(&self) {
        let _ = self.mpv.command("stop", &[]);
    }

    pub(crate) fn set_prop<V: libmpv2::SetData>(&self, name: &str, value: V) -> Result<(), String> {
        self.mpv.set_property(name, value).map_err(describe)
    }

    pub(crate) fn command(&self, name: &str, args: &[&str]) -> Result<(), String> {
        self.mpv.command(name, args).map_err(describe)
    }

    pub(crate) fn add_subtitles(
        &self,
        url: &str,
        title: &str,
        language: &str,
    ) -> Result<(), String> {
        self.command("sub-add", &sub_add_arguments(url, title, language))
    }

    pub(crate) fn apply_desired(&self, config: Option<&PlayerConfig>, desired: DesiredSettings) {
        if let Some(cfg) = config {
            self.apply_config(cfg);
        }
        self.apply_scale(desired.scale);
        let _ = desired.volume.map(|v| self.set_prop("volume", v));
        let _ = desired.muted.map(|m| self.set_prop("mute", m));
        let _ = desired.speed.map(|s| self.set_prop("speed", s));
    }

    pub(crate) fn apply_scale(&self, scale: VideoScale) {
        let (keepaspect, panscan) = match scale {
            VideoScale::Contain => ("yes", "0.0"),
            VideoScale::Cover => ("yes", "1.0"),
            VideoScale::Fill => ("no", "0.0"),
        };
        let _ = self.mpv.set_property("keepaspect", keepaspect);
        let _ = self.mpv.set_property("panscan", panscan);
    }

    pub(crate) fn apply_config(&self, settings: &PlayerConfig) {
        let hwdec = if settings.hardware_decoding {
            "auto-safe"
        } else {
            "no"
        };
        let _ = self.mpv.set_property("hwdec", hwdec);
        let size = f64::from(settings.subtitles_size).max(1.0) / 100.0;
        let _ = self.mpv.set_property("sub-scale", size);
        if !settings.subtitles_language.is_empty() && settings.subtitles_language != "none" {
            let _ = self
                .mpv
                .set_property("slang", settings.subtitles_language.as_str());
            let _ = self.mpv.set_property("sub-visibility", "yes");
        } else if settings.subtitles_language == "none" {
            let _ = self.mpv.set_property("sub-visibility", "no");
        }
        for (prop, val) in [
            ("alang", &settings.audio_language),
            ("sub-color", &settings.subtitles_text_color),
            ("sub-back-color", &settings.subtitles_background_color),
            ("sub-border-color", &settings.subtitles_outline_color),
        ] {
            if !val.is_empty() {
                let _ = self.mpv.set_property(prop, val.as_str());
            }
        }
    }

    /// Drains queued mpv events, reads live properties wholesale, and returns `Some(next)` iff any
    /// field of `Status` changed from `previous`.
    pub(crate) fn poll_status(&mut self, previous: &Status, logs: &Logs) -> Option<Status> {
        let mut next = previous.clone();
        self.drain_events(&mut next, logs);
        let merged = next.with_measured(self.read_status());
        (&merged != previous).then_some(merged)
    }

    fn number(&self, name: &str) -> Option<f64> {
        self.mpv.get_property::<f64>(name).ok()
    }

    fn flag(&self, name: &str) -> Option<bool> {
        self.mpv.get_property::<bool>(name).ok()
    }

    fn tracks(&self, kind: &str) -> Vec<Track> {
        let Ok(count) = self.mpv.get_property::<i64>("track-list/count") else {
            return Vec::new();
        };
        let mut tracks = Vec::new();
        for index in 0..count.max(0) {
            let at = |field: &str| format!("track-list/{index}/{field}");
            if self.mpv.get_property::<String>(&at("type")).ok().as_deref() != Some(kind) {
                continue;
            }
            tracks.push(Track {
                id: self
                    .mpv
                    .get_property::<i64>(&at("id"))
                    .map(|id| id.to_string())
                    .unwrap_or_default(),
                title: self
                    .mpv
                    .get_property::<String>(&at("title"))
                    .unwrap_or_default(),
                language: self
                    .mpv
                    .get_property::<String>(&at("lang"))
                    .unwrap_or_default(),
                selected: self.flag(&at("selected")).unwrap_or(false),
                external: self.flag(&at("external")).unwrap_or(false),
            });
        }
        tracks
    }

    fn read_status(&self) -> Status {
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

    fn drain_events(&mut self, status: &mut Status, logs: &Logs) {
        while let Some(event) = self.mpv.wait_event(0.0) {
            let Ok(event) = event else { continue };
            match event {
                Event::StartFile => {
                    status.loaded = false;
                    status.ended = false;
                }
                Event::FileLoaded => {
                    status.loaded = true;
                    status.error.clear();
                    if let Some(at) = self.pending_resume.take() {
                        let seconds = format!("{at:.3}");
                        let _ = self.mpv.command("seek", &[&seconds, "absolute+exact"]);
                    }
                    let _ = self.mpv.set_property("pause", false);
                }
                Event::LogMessage {
                    prefix,
                    level,
                    text,
                    ..
                } => {
                    if matches!(level, "error" | "fatal" | "warn") {
                        let line = format!("{prefix}{text}");
                        eprintln!("[mpv {level}] {line}");
                        let severity = if matches!(level, "error" | "fatal") {
                            Level::Error
                        } else {
                            Level::Warn
                        };
                        logs.record(
                            Source::App,
                            "Player",
                            severity,
                            format!("mpv {level}: {line}"),
                        );
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
}
