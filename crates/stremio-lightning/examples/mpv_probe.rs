//! Load one URL into mpv and report what happens. A probe for "can this build play this
//! stream", with none of the window in the way.
//!
//! cargo run -p stremio-lightning --example mpv_probe -- <url> [property=value ...]

use std::time::{Duration, Instant};

fn main() {
    let mut args = std::env::args().skip(1);
    let url = args
        .next()
        .expect("usage: mpv_probe <url> [property=value ...]");
    let properties: Vec<(String, String)> = args
        .filter_map(|arg| {
            arg.split_once('=')
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
        })
        .collect();

    let app_like = properties.iter().any(|(k, _)| k == "applike");
    let properties: Vec<(String, String)> = properties
        .into_iter()
        .filter(|(k, _)| k != "applike")
        .collect();
    let mut mpv = libmpv2::Mpv::with_initializer(move |init| {
        init.set_property("vo", "null")?;
        init.set_property("ao", "null")?;
        init.set_property("terminal", "yes")?;
        init.set_property("msg-level", "all=warn,ffmpeg=debug,tls=debug")?;
        init.set_property("idle", "yes")?;
        if app_like {
            init.set_property("video-timing-offset", "0")?;
            init.set_property("cache", "yes")?;
            init.set_property("fbo-format", "rgba8")?;
            init.set_property("hwdec", "auto-safe")?;
            init.set_property("osc", "no")?;
            init.set_property("input-default-bindings", "no")?;
            init.set_property("input-vo-keyboard", "no")?;
            init.set_property("keep-open", "no")?;
        }
        for (key, value) in &properties {
            init.set_property(key.as_str(), value.as_str())?;
        }
        Ok(())
    })
    .expect("mpv");

    let started = Instant::now();
    mpv.command("loadfile", &[&url, "replace"])
        .expect("loadfile");

    loop {
        if started.elapsed() > Duration::from_secs(45) {
            println!("TIMEOUT after 45s");
            std::process::exit(3);
        }
        match mpv.wait_event(0.5) {
            Some(Ok(libmpv2::events::Event::FileLoaded)) => {
                let duration: f64 = mpv.get_property("duration").unwrap_or(0.0);
                let w: i64 = mpv.get_property("width").unwrap_or(0);
                let h: i64 = mpv.get_property("height").unwrap_or(0);
                let pos: f64 = mpv.get_property("time-pos").unwrap_or(-1.0);
                println!(
                    "LOADED in {:?} duration={duration:.1}s pos={pos:.1} {w}x{h}",
                    started.elapsed()
                );
                std::process::exit(0);
            }
            Some(Ok(libmpv2::events::Event::EndFile(reason))) => {
                println!("END_FILE {reason:?} after {:?}", started.elapsed());
                std::process::exit(1);
            }
            Some(Ok(libmpv2::events::Event::LogMessage { prefix, text, .. })) => {
                println!("[{prefix}] {text}");
            }
            _ => {}
        }
    }
}
