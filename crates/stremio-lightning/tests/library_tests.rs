//! The Library screen, asked of the app's own model.
//!
//! The grid is built here from a library the test makes itself: no account, no network, no
//! window. The core's `LibraryWithFilters` is the same model stremio-web's Library route reads,
//! so the ordering, filtering and per-item numbers are checked through the real core.

use std::sync::{Arc, Once};

use futures::channel::mpsc::Receiver;
use stremio_core::models::catalog_with_filters::CatalogWithFilters;
use stremio_core::models::continue_watching_preview::ContinueWatchingPreview;
use stremio_core::models::ctx::Ctx;
use stremio_core::models::installed_addons_with_filters::InstalledAddonsWithFilters;
use stremio_core::models::library_with_filters::{
    LibraryRequest, LibraryRequestPage, LibraryWithFilters, NotRemovedFilter, Selected, Sort,
};
use stremio_core::runtime::msg::{Action, ActionLoad};
use stremio_core::runtime::{RuntimeAction, RuntimeEvent};
use stremio_core::types::events::DismissedEventsBucket;
use stremio_core::types::library::{LibraryBucket, LibraryItem};
use stremio_core::types::notifications::{NotificationItem, NotificationsBucket};
use stremio_core::types::profile::Profile;
use stremio_core::types::resource::MetaItemPreview;
use stremio_core::types::search_history::SearchHistoryBucket;
use stremio_core::types::server_urls::ServerUrlsBucket;
use stremio_core::types::streams::StreamsBucket;
use stremio_lightning::{app, screens};

fn init_env() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let dir =
            std::env::temp_dir().join(format!("stremio-lightning-library-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temp folder");
        let tokio = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a tokio runtime");
        stremio_lightning_env::AppEnv::init(dir, tokio.handle().clone()).expect("an environment");
    });
}

/// The app's runtime over a library the test hands it, and the library already selected the way
/// `app::build::load_everything` selects it. The receiver is returned so the core can keep
/// reporting events for as long as the test dispatches; dropping it panics the runtime.
type Events = Receiver<RuntimeEvent<stremio_lightning_env::AppEnv, app::AppModel>>;

fn runtime_with(
    items: Vec<LibraryItem>,
    notifications: Vec<NotificationItem>,
) -> (Arc<app::AppRuntime>, Events) {
    init_env();
    let profile = Profile::default();
    let uid = profile.uid();
    let library = LibraryBucket::new(uid.clone(), items);
    let notifications =
        NotificationsBucket::new::<stremio_lightning_env::AppEnv>(uid.clone(), notifications);
    let (continue_watching_preview, _) = ContinueWatchingPreview::new(&library, &notifications);
    let (library_with_filters, _) =
        LibraryWithFilters::<NotRemovedFilter>::new(&library, &notifications);
    let (discover, _) = CatalogWithFilters::<MetaItemPreview>::new(&profile);
    let (installed_addons, _) = InstalledAddonsWithFilters::new(&profile);
    let model = app::AppModel {
        ctx: Ctx::new(
            profile,
            library,
            StreamsBucket::new(uid.clone()),
            ServerUrlsBucket::new::<stremio_lightning_env::AppEnv>(uid.clone()),
            notifications,
            SearchHistoryBucket::new(uid.clone()),
            DismissedEventsBucket::new(uid),
        ),
        board: Default::default(),
        continue_watching_preview,
        discover,
        library: library_with_filters,
        installed_addons,
        meta_details: Default::default(),
        player: Default::default(),
    };
    let (runtime, events) = app::AppRuntime::new(model, vec![], 1000);
    let runtime = Arc::new(runtime);
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Load(ActionLoad::LibraryWithFilters(Selected {
            request: LibraryRequest {
                r#type: None,
                sort: Sort::LastWatched,
                page: LibraryRequestPage::default(),
            },
        })),
    });
    (runtime, events)
}

// Each test varies a different handful of these, so the arguments stay positional: a builder
// would say less than the call sites do.
#[allow(clippy::too_many_arguments)]
fn library_item(
    id: &str,
    name: &str,
    kind: &str,
    shape: &str,
    last_watched: &str,
    offset: u64,
    duration: u64,
    times_watched: u32,
) -> LibraryItem {
    serde_json::from_value(serde_json::json!({
        "_id": id,
        "name": name,
        "type": kind,
        "poster": format!("https://images.example.com/{id}.jpg"),
        "posterShape": shape,
        "removed": false,
        "temp": false,
        "_mtime": "2026-10-01T00:00:00Z",
        "_ctime": "2026-09-01T00:00:00Z",
        "state": {
            "lastWatched": last_watched,
            "timeWatched": 0,
            "timeOffset": offset,
            "overallTimeWatched": 0,
            "timesWatched": times_watched,
            "flaggedWatched": times_watched,
            "duration": duration
        }
    }))
    .expect("a library item")
}

fn notification(meta_id: &str, video_id: &str) -> NotificationItem {
    serde_json::from_value(serde_json::json!({
        "metaId": meta_id,
        "videoId": video_id,
        "videoReleased": "2026-10-01T00:00:00Z"
    }))
    .expect("a notification item")
}

