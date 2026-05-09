use std::path::PathBuf;

fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if !target.contains("apple-darwin") {
        return;
    }

    println!("cargo:rerun-if-env-changed=MPV_DIR");
    println!("cargo:rerun-if-env-changed=STREMIO_LIGHTNING_MPV_DIR");

    for root in mpv_roots() {
        let lib = root.join("lib");
        if lib.join("libmpv.dylib").exists() || lib.join("libmpv.2.dylib").exists() {
            println!("cargo:rustc-link-search=native={}", lib.display());
            println!("cargo:rustc-link-lib=dylib=mpv");
            println!(
                "cargo:rustc-env=STREMIO_LIGHTNING_MACOS_MPV_ROOT={}",
                root.display()
            );
            return;
        }
    }

    println!("cargo:rustc-link-lib=dylib=mpv");
}

fn mpv_roots() -> Vec<PathBuf> {
    ["MPV_DIR", "STREMIO_LIGHTNING_MPV_DIR"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .chain([
            PathBuf::from("mpv-dev"),
            PathBuf::from("/opt/homebrew/opt/mpv"),
            PathBuf::from("/usr/local/opt/mpv"),
        ])
        .collect()
}
