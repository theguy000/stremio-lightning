use stremio_lightning_macos::app::{parse_args, run};

fn main() {
    if let Err(error) = parse_args(std::env::args()).and_then(run) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
