fn main() {
    // Debug info in the generated code is what lets `i-slint-backend-testing` find an element by
    // its id or its accessible label, which is how the UI tests drive the window. It is on for
    // dev and test builds and off for release, where it would only be weight.
    let config = slint_build::CompilerConfiguration::new().with_debug_info(cfg!(debug_assertions));
    slint_build::compile_with_config("ui/main.slint", config).expect("failed to compile Slint UI");

    // Watching the whole UI folder, not just `main.slint`: a build script that names any
    // `rerun-if-changed` path is no longer rerun for every file in the crate, and a screen that is
    // edited without touching `main.slint` would otherwise keep compiling its last version.
    println!("cargo:rerun-if-changed=ui");
    // The window's icon: `shell::window_icon` embeds the project's logo, and a logo redrawn
    // without touching any Rust file would otherwise keep the last build's copy.
    println!("cargo:rerun-if-changed=../../assets/icons/128x128.png");
    println!("cargo:rerun-if-env-changed=MPV_DEV_DIR");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        link_windows_mpv();
        embed_windows_icon();
    }
}

/// Embeds the executable's icon, so Explorer, the Start menu and a taskbar button that falls back
/// to the executable all show the app's mark rather than a blank page. The source is the same
/// multi-size `src/favicon.ico` the WebView2 shell embeds.
fn embed_windows_icon() {
    println!("cargo:rerun-if-changed=stremio-lightning.rc");
    println!("cargo:rerun-if-changed=../../src/favicon.ico");
    embed_resource::compile("stremio-lightning.rc", embed_resource::NONE)
        .manifest_required()
        .expect("failed to embed the executable icon");
}

/// On Windows libmpv is linked from the import library that `cargo xtask setup-windows`
/// downloads for the Windows shell, and its DLL is copied next to the executable.
fn link_windows_mpv() {
    let manifest = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let shell = manifest.join("../stremio-lightning-windows");
    let dev = std::env::var_os("MPV_DEV_DIR").map_or_else(|| shell.join("mpv-dev"), Into::into);
    let lib = dev.join("mpv.lib");
    assert!(
        lib.exists(),
        "mpv.lib not found in {}\nRun `cargo xtask setup-windows`, or set MPV_DEV_DIR.",
        dev.display()
    );
    println!("cargo:rustc-link-search=native={}", dev.display());
    println!("cargo:rerun-if-changed={}", lib.display());

    let dll = shell.join("resources/libmpv-2.dll");
    println!("cargo:rerun-if-changed={}", dll.display());
    let out_dir = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let profile_dir = out_dir.ancestors().nth(3).unwrap();
    if let Err(error) = std::fs::copy(&dll, profile_dir.join("libmpv-2.dll")) {
        println!(
            "cargo:warning=could not copy {} beside the executable: {error}",
            dll.display()
        );
    }
}
