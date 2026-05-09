use stremio_lightning_macos::app::{parse_args, run};

fn main() {
    if let Err(error) = run(parse_args(std::env::args())) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
