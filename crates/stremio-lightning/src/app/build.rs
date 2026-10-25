//! Building the app: the initial state, the effects core wants run first, and the loads the
//! window needs before it can show anything.

use futures::channel::mpsc::Receiver;
use stremio_core::models::catalog_with_filters::CatalogWithFilters;
use stremio_core::models::catalogs_with_extra::Selected;
use stremio_core::models::continue_watching_preview::ContinueWatchingPreview;
use stremio_core::models::ctx::Ctx;
use stremio_core::models::installed_addons_with_filters::{
    InstalledAddonsRequest, InstalledAddonsWithFilters, Selected as InstalledSelected,
};
use stremio_core::models::library_with_filters::{
    LibraryRequest, LibraryRequestPage, LibraryWithFilters, NotRemovedFilter,
    Selected as LibrarySelected,
};
use stremio_core::runtime::msg::{Action, ActionCatalogsWithExtra, ActionCtx, ActionLoad};
use stremio_core::runtime::{RuntimeAction, RuntimeEvent};
use stremio_core::types::events::DismissedEventsBucket;
use stremio_core::types::library::LibraryBucket;
use stremio_core::types::notifications::NotificationsBucket;
use stremio_core::types::profile::Profile;
use stremio_core::types::resource::MetaItemPreview;
use stremio_core::types::search_history::SearchHistoryBucket;
use stremio_core::types::server_urls::ServerUrlsBucket;
use stremio_core::types::streams::StreamsBucket;
use stremio_lightning_env::AppEnv;

use super::{AppModel, AppRuntime};

/// The runtime, and the events it will report. The effects the models asked for are already
/// queued inside it, so calling this and then [`load_everything`] is all a caller has to do.
pub fn runtime(profile: Profile) -> (AppRuntime, Receiver<RuntimeEvent<AppEnv, AppModel>>) {
    let uid = profile.uid();
    // `default()` has nothing to pick from, so a `Load` with no selection would select nothing.
    let (discover, discover_effects) = CatalogWithFilters::<MetaItemPreview>::new(&profile);
    let (installed_addons, installed_effects) = InstalledAddonsWithFilters::new(&profile);
    let library = LibraryBucket::new(uid.clone(), vec![]);
    let notifications = NotificationsBucket::new::<AppEnv>(uid.clone(), vec![]);
    // The preview is built from the buckets rather than defaulted, so a library that came back
    // with the stored session has its row on the first frame. After that the core keeps it in
    // step with the library by itself: it is stremio-core's model, not a second copy of it.
    let (continue_watching_preview, _) = ContinueWatchingPreview::new(&library, &notifications);
    // Web: `useLibrary` dispatches a `Load` with the route's request on mount. There is no route
    // here, so the Library opens on the core's default request -- everything, last watched first.
    let (library_with_filters, library_effects) =
        LibraryWithFilters::<NotRemovedFilter>::new(&library, &notifications);
    let model = AppModel {
        ctx: Ctx::new(
            profile,
            library,
            StreamsBucket::new(uid.clone()),
            ServerUrlsBucket::new::<AppEnv>(uid.clone()),
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
        addon_details: Default::default(),
        meta_details: Default::default(),
        player: Default::default(),
    };

    AppRuntime::new(
        model,
        discover_effects
            .into_iter()
            .chain(installed_effects)
            .chain(library_effects)
            .collect(),
        1000,
    )
}

/// The loads that fill every screen on startup, in the order the window needs them.
pub fn load_everything(runtime: &AppRuntime) {
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Load(ActionLoad::CatalogsWithExtra(Selected {
            r#type: None,
            extra: vec![],
        })),
    });
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::CatalogsWithExtra(ActionCatalogsWithExtra::LoadRange(0..8)),
    });
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Load(ActionLoad::CatalogWithFilters(None)),
    });
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Load(ActionLoad::InstalledAddonsWithFilters(InstalledSelected {
            request: InstalledAddonsRequest { r#type: None },
        })),
    });
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Load(ActionLoad::LibraryWithFilters(LibrarySelected {
            request: LibraryRequest {
                r#type: None,
                sort: Default::default(),
                page: LibraryRequestPage::default(),
            },
        })),
    });
}

/// Re-reads the parts of the account another client may have changed: the library (which carries
/// every `watched` mark and the one watch position per title), the user's profile, the installed
/// addons and the notifications.
///
/// Web: `src/App/App.js` dispatches exactly these four on mount and again on every window focus,
/// which is how a mark made in the browser shows up in another client.
pub fn pull_from_api(runtime: &AppRuntime) {
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Ctx(ActionCtx::PullAddonsFromAPI),
    });
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Ctx(ActionCtx::PullUserFromAPI { token: None }),
    });
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Ctx(ActionCtx::SyncLibraryWithAPI),
    });
    runtime.dispatch(RuntimeAction {
        field: None,
        action: Action::Ctx(ActionCtx::PullNotifications),
    });
}
