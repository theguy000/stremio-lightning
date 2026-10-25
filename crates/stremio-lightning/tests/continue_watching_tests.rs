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
use stremio_core::types::streams::{StreamsBucket, StreamsItemKey};
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
    let notifications =
        NotificationsBucket::new::<stremio_lightning_env::AppEnv>(uid.clone(), vec![]);
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
fn library_item(
    id: &str,
    name: &str,
    shape: &str,
    modified: &str,
    offset: u64,
    duration: u64,
) -> LibraryItem {
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
        library_item(
            "tt1",
            "Half Watched",
            "landscape",
            "2026-10-03T00:00:00Z",
            30_000,
            120_000,
        ),
        // Older: one of four hours behind it.
        library_item(
            "tt2",
            "Just Started",
            "poster",
            "2026-10-02T00:00:00Z",
            60_000,
            240_000,
        ),
        // Never played, so it is in the library but not in Continue Watching.
        library_item(
            "tt3",
            "Not Started",
            "poster",
            "2026-10-01T00:00:00Z",
            0,
            120_000,
        ),
    ]);

    let (_, rows) = screens::board::data(&model);
    let first = rows.first().expect("a continued title makes a row");

    assert_eq!(first.title, "Continue Watching");
    // Web: a Landscape poster in the row is drawn as a Square one.
    assert_eq!(first.shape, "square");

    let ids = first
        .cards
        .iter()
        .map(|card| card.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        ["tt1", "tt2"],
        "newest first, and only what was started"
    );

    let half = &first.cards[0];
    assert_eq!(half.title, "Half Watched");
    assert_eq!(half.kind, "movie");
    assert_eq!(
        half.poster.as_deref(),
        Some("https://images.example.com/tt1.jpg")
    );
    assert_eq!(half.progress, 25.0, "ten minutes of a two-hour film");
    assert_eq!(first.cards[1].progress, 25.0, "one hour of four");

    // The row's cards are library items, so the web offers its action list on them, and the
    // poster carries the check mark the library's own grid draws.
    assert!(
        first.menu,
        "the Continue Watching row offers the action list"
    );
    assert!(!half.watched, "nothing has been watched through");
}

#[test]
fn a_continued_title_watched_through_carries_the_mark() {
    let mut item = library_item(
        "tt1",
        "Watched Through",
        "poster",
        "2026-10-03T00:00:00Z",
        30_000,
        120_000,
    );
    item.state.times_watched = 1;
    let model = model_with_library(vec![item]);

    let (_, rows) = screens::board::data(&model);
    let card = rows[0].cards.first().expect("a continued title");
    assert!(card.watched, "the item's own state says it was watched");
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

/// Marking a continued title watched is not a second kind of Dismiss, but on this row it reads
/// like one: the core clears the resume position, and a card is only in the row while it has one.
/// Pinned here so the two are never mistaken for each other again.
#[test]
fn marking_a_continued_title_watched_takes_it_out_of_the_row() {
    let mut item = library_item(
        "tt1",
        "Half Watched",
        "poster",
        "2026-10-03T00:00:00Z",
        30_000,
        120_000,
    );
    item.mark_as_watched::<stremio_lightning_env::AppEnv>(true);
    assert!(item.watched(), "the watch count went up");
    assert_eq!(item.state.time_offset, 0, "and the resume position is gone");

    let model = model_with_library(vec![item]);
    let (_, rows) = screens::board::data(&model);
    assert!(
        rows.is_empty(),
        "a title with no resume position is not something to continue"
    );
}

/// Web: a card offers Play only when the Streams bucket already holds a Stream for the Video the
/// title is sitting on. Both of `stremio-core-web`'s serializers look it up there to build the
/// player link, and `LibItem` drops its play button and its "Play" row when that link is null.
#[test]
fn play_is_offered_only_for_a_title_whose_stream_is_at_hand() {
    let mut item = library_item(
        "tt1",
        "Half Watched",
        "poster",
        "2026-10-03T00:00:00Z",
        30_000,
        120_000,
    );
    let model = model_with_library(vec![item.clone()]);
    let (_, rows) = screens::board::data(&model);
    assert!(
        !rows[0].cards[0].playable,
        "nothing has been played, so there is no Stream for Play to open"
    );

    item.state.video_id = Some("tt1:1:1".to_owned());
    let mut model = model_with_library(vec![item]);
    model.ctx.streams.items.insert(
        StreamsItemKey {
            meta_id: "tt1".to_owned(),
            video_id: "tt1:1:1".to_owned(),
        },
        serde_json::from_value(serde_json::json!({
            "stream": { "url": "https://example.com/tt1.mkv" },
            "type": "movie",
            "metaId": "tt1",
            "videoId": "tt1:1:1",
            "metaTransportUrl": "https://v3-cinemeta.strem.io/manifest.json",
            "streamTransportUrl": "https://torrentio.strem.fun/manifest.json",
            "state": null,
            "_mtime": "2026-10-03T00:00:00Z"
        }))
        .expect("a streams item"),
    );
    let (_, rows) = screens::board::data(&model);
    assert!(
        rows[0].cards[0].playable,
        "the Stream the core kept for this Video is what Play opens"
    );
}
