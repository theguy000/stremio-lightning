fn main() {
    if let Err(error) = stremio_lightning_windows::run() {
        stremio_lightning_core::logging::error("native.application", error);
        std::process::exit(1);
    }
}
