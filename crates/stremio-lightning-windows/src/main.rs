fn main() {
    if let Err(error) = stremio_lightning_windows::run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
