fn main() {
    slint_build::compile("ui/main.slint").expect("failed to compile Slint UI");

    println!("cargo:rerun-if-env-changed=MPV_DEV_DIR");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        link_windows_mpv();
    }
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
