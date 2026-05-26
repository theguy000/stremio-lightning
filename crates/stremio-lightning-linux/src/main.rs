use stremio_lightning_linux::app::{parse_args, run};

fn main() {
    // Optimize memory allocator behaviour: bypass slice caching and trim aggressively.
    std::env::set_var("G_SLICE", "always-malloc");
    std::env::set_var("MALLOC_TRIM_THRESHOLD_", "131072");

    if let Err(error) = run(parse_args(std::env::args())) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
