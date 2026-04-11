fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rustc-link-search=native=mpv-dev");
        println!("cargo:rustc-link-lib=dylib=mpv");
        // Delay-load libmpv-2.dll so it's loaded on first use, not at process startup.
        // This allows us to add the resource directory to the DLL search path before
        // the OS loader tries to find it, fixing the "DLL not found" error on install.
        println!("cargo:rustc-link-lib=dylib=delayimp");
        println!("cargo:rustc-link-arg=/DELAYLOAD:libmpv-2.dll");
        println!("cargo:rerun-if-changed=mpv-dev/mpv.lib");
    }

    tauri_build::build()
}
