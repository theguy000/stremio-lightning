fn main() {
    println!("cargo:rerun-if-changed=mpv-dev/mpv.lib");
    println!("cargo:rerun-if-changed=resources/libmpv-2.dll");
    println!("cargo:rerun-if-changed=windows-shell.rc");
    println!("cargo:rerun-if-changed=windows-shell.exe.manifest");
    println!("cargo:rerun-if-changed=../../src-tauri/icons/icon.ico");

    if is_windows_target() {
        configure_windows_build();
    }
}

fn configure_windows_build() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("missing CARGO_MANIFEST_DIR");
    let mpv_dev_dir = std::path::Path::new(&manifest_dir).join("mpv-dev");
    println!("cargo:rustc-link-search=native={}", mpv_dev_dir.display());
    println!("cargo:rustc-link-lib=dylib=mpv");
    println!("cargo:rustc-link-lib=dylib=delayimp");
    println!("cargo:rustc-link-arg=/DELAYLOAD:libmpv-2.dll");

    copy_libmpv_to_profile_dir();
    embed_windows_resources();
}

fn copy_libmpv_to_profile_dir() {
    let source = std::path::Path::new("resources/libmpv-2.dll");
    if !source.exists() {
        println!(
            "cargo:warning=resources/libmpv-2.dll is missing; run npm run setup:windows-shell"
        );
        return;
    }

    let Some(profile_dir) = target_profile_dir() else {
        println!("cargo:warning=Could not determine target profile directory for libmpv copy");
        return;
    };

    if let Err(error) = std::fs::copy(source, profile_dir.join("libmpv-2.dll")) {
        println!("cargo:warning=Could not copy libmpv-2.dll beside executable: {error}");
    }
}

fn target_profile_dir() -> Option<std::path::PathBuf> {
    let out_dir = std::env::var_os("OUT_DIR")?;
    profile_dir_from_out_dir(std::path::Path::new(&out_dir))
}

fn profile_dir_from_out_dir(out_dir: &std::path::Path) -> Option<std::path::PathBuf> {
    out_dir.ancestors().nth(3).map(std::path::Path::to_path_buf)
}

fn is_windows_target() -> bool {
    std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
}

#[cfg(windows)]
fn embed_windows_resources() {
    embed_resource::compile("windows-shell.rc", embed_resource::NONE)
        .manifest_required()
        .expect("failed to embed Windows shell resources");
}

#[cfg(not(windows))]
fn embed_windows_resources() {
    println!("cargo:warning=Skipping Windows resource embedding during cross-compilation");
}
