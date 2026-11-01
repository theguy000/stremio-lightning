//! The details page's library pair: whether the title is in the library, and whether it has been
//! watched. Both come from the account, not from the addons, so the test makes its own library.

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
            std::env::temp_dir().join(format!("stremio-lightning-actions-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temp folder");
        let tokio = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a tokio runtime");
        stremio_lightning_env::AppEnv::init(dir, tokio.handle().clone()).expect("an environment");
    });
}

/// The app's model over a library the test hands it, built the way `app::build::runtime` builds
/// it around whatever the stored library came back with.
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
        calendar: Default::default(),
        discover,
        library: library_with_filters,
        installed_addons,
        remote_addons: Default::default(),
        addon_details: Default::default(),
        meta_details: Default::default(),
        player: Default::default(),
    }
}

/// A library item the way the API sends one.
fn library_item(id: &str, removed: bool, temp: bool, times_watched: u32) -> LibraryItem {
    serde_json::from_value(serde_json::json!({
        "_id": id,
        "name": id,
        "type": "movie",
        "poster": format!("https://images.example.com/{id}.jpg"),
        "posterShape": "poster",
        "removed": removed,
        "temp": temp,
        "_mtime": "2026-10-03T00:00:00Z",
        "state": {
            "timeWatched": 0,
            "timeOffset": 0,
            "overallTimeWatched": 0,
            "timesWatched": times_watched,
            "flaggedWatched": 0,
            "duration": 0
        }
    }))
    .expect("a library item")
}

#[test]
fn a_title_in_the_library_offers_to_leave_it() {
    let model = model_with_library(vec![library_item("tt1", false, false, 0)]);

    let (in_library, watched) = screens::library_state(&model, "tt1");

    assert!(in_library, "the pair draws `remove from library`");
    assert!(!watched, "never played, so the eye offers to mark it");
}

#[test]
fn a_title_that_has_been_watched_says_so() {
    let model = model_with_library(vec![library_item("tt1", false, false, 1)]);

    let (in_library, watched) = screens::library_state(&model, "tt1");

    assert!(in_library);
    assert!(watched, "the core's own answer for the title");
}

#[test]
fn a_title_the_core_only_has_a_tombstone_for_is_not_in_the_library() {
    // Web: `inLibrary` is `!removed`, so a title that was taken out -- or only ever looked at,
    // which the core records as a temporary item -- offers to be added rather than removed.
    let model = model_with_library(vec![
        library_item("tt1", true, false, 0),
        library_item("tt2", true, true, 0),
    ]);

    assert!(!screens::library_state(&model, "tt1").0, "taken out");
    assert!(!screens::library_state(&model, "tt2").0, "only looked at");
    assert_eq!(
        screens::library_state(&model, "tt3"),
        (false, false),
        "a title the library never saw"
    );
}
