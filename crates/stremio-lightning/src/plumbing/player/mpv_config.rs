//! libmpv initialization, custom `mpv.conf` loading, and live `PlayerConfig` property application.

use std::path::Path;

use libmpv2::Mpv;

use super::PlayerConfig;
use crate::plumbing::logs::{Level, Logs, Source};

/// Initializes libmpv with built-in defaults, optional user `mpv.conf`, and mandatory embedded
/// invariants (`vo=libmpv`, `osc=no`, `keep-open=no`, `loop-file=no`, no mpv keyboard bindings).
pub(super) fn create_mpv(mpv_config_path: Option<&str>, logs: &Logs) -> Result<Mpv, String> {
    let custom_path = mpv_config_path.map(str::trim).filter(|p| !p.is_empty());
    let mut config_warning: Option<String> = None;

    let mpv = Mpv::with_initializer(|init| {
        init.set_property("video-timing-offset", "0")?;
        init.set_property("terminal", "yes")?;
        init.set_property("cache", "yes")?;
        // rgba8 avoids the dotted pattern rgba16f produced on OpenGL ES (RTX 3070, Windows).
        init.set_property("fbo-format", "rgba8")?;
        init.set_property("hwdec", "auto-safe")?;

        if let Some(path) = custom_path {
            if !Path::new(path).is_file() {
                config_warning = Some(format!("custom mpv config not found: {path}"));
            } else if let Err(err) = init.load_config(path) {
                config_warning = Some(format!("could not load custom mpv config '{path}': {err}"));
            }
        }

        // Re-enforce embedded libmpv invariants after user config so playback and controls cannot break.
        init.set_property("vo", "libmpv")?;
        init.set_property("osc", "no")?;
        init.set_property("input-default-bindings", "no")?;
        init.set_property("input-vo-keyboard", "no")?;
        init.set_property("keep-open", "no")?;
        init.set_property("loop-file", "no")?;
        Ok(())
    })
    .map_err(|error| format!("could not start mpv: {error}"))?;

    if let Some(warn) = config_warning {
        eprintln!("[mpv warn] {warn}");
        logs.record(Source::App, "Player", Level::Warn, warn);
    }
    Ok(mpv)
}

pub(super) fn apply_config(mpv: &Mpv, settings: &PlayerConfig) {
    let pick = |on: bool, yes: &'static str, no: &'static str| if on { yes } else { no };
    let _ = mpv.set_property(
        "sub-scale",
        f64::from(settings.subtitles_size).max(1.0) / 100.0,
    );
    let _ = mpv.set_property(
        "sub-pos",
        i64::from(100u32.saturating_sub(settings.subtitles_offset.min(100))),
    );
    let _ = mpv.set_property("sub-bold", settings.subtitles_bold);
    for (prop, val) in [
        ("hwdec", pick(settings.hardware_decoding, "auto-safe", "no")),
        (
            "sub-ass-override",
            pick(settings.ass_subtitles_styling, "force", "no"),
        ),
        (
            "audio-channels",
            pick(settings.surround_sound, "auto-safe", "stereo"),
        ),
        (
            "audio-spdif",
            pick(settings.audio_passthrough, "ac3,eac3,dts,dts-hd,truehd", ""),
        ),
    ] {
        let _ = mpv.set_property(prop, val);
    }
    if !settings.subtitles_language.is_empty() && settings.subtitles_language != "none" {
        let _ = mpv.set_property("slang", settings.subtitles_language.as_str());
        let _ = mpv.set_property("sub-visibility", "yes");
    } else if settings.subtitles_language == "none" {
        let _ = mpv.set_property("sub-visibility", "no");
    }
    for (prop, val) in [
        ("alang", &settings.audio_language),
        ("sub-color", &settings.subtitles_text_color),
        ("sub-back-color", &settings.subtitles_background_color),
        ("sub-border-color", &settings.subtitles_outline_color),
    ] {
        if !val.is_empty() {
            let _ = mpv.set_property(prop, val.as_str());
        }
    }
}
