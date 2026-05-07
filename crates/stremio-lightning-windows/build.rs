fn main() {
    #[cfg(windows)]
    {
        println!("cargo:rustc-link-search=native=mpv-dev");
        println!("cargo:rustc-link-lib=dylib=mpv");
        println!("cargo:rustc-link-lib=dylib=delayimp");
        println!("cargo:rustc-link-arg=/DELAYLOAD:libmpv-2.dll");
        println!("cargo:rerun-if-changed=mpv-dev/mpv.lib");
    }
}
