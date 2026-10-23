//! What the Board shows above its addon rows.
//!
//! The Continue Watching row is the one part of the Board that comes from the account rather
//! than from the addons, so it is built here from a library the test makes itself: no account,
//! no network, no window.

use std::sync::Once;

use stremio_core::models::catalog_with_filters::CatalogWithFilters;
use stremio_core::models::continue_watching_preview::ContinueWatchingPreview;
use stremio_core::models::ctx::Ctx;
use stremio_core::models::installed_addons_with_filters::InstalledAddonsWithFilters;
use stremio_core::models::library_with_filters::{LibraryWithFilters, NotRemovedFilter};
use stremio_core::types::events::DismissedEventsBucket;
use stremio_core::types::library::{LibraryBucket, LibraryItem};
use stremio_core::types::notifications::NotificationsBucket;
use stremio_core::types::profile::Profile;
use stremio_core::types::resource::MetaItemPreview;
use stremio_core::types::search_history::SearchHistoryBucket;
use stremio_core::types::server_urls::ServerUrlsBucket;
use stremio_core::types::streams::StreamsBucket;
use stremio_lightning::{app, screens};

/// `AppEnv::init` may only run once per process, so every test in this binary shares it.
fn init_env() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let dir =
            std::env::temp_dir().join(format!("stremio-lightning-board-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temp folder");
        let tokio = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a tokio runtime");
        stremio_lightning_env::AppEnv::init(dir, tokio.handle().clone()).expect("an environment");
    });
}

/// The app's model over a library the test hands it, built exactly as `app::build::runtime`
/// builds it around whatever the stored library came back with.
fn model_with_library(items: Vec<LibraryItem>) -> app::AppModel {
    init_env();
    let profile = Profile::default();
    let uid = profile.uid();
    let library = LibraryBucket::new(uid.clone(), items);
    let notifications = NotificationsBucket::new::<stremio_lightning_env::AppEnv>(uid.clone(), vec![]);
    let (continue_watching_preview, _) = ContinueWatchingPreview::new(&library, &notifications);
    let (library_with_filters, _) =
        LibraryWithFilters::<NotRemovedFilter>::new(&library, &notifications);
    let (discover, _) = CatalogWithFilters::<MetaItemPreview>::new(&profile);
    let (installed_addons, _) = InstalledAddonsWithFilters::new(&profile);
    app::AppModel {
        ctx: Ctx::new(
            profile,
            library,
            StreamsBucket::new(uid.clone()),
            ServerUrlsBucket::new::<stremio_lightning_env::AppEnv>(uid.clone()),
            notifications,
            SearchHistoryBucket::new(uid.clone()),
            DismissedEventsBucket::new(uid),
        ),
        continue_watching_preview,
        board: Default::default(),
        discover,
        library: library_with_filters,
        installed_addons,
        meta_details: Default::default(),
        player: Default::default(),
    }
}

/// A library item the way the API sends one, with the one watch position a title sits on.
fn library_item(id: &str, name: &str, shape: &str, modified: &str, offset: u64, duration: u64) -> LibraryItem {
    serde_json::from_value(serde_json::json!({
        "_id": id,
        "name": name,
        "type": "movie",
        "poster": format!("https://images.example.com/{id}.jpg"),
        "posterShape": shape,
        "removed": false,
        "temp": false,
        "_mtime": modified,
        "state": {
            "timeWatched": 0,
            "timeOffset": offset,
            "overallTimeWatched": 0,
            "timesWatched": 0,
            "flaggedWatched": 0,
            "duration": duration
        }
    }))
    .expect("a library item")
}

#[test]
fn the_board_opens_with_what_the_account_left_unfinished() {
    let model = model_with_library(vec![
        // Newest first: ten minutes into a two-hour film.
        library_item("tt1", "Half Watched", "landscape", "2026-10-03T00:00:00Z", 30_000, 120_000),
        // Older: one of four hours behind it.
        library_item("tt2", "Just Started", "poster", "2026-10-02T00:00:00Z", 60_000, 240_000),
        // Never played, so it is in the library but not in Continue Watching.
        library_item("tt3", "Not Started", "poster", "2026-10-01T00:00:00Z", 0, 120_000),
    ]);

    let (_, rows) = screens::board::data(&model);
    let first = rows.first().expect("a continued title makes a row");

    assert_eq!(first.title, "Continue Watching");
    // Web: a Landscape poster in the row is drawn as a Square one.
    assert_eq!(first.shape, "square");

    let ids = first.cards.iter().map(|card| card.id.as_str()).collect::<Vec<_>>();
    assert_eq!(ids, ["tt1", "tt2"], "newest first, and only what was started");

    let half = &first.cards[0];
    assert_eq!(half.title, "Half Watched");
    assert_eq!(half.kind, "movie");
    assert_eq!(
        half.poster.as_deref(),
        Some("https://images.example.com/tt1.jpg")
    );
    assert_eq!(half.progress, 25.0, "ten minutes of a two-hour film");
    assert_eq!(first.cards[1].progress, 25.0, "one hour of four");
}

#[test]
fn a_board_with_nothing_started_has_no_continue_watching_row() {
    let model = model_with_library(vec![]);
    let (status, rows) = screens::board::data(&model);

    assert!(rows.is_empty(), "no library, so nothing to continue");
    assert!(status.contains("0 catalogs ready"), "{status}");

    // A library that holds titles nobody has played is not a Continue Watching row either.
    let model = model_with_library(vec![library_item(
        "tt1",
        "Not Started",
        "poster",
        "2026-10-03T00:00:00Z",
        0,
        120_000,
    )]);
    let (_, rows) = screens::board::data(&model);
    assert!(rows.is_empty());
}