#[test]
fn the_grid_lists_the_whole_library_newest_watched_first() {
    let (runtime, _events) = runtime_with(
        vec![
            library_item(
                "tt1",
                "Half Watched",
                "movie",
                "landscape",
                "2026-10-03T00:00:00Z",
                30_000,
                120_000,
                0,
            ),
            library_item(
                "tt2",
                "Just Started",
                "series",
                "poster",
                "2026-10-02T00:00:00Z",
                60_000,
                240_000,
                0,
            ),
            library_item(
                "tt3",
                "Never Played",
                "movie",
                "poster",
                "2026-09-01T00:00:00Z",
                0,
                120_000,
                0,
            ),
        ],
        vec![
            notification("tt2", "tt2:1:1"),
            notification("tt2", "tt2:1:2"),
        ],
    );
    let model = runtime.model().expect("the model is readable");
    let data = screens::library::data(&model);

    let ids = data
        .cards
        .iter()
        .map(|card| card.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(ids, ["tt1", "tt2", "tt3"], "last watched first");

    let half = &data.cards[0];
    assert_eq!(half.title, "Half Watched");
    assert_eq!(half.kind, "movie");
    assert_eq!(
        half.poster.as_deref(),
        Some("https://images.example.com/tt1.jpg")
    );
    assert_eq!(half.shape, "square", "web draws a landscape poster square");
    assert_eq!(half.progress, 25.0);
    assert!(!half.watched);

    let started = &data.cards[1];
    assert_eq!(started.shape, "poster");
    assert_eq!(started.progress, 25.0);
    assert_eq!(
        started.new_videos, 2,
        "two new videos in the notification bucket"
    );

    assert_eq!(data.message, "");
    assert!(data.type_title == "All");
    assert_eq!(
        data.type_options
            .iter()
            .map(|(label, _)| label.as_str())
            .collect::<Vec<_>>(),
        ["All", "Movie", "Series"],
        "the core's priority order"
    );
    assert_eq!(
        data.sorts
            .iter()
            .map(|sort| sort.label.as_str())
            .collect::<Vec<_>>(),
        [
            "Last Watched",
            "A-Z",
            "Z-A",
            "Most Watched",
            "Watched",
            "Not Watched"
        ]
    );
    assert_eq!(
        data.sorts.iter().position(|sort| sort.selected),
        Some(0),
        "Last Watched is the default"
    );
}

#[test]
fn choosing_a_type_loads_only_that_type_and_the_message_says_when_it_is_empty() {
    let (runtime, _events) = runtime_with(
        vec![
            library_item(
                "tt1",
                "A Film",
                "movie",
                "poster",
                "2026-10-03T00:00:00Z",
                0,
                120_000,
                0,
            ),
            library_item(
                "tt2",
                "A Series",
                "series",
                "poster",
                "2026-10-02T00:00:00Z",
                0,
                240_000,
                0,
            ),
        ],
        vec![],
    );
    let request = {
        let model = runtime.model().expect("the model is readable");
        let data = screens::library::data(&model);
        data.type_requests[2].clone()
    };
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Load(ActionLoad::LibraryWithFilters(Selected { request })),
    });

    let model = runtime.model().expect("the model is readable");
    let data = screens::library::data(&model);
    assert_eq!(data.type_title, "Series");
    assert_eq!(
        data.cards
            .iter()
            .map(|card| card.id.as_str())
            .collect::<Vec<_>>(),
        ["tt2"]
    );
    assert_eq!(
        data.message, "",
        "a non-empty filtered library says nothing"
    );

    // A type with no titles in it: the core keeps the type in its selectable list only when the
    // library has one, so the empty case is the library itself being empty.
    let (empty, _events) = runtime_with(vec![], vec![]);
    let model = empty.model().expect("the model is readable");
    let data = screens::library::data(&model);
    assert_eq!(data.cards.len(), 0);
    assert_eq!(data.message, "Empty Library");
}

#[test]
fn a_watched_title_keeps_its_check_mark_and_a_removed_one_leaves_the_grid() {
    let watched = library_item(
        "tt1",
        "A Film",
        "movie",
        "poster",
        "2026-10-03T00:00:00Z",
        30_000,
        120_000,
        1,
    );
    let removed: LibraryItem = serde_json::from_value(serde_json::json!({
        "_id": "tt2",
        "name": "Removed Film",
        "type": "movie",
        "posterShape": "poster",
        "removed": true,
        "temp": false,
        "_mtime": "2026-10-01T00:00:00Z",
        "state": {
            "timeWatched": 0,
            "timeOffset": 0,
            "overallTimeWatched": 0,
            "timesWatched": 0,
            "flaggedWatched": 0,
            "duration": 120000
        }
    }))
    .expect("a removed library item");
    let (runtime, _events) = runtime_with(vec![watched, removed], vec![]);

    let model = runtime.model().expect("the model is readable");
    let data = screens::library::data(&model);
    assert_eq!(
        data.cards
            .iter()
            .map(|card| card.id.as_str())
            .collect::<Vec<_>>(),
        ["tt1"],
        "NotRemovedFilter keeps a removed title out of the catalog"
    );
    assert!(
        data.cards[0].watched,
        "times_watched is the web's watched check"
    );
    assert_eq!(data.cards[0].progress, 25.0);
}
