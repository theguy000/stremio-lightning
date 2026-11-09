//! Asks the skip source for one episode and prints what came back, so the chain can be seen
//! working without a window:
//!
//! cargo run -p stremio-lightning --example skip_probe -- tt2560140 1 1 1440

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let meta = args.first().cloned().unwrap_or_else(|| "tt2560140".to_owned());
    let season: u32 = args.get(1).and_then(|value| value.parse().ok()).unwrap_or(1);
    let episode: u32 = args.get(2).and_then(|value| value.parse().ok()).unwrap_or(1);
    let duration: f64 = args.get(3).and_then(|value| value.parse().ok()).unwrap_or(1440.0);

    let nominal = args.get(4).and_then(|value| value.parse().ok());
    let found = stremio_lightning::plumbing::skip::segments(&meta, season, episode, duration, nominal).await;
    println!("{meta} season {season} episode {episode} ({duration}s): {found:?}");
}
