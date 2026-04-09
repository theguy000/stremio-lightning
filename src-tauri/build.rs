fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rustc-link-search=native=mpv-dev");
        println!("cargo:rustc-link-lib=dylib=mpv");
        println!("cargo:rerun-if-changed=mpv-dev/mpv.lib");
    }

    tauri_build::build()
}
