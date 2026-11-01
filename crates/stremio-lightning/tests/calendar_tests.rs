//! The Calendar's month, as the core fills it in: the year and month the screen shows, the months
//! its arrows lead to, and one cell per day of the month. No account, no network, no window:
//! `Load(Calendar(None))` is the call the web's route makes on mount.

use std::sync::{Arc, Once};

use chrono::Datelike;
use futures::channel::mpsc::Receiver;
use stremio_core::models::catalog_with_filters::CatalogWithFilters;
use stremio_core::models::continue_watching_preview::ContinueWatchingPreview;
use stremio_core::models::ctx::Ctx;
use stremio_core::models::installed_addons_with_filters::InstalledAddonsWithFilters;
use stremio_core::models::library_with_filters::{LibraryWithFilters, NotRemovedFilter};
use stremio_core::runtime::msg::{Action, ActionLoad};
use stremio_core::runtime::{RuntimeAction, RuntimeEvent};
use stremio_core::types::events::DismissedEventsBucket;
use stremio_core::types::library::LibraryBucket;
use stremio_core::types::notifications::NotificationsBucket;
use stremio_core::types::profile::Profile;
use stremio_core::types::resource::MetaItemPreview;
use stremio_core::types::search_history::SearchHistoryBucket;
use stremio_core::types::server_urls::ServerUrlsBucket;
use stremio_core::types::streams::StreamsBucket;
use stremio_lightning::app;
use stremio_lightning::screens;

type Events = Receiver<RuntimeEvent<stremio_lightning_env::AppEnv, app::AppModel>>;

fn init_env() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let dir =
            std::env::temp_dir().join(format!("stremio-lightning-cal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a temp folder");
        let tokio = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("a tokio runtime");
        stremio_lightning_env::AppEnv::init(dir, tokio.handle().clone()).expect("an environment");
    });
}

/// The app over an empty library, exactly as `app::build::runtime` builds it. The receiver is
/// handed back so the core can keep reporting events while the test dispatches.
fn runtime() -> (Arc<app::AppRuntime>, Events) {
    init_env();
    let profile = Profile::default();
    let uid = profile.uid();
    let library = LibraryBucket::new(uid.clone(), vec![]);
    let notifications =
        NotificationsBucket::new::<stremio_lightning_env::AppEnv>(uid.clone(), vec![]);
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
        calendar: Default::default(),
        continue_watching_preview,
        discover,
        library: library_with_filters,
        installed_addons,
        remote_addons: Default::default(),
        addon_details: Default::default(),
        meta_details: Default::default(),
        player: Default::default(),
    };
    let (runtime, events) = app::AppRuntime::new(model, vec![], 1000);
    (Arc::new(runtime), events)
}

fn load(runtime: &app::AppRuntime, month: Option<(i32, u32)>) {
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Load(ActionLoad::Calendar(month.map(|(year, month)| {
            stremio_core::models::calendar::Selected { month, year }
        }))),
    });
}

fn month_year(runtime: &app::AppRuntime) -> (i32, u32) {
    let model = runtime.model().expect("the model is readable");
    let selected = model.calendar.selected.clone().expect("a month is chosen");
    (selected.year, selected.month)
}

#[test]
fn a_load_with_no_month_opens_the_one_it_is_now() {
    let (runtime, _events) = runtime();
    load(&runtime, None);
    let (year, month) = month_year(&runtime);
    let now = chrono::Local::now().date_naive();
    assert_eq!((year, month), (now.year(), now.month()));

    let model = runtime.model().expect("the model is readable");
    let data = screens::calendar::data(&model);
    assert_eq!(data.year, year);
    assert!(!data.month.is_empty(), "the selector names the month");
    assert_eq!(data.days.len(), model.calendar.month_info.days as usize);
    assert_ne!(data.prev, data.month);
    assert_ne!(data.next, data.month);
}

#[test]
fn a_month_given_is_the_month_opened() {
    let (runtime, _events) = runtime();
    load(&runtime, Some((2024, 2)));
    assert_eq!(month_year(&runtime), (2024, 2));

    let model = runtime.model().expect("the model is readable");
    let data = screens::calendar::data(&model);
    assert_eq!(
        data.month, "February",
        "the month in the middle is spelled out"
    );
    // Web: `toMonth(..., 'short')` on the two buttons either side.
    assert_eq!(data.prev, "Jan");
    assert_eq!(data.next, "Mar");
    assert_eq!(data.days.len(), 29, "2024 is a leap year");
    assert_eq!(
        data.days[0].heading, "Thursday, February 1",
        "the first of February 2024 was a Thursday"
    );
    assert_eq!(
        data.first_weekday, 3,
        "a Monday-first week puts Thursday fourth"
    );
}

#[test]
fn a_title_added_after_the_month_loaded_asks_for_it_again() {
    let (runtime, _events) = runtime();
    let before = {
        let model = runtime.model().expect("the model is readable");
        screens::calendar::library_signature(&model)
    };
    assert!(
        screens::calendar::needs_load(None, before, true),
        "a Calendar that has never been built asks for its month"
    );
    assert!(
        !screens::calendar::needs_load(Some(before), before, false),
        "and a visit with the same library keeps the month it has"
    );

    // A title arrives -- from the web, or from this app's own details page.
    let preview: MetaItemPreview = serde_json::from_value(serde_json::json!({
        "id": "tt1",
        "type": "series",
        "name": "A series",
        "posterShape": "poster"
    }))
    .expect("a preview");
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Ctx(stremio_core::runtime::msg::ActionCtx::AddToLibrary(preview)),
    });

    let after = {
        let model = runtime.model().expect("the model is readable");
        assert_eq!(
            model.ctx.library.items.len(),
            1,
            "the title is in the library"
        );
        screens::calendar::library_signature(&model)
    };
    assert_ne!(
        before, after,
        "the library is not the one the month was built from"
    );
    assert!(
        screens::calendar::needs_load(Some(before), after, false),
        "so the next visit asks for the month again, and the title's Episodes come with it"
    );
}

#[test]
fn the_months_either_side_are_abbreviated_the_way_english_abbreviates_them() {
    let (runtime, _events) = runtime();
    load(&runtime, Some((2026, 10)));

    let model = runtime.model().expect("the model is readable");
    let data = screens::calendar::data(&model);
    assert_eq!(
        data.month, "October",
        "the month on screen stays spelled out"
    );
    assert_eq!(
        data.prev, "Sep",
        "`toMonth(..., 'short')` for en-US, which is what the web shows"
    );
    assert_eq!(data.next, "Nov");
}
