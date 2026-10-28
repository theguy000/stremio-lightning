//! Whether `sub-add` takes the shell's own argument shape, against a real loaded file.
//!
//! cargo run -p stremio-lightning --example sub_probe2 -- <first.srt> <second.srt>

use libmpv2::events::Event;

fn main() {
    let mut args = std::env::args().skip(1);
    let first = std::fs::canonicalize(args.next().expect("first subtitle")).expect("first");
    let second = args.next().expect("second subtitle");
    // A path on disk or a URL: `sub-add` takes either.
    let second = std::fs::canonicalize(&second)
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or(second);

    let mut mpv = libmpv2::Mpv::with_initializer(|init| {
        init.set_property("vo", "null")?;
        init.set_property("ao", "null")?;
        init.set_property("terminal", "yes")?;
        init.set_property("msg-level", "all=info")?;
        init.set_property("idle", "yes")?;
        init.set_property("pause", "yes")?;
        Ok(())
    })
    .expect("mpv");
    mpv.disable_deprecated_events().ok();

    mpv.command("loadfile", &[first.to_str().unwrap(), "replace"]).expect("loadfile");
    let mut loaded = false;
    let mut waited = 0;
    while !loaded && waited < 60 {
        if let Some(Ok(event)) = mpv.wait_event(0.5) {
            if matches!(event, Event::FileLoaded) {
                loaded = true;
            }
        }
        waited += 1;
    }
    println!("file loaded: {loaded}");
    let count: i64 = mpv.get_property("track-list/count").unwrap_or(-1);
    println!("tracks before: {count}");
    let plain = mpv.command("sub-add", &[&second, "select", "Deutsch", "ger"]);
    println!("sub-add <path> select <title> <lang> -> {plain:?}");
    std::thread::sleep(std::time::Duration::from_millis(1200));
    println!("sid now: {:?}", mpv.get_property::<String>("sid"));
    let count: i64 = mpv.get_property("track-list/count").unwrap_or(-1);
    for index in 0..count.max(0) {
        let title: String = mpv.get_property(&format!("track-list/{index}/title")).unwrap_or_default();
        let lang: String = mpv.get_property(&format!("track-list/{index}/lang")).unwrap_or_default();
        let kind: String = mpv.get_property(&format!("track-list/{index}/type")).unwrap_or_default();
        let selected: bool = mpv.get_property(&format!("track-list/{index}/selected")).unwrap_or(false);
        println!("  {index}: {kind} title={title:?} lang={lang:?} selected={selected}");
    }
}
