//! Building the app: the initial state, the effects core wants run first, and the loads the
//! window needs before it can show anything.

use futures::channel::mpsc::Receiver;
use stremio_core::models::catalog_with_filters::CatalogWithFilters;
use stremio_core::models::catalogs_with_extra::Selected;
use stremio_core::models::ctx::Ctx;
use stremio_core::models::installed_addons_with_filters::{
    InstalledAddonsRequest, InstalledAddonsWithFilters, Selected as InstalledSelected,
};
use stremio_core::runtime::msg::{Action, ActionCatalogsWithExtra, ActionLoad};
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
    let model = AppModel {
        ctx: Ctx::new(
            profile,
            LibraryBucket::new(uid.clone(), vec![]),
            StreamsBucket::new(uid.clone()),
            ServerUrlsBucket::new::<AppEnv>(uid.clone()),
            NotificationsBucket::new::<AppEnv>(uid.clone(), vec![]),
            SearchHistoryBucket::new(uid.clone()),
            DismissedEventsBucket::new(uid),
        ),
        board: Default::default(),
        discover,
        installed_addons,
        meta_details: Default::default(),
        player: Default::default(),
    };

    AppRuntime::new(
        model,
        discover_effects
            .into_iter()
            .chain(installed_effects)
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
}
