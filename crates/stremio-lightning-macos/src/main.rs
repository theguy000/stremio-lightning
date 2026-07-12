use stremio_lightning_macos::app::{parse_args, run};

fn main() {
    if let Err(error) = run(parse_args(std::env::args())) {
        stremio_lightning_core::logging::error("native.application", error);
        std::process::exit(1);
    }
}
